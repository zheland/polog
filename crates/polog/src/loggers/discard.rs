use crate::{
    BaseHandler, BorrowingHandler, ContextArgs, Descriptor, DynHandler, Handler, LogArgs,
    SharedHandler, SpanId, SpanIdGenerator, SpanKind, StaticLogArgs,
};

static SPAN_ID_GENERATOR: SpanIdGenerator = SpanIdGenerator::new();

/// A no-op [`Handler`] that discards every event and span.
///
/// [`Handler::try_enable`] always returns `None`, so the logging macros skip
/// argument construction entirely: a call site that logs through `Discard`
/// costs only the [`try_enable`] call itself.
/// Every other lifecycle method (`log`, `enter_span`, `exit_span`, and the poll
/// callbacks) is a no-op.
///
/// `Discard` is the recommended default for tests, benchmarks, and
/// applications where logging is optional and log statements should be as
/// cheap as possible.
///
/// [`try_enable`]: Handler::try_enable
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct Discard;

impl BaseHandler for Discard {
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        SPAN_ID_GENERATOR.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {}

    #[inline]
    fn flush_on_panic(&mut self) {}
}

impl<Context> DynHandler<Context> for Discard
where
    Context: ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _span_kind: SpanKind,
        _context: Context,
    ) {
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
    }
}

impl<Context> Handler<Context> for Discard
where
    Context: ContextArgs,
{
    type EnrichedContext = Context;

    #[inline]
    fn try_enable(
        &self,
        _descriptor: &'static Descriptor,
        _context: Context,
    ) -> Option<Self::EnrichedContext> {
        None
    }

    #[inline]
    fn log<'a, Args>(
        &mut self,
        _descriptor: &'static Descriptor,
        _args: Args,
        _span_id: Option<SpanId>,
        _context: Context,
    ) where
        Args: LogArgs<'a>,
    {
    }

    #[inline]
    fn enter_span<'a, Args>(
        &mut self,
        _descriptor: &'static Descriptor,
        _args: Args,
        _parent_span_id: Option<SpanId>,
        _span_id: SpanId,
        _span_kind: SpanKind,
        _context: Context,
    ) where
        Args: LogArgs<'a>,
    {
    }

    #[inline]
    fn log_static<Args>(
        &mut self,
        _descriptor: &'static Descriptor,
        _args: Args,
        _span_id: Option<SpanId>,
        _context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
    }

    #[inline]
    fn enter_span_static<Args>(
        &mut self,
        _descriptor: &'static Descriptor,
        _args: Args,
        _parent_span_id: Option<SpanId>,
        _span_id: SpanId,
        _span_kind: SpanKind,
        _context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
    }
}

impl<Context> BorrowingHandler<Context> for Discard
where
    Context: ContextArgs,
{
    #[inline]
    fn log_borrowed<Args>(
        &mut self,
        _descriptor: &'static Descriptor,
        _args: &Args,
        _span_id: Option<SpanId>,
        _context: &Context,
    ) where
        Args: StaticLogArgs,
    {
    }

    #[inline]
    fn enter_span_borrowed<Args>(
        &mut self,
        _descriptor: &'static Descriptor,
        _args: &Args,
        _parent_span_id: Option<SpanId>,
        __span_id: SpanId,
        _span_kind: SpanKind,
        _context: &Context,
    ) where
        Args: StaticLogArgs,
    {
    }
}

impl<Context> SharedHandler<Context> for Discard where Context: ContextArgs {}

impl BaseHandler for &Discard {
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        SPAN_ID_GENERATOR.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {}

    #[inline]
    fn flush_on_panic(&mut self) {}
}

impl<Context> DynHandler<Context> for &Discard
where
    Context: ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _span_kind: SpanKind,
        _context: Context,
    ) {
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
    }
}

impl<Context> Handler<Context> for &Discard
where
    Context: ContextArgs,
{
    type EnrichedContext = Context;

    #[inline]
    fn try_enable(
        &self,
        _descriptor: &'static Descriptor,
        _context: Context,
    ) -> Option<Self::EnrichedContext> {
        None
    }

    #[inline]
    fn log<'a, Args>(
        &mut self,
        _descriptor: &'static Descriptor,
        _args: Args,
        _span_id: Option<SpanId>,
        _context: Context,
    ) where
        Args: LogArgs<'a>,
    {
    }

    #[inline]
    fn enter_span<'a, Args>(
        &mut self,
        _descriptor: &'static Descriptor,
        _args: Args,
        _parent_span_id: Option<SpanId>,
        _span_id: SpanId,
        _span_kind: SpanKind,
        _context: Context,
    ) where
        Args: LogArgs<'a>,
    {
    }

    #[inline]
    fn log_static<Args>(
        &mut self,
        _descriptor: &'static Descriptor,
        _args: Args,
        _span_id: Option<SpanId>,
        _context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
    }

    #[inline]
    fn enter_span_static<Args>(
        &mut self,
        _descriptor: &'static Descriptor,
        _args: Args,
        _parent_span_id: Option<SpanId>,
        _span_id: SpanId,
        _span_kind: SpanKind,
        _context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
    }
}

impl<Context> BorrowingHandler<Context> for &Discard
where
    Context: ContextArgs,
{
    #[inline]
    fn log_borrowed<Args>(
        &mut self,
        _descriptor: &'static Descriptor,
        _args: &Args,
        _span_id: Option<SpanId>,
        _context: &Context,
    ) where
        Args: StaticLogArgs,
    {
    }

    #[inline]
    fn enter_span_borrowed<Args>(
        &mut self,
        _descriptor: &'static Descriptor,
        _args: &Args,
        _parent_span_id: Option<SpanId>,
        __span_id: SpanId,
        _span_kind: SpanKind,
        _context: &Context,
    ) where
        Args: StaticLogArgs,
    {
    }
}

impl<Context> SharedHandler<Context> for &'static Discard where Context: ContextArgs {}
