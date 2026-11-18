#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]

use either_1::Either;
use polog::{
    BaseHandler, BorrowingHandler, Descriptor, Discard, DynHandler, Handler, Level, SpanKind,
    log_descriptor_ref,
};
use polog_test_helpers::check_handler;
use polog_test_helpers::panic::catch_unwind_on_thread;
use polog_test_helpers::tracker::Tracker;

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

    let mut logger: Either<Discard, Discard> = Either::Left(Discard);

    polog::info!(&mut logger, "Message");
    let span = polog::info_span!(&mut logger, "Span");
    drop(span);

    let mut logger: Either<Tracker, Discard> = Either::Left(tracker.clone());

    polog::info!(&mut logger, "Message");
    let span = polog::info_span!(&mut logger, "Span");
    drop(span);
    assert_eq!(tracker.take_events(), BASIC_AND_GEN);

    polog::trace!(&mut logger, "Message");
    let span = polog::trace_span!(&mut logger, "Span");
    drop(span);
    assert_eq!(tracker.take_events(), BASIC_AND_GEN);
}

#[test]
fn test_either_low_level() {
    const TRACE_DESC: &Descriptor = log_descriptor_ref!(Level::Trace, "Message");

    let tracker = Tracker::default();
    let logger1: Either<Tracker, Tracker> = Either::Left(tracker.clone());
    let logger2: Either<Tracker, Tracker> = Either::Right(tracker.clone());

    for (mut logger, context) in [(logger1, Either::Left(())), (logger2, Either::Right(()))] {
        let span_id = logger.generate_span_id();
        assert_eq!(tracker.take_events(), ["generate_span_id"]);

        logger.log(TRACE_DESC, (), None, context);
        logger.log_static(TRACE_DESC, (), None, context);
        logger.log_borrowed(TRACE_DESC, &(), None, &context);
        logger.log(TRACE_DESC, (), None, context);
        logger.enter_span(TRACE_DESC, (), None, span_id, SpanKind::Nested, context);
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        logger.enter_span_static(TRACE_DESC, (), None, span_id, SpanKind::Nested, context);
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        logger.enter_span_borrowed(TRACE_DESC, &(), None, span_id, SpanKind::Nested, &context);
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

        logger.enter_span(TRACE_DESC, (), None, span_id, SpanKind::Nested, context);
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

        check_handler::check_basic_dyn_handler_fns(&mut logger);
        check_handler::check_basic_handler_fns(&mut logger, Some(&context));
        check_handler::check_basic_borrowing_handler_fns(&mut logger, Some(&context));
        check_handler::check_basic_shared_handler_fns(&mut logger);
        drop(tracker.take_events());
    }
}

#[test]
fn test_either_low_level_noop_misused_context() {
    const TRACE_DESC: &Descriptor = log_descriptor_ref!(Level::Trace, "Message");

    let tracker = Tracker::default();
    let logger1: Either<Tracker, Tracker> = Either::Left(tracker.clone());
    let logger2: Either<Tracker, Tracker> = Either::Right(tracker.clone());

    for (mut logger, context) in [(logger1, Either::Right(())), (logger2, Either::Left(()))] {
        let span_id = logger.generate_span_id();
        assert_eq!(tracker.take_events(), ["generate_span_id"]);

        logger.log(TRACE_DESC, (), None, context);
        logger.log_static(TRACE_DESC, (), None, context);
        logger.log_borrowed(TRACE_DESC, &(), None, &context);
        logger.log(TRACE_DESC, (), None, context);
        logger.enter_span(TRACE_DESC, (), None, span_id, SpanKind::Nested, context);
        logger.enter_span_static(TRACE_DESC, (), None, span_id, SpanKind::Nested, context);
        logger.enter_span_borrowed(TRACE_DESC, &(), None, span_id, SpanKind::Nested, &context);
        assert_eq!(tracker.take_events(), [""; 0]);
    }
}

#[test]
fn test_either_flush_and_panic_predicate() {
    const BUG_DESC: &Descriptor = log_descriptor_ref!(Level::Bug, "Message");

    let tracker = Tracker::default();
    let logger1: Either<Tracker, Tracker> = Either::Left(tracker.clone());
    let logger2: Either<Tracker, Tracker> = Either::Right(tracker.clone());

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
    let logger1: Either<Tracker, Tracker> = Either::Left(tracker.clone());
    let logger2: Either<Tracker, Tracker> = Either::Right(tracker.clone());

    for logger in [logger1, logger2] {
        let _unused = catch_unwind_on_thread(&logger, |mut logger| {
            logger.panic(FATAL_DESC, None);
        })
        .unwrap_err();
        assert_eq!(tracker.take_events(), ["panic"]);
    }
}
