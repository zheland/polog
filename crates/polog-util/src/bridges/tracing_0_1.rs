//! Bridge from `tracing` 0.1 to `polog`.

use alloc::vec::Vec;
use core::cell::RefCell;
use core::fmt;
use std::thread_local;

use polog::{SharedHandler, SpanId};

use crate::util::MaybeStaticStr;
use crate::{
    ExternalArgValue, ExternalArgs, ExternalArgsVisitor, ExternalDescriptor, ExternalHandler,
    ExternalMessage, conv,
};

thread_local! {
    /// Per-thread stack of entered foreign spans.
    ///
    /// The `tracing::Subscriber` trait does not provide a portable way to query
    /// the currently-entered span, so this stack is maintained by
    /// `SharedPologger` itself.
    /// It lives only because `tracing` requires it.
    static ENTER_STACK: RefCell<Vec<EnterFrame>> = const { RefCell::new(Vec::new()) };
}

/// A [`tracing::Dispatch`] adapter over a [`polog::Handler`] backend.
///
/// Forwards `tracing` events into a `polog` handler chain.
///
/// Register a `BridgeLog` as the global `log` logger to receive
/// `log!` macros from the whole process into a `polog` backend.
///
/// [`polog::Handler`]: polog::Handler
/// [`tracing::Dispatch`]: tracing_0_1::Dispatch
#[derive(Default, Debug)]
pub struct BridgeTracing<H> {
    /// The `polog` backend to forward to.
    handler: H,

    /// Subscriber state used by the `tracing::Subscriber` adapter.
    data: Data,
}

impl<H> BridgeTracing<H> {
    /// Creates a new bridge from `tracing` 0.1.x to `polog`.
    #[inline]
    #[must_use]
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            data: Data::default(),
        }
    }

    /// Returns a reference to the wrapped handler.
    #[inline]
    #[must_use]
    pub const fn handler(&self) -> &H {
        &self.handler
    }

    /// Returns a mutable reference to the wrapped handler.
    #[inline]
    #[must_use]
    pub const fn handler_mut(&mut self) -> &mut H {
        &mut self.handler
    }

    /// Consumes the middleware, returning the wrapped handler.
    #[inline]
    #[must_use]
    pub fn into_handler(self) -> H {
        self.handler
    }

    /// Consumes and registers this `SharedPologger` as the process-wide
    /// `tracing` subscriber.
    ///
    /// # Errors
    ///
    /// Returns the same error as `tracing::subscriber::set_global_default`:
    /// the global subscriber has already been set.
    #[cfg(feature = "tracing-0-1")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tracing-0-1")))]
    #[inline]
    pub fn try_install_global(self) -> Result<(), tracing_0_1::subscriber::SetGlobalDefaultError>
    where
        H: SharedHandler<()> + ExternalHandler<()>,
    {
        tracing_0_1::subscriber::set_global_default(self)
    }
}

/// Per-subscriber state used by the `tracing::Subscriber` adapter.
///
/// `SharedPologger` can be registered as a global `tracing` subscriber.
/// This struct tracks the foreign span objects that subscriber created and the
/// frames of the current thread's entered-span stack, keyed by the
/// subscriber's unique ID.
#[derive(Default, Debug)]
struct Data {
    /// Live foreign span objects, keyed by polog `SpanId`.
    pub spans: dashmap::DashMap<SpanId, SpanData>,

    /// Unique ID of the subscriber that owns this state, so the
    /// thread-local enter stack can tell subscribers apart.
    pub subscriber_id: u64,
}

/// A foreign span object tracked by the `tracing::Subscriber` adapter.
#[derive(Copy, Clone, Debug)]
struct SpanData {
    /// Static metadata of the tracing span.
    metadata: &'static tracing_0_1::Metadata<'static>,

    /// Current ref-count, as tracked by `clone_span` and `try_close`.
    num_refs: usize,

    /// Determines if the span state change events must be emitted to `polog`.
    is_enabled: bool,
}

/// One frame of [`ENTER_STACK`].
///
/// Records everything `exit`, `record`, and `event` need to resolve the
/// current execution region without re-locking the subscriber's span map.
struct EnterFrame {
    /// The subscriber used to create the span.
    subscriber_id: u64,

    /// Span id emitted by `new_span`.
    /// Also a valid tracing span id.
    parent_span_id: SpanId,

    /// Span id emitted by `enter_span`.
    /// Only valid within `polog` scope.
    span_id: SpanId,

    /// Cached from `new_span`.
    /// Avoids re-locking `spans` on `exit`.
    is_enabled: bool,

    /// Cached from `new_span`.
    /// Avoids re-locking `spans` on `exit`.
    metadata: &'static tracing_0_1::Metadata<'static>,
}

impl<H> tracing_0_1::Subscriber for BridgeTracing<H>
where
    H: SharedHandler<()> + ExternalHandler<()>,
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
        H::try_enable_external(&self.handler, metadata, ()).is_some()
    }

    #[inline]
    fn max_level_hint(&self) -> Option<tracing_core_0_1::LevelFilter> {
        None
    }

    #[inline]
    fn new_span(&self, attributes: &tracing_0_1::span::Attributes<'_>) -> tracing_0_1::span::Id {
        let span_id = self.handler.generate_span_id();
        let subscriber_id = self.data.subscriber_id;
        let metadata = attributes.metadata();
        let context = H::try_enable_external(&self.handler, metadata, ());

        let parent_span_id = get_span_id_from_stack(subscriber_id, attributes.parent());

        let _prev = self.data.spans.insert(
            span_id,
            SpanData {
                metadata,
                num_refs: 1,
                is_enabled: context.is_some(),
            },
        );

        if let Some(context) = H::try_enable_external(&self.handler, metadata, ()) {
            H::new_span_external(
                &mut self.handler.clone(),
                metadata,
                attributes,
                parent_span_id,
                span_id,
                context,
            );
        }

        tracing_0_1::span::Id::from_non_zero_u64(span_id.0)
    }

    #[inline]
    fn clone_span(&self, id: &tracing_0_1::span::Id) -> tracing_0_1::span::Id {
        let span_id = SpanId(id.into_non_zero_u64());
        if let Some(mut span) = self.data.spans.get_mut(&span_id) {
            span.num_refs = span.num_refs.saturating_add(1);
        }
        id.clone()
    }

    #[inline]
    fn try_close(&self, id: tracing_0_1::span::Id) -> bool {
        let span_id = SpanId(id.into_non_zero_u64());

        let (metadata, was_enabled) = {
            let spans = &self.data.spans;
            match spans.entry(span_id) {
                dashmap::Entry::Occupied(mut entry) => {
                    let span = entry.get_mut();
                    span.num_refs = span.num_refs.saturating_sub(1);
                    if span.num_refs > 0 {
                        return false;
                    }
                    let span = entry.remove();
                    (span.metadata, span.is_enabled)
                }

                // The span is unknown.
                dashmap::Entry::Vacant(_) => return true,
            }
        };

        if was_enabled {
            H::close_span_external(&mut self.handler.clone(), metadata, span_id, ());
        }

        true
    }

    #[inline]
    fn enter(&self, span: &tracing_0_1::span::Id) {
        let parent_span_id = SpanId(span.into_non_zero_u64());

        let Some(metadata) = self.data.spans.get(&parent_span_id).map(|s| s.metadata) else {
            return;
        };

        let subscriber_id = self.data.subscriber_id;
        let span_id = self.handler.generate_span_id();
        let context = H::try_enable_external(&self.handler, metadata, ());

        ENTER_STACK.with(|cell| {
            cell.borrow_mut().push(EnterFrame {
                subscriber_id,
                parent_span_id,
                span_id,
                is_enabled: context.is_some(),
                metadata,
            });
        });

        if let Some(context) = context {
            H::enter_span_external(
                &mut self.handler.clone(),
                metadata,
                &(),
                Some(parent_span_id),
                span_id,
                context,
            );
        }
    }

    #[inline]
    fn exit(&self, span: &tracing_0_1::span::Id) {
        let parent_span_id = SpanId(span.into_non_zero_u64());
        let subscriber_id = self.data.subscriber_id;

        let frame = ENTER_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            if stack.last().is_some_and(|frame| {
                frame.subscriber_id == subscriber_id && frame.parent_span_id == parent_span_id
            }) {
                stack.pop()
            } else {
                // Out-of-order exit: pop the innermost matching frame.
                let pos = stack.iter().rposition(|frame| {
                    frame.subscriber_id == subscriber_id && frame.parent_span_id == parent_span_id
                })?;
                Some(stack.remove(pos))
            }
        });

        let Some(frame) = frame else {
            return;
        };

        if frame.is_enabled {
            H::exit_span_external(&mut self.handler.clone(), frame.metadata, frame.span_id, ());
        }
    }

    // -----------------------------------------------------------------------
    // Dynamic fields
    // -----------------------------------------------------------------------

    #[inline]
    fn record(&self, span: &tracing_0_1::span::Id, values: &tracing_0_1::span::Record<'_>) {
        let parent_span_id = SpanId(span.into_non_zero_u64());

        let Some(metadata) = self
            .data
            .spans
            .get(&parent_span_id)
            .map(|span| span.metadata)
        else {
            return;
        };

        // Attach to the innermost currently-entered region of this span if any.
        // Otherwise attach to the span object itself.
        let subscriber_id = self.data.subscriber_id;
        let span_id = ENTER_STACK
            .with(|stack| {
                (stack.borrow().iter().rev())
                    .find(|frame| {
                        frame.subscriber_id == subscriber_id
                            && frame.parent_span_id == parent_span_id
                    })
                    .map(|frame| frame.span_id)
            })
            .unwrap_or(parent_span_id);

        H::record_span_external(&mut self.handler.clone(), metadata, values, span_id, ());
    }

    #[expect(clippy::similar_names, reason = "better than alternatives")]
    #[inline]
    fn record_follows_from(&self, span: &tracing_0_1::span::Id, follows: &tracing_0_1::span::Id) {
        let follower_span_id = SpanId(span.into_non_zero_u64());
        let followed_span_id = SpanId(follows.clone().into_non_zero_u64());

        let Some(metadata) = (self.data.spans)
            .get(&follower_span_id)
            .map(|span| span.metadata)
        else {
            return;
        };

        H::link_span_external(
            &mut self.handler.clone(),
            metadata,
            followed_span_id,
            follower_span_id,
            (),
        );
    }

    // -----------------------------------------------------------------------
    // Events
    // -----------------------------------------------------------------------

    #[inline]
    fn event(&self, event: &tracing_0_1::Event<'_>) {
        let subscriber_id = self.data.subscriber_id;
        let metadata = event.metadata();

        let span_id = get_span_id_from_stack(subscriber_id, event.parent());

        if let Some(context) = H::try_enable_external(&self.handler, metadata, ()) {
            H::log_external(&mut self.handler.clone(), metadata, event, span_id, context);
        }
    }

    // -----------------------------------------------------------------------
    // Misc
    // -----------------------------------------------------------------------

    #[inline]
    fn on_register_dispatch(&self, _subscriber: &tracing_0_1::Dispatch) {}

    #[inline]
    fn current_span(&self) -> tracing_core_0_1::span::Current {
        let subscriber_id = self.data.subscriber_id;

        ENTER_STACK.with(|stack| {
            stack
                .borrow()
                .iter()
                .rev()
                .find(|frame| frame.subscriber_id == subscriber_id)
                .map_or_else(tracing_core_0_1::span::Current::none, |frame| {
                    tracing_core_0_1::span::Current::new(
                        tracing_core_0_1::span::Id::from_non_zero_u64(frame.parent_span_id.0),
                        frame.metadata,
                    )
                })
        })
    }
}

#[inline]
fn get_span_id_from_stack(
    subscriber_id: u64,
    explicit_tracing_parent_id: Option<&tracing_0_1::span::Id>,
) -> Option<SpanId> {
    explicit_tracing_parent_id.map_or_else(
        || {
            // If the span not specified get is from the enter stack.
            ENTER_STACK.with(|cell| {
                (cell.borrow().iter().rev())
                    .find(|frame| frame.subscriber_id == subscriber_id)
                    .map(|frame| frame.span_id)
            })
        },
        |tracing_parent_id| {
            let parent_span_id = SpanId(tracing_parent_id.into_non_zero_u64());
            let span_id = ENTER_STACK.with(|cell| {
                (cell.borrow().iter().rev())
                    .find(|frame| {
                        frame.subscriber_id == subscriber_id && frame.span_id == parent_span_id
                    })
                    .map(|frame| frame.span_id)
            });
            Some(span_id.unwrap_or(parent_span_id))
        },
    )
}

impl ExternalDescriptor for tracing_0_1::Metadata<'_> {
    #[inline]
    fn message(&self) -> Option<ExternalMessage<'_>> {
        Some(ExternalMessage::Str(MaybeStaticStr::Static(
            tracing_0_1::Metadata::name(self),
        )))
    }

    #[inline]
    fn level(&self) -> polog::Level {
        conv::tracing_core_0_1::level_from_foreign(*self.level())
    }

    #[inline]
    fn target(&self) -> Option<MaybeStaticStr<'_>> {
        Some(MaybeStaticStr::Borrowed(tracing_0_1::Metadata::target(
            self,
        )))
    }

    #[inline]
    fn module_path(&self) -> Option<MaybeStaticStr<'_>> {
        tracing_0_1::Metadata::module_path(self).map(MaybeStaticStr::Borrowed)
    }

    #[inline]
    fn file(&self) -> Option<MaybeStaticStr<'_>> {
        tracing_0_1::Metadata::file(self).map(MaybeStaticStr::Borrowed)
    }

    #[inline]
    fn line(&self) -> Option<u32> {
        tracing_0_1::Metadata::line(self)
    }

    #[inline]
    fn column(&self) -> Option<u32> {
        None
    }
}

impl ExternalArgs for tracing_0_1::span::Attributes<'_> {
    #[inline]
    fn visit<Visitor>(&self, visitor: &mut Visitor)
    where
        Visitor: ExternalArgsVisitor,
    {
        self.record(&mut ArgsVisitor(visitor));
    }
}

impl ExternalArgs for tracing_0_1::span::Record<'_> {
    #[inline]
    fn visit<Visitor>(&self, visitor: &mut Visitor)
    where
        Visitor: ExternalArgsVisitor,
    {
        self.record(&mut ArgsVisitor(visitor));
    }
}

impl ExternalArgs for tracing_0_1::Event<'_> {
    #[inline]
    fn visit<Visitor>(&self, visitor: &mut Visitor)
    where
        Visitor: ExternalArgsVisitor,
    {
        self.record(&mut MessageAndArgsVisitor(visitor, false));
    }
}

struct ArgsVisitor<'a, V>(&'a mut V);

impl<V> tracing_0_1::field::Visit for ArgsVisitor<'_, V>
where
    V: ExternalArgsVisitor,
{
    #[inline]
    fn record_bool(&mut self, field: &tracing_0_1::field::Field, value: bool) {
        self.0
            .visit_arg(field_name(field), ExternalArgValue::Bool(value));
    }

    #[inline]
    fn record_i64(&mut self, field: &tracing_0_1::field::Field, value: i64) {
        self.0
            .visit_arg(field_name(field), ExternalArgValue::I64(value));
    }

    #[inline]
    fn record_u64(&mut self, field: &tracing_0_1::field::Field, value: u64) {
        self.0
            .visit_arg(field_name(field), ExternalArgValue::U64(value));
    }

    #[inline]
    fn record_i128(&mut self, field: &tracing_0_1::field::Field, value: i128) {
        self.0
            .visit_arg(field_name(field), ExternalArgValue::I128(value));
    }

    #[inline]
    fn record_u128(&mut self, field: &tracing_0_1::field::Field, value: u128) {
        self.0
            .visit_arg(field_name(field), ExternalArgValue::U128(value));
    }

    #[inline]
    fn record_f64(&mut self, field: &tracing_0_1::field::Field, value: f64) {
        self.0
            .visit_arg(field_name(field), ExternalArgValue::F64(value));
    }

    #[inline]
    fn record_str(&mut self, field: &tracing_0_1::field::Field, value: &str) {
        self.0
            .visit_arg(field_name(field), ExternalArgValue::Str(value));
    }

    #[inline]
    fn record_error(
        &mut self,
        field: &tracing_0_1::field::Field,
        value: &(dyn core::error::Error + 'static),
    ) {
        self.0
            .visit_arg(field_name(field), ExternalArgValue::Error(value));
    }

    #[inline]
    fn record_debug(&mut self, field: &tracing_0_1::field::Field, value: &dyn fmt::Debug) {
        self.0
            .visit_arg(field_name(field), ExternalArgValue::Debug(value));
    }
}

/// Like [`ArgsVisitor`] but visits the first `message` field separately as
/// the message.
struct MessageAndArgsVisitor<'a, V>(&'a mut V, bool);

impl<V> MessageAndArgsVisitor<'_, V> {
    #[inline]
    fn is_first_message(&mut self, key: &str) -> bool {
        if !self.1 && key == "message" {
            self.1 = true;
            true
        } else {
            false
        }
    }
}

impl<V> tracing_0_1::field::Visit for MessageAndArgsVisitor<'_, V>
where
    V: ExternalArgsVisitor,
{
    #[inline]
    fn record_bool(&mut self, field: &tracing_0_1::field::Field, value: bool) {
        if self.is_first_message(field.name()) {
            self.0.visit_message(ExternalArgValue::Bool(value));
        } else {
            self.0
                .visit_arg(field_name(field), ExternalArgValue::Bool(value));
        }
    }

    #[inline]
    fn record_i64(&mut self, field: &tracing_0_1::field::Field, value: i64) {
        if self.is_first_message(field.name()) {
            self.0.visit_message(ExternalArgValue::I64(value));
        } else {
            self.0
                .visit_arg(field_name(field), ExternalArgValue::I64(value));
        }
    }

    #[inline]
    fn record_u64(&mut self, field: &tracing_0_1::field::Field, value: u64) {
        if self.is_first_message(field.name()) {
            self.0.visit_message(ExternalArgValue::U64(value));
        } else {
            self.0
                .visit_arg(field_name(field), ExternalArgValue::U64(value));
        }
    }

    #[inline]
    fn record_i128(&mut self, field: &tracing_0_1::field::Field, value: i128) {
        if self.is_first_message(field.name()) {
            self.0.visit_message(ExternalArgValue::I128(value));
        } else {
            self.0
                .visit_arg(field_name(field), ExternalArgValue::I128(value));
        }
    }

    #[inline]
    fn record_u128(&mut self, field: &tracing_0_1::field::Field, value: u128) {
        if self.is_first_message(field.name()) {
            self.0.visit_message(ExternalArgValue::U128(value));
        } else {
            self.0
                .visit_arg(field_name(field), ExternalArgValue::U128(value));
        }
    }

    #[inline]
    fn record_f64(&mut self, field: &tracing_0_1::field::Field, value: f64) {
        if self.is_first_message(field.name()) {
            self.0.visit_message(ExternalArgValue::F64(value));
        } else {
            self.0
                .visit_arg(field_name(field), ExternalArgValue::F64(value));
        }
    }

    #[inline]
    fn record_str(&mut self, field: &tracing_0_1::field::Field, value: &str) {
        if self.is_first_message(field.name()) {
            self.0.visit_message(ExternalArgValue::Str(value));
        } else {
            self.0
                .visit_arg(field_name(field), ExternalArgValue::Str(value));
        }
    }

    #[inline]
    fn record_error(
        &mut self,
        field: &tracing_0_1::field::Field,
        value: &(dyn core::error::Error + 'static),
    ) {
        if self.is_first_message(field.name()) {
            self.0.visit_message(ExternalArgValue::Error(value));
        } else {
            self.0
                .visit_arg(field_name(field), ExternalArgValue::Error(value));
        }
    }

    #[inline]
    fn record_debug(&mut self, field: &tracing_0_1::field::Field, value: &dyn fmt::Debug) {
        if self.is_first_message(field.name()) {
            self.0.visit_message(ExternalArgValue::Debug(value));
        } else {
            self.0
                .visit_arg(field_name(field), ExternalArgValue::Debug(value));
        }
    }
}

#[inline]
fn field_name(field: &tracing_0_1::field::Field) -> MaybeStaticStr<'_> {
    MaybeStaticStr::Static(field.name())
}
