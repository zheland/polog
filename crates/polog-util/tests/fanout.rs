#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]

use polog::{Discard, LevelFilter};
use polog_test_helpers::check_external_handler::test_external;
use polog_test_helpers::tracker::Tracker;
use polog_util::{BaseExternalHandler, BorrowingFanout1, BorrowingFanout4, Fanout1, Fanout4};

const EXTERNAL_AND_GEN: &[&str] = &[
    "max_level_external",
    "generate_span_id",
    "generate_span_id",
    "generate_span_id",
    "try_enable_external",
    "log_external",
    "new_span_external",
    "enter_span_external",
    "record_span_external",
    "link_span_external",
    "exit_span_external",
    "close_span_external",
];

const EXTERNAL: &[&str] = &[
    "max_level_external",
    "try_enable_external",
    "log_external",
    "new_span_external",
    "enter_span_external",
    "record_span_external",
    "link_span_external",
    "exit_span_external",
    "close_span_external",
];

#[test]
fn test_fanout_external() {
    let tracker1 = Tracker::default();
    let tracker2 = Tracker::default();
    let tracker3 = Tracker::default();
    let tracker4 = Tracker::default();

    let mut logger = Fanout1::new((tracker1.clone(),));
    assert_eq!(logger.max_level_external(), LevelFilter::Trace);
    test_external(&mut logger, &());
    assert_eq!(tracker1.take_events(), EXTERNAL_AND_GEN);

    let mut logger = Fanout4::new((
        Fanout4::new((tracker1.clone(), Discard, Discard, Discard)),
        Fanout4::new((Discard, tracker2.clone(), Discard, Discard)),
        Fanout4::new((Discard, Discard, tracker3.clone(), Discard)),
        Fanout4::new((Discard, Discard, Discard, tracker4.clone())),
    ));
    assert_eq!(logger.max_level_external(), LevelFilter::Trace);
    test_external(&mut logger, &());
    assert_eq!(tracker1.take_events(), EXTERNAL_AND_GEN);
    assert_eq!(tracker2.take_events(), EXTERNAL);
    assert_eq!(tracker3.take_events(), EXTERNAL);
    assert_eq!(tracker4.take_events(), EXTERNAL);

    let mut logger = Fanout4::new((
        BorrowingFanout4::new((tracker1.clone(), Discard, Discard, Discard)),
        BorrowingFanout4::new((Discard, tracker2.clone(), Discard, Discard)),
        BorrowingFanout4::new((Discard, Discard, tracker3.clone(), Discard)),
        BorrowingFanout4::new((Discard, Discard, Discard, tracker4.clone())),
    ));
    assert_eq!(logger.max_level_external(), LevelFilter::Trace);
    test_external(&mut logger, &());
    assert_eq!(tracker1.take_events(), EXTERNAL_AND_GEN);
    assert_eq!(tracker2.take_events(), EXTERNAL);
    assert_eq!(tracker3.take_events(), EXTERNAL);
    assert_eq!(tracker4.take_events(), EXTERNAL);
}

#[test]
fn test_borrowing_fanout_external() {
    let tracker1 = Tracker::default();
    let tracker2 = Tracker::default();
    let tracker3 = Tracker::default();
    let tracker4 = Tracker::default();

    let mut logger = BorrowingFanout1::new((tracker1.clone(),));
    assert_eq!(logger.max_level_external(), LevelFilter::Trace);
    test_external(&mut logger, &());
    assert_eq!(tracker1.take_events(), EXTERNAL_AND_GEN);

    let mut logger = BorrowingFanout4::new((
        Fanout4::new((tracker1.clone(), Discard, Discard, Discard)),
        Fanout4::new((Discard, tracker2.clone(), Discard, Discard)),
        Fanout4::new((Discard, Discard, tracker3.clone(), Discard)),
        Fanout4::new((Discard, Discard, Discard, tracker4.clone())),
    ));
    assert_eq!(logger.max_level_external(), LevelFilter::Trace);
    test_external(&mut logger, &());
    assert_eq!(tracker1.take_events(), EXTERNAL_AND_GEN);
    assert_eq!(tracker2.take_events(), EXTERNAL);
    assert_eq!(tracker3.take_events(), EXTERNAL);
    assert_eq!(tracker4.take_events(), EXTERNAL);

    let mut logger = BorrowingFanout4::new((
        BorrowingFanout4::new((tracker1.clone(), Discard, Discard, Discard)),
        BorrowingFanout4::new((Discard, tracker2.clone(), Discard, Discard)),
        BorrowingFanout4::new((Discard, Discard, tracker3.clone(), Discard)),
        BorrowingFanout4::new((Discard, Discard, Discard, tracker4.clone())),
    ));
    assert_eq!(logger.max_level_external(), LevelFilter::Trace);
    test_external(&mut logger, &());
    assert_eq!(tracker1.take_events(), EXTERNAL_AND_GEN);
    assert_eq!(tracker2.take_events(), EXTERNAL);
    assert_eq!(tracker3.take_events(), EXTERNAL);
    assert_eq!(tracker4.take_events(), EXTERNAL);
}
