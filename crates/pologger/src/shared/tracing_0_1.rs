use alloc::vec::Vec;
use core::cell::RefCell;
use core::fmt;
use core::num::NonZero;
use std::thread_local;

use polog::{BaseHandler, SpanId};
use polog_util::{ExternalDescriptor, ExternalFilterFn};

use crate::{
    ExternalArg, ExternalArguments, ExternalEntry, Formatter, FormatterEventKind, KnownSpanId,
    MaybeKnownSpanId, SharedPologger, SharedPologgerTracingSpan,
};

thread_local! {
    /// Per-thread stack of entered foreign spans, keyed by subscriber ID.
    ///
    /// The `tracing::Subscriber` trait does not provide a portable way to query
    /// the currently-entered span, so this stack is maintained by
    /// `SharedPologger` itself. It lives only because `tracing` requires it;
    /// `polog` call sites never see this state.
    static ENTER_STACK: RefCell<Vec<EnterFrame>> = const { RefCell::new(Vec::new()) };
}

/// One frame of [`ENTER_STACK`].
///
/// Records everything `exit`, `record`, and `event` need to resolve the
/// current execution region without re-locking the subscriber's span map.
struct EnterFrame {
    /// Thr subscriber used to create the span.
    subscriber_id: u64,

    /// Tracing-side id (created in `new_span`).
    tracing_id: NonZero<u64>,

    /// Polog id of this execution region (created in `enter`).
    polog_id: SpanId,

    /// Parent execution region, or `None` if this is the thread's outermost
    /// entered span.
    parent_polog_id: Option<SpanId>,

    /// Cached from `new_span`.
    /// Avoids re-locking `spans` on `exit`.
    metadata: &'static tracing_0_1::Metadata<'static>,
}

impl<Fmt, Fff> tracing_0_1::Subscriber for SharedPologger<Fmt, Fff>
where
    Fmt: 'static + Formatter,
    Fff: 'static + ExternalFilterFn,
{
    #[inline]
    fn register_callsite(
        &self,
        metadata: &'static tracing_0_1::Metadata<'static>,
    ) -> tracing_core_0_1::Interest {
        if self.enabled(metadata) {
            tracing_core_0_1::Interest::always()
        } else {
            tracing_core_0_1::Interest::never()
        }
    }

    #[inline]
    fn event_enabled(&self, event: &tracing_0_1::Event<'_>) -> bool {
        self.enabled(event.metadata())
    }

    #[inline]
    fn enabled(&self, metadata: &tracing_0_1::Metadata<'_>) -> bool {
        let descriptor = descriptor_from_metadata(metadata);
        self.filter().is_external_enabled(&descriptor)
    }

    #[inline]
    fn max_level_hint(&self) -> Option<tracing_core_0_1::LevelFilter> {
        use polog_util::conv::tracing_core_0_1 as conv;
        let level = self.filter().external_max_level();
        Some(conv::level_filter_into_foreign_approx(level))
    }

    // Span object lifecycle: ForeignNew / ForeignClose.
    // One stable id per tracing span object (`span_id`).
    // This id is the target of `record` / `record_follows_from`.

    #[inline]
    fn new_span(&self, attributes: &tracing_0_1::span::Attributes<'_>) -> tracing_0_1::span::Id {
        let span_id = self.generate_span_id();
        let metadata = attributes.metadata();
        let subscriber_id = self.tracing_span_state().subscriber_id;

        // Compute the polog-side parent id for the `ForeignNew` emission.
        //
        // Chain model:
        //   new1 → enter1 → new2 → enter2 → ...
        // so `new2`'s parent is `enter1`'s polog id (the currently active
        // position of the parent in the chain), *not* `new1`'s span-object id.
        //
        // - Explicit parent (`attributes.parent()`): use the parent's enter
        //   polog id if it is currently entered on this thread; otherwise fall
        //   back to the parent's span-object id (its own `ForeignNew`).
        // - Contextual parent: use the innermost currently-entered span of this
        //   subscriber on this thread (`ENTER_STACK`'s top). If none, `Root`.
        let emission_parent_span_id = attributes.parent().map_or_else(
            || {
                ENTER_STACK.with(|cell| {
                    cell.borrow()
                        .iter()
                        .rev()
                        .find(|frame| frame.subscriber_id == subscriber_id)
                        .map(|frame| frame.polog_id)
                })
            },
            |parent_id| {
                let parent_obj_id = tracing_id_to_polog_span_id(parent_id);
                let entered_polog_id = ENTER_STACK.with(|cell| {
                    cell.borrow()
                        .iter()
                        .rev()
                        .find(|frame| {
                            frame.subscriber_id == subscriber_id
                                && SpanId(frame.tracing_id) == parent_obj_id
                        })
                        .map(|frame| frame.polog_id)
                });
                Some(entered_polog_id.unwrap_or(parent_obj_id))
            },
        );

        let _prev = self.tracing_span_state().spans.insert(
            span_id.0,
            SharedPologgerTracingSpan {
                metadata,
                parent_span_id: emission_parent_span_id,
                num_refs: 1,
            },
        );

        let descriptor = descriptor_from_metadata(metadata);
        let args = AttributesArgs { attributes };

        let entry = ExternalEntry {
            descriptor: &descriptor,
            message_fmt: None,
            args_visitor: &args,
            parent_span_id: emission_parent_span_id
                .map_or(MaybeKnownSpanId::Root, MaybeKnownSpanId::SpanId),
            span_id: KnownSpanId::SpanId(span_id),
            kind: FormatterEventKind::ForeignNew,
        };
        self.log_external_entry(&entry);

        tracing_0_1::span::Id::from_non_zero_u64(span_id.0)
    }

    #[inline]
    fn clone_span(&self, id: &tracing_0_1::span::Id) -> tracing_0_1::span::Id {
        let raw = id.clone().into_non_zero_u64();
        if let Some(mut span) = self.tracing_span_state().spans.get_mut(&raw) {
            span.num_refs = span.num_refs.saturating_add(1);
        }
        id.clone()
    }

    #[inline]
    fn try_close(&self, id: tracing_0_1::span::Id) -> bool {
        let raw = id.into_non_zero_u64();

        let (metadata, emission_parent_span_id) = {
            let spans = &self.tracing_span_state().spans;
            match spans.entry(raw) {
                dashmap::Entry::Occupied(mut entry) => {
                    let span = entry.get_mut();
                    span.num_refs = span.num_refs.saturating_sub(1);
                    if span.num_refs > 0 {
                        return false;
                    }
                    let span = entry.remove();
                    (span.metadata, span.parent_span_id)
                }
                dashmap::Entry::Vacant(_) => return true,
            }
        };

        let descriptor = descriptor_from_metadata(metadata);
        let entry = ExternalEntry {
            descriptor: &descriptor,
            message_fmt: None,
            args_visitor: &EmptyArgs,
            parent_span_id: emission_parent_span_id
                .map_or(MaybeKnownSpanId::Root, MaybeKnownSpanId::SpanId),
            span_id: KnownSpanId::SpanId(SpanId(raw)),
            kind: FormatterEventKind::ForeignClose,
        };
        self.log_external_entry(&entry);

        true
    }

    // Execution regions: DetachedEnter / DetachedExit.
    //
    // Each `enter` allocates a fresh polog `SpanId` for its region and pushes
    // a frame on the *thread's* stack.
    // `exit` pops the matching frame.

    #[inline]
    fn enter(&self, span: &tracing_0_1::span::Id) {
        let tracing_id = span.clone().into_non_zero_u64();

        let Some(metadata) = self
            .tracing_span_state()
            .spans
            .get(&tracing_id)
            .map(|s| s.metadata)
        else {
            return;
        };

        let subscriber_id = self.tracing_span_state().subscriber_id;
        let polog_id = self.generate_span_id();

        // Chain model: `enter`'s parent is the span's own `ForeignNew`
        // (span-object id), forming: new → enter → exit.
        // Both `enter` and `exit` use the same parent and the same polog id,
        // so the exit is the close of the enter, not a separate node.
        let parent_polog_id = Some(SpanId(tracing_id));

        ENTER_STACK.with(|cell| {
            cell.borrow_mut().push(EnterFrame {
                subscriber_id,
                tracing_id,
                polog_id,
                parent_polog_id,
                metadata,
            });
        });

        let descriptor = descriptor_from_metadata(metadata);
        let entry = ExternalEntry {
            descriptor: &descriptor,
            message_fmt: None,
            args_visitor: &EmptyArgs,
            parent_span_id: parent_polog_id
                .map_or(MaybeKnownSpanId::Root, MaybeKnownSpanId::SpanId),
            span_id: KnownSpanId::SpanId(polog_id),
            kind: FormatterEventKind::DetachedEnter,
        };
        self.log_external_entry(&entry);
    }

    #[inline]
    fn exit(&self, span: &tracing_0_1::span::Id) {
        let tracing_id = span.clone().into_non_zero_u64();
        let subscriber_id = self.tracing_span_state().subscriber_id;

        let frame = ENTER_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            if stack.last().is_some_and(|frame| {
                frame.subscriber_id == subscriber_id && frame.tracing_id == tracing_id
            }) {
                stack.pop()
            } else {
                // Out-of-order exit: pop the innermost matching frame.
                let pos = stack.iter().rposition(|frame| {
                    frame.subscriber_id == subscriber_id && frame.tracing_id == tracing_id
                })?;
                Some(stack.remove(pos))
            }
        });

        let Some(frame) = frame else {
            return;
        };

        let descriptor = descriptor_from_metadata(frame.metadata);
        let entry = ExternalEntry {
            descriptor: &descriptor,
            message_fmt: None,
            args_visitor: &EmptyArgs,
            parent_span_id: frame
                .parent_polog_id
                .map_or(MaybeKnownSpanId::Root, MaybeKnownSpanId::SpanId),
            span_id: KnownSpanId::SpanId(frame.polog_id),
            kind: FormatterEventKind::DetachedExit,
        };
        self.log_external_entry(&entry);
    }

    // -----------------------------------------------------------------------
    // Dynamic fields
    // -----------------------------------------------------------------------

    #[inline]
    fn record(&self, span: &tracing_0_1::span::Id, values: &tracing_0_1::span::Record<'_>) {
        let tracing_id = span.clone().into_non_zero_u64();

        let Some(metadata) = self
            .tracing_span_state()
            .spans
            .get(&tracing_id)
            .map(|span| span.metadata)
        else {
            return;
        };

        // Attach to the innermost currently-entered region of this span if
        // any.
        // Otherwise attach to the span object itself.
        let subscriber_id = self.tracing_span_state().subscriber_id;
        let span_id = ENTER_STACK
            .with(|s| {
                s.borrow()
                    .iter()
                    .rev()
                    .find(|frame| {
                        frame.subscriber_id == subscriber_id && frame.tracing_id == tracing_id
                    })
                    .map(|frame| KnownSpanId::SpanId(frame.polog_id))
            })
            .unwrap_or(KnownSpanId::SpanId(SpanId(tracing_id)));

        let mut descriptor = descriptor_from_metadata(metadata);
        descriptor.message = Some("record");
        let args = RecordArgs { record: values };

        let entry = ExternalEntry {
            descriptor: &descriptor,
            message_fmt: None,
            args_visitor: &args,
            parent_span_id: MaybeKnownSpanId::Unknown,
            span_id,
            kind: FormatterEventKind::Log,
        };
        self.log_external_entry(&entry);
    }

    #[inline]
    fn record_follows_from(&self, span: &tracing_0_1::span::Id, follows: &tracing_0_1::span::Id) {
        let tracing_id = span.clone().into_non_zero_u64();
        let follows_id = follows.clone().into_non_zero_u64();

        let Some(metadata) = self
            .tracing_span_state()
            .spans
            .get(&tracing_id)
            .map(|span| span.metadata)
        else {
            return;
        };

        let mut descriptor = descriptor_from_metadata(metadata);
        descriptor.message = Some("record_follows_from");

        let entry = ExternalEntry {
            descriptor: &descriptor,
            message_fmt: None,
            args_visitor: &RecordFollowsFromArgs(SpanId(follows_id)),
            parent_span_id: MaybeKnownSpanId::Unknown,
            span_id: KnownSpanId::SpanId(SpanId(tracing_id)),
            kind: FormatterEventKind::Log,
        };
        self.log_external_entry(&entry);
    }

    // -----------------------------------------------------------------------
    // Events
    // -----------------------------------------------------------------------

    #[inline]
    fn event(&self, event: &tracing_0_1::Event<'_>) {
        let metadata = event.metadata();
        let descriptor = descriptor_from_metadata(metadata);
        let args = EventArgs { event };

        let (parent_span_id, span_id) = event.parent().map_or_else(
            // Attach to the innermost currently-entered region of this
            // subscriber on this thread, if any.
            || {
                let subscriber_id = self.tracing_span_state().subscriber_id;
                ENTER_STACK.with(|stack| {
                    stack
                        .borrow()
                        .iter()
                        .rev()
                        .find(|frame| frame.subscriber_id == subscriber_id)
                        .map_or((MaybeKnownSpanId::Root, KnownSpanId::Root), |frame| {
                            (
                                frame
                                    .parent_polog_id
                                    .map_or(MaybeKnownSpanId::Root, MaybeKnownSpanId::SpanId),
                                KnownSpanId::SpanId(frame.polog_id),
                            )
                        })
                })
            },
            // Explicit `child_of` parent wins.
            |parent| {
                (
                    MaybeKnownSpanId::Root,
                    KnownSpanId::SpanId(tracing_id_to_polog_span_id(parent)),
                )
            },
        );

        let entry = ExternalEntry {
            descriptor: &descriptor,
            message_fmt: None,
            args_visitor: &args,
            parent_span_id,
            span_id,
            kind: FormatterEventKind::Log,
        };
        self.log_external_entry(&entry);
    }

    // -----------------------------------------------------------------------
    // Misc
    // -----------------------------------------------------------------------

    #[inline]
    fn on_register_dispatch(&self, _subscriber: &tracing_0_1::Dispatch) {}

    #[inline]
    fn current_span(&self) -> tracing_core_0_1::span::Current {
        let subscriber_id = self.tracing_span_state().subscriber_id;

        ENTER_STACK.with(|stack| {
            stack
                .borrow()
                .iter()
                .rev()
                .find(|frame| frame.subscriber_id == subscriber_id)
                .map_or_else(tracing_core_0_1::span::Current::none, |frame| {
                    tracing_core_0_1::span::Current::new(
                        tracing_core_0_1::span::Id::from_non_zero_u64(frame.tracing_id),
                        frame.metadata,
                    )
                })
        })
    }
}

/// Converts `tracing` metadata into an `ExternalDescriptor`.
///
/// All fields except `column` are populated, because `tracing` does not
/// expose a source column.
#[inline]
fn descriptor_from_metadata<'a>(metadata: &'a tracing_0_1::Metadata<'a>) -> ExternalDescriptor<'a> {
    let level = polog_util::conv::tracing_core_0_1::level_from_foreign(*metadata.level());
    ExternalDescriptor {
        name: Some(metadata.name()),
        level,
        target: Some(metadata.target()),
        module_path: metadata.module_path(),
        file: metadata.file(),
        line: metadata.line(),
        column: None,
    }
}

/// Converts a `tracing` span ID to a `polog::SpanId`.
///
/// The raw `u64` from the `tracing` span ID is the polog span object's ID,
/// which `new_span` allocated from the shared `SpanIdGenerator`.
#[inline]
fn tracing_id_to_polog_span_id(id: &tracing_0_1::span::Id) -> SpanId {
    SpanId(id.clone().into_non_zero_u64())
}

/// `ExternalArguments` adapter over `tracing::span::Attributes`.
struct AttributesArgs<'a> {
    attributes: &'a tracing_0_1::span::Attributes<'a>,
}

impl ExternalArguments for AttributesArgs<'_> {
    #[inline]
    fn visit_msg<F>(&self, _cb: F)
    where
        F: for<'b> FnOnce(ExternalArg<'b>),
    {
    }

    #[inline]
    fn visit_args<F>(&self, cb: F)
    where
        F: for<'b> FnMut(&'b str, ExternalArg<'b>),
    {
        self.attributes.record(&mut ArgsVisitor(cb));
    }
}

/// `ExternalArguments` adapter over `tracing::Event`.
///
/// Distinguishes the `message` field from the remaining fields so that
/// `write_external_message_fmt` can pick up the message without also
/// duplicating it in the argument list.
struct EventArgs<'a> {
    event: &'a tracing_0_1::Event<'a>,
}

impl ExternalArguments for EventArgs<'_> {
    #[inline]
    fn visit_msg<F>(&self, cb: F)
    where
        F: for<'b> FnOnce(ExternalArg<'b>),
    {
        self.event.record(&mut MessageVisitor(Some(cb)));
    }

    #[inline]
    fn visit_args<F>(&self, cb: F)
    where
        F: for<'b> FnMut(&'b str, ExternalArg<'b>),
    {
        self.event.record(&mut ArgsVisitorSkipMessage(cb, false));
    }
}

/// `ExternalArguments` adapter over `tracing::span::Record`.
struct RecordArgs<'a> {
    record: &'a tracing_0_1::span::Record<'a>,
}

impl ExternalArguments for RecordArgs<'_> {
    #[inline]
    fn visit_msg<F>(&self, _cb: F)
    where
        F: for<'b> FnOnce(ExternalArg<'b>),
    {
    }

    #[inline]
    fn visit_args<F>(&self, cb: F)
    where
        F: for<'b> FnMut(&'b str, ExternalArg<'b>),
    {
        self.record.record(&mut ArgsVisitor(cb));
    }
}

/// `ExternalArguments` adapter for a `record_follows_from` event.
///
/// Exposes the followed span ID as a `follows_from_span_id` argument.
struct RecordFollowsFromArgs(SpanId);

impl ExternalArguments for RecordFollowsFromArgs {
    #[inline]
    fn visit_msg<F>(&self, _cb: F)
    where
        F: for<'b> FnOnce(ExternalArg<'b>),
    {
    }

    #[inline]
    fn visit_args<F>(&self, mut cb: F)
    where
        F: for<'b> FnMut(&'b str, ExternalArg<'b>),
    {
        cb("follows_from_span_id", ExternalArg::U64(self.0.0.get()));
    }
}

/// `ExternalArguments` with no fields.
struct EmptyArgs;

impl ExternalArguments for EmptyArgs {
    #[inline]
    fn visit_msg<F>(&self, _cb: F)
    where
        F: for<'b> FnOnce(ExternalArg<'b>),
    {
    }

    #[inline]
    fn visit_args<F>(&self, _cb: F)
    where
        F: for<'b> FnMut(&'b str, ExternalArg<'b>),
    {
    }
}

/// `tracing::field::Visit` that forwards each field to a
/// `FnMut(&str, ExternalArg<'_>)`.
struct ArgsVisitor<F>(F);

impl<F> tracing_0_1::field::Visit for ArgsVisitor<F>
where
    F: for<'b> FnMut(&'b str, ExternalArg<'b>),
{
    #[inline]
    fn record_bool(&mut self, field: &tracing_0_1::field::Field, value: bool) {
        (self.0)(field.name(), ExternalArg::Bool(value));
    }

    #[inline]
    fn record_i64(&mut self, field: &tracing_0_1::field::Field, value: i64) {
        (self.0)(field.name(), ExternalArg::I64(value));
    }

    #[inline]
    fn record_u64(&mut self, field: &tracing_0_1::field::Field, value: u64) {
        (self.0)(field.name(), ExternalArg::U64(value));
    }

    #[inline]
    fn record_i128(&mut self, field: &tracing_0_1::field::Field, value: i128) {
        (self.0)(field.name(), ExternalArg::I128(value));
    }

    #[inline]
    fn record_u128(&mut self, field: &tracing_0_1::field::Field, value: u128) {
        (self.0)(field.name(), ExternalArg::U128(value));
    }

    #[inline]
    fn record_f64(&mut self, field: &tracing_0_1::field::Field, value: f64) {
        (self.0)(field.name(), ExternalArg::F64(value));
    }

    #[inline]
    fn record_str(&mut self, field: &tracing_0_1::field::Field, value: &str) {
        (self.0)(field.name(), ExternalArg::Str(value));
    }

    #[inline]
    fn record_error(
        &mut self,
        field: &tracing_0_1::field::Field,
        value: &(dyn core::error::Error + 'static),
    ) {
        (self.0)(field.name(), ExternalArg::Error(value));
    }

    #[inline]
    fn record_debug(&mut self, field: &tracing_0_1::field::Field, value: &dyn fmt::Debug) {
        (self.0)(field.name(), ExternalArg::Debug(value));
    }
}

/// Like [`ArgsVisitor`] but skips the first `message` field, which is
/// emitted separately by `write_external_message_fmt`.
struct ArgsVisitorSkipMessage<F>(F, bool);

impl<F> ArgsVisitorSkipMessage<F> {
    fn can_use(&mut self, key: &str) -> bool {
        if self.1 || key != "message" {
            true
        } else {
            self.1 = true;
            false
        }
    }
}

impl<F> tracing_0_1::field::Visit for ArgsVisitorSkipMessage<F>
where
    F: for<'b> FnMut(&'b str, ExternalArg<'b>),
{
    #[inline]
    fn record_bool(&mut self, field: &tracing_0_1::field::Field, value: bool) {
        if self.can_use(field.name()) {
            (self.0)(field.name(), ExternalArg::Bool(value));
        }
    }

    #[inline]
    fn record_i64(&mut self, field: &tracing_0_1::field::Field, value: i64) {
        if self.can_use(field.name()) {
            (self.0)(field.name(), ExternalArg::I64(value));
        }
    }

    #[inline]
    fn record_u64(&mut self, field: &tracing_0_1::field::Field, value: u64) {
        if self.can_use(field.name()) {
            (self.0)(field.name(), ExternalArg::U64(value));
        }
    }

    #[inline]
    fn record_i128(&mut self, field: &tracing_0_1::field::Field, value: i128) {
        if self.can_use(field.name()) {
            (self.0)(field.name(), ExternalArg::I128(value));
        }
    }

    #[inline]
    fn record_u128(&mut self, field: &tracing_0_1::field::Field, value: u128) {
        if self.can_use(field.name()) {
            (self.0)(field.name(), ExternalArg::U128(value));
        }
    }

    #[inline]
    fn record_f64(&mut self, field: &tracing_0_1::field::Field, value: f64) {
        if self.can_use(field.name()) {
            (self.0)(field.name(), ExternalArg::F64(value));
        }
    }

    #[inline]
    fn record_str(&mut self, field: &tracing_0_1::field::Field, value: &str) {
        if self.can_use(field.name()) {
            (self.0)(field.name(), ExternalArg::Str(value));
        }
    }

    #[inline]
    fn record_error(
        &mut self,
        field: &tracing_0_1::field::Field,
        value: &(dyn core::error::Error + 'static),
    ) {
        if self.can_use(field.name()) {
            (self.0)(field.name(), ExternalArg::Error(value));
        }
    }

    #[inline]
    fn record_debug(&mut self, field: &tracing_0_1::field::Field, value: &dyn fmt::Debug) {
        if self.can_use(field.name()) {
            (self.0)(field.name(), ExternalArg::Debug(value));
        }
    }
}

/// `tracing::field::Visit` that captures only the first `message` field.
struct MessageVisitor<F>(Option<F>);

impl<F> MessageVisitor<F> {
    fn as_first_message_visitor(&mut self, key: &str) -> Option<F> {
        if key == "message" {
            self.0.take()
        } else {
            None
        }
    }
}

impl<F> tracing_0_1::field::Visit for MessageVisitor<F>
where
    F: for<'b> FnOnce(ExternalArg<'b>),
{
    #[inline]
    fn record_bool(&mut self, field: &tracing_0_1::field::Field, value: bool) {
        if let Some(visitor) = self.as_first_message_visitor(field.name()) {
            visitor(ExternalArg::Bool(value));
        }
    }

    #[inline]
    fn record_i64(&mut self, field: &tracing_0_1::field::Field, value: i64) {
        if let Some(visitor) = self.as_first_message_visitor(field.name()) {
            visitor(ExternalArg::I64(value));
        }
    }

    #[inline]
    fn record_u64(&mut self, field: &tracing_0_1::field::Field, value: u64) {
        if let Some(visitor) = self.as_first_message_visitor(field.name()) {
            visitor(ExternalArg::U64(value));
        }
    }

    #[inline]
    fn record_i128(&mut self, field: &tracing_0_1::field::Field, value: i128) {
        if let Some(visitor) = self.as_first_message_visitor(field.name()) {
            visitor(ExternalArg::I128(value));
        }
    }

    #[inline]
    fn record_u128(&mut self, field: &tracing_0_1::field::Field, value: u128) {
        if let Some(visitor) = self.as_first_message_visitor(field.name()) {
            visitor(ExternalArg::U128(value));
        }
    }

    #[inline]
    fn record_f64(&mut self, field: &tracing_0_1::field::Field, value: f64) {
        if let Some(visitor) = self.as_first_message_visitor(field.name()) {
            visitor(ExternalArg::F64(value));
        }
    }

    #[inline]
    fn record_str(&mut self, field: &tracing_0_1::field::Field, value: &str) {
        if let Some(visitor) = self.as_first_message_visitor(field.name()) {
            visitor(ExternalArg::Str(value));
        }
    }

    #[inline]
    fn record_error(
        &mut self,
        field: &tracing_0_1::field::Field,
        value: &(dyn core::error::Error + 'static),
    ) {
        if let Some(visitor) = self.as_first_message_visitor(field.name()) {
            visitor(ExternalArg::Error(value));
        }
    }

    #[inline]
    fn record_debug(&mut self, field: &tracing_0_1::field::Field, value: &dyn fmt::Debug) {
        if let Some(visitor) = self.as_first_message_visitor(field.name()) {
            visitor(ExternalArg::Debug(value));
        }
    }
}
