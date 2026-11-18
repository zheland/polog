use either_1::Either;

use crate::{
    BaseHandler, BorrowingHandler, ContextArgs, Descriptor, DynHandler, Handler, LogArgs,
    SharedHandler, SpanId, SpanKind, StaticLogArgs,
};

impl<H1, H2> BaseHandler for Either<H1, H2>
where
    H1: BaseHandler,
    H2: BaseHandler,
{
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        match self {
            Self::Left(handler) => H1::generate_span_id(handler),
            Self::Right(handler) => H2::generate_span_id(handler),
        }
    }

    #[inline]
    fn flush_blocking(&mut self) {
        match self {
            Self::Left(handler) => H1::flush_blocking(&mut *handler),
            Self::Right(handler) => H2::flush_blocking(&mut *handler),
        }
    }

    #[inline]
    fn flush_on_panic(&mut self) {
        match self {
            Self::Left(handler) => H1::flush_on_panic(&mut *handler),
            Self::Right(handler) => H2::flush_on_panic(&mut *handler),
        }
    }

    #[inline]
    fn should_panic_on_bug(
        &self,
        descriptor: &'static Descriptor,
        span_id: Option<SpanId>,
    ) -> bool {
        match self {
            Self::Left(handler) => H1::should_panic_on_bug(handler, descriptor, span_id),
            Self::Right(handler) => H2::should_panic_on_bug(handler, descriptor, span_id),
        }
    }

    #[inline]
    fn panic(&mut self, descriptor: &'static Descriptor, span_id: Option<SpanId>) -> ! {
        match self {
            Self::Left(handler) => H1::panic(&mut *handler, descriptor, span_id),
            Self::Right(handler) => H2::panic(&mut *handler, descriptor, span_id),
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
            Self::Left(handler) => {
                H1::exit_span(&mut *handler, descriptor, span_id, span_kind, context);
            }
            Self::Right(handler) => {
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
            Self::Left(handler) => {
                H1::enter_poll_span(&mut *handler, descriptor, span_id, context);
            }
            Self::Right(handler) => {
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
            Self::Left(handler) => {
                H1::pending_poll_span(&mut *handler, descriptor, span_id, context);
            }
            Self::Right(handler) => {
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
            Self::Left(handler) => {
                H1::ready_poll_span(&mut *handler, descriptor, span_id, context);
            }
            Self::Right(handler) => {
                H2::ready_poll_span(&mut *handler, descriptor, span_id, context);
            }
        }
    }
}

impl<H1, H2, Context> Handler<Context> for Either<H1, H2>
where
    H1: Handler<Context>,
    H2: Handler<Context>,
    Context: ContextArgs,
{
    type EnrichedContext = Either<H1::EnrichedContext, H2::EnrichedContext>;

    #[inline]
    fn try_enable(
        &self,
        descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        match self {
            Self::Left(handler) => H1::try_enable(handler, descriptor, context).map(Either::Left),
            Self::Right(handler) => H2::try_enable(handler, descriptor, context).map(Either::Right),
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
        match (self, context) {
            (Self::Left(handler), Self::EnrichedContext::Left(context)) => {
                H1::log(&mut *handler, descriptor, args, span_id, context);
            }
            (Self::Right(handler), Self::EnrichedContext::Right(context)) => {
                H2::log(&mut *handler, descriptor, args, span_id, context);
            }
            (Self::Left(_), Self::EnrichedContext::Right(_))
            | (Self::Right(_), Self::EnrichedContext::Left(_)) => {
                // Silently ignore the wrong combination of handlers and
                // enriched contexts.
            }
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
        match (self, context) {
            (Self::Left(handler), Self::EnrichedContext::Left(context)) => H1::enter_span(
                &mut *handler,
                descriptor,
                args,
                parent_span_id,
                span_id,
                span_kind,
                context,
            ),
            (Self::Right(handler), Self::EnrichedContext::Right(context)) => H2::enter_span(
                &mut *handler,
                descriptor,
                args,
                parent_span_id,
                span_id,
                span_kind,
                context,
            ),
            (Self::Left(_), Self::EnrichedContext::Right(_))
            | (Self::Right(_), Self::EnrichedContext::Left(_)) => {
                // Silently ignore the wrong combination of handlers and
                // enriched contexts.
            }
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
        match (self, context) {
            (Self::Left(handler), Self::EnrichedContext::Left(context)) => {
                H1::log_static(&mut *handler, descriptor, args, span_id, context);
            }
            (Self::Right(handler), Self::EnrichedContext::Right(context)) => {
                H2::log_static(&mut *handler, descriptor, args, span_id, context);
            }
            (Self::Left(_), Self::EnrichedContext::Right(_))
            | (Self::Right(_), Self::EnrichedContext::Left(_)) => {
                // Silently ignore the wrong combination of handlers and
                // enriched contexts.
            }
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
        match (self, context) {
            (Self::Left(handler), Self::EnrichedContext::Left(context)) => H1::enter_span_static(
                &mut *handler,
                descriptor,
                args,
                parent_span_id,
                span_id,
                span_kind,
                context,
            ),
            (Self::Right(handler), Self::EnrichedContext::Right(context)) => H2::enter_span_static(
                &mut *handler,
                descriptor,
                args,
                parent_span_id,
                span_id,
                span_kind,
                context,
            ),
            (Self::Left(_), Self::EnrichedContext::Right(_))
            | (Self::Right(_), Self::EnrichedContext::Left(_)) => {
                // Silently ignore the wrong combination of handlers and
                // enriched contexts.
            }
        }
    }
}

impl<H1, H2, Context> BorrowingHandler<Context> for Either<H1, H2>
where
    H1: BorrowingHandler<Context>,
    H2: BorrowingHandler<Context>,
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
        match (self, context) {
            (Self::Left(handler), Self::EnrichedContext::Left(context)) => {
                H1::log_borrowed(&mut *handler, descriptor, args, span_id, context);
            }
            (Self::Right(handler), Self::EnrichedContext::Right(context)) => {
                H2::log_borrowed(&mut *handler, descriptor, args, span_id, context);
            }
            (Self::Left(_), Self::EnrichedContext::Right(_))
            | (Self::Right(_), Self::EnrichedContext::Left(_)) => {
                // Silently ignore the wrong combination of handlers and
                // enriched contexts.
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
        match (self, context) {
            (Self::Left(handler), Self::EnrichedContext::Left(context)) => H1::enter_span_borrowed(
                &mut *handler,
                descriptor,
                args,
                parent_span_id,
                span_id,
                span_kind,
                context,
            ),
            (Self::Right(handler), Self::EnrichedContext::Right(context)) => {
                H2::enter_span_borrowed(
                    &mut *handler,
                    descriptor,
                    args,
                    parent_span_id,
                    span_id,
                    span_kind,
                    context,
                );
            }
            (Self::Left(_), Self::EnrichedContext::Right(_))
            | (Self::Right(_), Self::EnrichedContext::Left(_)) => {
                // Silently ignore the wrong combination of handlers and
                // enriched contexts.
            }
        }
    }
}

impl<H1, H2, Context> SharedHandler<Context> for Either<H1, H2>
where
    H1: SharedHandler<Context>,
    H2: SharedHandler<Context>,
    Context: ContextArgs,
{
}
