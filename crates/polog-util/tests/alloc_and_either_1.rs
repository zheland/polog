#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]

use std::borrow::Cow;

use either_1::Either;
use polog::{BaseHandler, Level, LevelFilter};
use polog_test_helpers::tracker::Tracker;
use polog_util::{BaseExternalHandler, ExternalHandler, TypeErasedExternalDescriptor};

#[test]
fn test_external_handler_either() {
    use BaseExternalHandler as Bex;
    use ExternalHandler as Ex;
    type Tr = Either<Tracker, Tracker>;

    const DESC: TypeErasedExternalDescriptor = TypeErasedExternalDescriptor {
        message: Some(Cow::Borrowed("message")),
        level: Level::Info,
        target: Some(Cow::Borrowed("target")),
        module_path: Some(Cow::Borrowed("module_path")),
        file: Some(Cow::Borrowed("file.rs")),
        line: Some(12),
        column: Some(23),
    };

    let tracker = Tracker::default();
    let span_id1 = tracker.generate_span_id();
    let span_id2 = tracker.generate_span_id();
    assert_eq!(tracker.take_events(), &["generate_span_id"; 2]);

    let mut tr1 = Either::Left(tracker.clone());
    let mut tr2 = Either::Right(tracker.clone());

    assert_eq!(<Tr as Bex>::max_level_external(&tr1), LevelFilter::Trace);
    assert_eq!(<Tr as Bex>::max_level_external(&tr2), LevelFilter::Trace);
    assert_eq!(tracker.take_events(), &["max_level_external"; 2]);

    assert!(<Tr as Bex>::is_enabled_external(&tr1, &DESC));
    assert!(<Tr as Bex>::is_enabled_external(&tr2, &DESC));
    assert_eq!(tracker.take_events(), &["is_enabled_external"; 2]);

    <Tr as Ex<_>>::log_external(&mut tr1, &DESC, &(), Some(span_id1), ());
    <Tr as Ex<_>>::log_external(&mut tr2, &DESC, &(), Some(span_id1), ());
    assert_eq!(tracker.take_events(), &["log_external"; 2]);

    <Tr as Ex<_>>::new_span_external(&mut tr1, &DESC, &(), Some(span_id1), span_id2, ());
    <Tr as Ex<_>>::new_span_external(&mut tr2, &DESC, &(), Some(span_id1), span_id2, ());
    assert_eq!(tracker.take_events(), &["new_span_external"; 2]);

    <Tr as Ex<_>>::enter_span_external(&mut tr1, &DESC, &(), Some(span_id1), span_id2, ());
    <Tr as Ex<_>>::enter_span_external(&mut tr2, &DESC, &(), Some(span_id1), span_id2, ());
    assert_eq!(tracker.take_events(), &["enter_span_external"; 2]);

    <Tr as Ex<_>>::exit_span_external(&mut tr1, &DESC, span_id1, ());
    <Tr as Ex<_>>::exit_span_external(&mut tr2, &DESC, span_id1, ());
    assert_eq!(tracker.take_events(), &["exit_span_external"; 2]);

    <Tr as Ex<_>>::close_span_external(&mut tr1, &DESC, span_id1, ());
    <Tr as Ex<_>>::close_span_external(&mut tr2, &DESC, span_id1, ());
    assert_eq!(tracker.take_events(), &["close_span_external"; 2]);

    <Tr as Ex<_>>::record_span_external(&mut tr1, &DESC, &(), span_id1, ());
    <Tr as Ex<_>>::record_span_external(&mut tr2, &DESC, &(), span_id1, ());
    assert_eq!(tracker.take_events(), &["record_span_external"; 2]);

    <Tr as Ex<_>>::link_span_external(&mut tr1, &DESC, span_id1, span_id2, ());
    <Tr as Ex<_>>::link_span_external(&mut tr2, &DESC, span_id1, span_id2, ());
    assert_eq!(tracker.take_events(), &["link_span_external"; 2]);
}
