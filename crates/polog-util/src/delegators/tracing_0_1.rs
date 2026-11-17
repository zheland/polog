//! Delegators from `polog` to `tracing` 0.1.x.

use alloc::boxed::Box;
use alloc::sync::Arc;
use std::sync::OnceLock;

use dashmap::DashMap;
use polog::{
    BaseHandler, BorrowingHandler, ContextArgs, Descriptor, DisplayableMixedArgs, DynHandler,
    Handler, LogArgs, SharedHandler, SpanId, SpanIdGenerator, SpanKind, StaticLogArgs,
};
use tracing_0_1::Span;
#[cfg(feature = "tracing-0-1-semver-unstable")]
use tracing_0_1::Value;
use tracing_0_1::field::ValueSet;
use tracing_core_0_1::callsite::{self, Callsite, DefaultCallsite};
use tracing_core_0_1::metadata::Kind;
use tracing_core_0_1::subscriber::Interest;
use tracing_core_0_1::{Dispatch, Event, Metadata, dispatcher, field, identify_callsite};

use crate::conv::tracing_core_0_1::level_into_foreign_approx;

static SPAN_ID_GENERATOR: SpanIdGenerator = SpanIdGenerator::new();

/// A `polog` [`Handler`] adapter over a [`tracing::Dispatch`].
///
/// Forwards `polog` events and span lifecycle to a `tracing` subscriber:
///
/// - `log` and `log_static` emit a [`tracing::Event`] whose parent `Id` is
///   taken from the cached `tracing` span for the polog `SpanId`, if any.
/// - `enter_span` and `enter_span_static` create a [`tracing::Span`] via
///   `Span::new` or `Span::child_of`, enter it, and cache the mapping from
///   `SpanId` to the created span.
/// - `exit_span` removes the cached span and drops it, firing `on_exit` and
///   `on_close`.
/// - `enter_poll_span` / `pending_poll_span` / `ready_poll_span` pair `on_exit`
///   and `on_enter` around every `Future::poll`.
///
/// `ViaTracing` fires `Subscriber::enter` / `Subscriber::exit` via
/// `Dispatch::enter` / `Dispatch::exit` and does not push onto the calling
/// thread's `tracing` enter stack.
/// `tracing::Span::current()` therefore returns `None` inside a
/// `ViaTracing`-managed span.
/// `polog` events carry their parent `Id` explicitly, so the thread‑local is
/// not needed for parent resolution.
/// Code that requires `Span::current()` must be driven through a native
/// `tracing` span.
///
/// # Static metadata
///
/// [`tracing::Event::new`] requires `&'static Metadata<'static>`, but a
/// `polog` [`Descriptor`] is already `'static`, so `ViaTracing` leaks one
/// `Metadata` per distinct descriptor and caches the pair in an internal
/// `DashMap`.
/// The leak is bounded by the number of distinct `polog` call sites: it does
/// not grow with the number of events emitted.
///
/// [`Descriptor`]: polog::Descriptor
/// [`tracing::Dispatch`]: tracing_0_1::Dispatch
/// [`tracing::Event::new`]: tracing_0_1::Event::new
/// [`tracing::Event`]: tracing_0_1::Event
/// [`tracing::Span`]: tracing_0_1::Span
#[derive(Clone, Debug, Default)]
pub struct ViaTracing {
    dispatch: Dispatch,

    /// Live spans keyed by polog `SpanId`.
    /// Dropping the entry drops the last `Span` clone, which fires `on_close`.
    spans: Arc<DashMap<SpanId, CachedSpan>>,

    /// One leaked callsite + metadata per `&'static Descriptor`.
    leaked: Arc<DashMap<&'static Descriptor, LeakedData>>,
}

impl ViaTracing {
    /// Creates a `ViaTracing` that dispatches to `dispatch`.
    ///
    /// The dispatch is fixed for the lifetime of the adapter: events and
    /// spans created through this `ViaTracing` go to `dispatch` regardless
    /// of any outer `tracing::dispatcher::with_default` scope that may be
    /// active when a `polog` call site runs.
    #[inline]
    #[must_use]
    pub fn new(dispatch: Dispatch) -> Self {
        Self {
            dispatch,
            spans: Arc::default(),
            leaked: Arc::default(),
        }
    }

    #[inline]
    fn static_data_for(&self, descriptor: &'static Descriptor) -> LeakedData {
        // The fast path for a shard read.
        if let Some(data) = self.leaked.get(descriptor) {
            return *data;
        }

        dispatcher::with_default(&self.dispatch, || {
            *self
                .leaked
                .entry(descriptor)
                .or_insert_with(|| LeakedData::build_for(descriptor))
        })
    }

    #[inline]
    fn is_enabled(&self, descriptor: &'static Descriptor) -> bool {
        dispatcher::with_default(&self.dispatch, || {
            let level = level_into_foreign_approx(descriptor.level);
            if level > tracing_0_1::level_filters::STATIC_MAX_LEVEL
                || level > tracing_core_0_1::LevelFilter::current()
            {
                return false;
            }

            let data = self.static_data_for(descriptor);
            let interest = data.callsite.interest();
            !interest.is_never() && (interest.is_always() || self.dispatch.enabled(data.metadata))
        })
    }

    // Dispatches via `self.dispatch` directly.
    // `ViaTracing` is constructed with a fixed `Dispatch`, and events
    // from it must go to that dispatch regardless of any outer
    // `with_default` the caller may have set up.
    #[inline]
    fn log_impl<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        span_id: Option<SpanId>,
    ) where
        Args: DisplayableMixedArgs<'a>,
    {
        let data = self.static_data_for(descriptor);

        // Resolve the parent `Id` *before* taking the values, so the
        // DashMap read guard is not held while we build the ValueSet.
        let parent_id =
            span_id.and_then(|id| self.spans.get(&id).and_then(|s| s.value().span.id()));

        with_values(data.metadata, args, |values| {
            let event = parent_id.map_or_else(
                || Event::new(data.metadata, values),
                |parent| Event::new_child_of(Some(parent), data.metadata, values),
            );
            self.dispatch.event(&event);
        });
    }

    #[inline]
    fn enter_span_impl<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
    ) where
        Args: DisplayableMixedArgs<'a>,
    {
        dispatcher::with_default(&self.dispatch, || {
            let parent_id = parent_span_id
                .and_then(|pid| self.spans.get(&pid).and_then(|s| s.value().span.id()));

            let data = self.static_data_for(descriptor);

            let span = with_values(
                data.metadata,
                args,
                #[expect(clippy::option_if_let_else, reason = "better because of side-effects")]
                |values| match parent_id {
                    // Fires `CREATE`.
                    Some(parent) => Span::child_of(parent, data.metadata, values),
                    // Fires `CREATE`.
                    None => Span::new(data.metadata, values),
                },
            );
            let new_tracing_span_id = span.id();

            let mut span = CachedSpan::new(span);
            span.enter(&self.dispatch); // Fires `ENTER`.

            let lost_span = self.spans.insert(span_id, span);
            if let Some(lost_span) = lost_span {
                exit_lost_span(
                    &self.dispatch,
                    lost_span,
                    span_id,
                    new_tracing_span_id.as_ref(),
                );
            }
        });
    }
}

impl BaseHandler for ViaTracing {
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        SPAN_ID_GENERATOR.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {}

    #[inline]
    fn flush_on_panic(&mut self) {}
}

impl<Context> DynHandler<Context> for ViaTracing
where
    Context: ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        _descriptor: &'static Descriptor,
        span_id: SpanId,
        _kind: SpanKind,
        _context: Context,
    ) {
        let Some((_, mut span)) = self.spans.remove(&span_id) else {
            return;
        };

        dispatcher::with_default(&self.dispatch, || {
            span.exit(&self.dispatch); // Fires `EXIT`.
            drop(span); // Fires `CLOSE`.
        });
    }

    // tracing has no equivalent of per-poll enter/ready/pending signals on
    // `Subscriber`.
    // The idiomatic answer is `tracing::Instrumented` `Future::instrument`,
    // which is a call-site concern, not a handler concern.
    // The delegator deliberately drops these.
    #[inline]
    fn enter_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        span_id: SpanId,
        _context: Context,
    ) {
        let Some(mut span) = self.spans.get_mut(&span_id) else {
            return;
        };

        dispatcher::with_default(&self.dispatch, || {
            // Pair the previous ENTER with an EXIT, then enter again.
            span.exit(&self.dispatch);
            span.enter(&self.dispatch);
        });
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        span_id: SpanId,
        _context: Context,
    ) {
        let Some(mut span) = self.spans.get_mut(&span_id) else {
            return;
        };
        dispatcher::with_default(&self.dispatch, || {
            span.exit(&self.dispatch);
        });
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        span_id: SpanId,
        _context: Context,
    ) {
        let Some(mut span) = self.spans.get_mut(&span_id) else {
            return;
        };
        dispatcher::with_default(&self.dispatch, || {
            span.exit(&self.dispatch);
        });
    }
}

impl<Context> Handler<Context> for ViaTracing
where
    Context: ContextArgs,
{
    type EnrichedContext = Context;

    #[inline]
    fn try_enable(
        &self,
        descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        self.is_enabled(descriptor).then_some(context)
    }

    #[inline]
    fn log<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        _context: Context,
    ) where
        Args: LogArgs<'a>,
    {
        self.log_impl(descriptor, &args, span_id);
    }

    #[inline]
    fn enter_span<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        _kind: SpanKind,
        _context: Context,
    ) where
        Args: LogArgs<'a>,
    {
        self.enter_span_impl(descriptor, &args, parent_span_id, span_id);
    }

    #[inline]
    fn log_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        _context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        self.log_impl(descriptor, &args, span_id);
    }

    #[inline]
    fn enter_span_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        _span_kind: SpanKind,
        _context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        self.enter_span_impl(descriptor, &args, parent_span_id, span_id);
    }
}

impl<Context> BorrowingHandler<Context> for ViaTracing
where
    Context: ContextArgs,
{
    #[inline]
    fn log_borrowed<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        span_id: Option<SpanId>,
        _context: &Context,
    ) where
        Args: StaticLogArgs,
    {
        self.log_impl(descriptor, args, span_id);
    }

    #[inline]
    fn enter_span_borrowed<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        _kind: SpanKind,
        _context: &Context,
    ) where
        Args: StaticLogArgs,
    {
        self.enter_span_impl(descriptor, args, parent_span_id, span_id);
    }
}

impl<Context> SharedHandler<Context> for ViaTracing where Context: ContextArgs {}

#[derive(Copy, Clone, Debug)]
struct LeakedData {
    callsite: &'static DynamicCallsite,
    metadata: &'static Metadata<'static>,
}

impl LeakedData {
    #[inline]
    fn build_for(descriptor: &'static Descriptor) -> Self {
        // Leak the wrapper first so we have a `&'static dyn Callsite` to hand
        // to `identify_callsite!` when building the metadata.
        let callsite = Box::leak(Box::new(DynamicCallsite::new()));

        // Build the metadata, using the stable `identify_callsite!` macro to
        // construct the (doc-hidden) `Identifier`.
        let level = level_into_foreign_approx(descriptor.level);
        let target = descriptor.target.unwrap_or(descriptor.module_path);
        let fields = field::FieldSet::new(descriptor.param_names, identify_callsite!(callsite));
        let kind = match descriptor.kind {
            polog::DescriptorKind::Log => Kind::EVENT,
            polog::DescriptorKind::Span => Kind::SPAN,
        };

        let metadata = Box::leak(Box::new(Metadata::new(
            descriptor.message,
            target,
            level,
            Some(descriptor.location.file()),
            Some(descriptor.location.line()),
            Some(descriptor.module_path),
            fields,
            kind,
        )));

        // Store the metadata inside the callsite and register.
        // `init` forwards `set_interest` into the inner `DefaultCallsite`.
        callsite.init(metadata);

        Self { callsite, metadata }
    }
}

/// Build a `ValueSet` for the given args and invoke `cb` with it.
///
/// The key-values carried by `args` are attached to the resulting event/span.
#[inline]
fn with_values<'a, Args, R>(
    metadata: &'static Metadata<'static>,
    args: &Args,
    cb: impl FnOnce(&ValueSet<'_>) -> R,
) -> R
where
    Args: DisplayableMixedArgs<'a>,
{
    let num_args = metadata.fields().len();
    let arg_len = args.num_args().min(num_args);

    let values: smallvec::SmallVec<[_; 16]> = (0..arg_len)
        .map(|i| args.nth_dyn(i).map(field::display))
        .collect();

    let mut value_refs: smallvec::SmallVec<[Option<&dyn Value>; 16]> = values
        .iter()
        .map(|v| v.as_ref().map(|v| -> &dyn Value { v }))
        .collect();
    value_refs.resize(num_args, None);

    // # UNRELIABLE API USAGE (feature-gated).
    //
    // `FieldSet::value_set_all` is `#[doc(hidden)]`. It is intended for
    // expansion of tracing's own `valueset!` / `event!` / `span!` macros and
    // is **not** part of the semver-stable surface of `tracing-core`; its
    // signature, layout, and even its presence may change between any two
    // versions of the crate.
    //
    // It is nonetheless the only way to build a non-empty `ValueSet` outside
    // of tracing's macros: `Event::dispatch`, `Event::child_of`, `Span::new`,
    // and `Span::child_of` all take a `&ValueSet<'_>`, `ValueSet::empty()` is
    // the only public constructor, and `valueset!` (the public-ish macro that
    // would otherwise build one) expands to this very function.
    // There is no stable path from "a heterogeneous tuple of `Display` values"
    // to a populated `ValueSet`.
    //
    // For details see the crate-level feature docs.
    let values = metadata.fields().value_set_all(&value_refs);
    cb(&values)
}

#[derive(Debug)]
struct DynamicCallsite(OnceLock<DefaultCallsite>);

impl DynamicCallsite {
    #[inline]
    const fn new() -> Self {
        Self(OnceLock::new())
    }

    /// Must be called exactly once, inside a `with_default(&dispatch, ...)`
    /// scope so that `callsite::register` sees the intended subscriber.
    #[inline]
    fn init(&'static self, meta: &'static Metadata<'static>) {
        self.0
            .set(DefaultCallsite::new(meta))
            .expect("callsite has already been initialized");

        callsite::register(self);
    }

    #[inline]
    fn interest(&'static self) -> Interest {
        self.0.get().expect("callsite not initialized").interest()
    }
}

impl Callsite for DynamicCallsite {
    #[inline]
    fn set_interest(&self, interest: Interest) {
        self.0
            .get()
            .expect("callsite not initialized")
            .set_interest(interest);
    }

    #[inline]
    fn metadata(&self) -> &Metadata<'_> {
        self.0.get().expect("callsite not initialized").metadata()
    }
}

/// A `tracing::Span` together with the guard that keeps it entered.
///
/// `CachedSpan` exists so that a `polog` `SpanId` can be mapped to a live
/// `tracing` span and re-entered on subsequent `enter_poll_span` calls
/// without creating a fresh `tracing` span each time.
#[derive(Debug)]
struct CachedSpan {
    span: Span,
    entered: bool,
}

impl CachedSpan {
    #[inline]
    const fn new(span: Span) -> Self {
        Self {
            span,
            entered: false,
        }
    }

    /// Fire `on_enter` if the span is not currently entered.
    #[inline]
    fn enter(&mut self, dispatch: &Dispatch) {
        if !self.entered {
            if let Some(id) = self.span.id() {
                dispatch.enter(&id);
            }
            self.entered = true;
        }
    }

    /// Fire `on_exit` if the span is currently entered.
    #[inline]
    fn exit(&mut self, dispatch: &Dispatch) {
        if self.entered {
            if let Some(id) = self.span.id() {
                dispatch.exit(&id);
            }
            self.entered = false;
        }
    }
}

/// Cleans up a `CachedSpan` whose polog `SpanId` has been reused by a newer
/// span before the old one was exited.
///
/// Fires `on_exit` if the old span was still entered, emits a diagnostic
/// `WARN` event mentioning the polog `SpanId`, the *old* tracing `Id`, and
/// the *new* tracing `Id` so subscribers can correlate, then drops the span
/// to fire `on_close`.
///
/// # Ordering
///
/// Must be called **after** the new span has been created and entered, so
/// that `new_tracing_id` is already available for the diagnostic.
/// Must be called **inside** `dispatcher::with_default(&self.dispatch, ...)`
/// so the diagnostic event is routed to this delegator's subscriber.
///
/// # When this fires
///
/// `polog` guarantees that a `SpanId` is not reused until the handler's
/// counter wraps.
/// A wrap during the same process is effectively impossible, so this path
/// exists only to defend against a backend that hands out IDs from a
/// generator that has been reset or shared improperly.
#[inline]
fn exit_lost_span(
    dispatch: &Dispatch,
    mut lost_span: CachedSpan,
    polog_span_id: SpanId,
    new_tracing_span_id: Option<&tracing_core_0_1::span::Id>,
) {
    // Grab the old tracing `Id` before `exit`/drop consume the span.
    let old_tracing_span_id = lost_span.span.id();

    // Fires `EXIT` only if the span is currently entered.
    lost_span.exit(dispatch);

    // Diagnostic. Fields must precede the message per `tracing::event!`.
    tracing_0_1::event!(
        target: module_path!(),
        tracing_0_1::Level::WARN,
        polog_span_id       = ? polog_span_id,
        old_tracing_span_id = ? old_tracing_span_id,
        new_tracing_span_id = ? new_tracing_span_id,
        "polog SpanId was reused while its previous span was still alive; \
         the old tracing span has been force-closed",
    );

    // Drop fires `CLOSE`.
    drop(lost_span);
}

#[cfg(feature = "fanout")]
#[expect(clippy::arithmetic_side_effects, reason = "okay in tests")]
#[cfg(test)]
mod tests {
    use alloc::borrow::ToOwned;
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec::Vec;
    use alloc::{format, vec};
    use core::sync::atomic;
    use core::sync::atomic::AtomicU64;
    use core::{fmt, mem};

    use parking_lot::Mutex;
    use polog::{Logger, LoggerExt, PollInstrument};
    use tracing_core_0_1 as tracing_core;

    use crate::Enricher;
    use crate::delegators::tracing_0_1::{SPAN_ID_GENERATOR, ViaTracing};

    #[derive(Clone, Debug)]
    pub struct Tracker {
        events: Arc<Mutex<Vec<Event>>>,
        next_span_id: Arc<AtomicU64>,
    }

    #[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
    struct FieldVisitor(Vec<(&'static str, String)>);

    #[expect(clippy::enum_variant_names, reason = "okay in tests")]
    #[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
    pub enum Event {
        RegisterCallsite(&'static str),
        Enabled(&'static str),
        NewSpan(u64, &'static str),
        EnterSpan(u64),
        ExitSpan(u64),
        CloseSpan(u64),
        RecordSpan(u64),
        RecordFollowsFrom(u64, u64),
        Event {
            parent: Option<u64>,
            name: &'static str,
            fields: Vec<(&'static str, String)>,
        },
    }

    impl Tracker {
        #[inline]
        #[must_use]
        pub fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
                // Span IDs must be > 0.
                next_span_id: Arc::new(AtomicU64::new(1)),
            }
        }

        #[inline]
        #[must_use]
        pub fn take_events(&self) -> Vec<Event> {
            mem::take(&mut *self.events.lock())
        }
    }

    impl tracing_core::field::Visit for FieldVisitor {
        fn record_debug(&mut self, field: &tracing_core::field::Field, value: &dyn fmt::Debug) {
            self.0.push((field.name(), format!("{value:?}")));
        }
    }

    impl tracing_core::Subscriber for Tracker {
        fn register_callsite(
            &self,
            metadata: &'static tracing_core::Metadata<'static>,
        ) -> tracing_core::Interest {
            self.events
                .lock()
                .push(Event::RegisterCallsite(metadata.name()));
            tracing_core::Interest::sometimes()
        }

        fn enabled(&self, metadata: &tracing_core::Metadata<'_>) -> bool {
            self.events.lock().push(Event::Enabled(metadata.name()));
            true
        }

        fn new_span(&self, span: &tracing_core::span::Attributes<'_>) -> tracing_core::span::Id {
            let id = self.next_span_id.fetch_add(1, atomic::Ordering::Relaxed);
            self.events
                .lock()
                .push(Event::NewSpan(id, span.metadata().name()));
            tracing_core::span::Id::from_u64(id)
        }

        fn record(&self, span: &tracing_core::span::Id, _values: &tracing_core::span::Record<'_>) {
            self.events.lock().push(Event::RecordSpan(span.into_u64()));
        }

        fn record_follows_from(
            &self,
            span: &tracing_core::span::Id,
            follows: &tracing_core::span::Id,
        ) {
            self.events.lock().push(Event::RecordFollowsFrom(
                span.into_u64(),
                follows.into_u64(),
            ));
        }

        fn event(&self, event: &tracing_core::Event<'_>) {
            let mut visitor = FieldVisitor::default();
            let parent = event.parent().map(tracing_0_1::span::Id::into_u64);
            let name = event.metadata().name();
            event.record(&mut visitor);
            self.events.lock().push(Event::Event {
                parent,
                name,
                fields: visitor.0,
            });
        }

        fn enter(&self, span: &tracing_core::span::Id) {
            self.events.lock().push(Event::EnterSpan(span.into_u64()));
        }

        fn exit(&self, span: &tracing_core::span::Id) {
            self.events.lock().push(Event::ExitSpan(span.into_u64()));
        }

        fn try_close(&self, span: tracing_core::span::Id) -> bool {
            self.events.lock().push(Event::CloseSpan(span.into_u64()));
            true
        }
    }

    #[tokio::test]
    async fn test_delegator_tracing() {
        // Because of the global subscriber all tracing tests are performed
        // sequentially in a single test.

        let tracker = Tracker::new();
        let enricher = Arc::new(Enricher::new(tracker.clone(), || 123));

        test_log_basic(tracker.clone(), &tracker, 1);
        test_log_basic(Arc::clone(&enricher), &tracker, 3);
        test_log_static(&tracker, &tracker, 5);
        test_log_static(&enricher, &tracker, 7);
        test_log_borrowed(&tracker, &tracker, 9);
        test_log_borrowed(&enricher, &tracker, 11);
        test_log_async(tracker.clone(), &tracker, 13).await;
        test_log_async(Arc::clone(&enricher), &tracker, 14).await;
        test_log_reuse_span_id(tracker.clone(), &tracker, 15, 11);
        test_log_reuse_span_id(Arc::clone(&enricher), &tracker, 17, 12);
        test_record(tracker.clone(), &tracker, 19);
        test_record(Arc::clone(&enricher), &tracker, 20);
        test_record_follows_from(tracker.clone(), &tracker, 21);
        test_record_follows_from(Arc::clone(&enricher), &tracker, 23);
    }

    fn test_log_basic<S>(dispatch: S, tracker: &Tracker, span_id_offset: u64)
    where
        S: 'static + Send + Sync + Clone + tracing_0_1::Subscriber,
    {
        let mut logger = ViaTracing::new(tracing_core::Dispatch::new(dispatch));
        drop(tracker.take_events()); // Drop multiple `RegisterCallsite` messages.

        // Test: `Handler::log` with no args.
        polog::info!(&mut logger, "Message");
        assert_eq!(
            tracker.take_events(),
            [
                Event::RegisterCallsite("Message"),
                Event::Enabled("Message"),
                Event::Event {
                    parent: None,
                    name: "Message",
                    fields: vec![],
                },
            ]
        );

        // Test: `Handler::log` with arguments.
        polog::info!(&mut logger, "With args"; foo = 1, bar = 2);
        assert_eq!(
            tracker.take_events(),
            [
                Event::RegisterCallsite("With args"),
                Event::Enabled("With args"),
                Event::Event {
                    parent: None,
                    name: "With args",
                    fields: vec![("foo", "1".to_owned()), ("bar", "2".to_owned())],
                },
            ]
        );

        // Test: `Handler::enter_span`.
        let mut span1 = polog::info_span!(&mut logger, "Span");
        assert_eq!(
            tracker.take_events(),
            [
                Event::RegisterCallsite("Span"),
                Event::Enabled("Span"),
                Event::NewSpan(span_id_offset, "Span"),
                Event::EnterSpan(span_id_offset),
            ]
        );

        // Test: `Handler::log` inside a span.
        polog::info!(&mut span1, "Inside");
        assert_eq!(
            tracker.take_events(),
            [
                Event::RegisterCallsite("Inside"),
                Event::Enabled("Inside"),
                Event::Event {
                    parent: Some(span_id_offset),
                    name: "Inside",
                    fields: vec![],
                },
            ]
        );

        // Test: `Handler::enter_span` inside a span.
        let span2 = polog::info_span!(&mut span1, "Span");
        assert_eq!(
            tracker.take_events(),
            [
                Event::RegisterCallsite("Span"),
                Event::Enabled("Span"),
                Event::NewSpan(span_id_offset + 1, "Span"),
                Event::EnterSpan(span_id_offset + 1),
            ]
        );

        // Test: `DynHandler::exit_span`.
        drop(span2);
        drop(span1);
        assert_eq!(
            tracker.take_events(),
            [
                Event::ExitSpan(span_id_offset + 1),
                Event::CloseSpan(span_id_offset + 1),
                Event::ExitSpan(span_id_offset),
                Event::CloseSpan(span_id_offset)
            ]
        );

        // Test: `BaseHandler::flush_blocking` (no-op).
        logger.flush_blocking();
        assert_eq!(tracker.take_events(), []);

        // Test: `BaseHandler::flush_on_panic` (no-op).
        logger.flush_on_panic();
        assert_eq!(tracker.take_events(), []);
    }

    #[cfg(feature = "fanout")]
    fn test_log_static<S>(dispatch: &S, tracker: &Tracker, span_id_offset: u64)
    where
        S: 'static + Send + Sync + Clone + tracing_0_1::Subscriber,
    {
        use crate::Fanout2;

        let logger = || ViaTracing::new(tracing_core::Dispatch::new(dispatch.clone()));
        let mut logger = Fanout2::new((logger(), logger()));
        drop(tracker.take_events()); // Drop multiple `RegisterCallsite` messages.

        // Test: `Handler::log_static`.
        polog::info!(&mut logger, "Message");
        assert_eq!(
            tracker.take_events(),
            [
                Event::RegisterCallsite("Message"),
                Event::RegisterCallsite("Message"),
                Event::Enabled("Message"),
                Event::RegisterCallsite("Message"),
                Event::RegisterCallsite("Message"),
                Event::Enabled("Message"),
                Event::Event {
                    parent: None,
                    name: "Message",
                    fields: vec![]
                },
                Event::Event {
                    parent: None,
                    name: "Message",
                    fields: vec![]
                },
            ]
        );

        // Test: `BaseHandler::generate_span_id` and
        // `Handler::enter_span_static`.
        let span = polog::info_span!(&mut logger, "Span");
        assert_eq!(
            tracker.take_events(),
            [
                Event::RegisterCallsite("Span"),
                Event::RegisterCallsite("Span"),
                Event::Enabled("Span"),
                Event::RegisterCallsite("Span"),
                Event::RegisterCallsite("Span"),
                Event::Enabled("Span"),
                Event::NewSpan(span_id_offset, "Span"),
                Event::EnterSpan(span_id_offset),
                Event::NewSpan(span_id_offset + 1, "Span"),
                Event::EnterSpan(span_id_offset + 1),
            ]
        );

        // Test: `DynHandler::exit_span`.
        drop(span);
        assert_eq!(
            tracker.take_events(),
            [
                Event::ExitSpan(span_id_offset),
                Event::CloseSpan(span_id_offset),
                Event::ExitSpan(span_id_offset + 1),
                Event::CloseSpan(span_id_offset + 1)
            ]
        );
    }

    #[cfg(not(feature = "fanout"))]
    fn test_log_static<S>(_dispatch: S, _tracker: &Tracker, _span_id_offset: u64)
    where
        S: 'static + Send + Sync + Clone + tracing_0_1::Subscriber,
    {
    }

    #[cfg(feature = "fanout")]
    fn test_log_borrowed<S>(dispatch: &S, tracker: &Tracker, span_id_offset: u64)
    where
        S: 'static + Send + Sync + Clone + tracing_0_1::Subscriber,
    {
        use crate::BorrowingFanout2;

        let logger = || ViaTracing::new(tracing_core::Dispatch::new(dispatch.clone()));
        let mut logger = BorrowingFanout2::new((logger(), logger()));
        drop(tracker.take_events()); // Drop multiple `RegisterCallsite` messages.

        // Test: `Handler::log_borrowed`.
        polog::info!(&mut logger, "Message");
        assert_eq!(
            tracker.take_events(),
            [
                Event::RegisterCallsite("Message"),
                Event::RegisterCallsite("Message"),
                Event::Enabled("Message"),
                Event::RegisterCallsite("Message"),
                Event::RegisterCallsite("Message"),
                Event::Enabled("Message"),
                Event::Event {
                    parent: None,
                    name: "Message",
                    fields: vec![]
                },
                Event::Event {
                    parent: None,
                    name: "Message",
                    fields: vec![]
                },
            ]
        );

        // Test: `BaseHandler::generate_span_id` and
        // `Handler::enter_span_borrowed`.
        let span = polog::info_span!(&mut logger, "Span");
        assert_eq!(
            tracker.take_events(),
            [
                Event::RegisterCallsite("Span"),
                Event::RegisterCallsite("Span"),
                Event::Enabled("Span"),
                Event::RegisterCallsite("Span"),
                Event::RegisterCallsite("Span"),
                Event::Enabled("Span"),
                Event::NewSpan(span_id_offset, "Span"),
                Event::EnterSpan(span_id_offset),
                Event::NewSpan(span_id_offset + 1, "Span"),
                Event::EnterSpan(span_id_offset + 1),
            ]
        );

        // Test: `DynHandler::exit_span`.
        drop(span);
        assert_eq!(
            tracker.take_events(),
            [
                Event::ExitSpan(span_id_offset),
                Event::CloseSpan(span_id_offset),
                Event::ExitSpan(span_id_offset + 1),
                Event::CloseSpan(span_id_offset + 1)
            ]
        );
    }

    #[cfg(not(feature = "fanout"))]
    fn test_log_borrowed<S>(_dispatch: S, _tracker: &Tracker, _span_id_offset: u64)
    where
        S: 'static + Send + Sync + Clone + tracing_0_1::Subscriber,
    {
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    async fn test_log_async<S>(dispatch: S, tracker: &Tracker, span_id_offset: u64)
    where
        S: 'static + Send + Sync + Clone + tracing_0_1::Subscriber,
    {
        let mut logger = ViaTracing::new(tracing_core::Dispatch::new(dispatch));
        drop(tracker.take_events()); // Drop multiple `RegisterCallsite` messages.

        let deferred = polog::deferred_info_span!(&mut logger, "Polled");
        let task = tokio::task::spawn(example_async_fn.poll_instrument(deferred));
        task.await.unwrap();

        assert_eq!(
            tracker.take_events(),
            [
                // Construction of `PollInstrumented`.
                Event::RegisterCallsite("Polled"),
                Event::Enabled("Polled"),
                Event::NewSpan(span_id_offset, "Polled"),
                Event::EnterSpan(span_id_offset),
                // First poll: exit then re-enter around the poll body.
                Event::ExitSpan(span_id_offset),
                Event::EnterSpan(span_id_offset),
                Event::RegisterCallsite("Some warn!"),
                Event::Enabled("Some warn!"),
                Event::Event {
                    parent: Some(span_id_offset),
                    name: "Some warn!",
                    fields: vec![("3 * 4", "12".to_owned())]
                },
                Event::ExitSpan(span_id_offset),
                // Second poll: re-enter, body returns Ready, then exit.
                Event::EnterSpan(span_id_offset),
                Event::RegisterCallsite("Another warn!"),
                Event::Enabled("Another warn!"),
                Event::Event {
                    parent: Some(span_id_offset),
                    name: "Another warn!",
                    fields: vec![("4 * 5", "20".to_owned())]
                },
                Event::ExitSpan(span_id_offset),
                // Drop of `PollInstrumented` closes the span.
                Event::CloseSpan(span_id_offset),
            ]
        );
    }

    async fn example_async_fn<L>(mut logger: L)
    where
        L: Logger,
    {
        polog::info!(&mut logger, "Some warn!"; 3 * 4);
        tokio::task::yield_now().await;
        polog::info!(&mut logger, "Another warn!"; 4 * 5);
    }

    fn test_log_reuse_span_id<S>(dispatch: S, tracker: &Tracker, span_id_offset: u64, polog_id: u64)
    where
        S: 'static + Send + Sync + Clone + tracing_0_1::Subscriber,
    {
        let mut logger = ViaTracing::new(tracing_core::Dispatch::new(dispatch));
        drop(tracker.take_events()); // Drop multiple `RegisterCallsite` messages.

        let next_span_id = SPAN_ID_GENERATOR.as_inner().load(atomic::Ordering::Relaxed);

        let mut span = polog::info_span!(&mut logger, "Span");
        assert_eq!(
            tracker.take_events(),
            [
                Event::RegisterCallsite("Span"),
                Event::Enabled("Span"),
                Event::NewSpan(span_id_offset, "Span"),
                Event::EnterSpan(span_id_offset),
            ]
        );
        span.forget();
        drop(span);
        assert_eq!(tracker.take_events(), []);

        SPAN_ID_GENERATOR
            .as_inner()
            .store(next_span_id, atomic::Ordering::Relaxed);

        let span = polog::info_span!(&mut logger, "Span");
        assert_eq!(
            tracker
                .take_events()
                .into_iter()
                .filter(|event| !matches!(event, Event::RegisterCallsite(_)))
                .collect::<Vec<_>>(),
            [
                Event::Enabled("Span"),
                Event::NewSpan(span_id_offset + 1, "Span"),
                Event::EnterSpan(span_id_offset + 1),
                Event::ExitSpan(span_id_offset),
                Event::Enabled("event crates/polog-util/src/delegators/tracing_0_1.rs:601"),
                Event::Event {
                    parent: None,
                    name: "event crates/polog-util/src/delegators/tracing_0_1.rs:601",
                    fields: vec![
                        (
                            "message",
                            "polog SpanId was reused while its previous span was still alive; the \
                             old tracing span has been force-closed"
                                .to_owned()
                        ),
                        ("polog_span_id", format!("SpanId({polog_id})")),
                        ("old_tracing_span_id", format!("Some(Id({span_id_offset}))")),
                        (
                            "new_tracing_span_id",
                            format!("Some(Id({}))", span_id_offset + 1)
                        )
                    ]
                },
                Event::CloseSpan(span_id_offset)
            ]
        );

        // Test: `DynHandler::exit_span`.
        drop(span);
        assert_eq!(
            tracker.take_events(),
            [
                Event::ExitSpan(span_id_offset + 1),
                Event::CloseSpan(span_id_offset + 1)
            ]
        );
    }

    fn test_record<S>(dispatch: S, tracker: &Tracker, span_id_offset: u64)
    where
        S: 'static + Send + Sync + Clone + tracing_0_1::Subscriber,
    {
        let dispatch = tracing_core::Dispatch::new(dispatch);
        drop(tracker.take_events());

        tracing_0_1::dispatcher::with_default(&dispatch, || {
            let span = tracing_0_1::info_span!("RecordSpan", foo = tracing_0_1::field::Empty);
            let &_ = span.record("foo", "recorded_value");
        });

        assert_eq!(
            tracker
                .take_events()
                .into_iter()
                .filter(|event| !matches!(event, Event::RegisterCallsite(_)))
                .collect::<Vec<_>>(),
            [
                Event::Enabled("RecordSpan"),
                Event::NewSpan(span_id_offset, "RecordSpan"),
                Event::RecordSpan(span_id_offset),
                Event::CloseSpan(span_id_offset)
            ]
        );
    }

    fn test_record_follows_from<S>(dispatch: S, tracker: &Tracker, span_id_offset: u64)
    where
        S: 'static + Send + Sync + Clone + tracing_0_1::Subscriber,
    {
        let dispatch = tracing_core::Dispatch::new(dispatch);
        drop(tracker.take_events());

        tracing_0_1::dispatcher::with_default(&dispatch, || {
            let span1 = tracing_0_1::info_span!("FollowsFrom1");
            let span2 = tracing_0_1::info_span!("FollowsFrom2");
            let &_ = span1.follows_from(&span2);
        });

        assert_eq!(
            tracker
                .take_events()
                .into_iter()
                .filter(|event| !matches!(event, Event::RegisterCallsite(_)))
                .collect::<Vec<_>>(),
            [
                Event::Enabled("FollowsFrom1"),
                Event::NewSpan(span_id_offset, "FollowsFrom1"),
                Event::Enabled("FollowsFrom2"),
                Event::NewSpan(span_id_offset + 1, "FollowsFrom2"),
                Event::RecordFollowsFrom(span_id_offset, span_id_offset + 1),
                Event::CloseSpan(span_id_offset + 1),
                Event::CloseSpan(span_id_offset)
            ]
        );
    }
}
