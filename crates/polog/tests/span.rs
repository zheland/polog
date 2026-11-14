#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]

use polog::{
    BaseHandler, BorrowingSpan, DeferredSpan, Descriptor, DetachedLogger, Level, Logger, LoggerExt,
    OwningSpan, SpanKind, log_descriptor_ref,
};
use polog_test_helpers::tracker::Tracker;

#[test]
fn test_borrowing_span() {
    const DESC: &Descriptor = log_descriptor_ref!(Level::Info, "Message");

    let mut logger = Tracker::default();
    let span_id = logger.generate_span_id();
    assert_eq!(logger.take_events(), ["generate_span_id"]);

    let span = BorrowingSpan::new(&mut logger, Some(DESC), Some(span_id), SpanKind::Nested);
    assert_eq!(span.handler_ref().take_events(), [""; 0]);
    assert_eq!(span.descriptor(), Some(DESC));
    assert_eq!(span.id(), Some(span_id));
    assert_eq!(span.kind(), SpanKind::Nested);
    drop(span);
    assert_eq!(logger.take_events(), ["exit_span(nested)"]);

    // `drop_into_handler` preserves `exit_span` event on drop.
    let span = BorrowingSpan::new(&mut logger, Some(DESC), Some(span_id), SpanKind::Nested);
    assert_eq!(span.handler_ref().take_events(), [""; 0]);
    let handler = span.drop_into_handler();
    assert_eq!(&raw const *handler, &raw const logger);
    assert_eq!(logger.take_events(), ["exit_span(nested)"]);

    // `exit_span` event suppressed on drop if the `parent_id` is `None`.
    let span = BorrowingSpan::new(&mut logger, Some(DESC), None, SpanKind::Nested);
    assert_eq!(span.handler_ref().take_events(), [""; 0]);
    let _handler = span.drop_into_handler();
    assert_eq!(logger.take_events(), [""; 0]);

    // Forget suppresses `exit_span` event on drop.
    let mut span = BorrowingSpan::new(&mut logger, Some(DESC), Some(span_id), SpanKind::Nested);
    assert_eq!(span.handler_ref().take_events(), [""; 0]);
    assert_eq!(span.child_borrowing_span_kind(), SpanKind::Nested);
    span.forget();
    assert_eq!(span.child_borrowing_span_kind(), SpanKind::Nested);
    assert_eq!(span.handler_ref().take_events(), [""; 0]);
    drop(span);
    assert_eq!(logger.take_events(), [""; 0]);

    // Transparent span does not emit `exit_span` event on drop.
    let span = BorrowingSpan::transparent(&mut logger, Some(span_id), SpanKind::Nested);
    assert_eq!(span.handler_ref().take_events(), [""; 0]);
    drop(span);
    assert_eq!(logger.take_events(), [""; 0]);

    // Transparent span with `drop_into_handler` does not emit `exit_span`
    // event.
    let span = BorrowingSpan::transparent(&mut logger, Some(span_id), SpanKind::Nested);
    assert_eq!(span.handler_ref().take_events(), [""; 0]);
    let _handler = span.drop_into_handler();
    assert_eq!(logger.take_events(), [""; 0]);
}

#[test]
fn test_owning_span() {
    const DESC: &Descriptor = log_descriptor_ref!(Level::Info, "Message");

    let mut logger = Tracker::default();
    let span_id = logger.generate_span_id();
    assert_eq!(logger.take_events(), ["generate_span_id"]);

    let span = OwningSpan::new(logger.clone(), Some(DESC), Some(span_id));
    assert_eq!(logger.take_events(), [""; 0]);
    assert_eq!(span.descriptor(), Some(DESC));
    assert_eq!(span.id(), Some(span_id));
    drop(span);
    assert_eq!(logger.take_events(), ["exit_span(detached)"]);

    // `drop_into_handler` preserves `exit_span` event on drop.
    let span = OwningSpan::new(logger.clone(), Some(DESC), Some(span_id));
    assert_eq!(logger.take_events(), [""; 0]);
    let handler = span.drop_into_handler();
    // Handlers have different addresses.
    assert_ne!(&raw const handler, &raw const *logger.handler_mut());
    assert_eq!(logger.take_events(), ["exit_span(detached)"]);

    // `exit_span` event suppressed on drop if the `parent_id` is `None`.
    let span = OwningSpan::new(logger.clone(), Some(DESC), None);
    assert_eq!(logger.take_events(), [""; 0]);
    let _handler = span.drop_into_handler();
    assert_eq!(logger.take_events(), [""; 0]);

    // Forget suppresses `exit_span` event on drop.
    let mut span = OwningSpan::new(logger.clone(), Some(DESC), Some(span_id));
    assert_eq!(logger.take_events(), [""; 0]);
    assert_eq!(span.child_borrowing_span_kind(), SpanKind::Nested);
    span.forget();
    assert_eq!(span.child_borrowing_span_kind(), SpanKind::Detached);
    assert_eq!(logger.take_events(), [""; 0]);
    drop(span);
    assert_eq!(logger.take_events(), [""; 0]);

    // Transparent span does not emit `exit_span` event on drop.
    let span = OwningSpan::transparent(logger.clone(), Some(span_id));
    assert_eq!(logger.take_events(), [""; 0]);
    drop(span);
    assert_eq!(logger.take_events(), [""; 0]);

    // Transparent span with `drop_into_handler` does not emit `exit_span`
    // event.
    let span = OwningSpan::transparent(logger.clone(), Some(span_id));
    assert_eq!(logger.take_events(), [""; 0]);
    let _handler = span.drop_into_handler();
    assert_eq!(logger.take_events(), [""; 0]);

    // Erased span emits `exit_span` event on drop.
    let span = OwningSpan::new(logger.clone(), Some(DESC), Some(span_id));
    assert_eq!(logger.take_events(), [""; 0]);
    let span = span.into_erased();
    assert_eq!(logger.take_events(), [""; 0]);
    drop(span);
    assert_eq!(logger.take_events(), ["exit_span(detached)"]);

    // Debuggable erased span emits `exit_span` event on drop.
    let span = OwningSpan::new(logger.clone(), Some(DESC), Some(span_id));
    assert_eq!(logger.take_events(), [""; 0]);
    let span = span.into_debuggable_erased();
    assert_eq!(logger.take_events(), [""; 0]);
    drop(span);
    assert_eq!(logger.take_events(), ["exit_span(detached)"]);
}

#[test]
fn test_deferred_span() {
    const DESC: &Descriptor = log_descriptor_ref!(Level::Info, "Message");

    let mut logger = Tracker::default();
    let span_id = logger.generate_span_id();
    assert_eq!(logger.take_events(), ["generate_span_id"]);

    let span = DeferredSpan::new(logger.clone(), DESC, || (), Some(span_id));
    assert_eq!(logger.take_events(), [""; 0]);
    assert_eq!(span.descriptor(), Some(DESC));
    assert_eq!(span.parent_id(), Some(span_id));
    let span = span.enter();
    // Span parent id is not equal to span id.
    assert_ne!(span.id(), Some(span_id));
    assert_eq!(
        logger.take_events(),
        ["try_enable", "generate_span_id", "enter_span(detached)"]
    );
    drop(span);
    assert_eq!(logger.take_events(), ["exit_span(detached)"]);

    // Forget makes span transparent.
    let mut span = DeferredSpan::new(logger.clone(), DESC, || (), Some(span_id));
    assert_eq!(logger.take_events(), [""; 0]);
    span.forget();
    assert_eq!(span.descriptor(), None);
    assert_eq!(logger.take_events(), [""; 0]);
    let span = span.enter();
    // Transparent span uses parent span id.
    assert_eq!(span.id(), Some(span_id));
    assert_eq!(logger.take_events(), [""; 0]);
    drop(span);
    assert_eq!(logger.take_events(), [""; 0]);

    let span = DeferredSpan::new(logger.clone(), DESC, || (), Some(span_id));
    let handler = span.into_handler();
    // Handlers have different addresses.
    assert_ne!(&raw const handler, &raw const *logger.handler_mut());
    assert_eq!(logger.take_events(), [""; 0]);
}

#[test]
fn test_detached_logger() {
    let logger = Tracker::default();
    let span_id = logger.generate_span_id();
    assert_eq!(logger.take_events(), ["generate_span_id"]);

    let mut span = DetachedLogger::new(logger.clone(), Some(span_id));
    assert_eq!(span.parent_id(), Some(span_id));
    assert_eq!(logger.take_events(), [""; 0]);
    polog::info!(&mut span, "Message");
    assert_eq!(logger.take_events(), ["try_enable", "log"]);

    let mut span = DetachedLogger::transparent(logger.clone());
    assert_eq!(span.parent_id(), None);
    assert_eq!(logger.take_events(), [""; 0]);
    polog::info!(&mut span, "Message");
    assert_eq!(logger.take_events(), ["try_enable", "log"]);

    let span = DetachedLogger::new(logger.clone(), Some(span_id));
    let handler = span.into_handler();
    // Handlers have different addresses.
    assert_ne!(&raw const handler, &raw const logger);
}

#[test]
fn test_nested_span_kind() {
    let mut logger = Tracker::default();

    let span = polog::info_span!(&mut logger, "Message");
    assert_eq!(
        span.handler_ref().take_events(),
        ["try_enable", "generate_span_id", "enter_span(nested)"]
    );
    assert_eq!(span.kind(), SpanKind::Nested);
    drop(span);
    assert_eq!(logger.take_events(), ["exit_span(nested)"]);

    let mut span1 = LoggerExt::transparent_borrowing_span(&mut logger);
    let mut span2 = LoggerExt::transparent_borrowing_span(&mut span1);
    let mut span3 = LoggerExt::transparent_borrowing_span(&mut span2);
    let span4 = polog::info_span!(&mut span3, "Message");
    assert_eq!(
        span4.handler_ref().take_events(),
        ["try_enable", "generate_span_id", "enter_span(nested)"]
    );
    assert_eq!(span4.kind(), SpanKind::Nested);
    drop(span4);
    drop(span3);
    drop(span2);
    drop(span1);
    assert_eq!(logger.take_events(), ["exit_span(nested)"]);
}

#[test]
fn test_detached_span_kind() {
    let logger = Tracker::default();

    let mut span = logger.detach();
    let span = polog::info_span!(&mut span, "Message");
    assert_eq!(
        span.handler_ref().take_events(),
        ["try_enable", "generate_span_id", "enter_span(detached)"]
    );
    assert_eq!(span.kind(), SpanKind::Detached);
    drop(span);
    assert_eq!(logger.take_events(), ["exit_span(detached)"]);

    let mut span = logger.detach();
    let mut span = LoggerExt::transparent_borrowing_span(&mut span);
    let mut span = LoggerExt::transparent_borrowing_span(&mut span);
    let mut span = LoggerExt::transparent_borrowing_span(&mut span);
    let span = polog::info_span!(&mut span, "Message");
    assert_eq!(
        span.handler_ref().take_events(),
        ["try_enable", "generate_span_id", "enter_span(detached)"]
    );
    assert_eq!(span.kind(), SpanKind::Detached);
    drop(span);
    assert_eq!(logger.take_events(), ["exit_span(detached)"]);

    let span = logger.detach();
    let span = LoggerExt::transparent_owning_span(&span);
    let span = LoggerExt::transparent_owning_span(&span);
    let span = LoggerExt::transparent_owning_span(&span);
    let span = polog::owning_info_span!(&span, "Message");
    assert_eq!(
        span.handler_ref().take_events(),
        ["try_enable", "generate_span_id", "enter_span(detached)"]
    );
    drop(span);
    assert_eq!(logger.take_events(), ["exit_span(detached)"]);
}
