use core::fmt::Debug;

use polog::{
    BaseHandler, ContextArgs, Descriptor, DynHandler, Handler, LevelFilter, LogArgs, SharedHandler,
    SpanId, SpanKind, StaticLogArgs,
};

use crate::{BaseExternalHandler, ExternalArgs, ExternalDescriptor, ExternalHandler};

/// A [`Handler`] that appends a value to the context of every event and span.
///
/// `Enricher<H, F>` wraps a downstream [`Handler`] and, before delegating,
/// calls [`EnricherFn::call`] and appends the produced value to the context
/// tuple via [`DebuggableMixedArgsBuilder::with_debuggable_arg`].
///
/// # Placement
///
/// The factory runs inside [`Handler::try_enable`], so the produced value
/// reflects the calling thread's view of the environment:
///
/// - Placed before an [`OffloaderHandle`], the value is captured on the
///   caller's thread (`SystemTime::now`, `Instant::now`, `thread::current`).
/// - Placed after an `OffloaderHandle`, the value is captured on the worker
///   thread.
///
/// # Cost
///
/// [`EnricherFn::call`] runs once per event and once per lifecycle event of
/// an accepted span. It should be cheap: prefer a factory that copies a
/// cached value or reads a thread‑local over one that allocates.
///
/// # Subscriber and `log` compatibility
///
/// When the `log-0-4` feature is enabled, `Enricher` also implements
/// [`log::Log`] by delegating to the wrapped handler.
/// When `tracing-core-0-1` is enabled, it also implements
/// [`tracing_core::Subscriber`].
/// Both implementations are transparent: the enrichment still runs on every
/// event that flows through.
///
/// [`log::Log`]: https://docs.rs/log/latest/log/trait.Log.html
/// [`tracing_core::Subscriber`]: https://docs.rs/tracing-core/latest/tracing_core/subscriber/trait.Subscriber.html
/// [`DebuggableMixedArgsBuilder::with_debuggable_arg`]: polog::DebuggableMixedArgsBuilder::with_debuggable_arg
/// [`OffloaderHandle`]: crate::OffloaderHandle
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct Enricher<H, F> {
    /// The wrapped handler.
    pub handler: H,

    /// Factory for the value appended to the context of every accepted event
    /// and span.
    ///
    /// Called once per `try_enable` (and once per lifecycle event that
    /// follows an accepted span), so it must be cheap.
    /// Returning a value whose type is already in the context is allowed but
    /// produces a duplicate entry.
    /// Prefer a distinct type per enricher so backends can tell them apart.
    pub arg_fn: F,
}

/// Factory for a single value to append to the context.
///
/// Implemented for every `Fn() -> T`.
/// `Enricher` calls [`EnricherFn::call`] once per accepted event or span,
/// before delegating to the wrapped handler.
pub trait EnricherFn {
    /// The type of the produced value. Must be `'static + Debug` because the
    /// value is appended to the context tuple.
    type Output;

    /// Produces the value to append to the context.
    fn call(&self) -> Self::Output;
}

impl<H, F> Enricher<H, F> {
    /// Creates a new filtering middleware.
    #[inline]
    #[must_use]
    pub const fn new(handler: H, arg_fn: F) -> Self {
        Self { handler, arg_fn }
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
}

impl<F, T> EnricherFn for F
where
    F: Fn() -> T,
{
    type Output = T;

    #[inline]
    fn call(&self) -> Self::Output {
        (self)()
    }
}

impl<H, F> BaseHandler for Enricher<H, F>
where
    H: BaseHandler,
{
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        H::generate_span_id(&self.handler)
    }

    #[inline]
    fn flush_blocking(&mut self) {
        H::flush_blocking(&mut self.handler);
    }

    #[inline]
    fn flush_on_panic(&mut self) {
        H::flush_on_panic(&mut self.handler);
    }

    #[inline]
    fn should_panic_on_bug(
        &self,
        descriptor: &'static Descriptor,
        span_id: Option<SpanId>,
    ) -> bool {
        H::should_panic_on_bug(&self.handler, descriptor, span_id)
    }

    #[inline]
    fn panic(&mut self, descriptor: &'static Descriptor, span_id: Option<SpanId>) -> ! {
        H::panic(&mut self.handler, descriptor, span_id);
    }
}

impl<H, F, Context> DynHandler<Context> for Enricher<H, F>
where
    H: DynHandler<Context::WithDebuggableArg<F::Output>>,
    F: EnricherFn,
    F::Output: 'static + Debug,
    Context: ContextArgs,
    Context::WithDebuggableArg<F::Output>: ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    ) {
        let context = context.with_debuggable_arg(self.arg_fn.call());
        H::exit_span(&mut self.handler, descriptor, span_id, span_kind, context);
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        let context = context.with_debuggable_arg(self.arg_fn.call());
        H::enter_poll_span(&mut self.handler, descriptor, span_id, context);
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        let context = context.with_debuggable_arg(self.arg_fn.call());
        H::pending_poll_span(&mut self.handler, descriptor, span_id, context);
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        let context = context.with_debuggable_arg(self.arg_fn.call());
        H::ready_poll_span(&mut self.handler, descriptor, span_id, context);
    }
}

impl<H, F, Context> Handler<Context> for Enricher<H, F>
where
    H: Handler<Context::WithDebuggableArg<F::Output>>,
    F: EnricherFn,
    F::Output: 'static + Debug,
    Context: ContextArgs,
    Context::WithDebuggableArg<F::Output>: ContextArgs,
{
    type EnrichedContext = H::EnrichedContext;

    #[inline]
    fn try_enable(
        &self,
        descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        let context = context.with_debuggable_arg(self.arg_fn.call());
        H::try_enable(&self.handler, descriptor, context)
    }

    #[inline]
    fn log<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        context: Self::EnrichedContext,
    ) where
        Args: LogArgs<'a>,
    {
        H::log(&mut self.handler, descriptor, args, span_id, context);
    }

    #[inline]
    fn enter_span<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Self::EnrichedContext,
    ) where
        Args: LogArgs<'a>,
    {
        H::enter_span(
            &mut self.handler,
            descriptor,
            args,
            parent_span_id,
            span_id,
            span_kind,
            context,
        );
    }

    #[inline]
    fn log_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        H::log_static(&mut self.handler, descriptor, args, span_id, context);
    }

    #[inline]
    fn enter_span_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        H::enter_span_static(
            &mut self.handler,
            descriptor,
            args,
            parent_span_id,
            span_id,
            span_kind,
            context,
        );
    }
}

impl<H, F, Context> SharedHandler<Context> for Enricher<H, F>
where
    H: SharedHandler<Context::WithDebuggableArg<F::Output>>,
    F: 'static + Send + Sync + Clone + EnricherFn,
    F::Output: 'static + Debug,
    Context: ContextArgs,
    Context::WithDebuggableArg<F::Output>: ContextArgs,
{
}

impl<H, F> BaseExternalHandler for Enricher<H, F>
where
    H: BaseExternalHandler,
{
    #[inline]
    fn max_level_external(&self) -> LevelFilter {
        H::max_level_external(&self.handler)
    }

    #[inline]
    fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor,
    {
        H::is_enabled_external(&self.handler, descriptor)
    }
}

impl<H, F, Context> ExternalHandler<Context> for Enricher<H, F>
where
    H: ExternalHandler<Context::WithDebuggableArg<F::Output>>,
    F: EnricherFn,
    F::Output: 'static + Debug,
    Context: ContextArgs,
    Context::WithDebuggableArg<F::Output>: ContextArgs,
{
    #[inline]
    fn log_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        span_id: Option<SpanId>,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let context = context.with_debuggable_arg(self.arg_fn.call());
        H::log_external(&mut self.handler, descriptor, args, span_id, context);
    }

    #[inline]
    fn new_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let context = context.with_debuggable_arg(self.arg_fn.call());
        H::new_span_external(
            &mut self.handler,
            descriptor,
            args,
            parent_span_id,
            span_id,
            context,
        );
    }

    #[inline]
    fn enter_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let context = context.with_debuggable_arg(self.arg_fn.call());
        H::enter_span_external(
            &mut self.handler,
            descriptor,
            args,
            parent_span_id,
            span_id,
            context,
        );
    }

    #[inline]
    fn exit_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        let context = context.with_debuggable_arg(self.arg_fn.call());
        H::exit_span_external(&mut self.handler, descriptor, span_id, context);
    }

    #[inline]
    fn close_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        let context = context.with_debuggable_arg(self.arg_fn.call());
        H::close_span_external(&mut self.handler, descriptor, span_id, context);
    }

    #[inline]
    fn record_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let context = context.with_debuggable_arg(self.arg_fn.call());
        H::record_span_external(&mut self.handler, descriptor, args, span_id, context);
    }

    #[expect(clippy::similar_names, reason = "better than alternatives")]
    #[inline]
    fn link_span_external<Desc>(
        &mut self,
        descriptor: &Desc,
        followed_span_id: SpanId,
        follower_span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
    {
        let context = context.with_debuggable_arg(self.arg_fn.call());
        H::link_span_external(
            &mut self.handler,
            descriptor,
            followed_span_id,
            follower_span_id,
            context,
        );
    }
}

#[cfg(feature = "log-0-4")]
mod log_0_4_impls {
    use log_0_4::{Log, Metadata, Record};

    use crate::Enricher;

    impl<H, F> Log for Enricher<H, F>
    where
        H: Log,
        F: Send + Sync,
    {
        #[inline]
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            H::enabled(&self.handler, metadata)
        }

        #[inline]
        fn log(&self, record: &Record<'_>) {
            H::log(&self.handler, record);
        }

        #[inline]
        fn flush(&self) {
            H::flush(&self.handler);
        }
    }
}

#[cfg(feature = "tracing-core-0-1")]
mod tracing_core_0_1_impls {
    use tracing_core_0_1::{Dispatch, Event, Interest, LevelFilter, Metadata, Subscriber, span};

    use crate::Enricher;

    impl<H, F> Subscriber for Enricher<H, F>
    where
        H: Subscriber,
        F: 'static,
    {
        #[inline]
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            H::enabled(&self.handler, metadata)
        }

        #[inline]
        fn new_span(&self, span: &span::Attributes<'_>) -> span::Id {
            H::new_span(&self.handler, span)
        }

        #[inline]
        fn record(&self, span: &span::Id, values: &span::Record<'_>) {
            H::record(&self.handler, span, values);
        }

        #[inline]
        fn record_follows_from(&self, span: &span::Id, follows: &span::Id) {
            H::record_follows_from(&self.handler, span, follows);
        }

        #[inline]
        fn event(&self, event: &Event<'_>) {
            H::event(&self.handler, event);
        }

        #[inline]
        fn enter(&self, span: &span::Id) {
            H::enter(&self.handler, span);
        }

        #[inline]
        fn exit(&self, span: &span::Id) {
            H::exit(&self.handler, span);
        }

        #[inline]
        fn on_register_dispatch(&self, subscriber: &Dispatch) {
            H::on_register_dispatch(&self.handler, subscriber);
        }

        #[inline]
        fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
            H::register_callsite(&self.handler, metadata)
        }

        #[inline]
        fn max_level_hint(&self) -> Option<LevelFilter> {
            H::max_level_hint(&self.handler)
        }

        #[inline]
        fn event_enabled(&self, event: &Event<'_>) -> bool {
            H::event_enabled(&self.handler, event)
        }

        #[inline]
        fn clone_span(&self, id: &span::Id) -> span::Id {
            H::clone_span(&self.handler, id)
        }

        #[expect(deprecated, reason = "all methods must be delegated")]
        #[inline]
        fn drop_span(&self, id: span::Id) {
            H::drop_span(&self.handler, id);
        }

        #[inline]
        fn try_close(&self, id: span::Id) -> bool {
            H::try_close(&self.handler, id)
        }

        #[inline]
        fn current_span(&self) -> span::Current {
            H::current_span(&self.handler)
        }

        #[inline]
        unsafe fn downcast_raw(&self, id: core::any::TypeId) -> Option<*const ()> {
            // SAFETY: Guaranteed by the caller.
            unsafe { H::downcast_raw(&self.handler, id) }
        }
    }
}

#[cfg(test)]
mod tests {
    use polog::{BaseHandler, Descriptor, Level, log_descriptor_ref};
    use polog_test_helpers::panic::catch_unwind_on_thread;
    use polog_test_helpers::tracker::Tracker;

    use crate::Enricher;

    #[test]
    fn test_enricher_accessors() {
        let tracker = Tracker::default();
        let mut enricher = Enricher::new(tracker.clone(), || 123);

        let _unused = enricher.handler();
        let _unused = enricher.handler_mut();
        let _unused = enricher.into_handler();
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn test_enricher_arg_fn_calls() {
        use alloc::sync::Arc;
        use core::sync::atomic::{self, AtomicUsize};

        let tracker = Tracker::default();
        tracker.set_details_enabled(true);

        let counter = Arc::new(AtomicUsize::new(0));
        let mut logger = Enricher::new(tracker.clone(), move || {
            counter.fetch_add(1, atomic::Ordering::Relaxed)
        });

        polog::info!(&mut logger, "Message");
        assert_eq!(
            tracker.take_details(),
            [
                "try_enable(\"Message\", (0))",
                "log(\"Message\", (), None, (0))"
            ]
        );

        let span = polog::info_span!(&mut logger, "Span");
        assert_eq!(
            tracker.take_details(),
            [
                "try_enable(\"Span\", (1))",
                "enter_span(\"Span\", (), None, SpanId(1), Nested, (1))"
            ],
        );
        drop(span);
        assert_eq!(
            tracker.take_details(),
            ["exit_span(\"Span\", SpanId(1), Nested, (2))"]
        );
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[cfg(feature = "alloc")]
    #[tokio::test]
    async fn test_enricher_async_poll() {
        use alloc::sync::Arc;
        use core::sync::atomic::{self, AtomicUsize};

        use polog::PollInstrument;

        let tracker = Tracker::default();
        tracker.set_details_enabled(true);

        let counter = Arc::new(AtomicUsize::new(0));
        let mut logger = Enricher::new(tracker.clone(), move || {
            counter.fetch_add(1, atomic::Ordering::Relaxed)
        });

        let deferred = polog::deferred_info_span!(&mut logger, "Polled");
        let task = tokio::task::spawn(example_async_fn.poll_instrument(deferred));
        task.await.unwrap();

        assert_eq!(
            tracker.take_details(),
            [
                "try_enable(\"Polled\", (0))",
                "enter_span(\"Polled\", (), None, SpanId(1), Detached, (0))",
                "enter_poll_span(\"Polled\", SpanId(1), (1))",
                "try_enable(\"Some warn!\", (2))",
                "log(\"Some warn!\", (12), Some(SpanId(1)), (2))",
                "pending_poll_span(\"Polled\", SpanId(1), (3))",
                "enter_poll_span(\"Polled\", SpanId(1), (4))",
                "try_enable(\"Another warn!\", (5))",
                "log(\"Another warn!\", (20), Some(SpanId(1)), (5))",
                "ready_poll_span(\"Polled\", SpanId(1), (6))",
                "exit_span(\"Polled\", SpanId(1), Detached, (7))",
            ],
        );
    }

    #[cfg(feature = "alloc")]
    async fn example_async_fn<L>(mut logger: L)
    where
        L: polog::Logger,
    {
        polog::info!(&mut logger, "Some warn!"; 3 * 4);
        tokio::task::yield_now().await;
        polog::info!(&mut logger, "Another warn!"; 4 * 5);
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[cfg(feature = "alloc")]
    #[test]
    fn test_enricher_low_level() {
        use alloc::sync::Arc;
        use core::sync::atomic::{self, AtomicUsize};

        use polog::{DynHandler, Handler, SpanKind};

        const TRACE_DESC: &Descriptor = log_descriptor_ref!(Level::Trace, "Message");

        let tracker = Tracker::default();
        tracker.set_details_enabled(true);

        let counter = Arc::new(AtomicUsize::new(0));
        let mut logger = Enricher::new(tracker.clone(), move || {
            counter.fetch_add(1, atomic::Ordering::Relaxed)
        });
        let span_id = logger.generate_span_id();
        assert_eq!(tracker.take_events(), ["generate_span_id"]);

        let context = logger.try_enable(TRACE_DESC, ()).unwrap();
        let n = SpanKind::Nested;

        Handler::<()>::log(&mut logger, TRACE_DESC, (), None, context);
        Handler::<()>::log_static(&mut logger, TRACE_DESC, (), None, context);
        Handler::<()>::enter_span(&mut logger, TRACE_DESC, (), None, span_id, n, context);
        DynHandler::exit_span(&mut logger, TRACE_DESC, span_id, n, ());
        Handler::<()>::enter_span_static(&mut logger, TRACE_DESC, (), None, span_id, n, context);
        DynHandler::exit_span(&mut logger, TRACE_DESC, span_id, n, ());
        assert_eq!(
            tracker.take_details(),
            [
                "try_enable(\"Message\", (0))",
                "log(\"Message\", (), None, (0))",
                "log_static(\"Message\", (), None, (0))",
                "enter_span(\"Message\", (), None, SpanId(1), Nested, (0))",
                "exit_span(\"Message\", SpanId(1), Nested, (1))",
                "enter_span_static(\"Message\", (), None, SpanId(1), Nested, (0))",
                "exit_span(\"Message\", SpanId(1), Nested, (2))",
            ]
        );

        Handler::<()>::enter_span(&mut logger, TRACE_DESC, (), None, span_id, n, context);
        logger.enter_poll_span(TRACE_DESC, span_id, ());
        logger.pending_poll_span(TRACE_DESC, span_id, ());
        logger.enter_poll_span(TRACE_DESC, span_id, ());
        logger.ready_poll_span(TRACE_DESC, span_id, ());
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        assert_eq!(
            tracker.take_details(),
            [
                "enter_span(\"Message\", (), None, SpanId(1), Nested, (0))",
                "enter_poll_span(\"Message\", SpanId(1), (3))",
                "pending_poll_span(\"Message\", SpanId(1), (4))",
                "enter_poll_span(\"Message\", SpanId(1), (5))",
                "ready_poll_span(\"Message\", SpanId(1), (6))",
                "exit_span(\"Message\", SpanId(1), Nested, (7))",
            ]
        );
    }

    #[test]
    fn test_enricher_flush_and_panic_predicate() {
        const BUG_DESC: &Descriptor = log_descriptor_ref!(Level::Bug, "Bug message");

        let tracker = Tracker::default();
        let mut logger = Enricher::new(tracker.clone(), || 123);

        logger.flush_blocking();
        logger.flush_on_panic();
        assert_eq!(
            logger.should_panic_on_bug(BUG_DESC, None),
            cfg!(debug_assertions),
        );
        assert_eq!(
            tracker.take_events(),
            ["flush_blocking", "flush_on_panic", "should_panic_on_bug"],
        );
    }

    #[test]
    fn test_enricher_panic() {
        const FATAL_DESC: &Descriptor = log_descriptor_ref!(Level::Fatal, "Fatal message");

        let tracker = Tracker::default();
        let logger = Enricher::new(tracker.clone(), || 123);

        let _unused = catch_unwind_on_thread(&logger, |mut logger| {
            logger.panic(FATAL_DESC, None);
        })
        .unwrap_err();
        assert_eq!(tracker.take_events(), ["panic"]);
    }

    #[cfg(feature = "log-0-4")]
    #[test]
    fn test_enricher_log_delegates() {
        use core::sync::atomic::{self, AtomicUsize};

        use crate::Enricher;

        #[derive(Default)]
        struct LogTracker {
            enabled: AtomicUsize,
            log: AtomicUsize,
            flush: AtomicUsize,
        }

        impl log_0_4::Log for LogTracker {
            fn enabled(&self, _: &log_0_4::Metadata<'_>) -> bool {
                let _ = self.enabled.fetch_add(1, atomic::Ordering::Relaxed);
                true
            }

            fn log(&self, _: &log_0_4::Record<'_>) {
                let _ = self.log.fetch_add(1, atomic::Ordering::Relaxed);
            }

            fn flush(&self) {
                let _ = self.flush.fetch_add(1, atomic::Ordering::Relaxed);
            }
        }

        let tracker = LogTracker::default();
        let enricher = Enricher::new(tracker, || 123);

        let meta = log_0_4::Metadata::builder()
            .level(log_0_4::Level::Info)
            .build();

        assert!(log_0_4::Log::enabled(&enricher, &meta));

        let record = log_0_4::Record::builder()
            .args(format_args!("msg"))
            .metadata(meta)
            .build();
        log_0_4::Log::log(&enricher, &record);
        log_0_4::Log::flush(&enricher);

        assert_eq!(
            enricher.handler().enabled.load(atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(enricher.handler().log.load(atomic::Ordering::Relaxed), 1);
        assert_eq!(enricher.handler().flush.load(atomic::Ordering::Relaxed), 1);
    }
}
