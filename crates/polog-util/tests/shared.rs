#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]

use polog::LevelFilter;
use polog_test_helpers::check_external_handler::test_external;
use polog_test_helpers::tracker::Tracker;
use polog_util::filters::MaxLevel;
use polog_util::{BaseExternalHandler, Filter, Shared};

#[test]
fn test_shared_external() {
    let tracker = Tracker::default();
    let mut logger = Shared::new(Filter::new(tracker.clone(), MaxLevel(LevelFilter::Debug)));

    assert_eq!(logger.max_level_external(), LevelFilter::Debug);
    test_external(&mut logger, &());
    assert_eq!(
        tracker.take_events(),
        &[
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
        ]
    );
}
