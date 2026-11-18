use polog::{
    BaseHandler, BorrowingHandler, ContextArgs, Descriptor, DynHandler, Handler, LevelFilter,
    LogArgs, SharedHandler, SpanId, SpanKind, StaticLogArgs,
};

use crate::{BaseExternalHandler, ExternalArgs, ExternalDescriptor, ExternalHandler};

/// Handler middleware that filters events and spans by a [`FilterFn`].
///
/// An event or span is only propagated to the wrapped [`Handler`] when the
/// filter accepts it.
/// Because the filter runs inside [`Handler::try_enable`], filtered-out events
/// do not cause the argument tuple to be constructed by the caller, and
/// per-event context is not built by the wrapped handler.
///
/// # Usage
///
/// ```rust,ignore
/// use polog::LevelFilter;
/// use polog_util::{Filter, MaxLevel};
///
/// let handler = Filter::new(handler, MaxLevel(LevelFilter::Info));
/// ```
///
/// Filters compose:
///
/// ```rust,ignore
/// use polog::LevelFilter;
/// use polog_util::{All, FilterFnExt, MaxLevel, TargetPrefix};
///
/// let filter = MaxLevel(LevelFilter::Debug)
///     .and(TargetPrefix("my_app"))
///     .not();
/// ```
///
/// For configuration-driven filtering (command-line arguments, config files,
/// environment variables), see [`FilterRules`], which parses a
/// `RUST_LOG`-style string:
///
/// ```rust,ignore
/// use polog_util::{Filter, FilterRules};
///
/// let rules = FilterRules::parse("info,my_crate=debug,my_crate::noisy=trace")
///     .expect("invalid filter specification");
/// let handler = Filter::new(handler, rules);
/// ```
///
/// [`FilterRules`]: crate::FilterRules
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct Filter<H, F> {
    /// The wrapped handler.
    pub handler: H,

    /// The filtering predicate.
    pub filter: F,
}

/// A predicate that decides whether a `polog` descriptor should be processed.
///
/// Implemented by every built‑in filter and by every closure
/// `Fn(&'static Descriptor) -> bool`.
/// Used by [`Filter`] to short‑circuit `try_enable` before constructing any
/// per‑event data.
pub trait FilterFn {
    /// Returns `true` if the descriptor passes the filter.
    fn is_enabled(&self, descriptor: &'static Descriptor) -> bool;

    /// The highest‑severity [`LevelFilter`] this filter will accept.
    ///
    /// Returning a tighter filter than `LevelFilter::ALL` lets the wrapped
    /// handler (or a foreign facade that consults this value) skip entire
    /// levels at the call site, before `try_enable` is even reached.
    /// Returning `LevelFilter::ALL` means "no early‑out hint available".
    fn max_level(&self) -> LevelFilter;
}

/// A predicate that decides whether an event from a foreign facade should be
/// processed.
///
/// Implemented by every built‑in filter and by every closure
/// `Fn(&'ExternalDescriptor<'_>) -> bool`.
/// Can be used to filter events that arrive from `log`, `slog`, or `tracing`.
///
/// [`FilterFn`]: crate::FilterFn
pub trait ExternalFilterFn: FilterFn {
    /// Returns `true` if the descriptor passes the filter.
    fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor;
}

impl<H, F> Filter<H, F> {
    /// Creates a new filtering middleware.
    #[inline]
    #[must_use]
    pub const fn new(handler: H, filter: F) -> Self {
        Self { handler, filter }
    }
}

impl<F> FilterFn for F
where
    F: Fn(&'static Descriptor) -> bool,
{
    #[inline]
    fn is_enabled(&self, descriptor: &'static Descriptor) -> bool {
        (self)(descriptor)
    }

    #[inline]
    fn max_level(&self) -> LevelFilter {
        LevelFilter::ALL
    }
}

impl<H, F> BaseHandler for Filter<H, F>
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

impl<H, F, Context> DynHandler<Context> for Filter<H, F>
where
    H: DynHandler<Context>,
    F: FilterFn,
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
        H::exit_span(&mut self.handler, descriptor, span_id, span_kind, context);
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        H::enter_poll_span(&mut self.handler, descriptor, span_id, context);
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        H::pending_poll_span(&mut self.handler, descriptor, span_id, context);
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        H::ready_poll_span(&mut self.handler, descriptor, span_id, context);
    }
}

impl<H, F, Context> Handler<Context> for Filter<H, F>
where
    H: Handler<Context>,
    F: FilterFn,
    Context: ContextArgs,
{
    type EnrichedContext = H::EnrichedContext;

    #[inline]
    fn try_enable(
        &self,
        descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        if self.filter.is_enabled(descriptor) {
            H::try_enable(&self.handler, descriptor, context)
        } else {
            None
        }
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

impl<H, F, Context> BorrowingHandler<Context> for Filter<H, F>
where
    H: BorrowingHandler<Context>,
    F: FilterFn,
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
        H::log_borrowed(&mut self.handler, descriptor, args, span_id, context);
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

impl<H, F, Context> SharedHandler<Context> for Filter<H, F>
where
    H: SharedHandler<Context>,
    F: 'static + Send + Sync + Clone + FilterFn,
    Context: ContextArgs,
{
}

impl<H, F> BaseExternalHandler for Filter<H, F>
where
    H: BaseExternalHandler,
    F: ExternalFilterFn,
{
    #[inline]
    fn max_level_external(&self) -> LevelFilter {
        self.filter
            .max_level()
            .min(H::max_level_external(&self.handler))
    }
}

impl<H, F, Context> ExternalHandler<Context> for Filter<H, F>
where
    H: ExternalHandler<Context>,
    F: ExternalFilterFn,
    Context: ContextArgs,
{
    #[inline]
    fn try_enable_external<Desc>(
        &self,
        descriptor: &Desc,
        context: Context,
    ) -> Option<Self::EnrichedContext>
    where
        Desc: ExternalDescriptor,
    {
        if self.filter.is_enabled_external(&descriptor) {
            H::try_enable_external(&self.handler, &descriptor, context)
        } else {
            None
        }
    }

    #[inline]
    fn log_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        span_id: Option<SpanId>,
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        H::log_external(&mut self.handler, descriptor, args, span_id, context);
    }

    #[inline]
    fn new_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
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
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
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
        H::exit_span_external(&mut self.handler, descriptor, span_id, context);
    }

    #[inline]
    fn close_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
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
        H::link_span_external(
            &mut self.handler,
            descriptor,
            followed_span_id,
            follower_span_id,
            context,
        );
    }
}

#[cfg(feature = "shared-filter")]
#[cfg_attr(docsrs, doc(cfg(feature = "shared-filter")))]
impl<T> FilterFn for alloc::sync::Arc<parking_lot::RwLock<T>>
where
    T: FilterFn,
{
    #[inline]
    fn is_enabled(&self, descriptor: &'static Descriptor) -> bool {
        T::is_enabled(&*self.read(), descriptor)
    }

    #[inline]
    fn max_level(&self) -> LevelFilter {
        T::max_level(&*self.read())
    }
}

#[cfg(feature = "shared-filter")]
#[cfg_attr(docsrs, doc(cfg(feature = "shared-filter")))]
impl<T> ExternalFilterFn for alloc::sync::Arc<parking_lot::RwLock<T>>
where
    T: ExternalFilterFn,
{
    #[inline]
    fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor,
    {
        T::is_enabled_external(&*self.read(), descriptor)
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

    use crate::filters::{Always, Never};
    use crate::{Filter, FilterFn};

    #[test]
    fn test_filter_accepts_and_rejects() {
        let tracker = Tracker::default();

        let mut logger = Filter::new(tracker.clone(), Always);
        polog::info!(&mut logger, "Message");
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);

        let mut logger = Filter::new(tracker.clone(), Never);
        polog::info!(&mut logger, "Message");
        assert_eq!(tracker.take_events(), [""; 0]);
    }

    #[test]
    fn test_filter_closure() {
        let tracker = Tracker::default();
        let filter = |d: &'static Descriptor| d.level == Level::Info;
        let mut logger = Filter::new(tracker.clone(), filter);

        polog::warn!(&mut logger, "Message");
        assert_eq!(tracker.take_events(), [""; 0]);

        polog::info!(&mut logger, "Message");
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);

        polog::debug!(&mut logger, "Message");
        assert_eq!(tracker.take_events(), [""; 0]);

        assert_eq!(FilterFn::max_level(&filter), LevelFilter::ALL);
    }

    #[test]
    fn test_either_low_level() {
        const TRACE_DESC: &Descriptor = log_descriptor_ref!(Level::Trace, "Trace message");

        let tracker = Tracker::default();
        let mut logger = Filter::new(tracker.clone(), Always);
        let span_id = logger.generate_span_id();
        assert_eq!(tracker.take_events(), ["generate_span_id"]);

        logger.log(TRACE_DESC, (), None, ());
        logger.log_static(TRACE_DESC, (), None, ());
        logger.log_borrowed(TRACE_DESC, &(), None, &());
        logger.log(TRACE_DESC, (), None, ());
        logger.enter_span(TRACE_DESC, (), None, span_id, SpanKind::Nested, ());
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        logger.enter_span_static(TRACE_DESC, (), None, span_id, SpanKind::Nested, ());
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        logger.enter_span_borrowed(TRACE_DESC, &(), None, span_id, SpanKind::Nested, &());
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        assert_eq!(
            tracker.take_events(),
            [
                "log",
                "log_static",
                "log_borrowed",
                "log",
                "enter_span(nested)",
                "exit_span(nested)",
                "enter_span_static(nested)",
                "exit_span(nested)",
                "enter_span_borrowed(nested)",
                "exit_span(nested)",
            ]
        );

        logger.enter_span(TRACE_DESC, (), None, span_id, SpanKind::Nested, ());
        logger.enter_poll_span(TRACE_DESC, span_id, ());
        logger.pending_poll_span(TRACE_DESC, span_id, ());
        logger.enter_poll_span(TRACE_DESC, span_id, ());
        logger.ready_poll_span(TRACE_DESC, span_id, ());
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        assert_eq!(
            tracker.take_events(),
            [
                "enter_span(nested)",
                "enter_poll_span",
                "pending_poll_span",
                "enter_poll_span",
                "ready_poll_span",
                "exit_span(nested)",
            ]
        );
    }

    #[test]
    fn test_filter_flush_and_panic_predicate() {
        const BUG_DESC: &Descriptor = log_descriptor_ref!(Level::Bug, "Bug message");

        let tracker = Tracker::default();
        let mut logger = Filter::new(tracker.clone(), Always);

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
    fn test_filter_panic() {
        const FATAL_DESC: &Descriptor = log_descriptor_ref!(Level::Fatal, "Fatal message");

        let tracker = Tracker::default();
        let logger = Filter::new(tracker.clone(), Always);

        let _unused = catch_unwind_on_thread(&logger, |mut logger| {
            logger.panic(FATAL_DESC, None);
        })
        .unwrap_err();
        assert_eq!(tracker.take_events(), ["panic"]);
    }

    #[cfg(feature = "shared-filter")]
    #[test]
    fn test_shared_filter() {
        use alloc::sync::Arc;

        use parking_lot::RwLock;

        use crate::filters::MaxLevel;

        let inner = Arc::new(RwLock::new(MaxLevel(LevelFilter::Info)));
        let filter = Arc::clone(&inner);

        let info_desc = polog::log_descriptor_ref!(Level::Info);
        let debug_desc = polog::log_descriptor_ref!(Level::Debug);

        assert!(FilterFn::is_enabled(&filter, info_desc));
        assert!(!FilterFn::is_enabled(&filter, debug_desc));
        assert_eq!(FilterFn::max_level(&filter), LevelFilter::Info);

        *inner.write() = MaxLevel(LevelFilter::Off);
        assert!(!FilterFn::is_enabled(&filter, info_desc));

        *inner.write() = MaxLevel(LevelFilter::Trace);
        assert!(FilterFn::is_enabled(&filter, debug_desc));
    }

    #[cfg(feature = "shared-filter")]
    #[test]
    fn test_shared_external_filter() {
        use alloc::borrow::Cow;
        use alloc::sync::Arc;

        use parking_lot::RwLock;

        use crate::filters::MaxLevel;
        use crate::{ExternalFilterFn, TypeErasedExternalDescriptor};

        let inner = Arc::new(RwLock::new(MaxLevel(LevelFilter::Info)));
        let filter = Arc::clone(&inner);

        let desc = |level| TypeErasedExternalDescriptor {
            message: Some(Cow::Borrowed("name")),
            level,
            target: Some(Cow::Borrowed("target")),
            module_path: None,
            file: None,
            line: None,
            column: None,
        };

        assert!(ExternalFilterFn::is_enabled_external(
            &filter,
            &desc(Level::Warn)
        ));
        assert!(ExternalFilterFn::is_enabled_external(
            &filter,
            &desc(Level::Info)
        ));
        assert!(!ExternalFilterFn::is_enabled_external(
            &filter,
            &desc(Level::Debug)
        ));
    }
}
