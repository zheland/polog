use polog::{
    BaseHandler, BorrowingHandler, ContextArgs, Descriptor, DynHandler, Handler, LogArgs,
    SharedHandler, SpanId, SpanKind, StaticLogArgs,
};

/// A `polog` [`Handler`] that switches between two handlers at runtime.
///
/// `Either` is used when the handler is not chosen until after construction,
/// typically by a CLI flag, a config file, or a feature probe.
/// Both variants must produce the same `EnrichedContext` type so that the
/// caller's code does not change based on the variant.
///
/// `Either` implements the full [`Handler`] family
/// ([`BaseHandler`], [`DynHandler`], [`Handler`], [`BorrowingHandler`],
/// [`SharedHandler`]) by delegating each method to the active variant.
/// It can be composed with any other `polog-util` middleware.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum Either<H1, H2> {
    /// The first handler.
    V1(H1),
    /// The second handler.
    V2(H2),
}

impl<H1, H2> BaseHandler for Either<H1, H2>
where
    H1: BaseHandler,
    H2: BaseHandler,
{
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        match self {
            Self::V1(handler) => H1::generate_span_id(handler),
            Self::V2(handler) => H2::generate_span_id(handler),
        }
    }

    #[inline]
    fn flush_blocking(&mut self) {
        match self {
            Self::V1(handler) => H1::flush_blocking(&mut *handler),
            Self::V2(handler) => H2::flush_blocking(&mut *handler),
        }
    }

    #[inline]
    fn flush_on_panic(&mut self) {
        match self {
            Self::V1(handler) => H1::flush_on_panic(&mut *handler),
            Self::V2(handler) => H2::flush_on_panic(&mut *handler),
        }
    }

    #[inline]
    fn should_panic_on_bug(
        &self,
        descriptor: &'static Descriptor,
        span_id: Option<SpanId>,
    ) -> bool {
        match self {
            Self::V1(handler) => H1::should_panic_on_bug(handler, descriptor, span_id),
            Self::V2(handler) => H2::should_panic_on_bug(handler, descriptor, span_id),
        }
    }

    #[inline]
    fn panic(&mut self, descriptor: &'static Descriptor, span_id: Option<SpanId>) -> ! {
        match self {
            Self::V1(handler) => H1::panic(&mut *handler, descriptor, span_id),
            Self::V2(handler) => H2::panic(&mut *handler, descriptor, span_id),
        }
    }
}

impl<H1, H2, Context> DynHandler<Context> for Either<H1, H2>
where
    H1: DynHandler<Context>,
    H2: DynHandler<Context>,
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
        match self {
            Self::V1(handler) => {
                H1::exit_span(&mut *handler, descriptor, span_id, span_kind, context);
            }
            Self::V2(handler) => {
                H2::exit_span(&mut *handler, descriptor, span_id, span_kind, context);
            }
        }
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        match self {
            Self::V1(handler) => {
                H1::enter_poll_span(&mut *handler, descriptor, span_id, context);
            }
            Self::V2(handler) => {
                H2::enter_poll_span(&mut *handler, descriptor, span_id, context);
            }
        }
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        match self {
            Self::V1(handler) => {
                H1::pending_poll_span(&mut *handler, descriptor, span_id, context);
            }
            Self::V2(handler) => {
                H2::pending_poll_span(&mut *handler, descriptor, span_id, context);
            }
        }
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        match self {
            Self::V1(handler) => {
                H1::ready_poll_span(&mut *handler, descriptor, span_id, context);
            }
            Self::V2(handler) => {
                H2::ready_poll_span(&mut *handler, descriptor, span_id, context);
            }
        }
    }
}

impl<H1, H2, Context, EnrichedContext> Handler<Context> for Either<H1, H2>
where
    H1: Handler<Context, EnrichedContext = EnrichedContext>,
    H2: Handler<Context, EnrichedContext = EnrichedContext>,
    Context: ContextArgs,
{
    type EnrichedContext = EnrichedContext;

    #[inline]
    fn try_enable(
        &self,
        descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        match self {
            Self::V1(handler) => H1::try_enable(handler, descriptor, context),
            Self::V2(handler) => H2::try_enable(handler, descriptor, context),
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
        match self {
            Self::V1(handler) => H1::log(&mut *handler, descriptor, args, span_id, context),
            Self::V2(handler) => H2::log(&mut *handler, descriptor, args, span_id, context),
        }
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
        match self {
            Self::V1(handler) => H1::enter_span(
                &mut *handler,
                descriptor,
                args,
                parent_span_id,
                span_id,
                span_kind,
                context,
            ),
            Self::V2(handler) => H2::enter_span(
                &mut *handler,
                descriptor,
                args,
                parent_span_id,
                span_id,
                span_kind,
                context,
            ),
        }
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
        match self {
            Self::V1(handler) => H1::log_static(&mut *handler, descriptor, args, span_id, context),
            Self::V2(handler) => H2::log_static(&mut *handler, descriptor, args, span_id, context),
        }
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
        match self {
            Self::V1(handler) => H1::enter_span_static(
                &mut *handler,
                descriptor,
                args,
                parent_span_id,
                span_id,
                span_kind,
                context,
            ),
            Self::V2(handler) => H2::enter_span_static(
                &mut *handler,
                descriptor,
                args,
                parent_span_id,
                span_id,
                span_kind,
                context,
            ),
        }
    }
}

impl<H1, H2, Context, EnrichedContext> BorrowingHandler<Context> for Either<H1, H2>
where
    H1: BorrowingHandler<Context, EnrichedContext = EnrichedContext>,
    H2: BorrowingHandler<Context, EnrichedContext = EnrichedContext>,
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
        match self {
            Self::V1(handler) => {
                H1::log_borrowed(&mut *handler, descriptor, args, span_id, context);
            }
            Self::V2(handler) => {
                H2::log_borrowed(&mut *handler, descriptor, args, span_id, context);
            }
        }
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
        match self {
            Self::V1(handler) => H1::enter_span_borrowed(
                &mut *handler,
                descriptor,
                args,
                parent_span_id,
                span_id,
                span_kind,
                context,
            ),
            Self::V2(handler) => H2::enter_span_borrowed(
                &mut *handler,
                descriptor,
                args,
                parent_span_id,
                span_id,
                span_kind,
                context,
            ),
        }
    }
}

impl<H1, H2, Context, EnrichedContext> SharedHandler<Context> for Either<H1, H2>
where
    H1: SharedHandler<Context, EnrichedContext = EnrichedContext>,
    H2: SharedHandler<Context, EnrichedContext = EnrichedContext>,
    Context: ContextArgs,
{
}

#[cfg(test)]
mod tests {
    use polog::{
        BaseHandler, BorrowingHandler, Descriptor, Discard, DynHandler, Handler, Level,
        LevelFilter, SpanKind, log_descriptor_ref,
    };
    use polog_test_helpers::panic::catch_unwind_on_thread;
    use polog_test_helpers::tracker::Tracker;

    use crate::filters::MaxLevel;
    use crate::{Either, Filter};

    #[test]
    fn test_either() {
        const BASIC_AND_GEN: &[&str] = &[
            "try_enable",
            "log",
            "try_enable",
            "generate_span_id",
            "enter_span(nested)",
            "exit_span(nested)",
        ];

        let tracker = Tracker::default();

        let mut logger: Either<Discard, Discard> = Either::V1(Discard);

        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);

        let mut logger: Either<Tracker, Filter<Tracker, MaxLevel>> = Either::V1(tracker.clone());

        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker.take_events(), BASIC_AND_GEN);

        polog::trace!(&mut logger, "Message");
        let span = polog::trace_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker.take_events(), BASIC_AND_GEN);

        let mut logger: Either<Tracker, Filter<Tracker, MaxLevel>> =
            Either::V2(Filter::new(tracker.clone(), MaxLevel(LevelFilter::Info)));

        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker.take_events(), BASIC_AND_GEN);

        polog::trace!(&mut logger, "Message");
        let span = polog::trace_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker.take_events(), [""; 0]);
    }

    #[test]
    fn test_either_low_level() {
        const TRACE_DESC: &Descriptor = log_descriptor_ref!(Level::Trace, "Message");

        let tracker = Tracker::default();
        let filtered = |tracker: Tracker, level: LevelFilter| Filter::new(tracker, MaxLevel(level));

        let logger1: Either<Filter<Tracker, MaxLevel>, Tracker> =
            Either::V1(filtered(tracker.clone(), LevelFilter::Info));
        let logger2: Either<Filter<Tracker, MaxLevel>, Tracker> = Either::V2(tracker.clone());

        for mut logger in [logger1, logger2] {
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
    }

    #[test]
    fn test_either_flush_and_panic_predicate() {
        const BUG_DESC: &Descriptor = log_descriptor_ref!(Level::Bug, "Message");

        let tracker = Tracker::default();
        let filtered = |tracker: Tracker, level: LevelFilter| Filter::new(tracker, MaxLevel(level));

        let logger1: Either<Filter<Tracker, MaxLevel>, Tracker> =
            Either::V1(filtered(tracker.clone(), LevelFilter::Info));
        let logger2: Either<Filter<Tracker, MaxLevel>, Tracker> = Either::V2(tracker.clone());

        for mut logger in [logger1, logger2] {
            logger.flush_blocking();
            logger.flush_on_panic();
            assert_eq!(
                logger.should_panic_on_bug(BUG_DESC, None),
                cfg!(debug_assertions)
            );
            assert_eq!(
                tracker.take_events(),
                ["flush_blocking", "flush_on_panic", "should_panic_on_bug"]
            );
        }
    }

    #[test]
    fn test_either_panic() {
        const FATAL_DESC: &Descriptor = log_descriptor_ref!(Level::Fatal, "Message");

        let tracker = Tracker::default();
        let filtered = |tracker: Tracker, level: LevelFilter| Filter::new(tracker, MaxLevel(level));

        let logger1: Either<Filter<Tracker, MaxLevel>, Tracker> =
            Either::V1(filtered(tracker.clone(), LevelFilter::Info));
        let logger2: Either<Filter<Tracker, MaxLevel>, Tracker> = Either::V2(tracker.clone());

        for logger in [logger1, logger2] {
            let _unused = catch_unwind_on_thread(&logger, |mut logger| {
                logger.panic(FATAL_DESC, None);
            })
            .unwrap_err();
            assert_eq!(tracker.take_events(), ["panic"]);
        }
    }
}
