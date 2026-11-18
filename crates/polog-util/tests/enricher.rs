#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]

use polog::LevelFilter;
use polog_test_helpers::check_external_handler::check_external_handler_fns;
use polog_test_helpers::tracker::Tracker;
use polog_util::{BaseExternalHandler, Enricher};

#[test]
fn test_enricher_external() {
    let tracker = Tracker::default();
    let mut logger = Enricher::new(tracker.clone(), || 123);

    assert_eq!(logger.max_level_external(), LevelFilter::Trace);
    check_external_handler_fns(&mut logger, &());
    assert_eq!(
        tracker.take_events(),
        &[
            "max_level_external",
            "generate_span_id",
            "generate_span_id",
            "generate_span_id",
            "is_enabled_external",
            "log_external",
            "new_span_external",
            "enter_span_external",
            "record_span_external",
            "link_span_external",
            "exit_span_external",
            "close_span_external"
        ]
    );
}
