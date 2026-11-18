use alloc::sync::Arc;

use parking_lot::Mutex;
use polog::{
    BaseHandler, BorrowingHandler, ContextArgs, Descriptor, DynHandler, Handler, LevelFilter,
    LogArgs, SharedHandler, SpanId, SpanKind, StaticLogArgs,
};

use crate::{BaseExternalHandler, ExternalArgs, ExternalDescriptor, ExternalHandler};

/// Wraps a non‑shared [`Handler`] so it can be cloned and shared across
/// threads.
///
/// `Shared<H>` stores the inner handler behind `Arc<Mutex<H>>` and implements
/// the full [`Handler`] family by locking the mutex for the duration of each
/// method.
/// It implements [`SharedHandler<Context>`] whenever `H` implements
/// `Handler<Context>` and `H` is `Send + 'static`, which is the case for any
/// leaf backend and for any middleware that would otherwise be single‑thread
/// only.
///
/// # Locking
///
/// Every method takes the mutex for the duration of the call.
/// Formatting in [`Handler::log`] therefore blocks other threads.
/// For throughput‑sensitive workloads use
/// [`OffloaderHandle`] instead, which moves the work to a dedicated worker
/// without a shared lock.
///
/// `Shared<H>` is the appropriate choice when the handler is cheap and
/// contention is low, or when the handler owns resources that cannot be
/// moved to another thread.
///
/// [`OffloaderHandle`]: crate::OffloaderHandle
#[derive(Default, Debug)]
pub struct Shared<H>(Arc<Mutex<H>>);

impl<H> Shared<H> {
    /// Builds new `Shared<H>` from `H`.
    #[inline]
    #[must_use]
    pub fn new(handler: H) -> Self {
        Self(Arc::new(Mutex::new(handler)))
    }

    /// Returns a shared reference to the mutex that guards the wrapped
    /// handler.
    ///
    /// The reference is to the whole [`parking_lot::Mutex<H>`], not to the
    /// handler inside it.
    /// Acquiring the handler requires the caller to lock the returned mutex
    /// themselves:
    ///
    /// # Locking
    ///
    /// Every [`Handler`] method on `Shared<H>` locks the same mutex for the
    /// duration of the call.
    /// The returned reference does not prevent those methods from blocking on
    /// your guard.
    /// It merely gives you direct access to the underlying lock.
    ///
    /// # Deadlocks
    ///
    /// [`parking_lot::Mutex`] is not reentrant.
    /// Calling any [`Handler`] method on the same `Shared<H>` from inside a
    /// scope that holds a guard obtained from `mutex()` deadlocks.
    ///
    /// The same rule applies to `Clone` handles: `shared.clone()` shares
    /// the `Arc<Mutex<H>>`, so a guard held by one clone blocks every
    /// method on every other clone.
    ///
    /// [`Handler`]: polog::Handler
    #[inline]
    #[must_use]
    pub const fn inner(&self) -> &Arc<Mutex<H>> {
        &self.0
    }
}

impl<H> BaseHandler for Shared<H>
where
    H: BaseHandler,
{
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        H::generate_span_id(&*self.0.lock())
    }

    #[inline]
    fn flush_blocking(&mut self) {
        H::flush_blocking(&mut *self.0.lock());
    }

    #[inline]
    fn flush_on_panic(&mut self) {
        H::flush_on_panic(&mut *self.0.lock());
    }

    #[inline]
    fn should_panic_on_bug(
        &self,
        descriptor: &'static Descriptor,
        span_id: Option<SpanId>,
    ) -> bool {
        H::should_panic_on_bug(&*self.0.lock(), descriptor, span_id)
    }

    #[inline]
    fn panic(&mut self, descriptor: &'static Descriptor, span_id: Option<SpanId>) -> ! {
        H::panic(&mut *self.0.lock(), descriptor, span_id)
    }
}

impl<H, Context> DynHandler<Context> for Shared<H>
where
    H: DynHandler<Context>,
    Context: ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    ) {
        H::exit_span(&mut *self.0.lock(), descriptor, span_id, span_kind, context);
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        H::enter_poll_span(&mut *self.0.lock(), descriptor, span_id, context);
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        H::pending_poll_span(&mut *self.0.lock(), descriptor, span_id, context);
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        H::ready_poll_span(&mut *self.0.lock(), descriptor, span_id, context);
    }
}

impl<H, Context> Handler<Context> for Shared<H>
where
    H: Handler<Context>,
    Context: ContextArgs,
{
    type EnrichedContext = H::EnrichedContext;

    #[inline]
    fn try_enable(
        &self,
        descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        H::try_enable(&*self.0.lock(), descriptor, context)
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
        H::log(&mut *self.0.lock(), descriptor, args, span_id, context);
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
            &mut *self.0.lock(),
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
        H::log_static(&mut *self.0.lock(), descriptor, args, span_id, context);
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
            &mut *self.0.lock(),
            descriptor,
            args,
            parent_span_id,
            span_id,
            span_kind,
            context,
        );
    }
}

impl<H, Context> BorrowingHandler<Context> for Shared<H>
where
    H: BorrowingHandler<Context>,
    Context: ContextArgs,
{
    #[inline]
    fn log_borrowed<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        span_id: Option<SpanId>,
        context: &Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        H::log_borrowed(&mut *self.0.lock(), descriptor, args, span_id, context);
    }

    #[inline]
    fn enter_span_borrowed<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: &Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        H::enter_span_borrowed(
            &mut *self.0.lock(),
            descriptor,
            args,
            parent_span_id,
            span_id,
            span_kind,
            context,
        );
    }
}

impl<H, Context> SharedHandler<Context> for Shared<H>
where
    H: 'static + Send + Handler<Context>,
    Context: ContextArgs,
{
}

impl<H> Clone for Shared<H> {
    #[inline]
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<H> BaseExternalHandler for Shared<H>
where
    H: BaseExternalHandler,
{
    #[inline]
    fn max_level_external(&self) -> LevelFilter {
        H::max_level_external(&*self.0.lock())
    }

    #[inline]
    fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor,
    {
        H::is_enabled_external(&*self.0.lock(), descriptor)
    }
}

impl<H, Context> ExternalHandler<Context> for Shared<H>
where
    H: ExternalHandler<Context>,
    Context: ContextArgs,
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
        H::log_external(&mut *self.0.lock(), descriptor, args, span_id, context);
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
        H::new_span_external(
            &mut *self.0.lock(),
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
        H::enter_span_external(
            &mut *self.0.lock(),
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
        H::exit_span_external(&mut *self.0.lock(), descriptor, span_id, context);
    }

    #[inline]
    fn close_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        H::close_span_external(&mut *self.0.lock(), descriptor, span_id, context);
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
        H::record_span_external(&mut *self.0.lock(), descriptor, args, span_id, context);
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
        H::link_span_external(
            &mut *self.0.lock(),
            descriptor,
            followed_span_id,
            follower_span_id,
            context,
        );
    }
}

#[cfg(test)]
mod tests {
    use polog::{
        BaseHandler, BorrowingHandler, Descriptor, DynHandler, Handler, Level, LevelFilter,
        SpanKind, log_descriptor_ref,
    };
    use polog_test_helpers::panic::catch_unwind_on_thread;
    use polog_test_helpers::tracker::Tracker;

    use crate::filters::MaxLevel;
    use crate::{Filter, Shared};

    #[test]
    fn test_shared_forward() {
        let tracker = Tracker::default();
        let mut logger = Shared::new(tracker.clone());

        polog::info!(&mut logger, "Message");
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);

        polog::info!(&mut logger, "Message");
        assert_eq!(logger.inner().lock().take_events(), ["try_enable", "log"]);

        let span = polog::info_span!(&mut logger, "Span");
        assert_eq!(
            tracker.take_events(),
            ["try_enable", "generate_span_id", "enter_span(nested)"],
        );
        drop(span);
        assert_eq!(tracker.take_events(), ["exit_span(nested)"]);
    }

    #[test]
    fn test_shared_clone() {
        let tracker = Tracker::default();
        let mut shared1 = Shared::new(tracker.clone());
        let mut shared2 = shared1.clone();

        polog::info!(&mut shared1, "Message");
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);
        drop(shared1);

        polog::info!(&mut shared2, "Message");
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);
    }

    #[test]
    fn test_shared_low_level() {
        const INFO_DESC: &Descriptor = log_descriptor_ref!(Level::Info, "Message");

        let tracker = Tracker::default();
        let mut logger = Shared::new(Filter::new(tracker.clone(), MaxLevel(LevelFilter::Debug)));

        let span_id = logger.generate_span_id();
        assert_eq!(tracker.take_events(), ["generate_span_id"]);

        logger.log(INFO_DESC, (), None, ());
        logger.log_static(INFO_DESC, (), None, ());
        logger.log_borrowed(INFO_DESC, &(), None, &());
        logger.enter_span(INFO_DESC, (), None, span_id, SpanKind::Nested, ());
        logger.exit_span(INFO_DESC, span_id, SpanKind::Nested, ());
        logger.enter_span_static(INFO_DESC, (), None, span_id, SpanKind::Nested, ());
        logger.exit_span(INFO_DESC, span_id, SpanKind::Nested, ());
        logger.enter_span_borrowed(INFO_DESC, &(), None, span_id, SpanKind::Nested, &());
        logger.exit_span(INFO_DESC, span_id, SpanKind::Nested, ());
        assert_eq!(
            tracker.take_events(),
            [
                "log",
                "log_static",
                "log_borrowed",
                "enter_span(nested)",
                "exit_span(nested)",
                "enter_span_static(nested)",
                "exit_span(nested)",
                "enter_span_borrowed(nested)",
                "exit_span(nested)"
            ]
        );

        logger.enter_span(INFO_DESC, (), None, span_id, SpanKind::Nested, ());
        logger.enter_poll_span(INFO_DESC, span_id, ());
        logger.pending_poll_span(INFO_DESC, span_id, ());
        logger.enter_poll_span(INFO_DESC, span_id, ());
        logger.ready_poll_span(INFO_DESC, span_id, ());
        logger.exit_span(INFO_DESC, span_id, SpanKind::Nested, ());
        assert_eq!(
            tracker.take_events(),
            [
                "enter_span(nested)",
                "enter_poll_span",
                "pending_poll_span",
                "enter_poll_span",
                "ready_poll_span",
                "exit_span(nested)"
            ],
        );
    }

    #[test]
    fn test_shared_flush_and_panic_predicate() {
        const BUG_DESC: &Descriptor = log_descriptor_ref!(Level::Bug, "Message");

        let tracker = Tracker::default();
        let mut logger = Shared::new(tracker.clone());

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
    fn test_shared_panic() {
        const FATAL_DESC: &Descriptor = log_descriptor_ref!(Level::Fatal, "Message");

        let tracker = Tracker::default();
        let logger = Shared::new(tracker.clone());

        let _unused = catch_unwind_on_thread(&logger, |mut logger| {
            logger.panic(FATAL_DESC, None);
        })
        .unwrap_err();
        assert_eq!(tracker.take_events(), ["panic"]);
    }

    #[test]
    fn test_shared_should_panic_on_bug_reaches_inner() {
        // `Shared` forwards `should_panic_on_bug` to the wrapped handler, so
        // any custom override on the backend is honored.
        const BUG_DESC: &Descriptor = log_descriptor_ref!(Level::Bug, "Bug message");

        let tracker = Tracker::default();
        let logger = Shared::new(tracker.clone());

        let _unused = logger.should_panic_on_bug(BUG_DESC, None);
        assert_eq!(tracker.take_events(), ["should_panic_on_bug"]);
    }
}
