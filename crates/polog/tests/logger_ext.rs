#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]

use polog::{Descriptor, Level, Logger, LoggerExt, log_descriptor_ref};
use polog_test_helpers::tracker::Tracker;

#[test]
fn test_logger_ext() {
    const INFO_DESC: &Descriptor = log_descriptor_ref!(
        Level::Info, "Message", keys: &["foo", "bar"]
    );

    let mut tracker = Tracker::default();
    let mut logger = polog::info_span!(&mut tracker, "Message");
    assert_eq!(
        logger.handler_ref().take_events(),
        ["try_enable", "generate_span_id", "enter_span(nested)"]
    );

    assert_eq!(LoggerExt::try_enable(&logger, INFO_DESC), Some(()));
    assert_eq!(logger.handler_ref().take_events(), ["try_enable"]);

    LoggerExt::flush_blocking(&mut logger);
    assert_eq!(logger.handler_ref().take_events(), ["flush_blocking"]);

    LoggerExt::flush_on_panic(&mut logger);
    assert_eq!(logger.handler_ref().take_events(), ["flush_on_panic"]);

    LoggerExt::log(&mut logger, INFO_DESC, (), ());
    assert_eq!(logger.handler_ref().take_events(), ["log"]);

    let span = LoggerExt::borrowing_span(&mut logger, INFO_DESC, (), ());
    drop(span);
    assert_eq!(
        logger.handler_ref().take_events(),
        [
            "generate_span_id",
            "enter_span(nested)",
            "exit_span(nested)"
        ]
    );

    let span = LoggerExt::transparent_borrowing_span(&mut logger);
    drop(span);
    assert_eq!(logger.handler_ref().take_events(), [""; 0]);

    let span = LoggerExt::owning_span(&logger, INFO_DESC, (), ());
    assert_eq!(
        logger.handler_ref().take_events(),
        ["generate_span_id", "enter_span(detached)"]
    );
    drop(span);
    assert_eq!(logger.handler_ref().take_events(), ["exit_span(detached)"]);

    let span = LoggerExt::transparent_owning_span(&logger);
    assert_eq!(logger.handler_ref().take_events(), [""; 0]);
    drop(span);
    assert_eq!(logger.handler_ref().take_events(), [""; 0]);

    let span = LoggerExt::deferred_span(&logger, INFO_DESC, || ());
    assert_eq!(logger.handler_ref().take_events(), [""; 0]);
    let span = span.enter();
    assert_eq!(
        logger.handler_ref().take_events(),
        ["try_enable", "generate_span_id", "enter_span(detached)"]
    );
    drop(span);
    assert_eq!(logger.handler_ref().take_events(), ["exit_span(detached)"]);

    let detached = LoggerExt::detach(&logger);
    assert_eq!(logger.handler_ref().take_events(), [""; 0]);
    drop(detached);
    assert_eq!(logger.handler_ref().take_events(), [""; 0]);

    LoggerExt::panic_if_critical(&mut logger, INFO_DESC);
    assert_eq!(logger.handler_ref().take_events(), [""; 0]);
}
