use polog::{
    Arg, BorrowingHandler, Descriptor, DynHandler, Handler, Level, SharedHandler, SpanKind,
    log_descriptor_ref,
};

#[inline]
pub fn check_basic_dyn_handler_fns<H>(handler: &mut H)
where
    H: DynHandler<()>,
{
    const PANIC_DESC: &Descriptor = log_descriptor_ref!(Level::Fatal, "Message");

    let span_id1 = handler.generate_span_id();
    let span_id2 = handler.generate_span_id();
    assert_ne!(span_id1, span_id2);

    handler.flush_blocking();
    handler.flush_on_panic();

    assert_eq!(
        handler.should_panic_on_bug(PANIC_DESC, None),
        cfg!(debug_assertions)
    );
}

#[inline]
pub fn check_basic_handler_fns<H, C>(handler: &mut H, default_enriched_context: Option<&C>)
where
    H: Handler<(), EnrichedContext = C>,
    C: Clone,
{
    const INFO_DESC: &Descriptor = log_descriptor_ref!(
        Level::Info, "Message", keys: &["foo", "bar"]
    );

    let span_id1 = handler.generate_span_id();
    let span_id2 = handler.generate_span_id();
    assert_ne!(span_id1, span_id2);

    if let Some(context) = handler
        .try_enable(INFO_DESC, ())
        .or_else(|| default_enriched_context.cloned())
    {
        handler.log(INFO_DESC, (Arg(123), Arg(456)), None, context);
    }

    if let Some(context) = handler
        .try_enable(INFO_DESC, ())
        .or_else(|| default_enriched_context.cloned())
    {
        handler.enter_span(
            INFO_DESC,
            (Arg(123), Arg(456)),
            None,
            span_id1,
            SpanKind::Nested,
            context,
        );

        if let Some(context) = handler
            .try_enable(INFO_DESC, ())
            .or_else(|| default_enriched_context.cloned())
        {
            handler.log(INFO_DESC, (Arg(123), Arg(456)), Some(span_id1), context);
        }

        handler.enter_poll_span(INFO_DESC, span_id1, ());
        handler.pending_poll_span(INFO_DESC, span_id1, ());
        handler.enter_poll_span(INFO_DESC, span_id1, ());
        handler.ready_poll_span(INFO_DESC, span_id1, ());

        handler.exit_span(INFO_DESC, span_id1, SpanKind::Nested, ());
    }

    if let Some(context) = handler
        .try_enable(INFO_DESC, ())
        .or_else(|| default_enriched_context.cloned())
    {
        handler.enter_span_static(
            INFO_DESC,
            (Arg(123), Arg(456)),
            None,
            span_id2,
            SpanKind::Nested,
            context,
        );

        if let Some(context) = handler
            .try_enable(INFO_DESC, ())
            .or_else(|| default_enriched_context.cloned())
        {
            handler.log_static(INFO_DESC, (Arg(123), Arg(456)), Some(span_id2), context);
        }

        handler.exit_span(INFO_DESC, span_id2, SpanKind::Nested, ());
    }
}

#[inline]
pub fn check_basic_borrowing_handler_fns<H, C>(
    handler: &mut H,
    default_enriched_context: Option<&C>,
) where
    H: BorrowingHandler<(), EnrichedContext = C>,
    C: Clone,
{
    const INFO_DESC: &Descriptor = log_descriptor_ref!(
        Level::Info, "Message", keys: &["foo", "bar"]
    );

    let span_id1 = handler.generate_span_id();

    if let Some(context) = handler
        .try_enable(INFO_DESC, ())
        .or_else(|| default_enriched_context.cloned())
    {
        handler.enter_span_borrowed(
            INFO_DESC,
            &(Arg(123), Arg(456)),
            None,
            span_id1,
            SpanKind::Nested,
            &context,
        );

        if let Some(context) = handler
            .try_enable(INFO_DESC, ())
            .or_else(|| default_enriched_context.cloned())
        {
            handler.log_borrowed(INFO_DESC, &(Arg(123), Arg(456)), Some(span_id1), &context);
        }

        handler.exit_span(INFO_DESC, span_id1, SpanKind::Nested, ());
    }
}

#[inline]
pub fn check_basic_shared_handler_fns<H>(logger: &mut H)
where
    H: SharedHandler<()>,
{
    polog::info!(logger, "First"; key1 = "value1", key2 = 123);
    let mut span = polog::info_span!(logger, "Second"; key1 = ? (123, 234));
    polog::info!(&mut span, "Third"; 1 + 2 + 3);
}
