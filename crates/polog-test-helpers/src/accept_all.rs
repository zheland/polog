use polog::{
    BaseHandler, BorrowingHandler, ContextArgs, Descriptor, DynHandler, Handler, LogArgs,
    SharedHandler, SpanId, SpanIdGenerator, SpanKind, StaticLogArgs,
};

static SPAN_ID_GENERATOR: SpanIdGenerator = SpanIdGenerator::new();

/// A no-op [`Handler`] for tests that accepts every event but emits
/// nothing.
///
/// [`Handler::try_enable`] always returns `Some(context)`, so `polog` macros
/// construct the argument tuple and pass it through the handler chain, just as
/// they would for a real backend. Every other lifecycle method — `log`,
/// `enter_span`, `exit_span`, and the poll callbacks — is
/// a no-op, so nothing is ever emitted or recorded.
///
/// # Relation to `Discard`
///
/// [`Discard`] returns `None` from `try_enable`, letting the macros skip
/// argument construction entirely.
/// `AcceptAll` is its counterpart: it accepts every event specifically so that
/// the argument tuple is built.
/// That is what a test needs when the argument-construction path — not the
/// emitted event — is what is being exercised.
///
/// Use `AcceptAll` in tests that verify a call site type-checks end to
/// end, for example that a value's `IntoSendable` and `Display`
/// implementations compose at the call site.
/// Use `Discard` in production code, or in tests where argument construction is
/// not part of what is being verified.
///
/// [`Discard`]: polog::Discard
/// [`try_enable`]: Handler::try_enable
#[doc(hidden)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct AcceptAll;

impl BaseHandler for AcceptAll {
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        SPAN_ID_GENERATOR.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {}

    #[inline]
    fn flush_on_panic(&mut self) {}
}

impl<Context> DynHandler<Context> for AcceptAll
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

impl<Context> Handler<Context> for AcceptAll
where
    Context: ContextArgs,
{
    type EnrichedContext = Context;

    #[inline]
    fn try_enable(
        &self,
        _descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        Some(context)
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

impl<Context> BorrowingHandler<Context> for AcceptAll
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

impl<Context> SharedHandler<Context> for AcceptAll where Context: ContextArgs {}

impl BaseHandler for &AcceptAll {
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        SPAN_ID_GENERATOR.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {}

    #[inline]
    fn flush_on_panic(&mut self) {}
}

impl<Context> DynHandler<Context> for &AcceptAll
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

impl<Context> Handler<Context> for &AcceptAll
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

impl<Context> BorrowingHandler<Context> for &AcceptAll
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

impl<Context> SharedHandler<Context> for &'static AcceptAll where Context: ContextArgs {}

#[cfg(test)]
mod tests {
    use crate::accept_all::AcceptAll;
    use crate::check_handler;

    #[test]
    fn test_accept_all() {
        check_handler::check_basic_dyn_handler_fns(&mut AcceptAll);
        check_handler::check_basic_handler_fns(&mut AcceptAll, Some(&()));
        check_handler::check_basic_borrowing_handler_fns(&mut AcceptAll, Some(&()));
        check_handler::check_basic_shared_handler_fns(&mut AcceptAll);
        check_handler::check_basic_dyn_handler_fns(&mut &AcceptAll);
        check_handler::check_basic_handler_fns(&mut &AcceptAll, Some(&()));
        check_handler::check_basic_borrowing_handler_fns(&mut &AcceptAll, Some(&()));
        check_handler::check_basic_shared_handler_fns(&mut &AcceptAll);
    }
}
