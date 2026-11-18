#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]

use polog_test_helpers::check_handler;
use polog_test_helpers::tracker::Tracker;
use polog_util::bridges::slog_2::SlogBridge;
use polog_util::delegators::slog_2::{ViaSlog, ViaSlogFlags};

#[test]
fn test_slog_global() {
    let tracker = Tracker::default();
    tracker.set_details_enabled(true);
    let slog_logger = SlogBridge::new(tracker.clone()).into_slog_logger();

    let mut logger = ViaSlog::new(slog_logger, ViaSlogFlags::all());

    polog::info!(&mut logger, "Info"; key = "value");
    assert_eq!(
        tracker.take_details(),
        &[
            "try_enable_external(Some(Fmt(Info)), ())",
            "log_external(Some(Fmt(Info)), (key = value), None, ())"
        ]
    );

    let span = polog::info_span!(&mut logger, "Span"; key = "value");
    assert_eq!(
        tracker.take_details(),
        &[
            "try_enable_external(Some(Fmt(Span)), ())",
            "log_external(Some(Fmt(Span)), (key = value), None, ())"
        ]
    );
    drop(span);

    check_handler::check_basic_dyn_handler_fns(&mut logger);
    check_handler::check_basic_handler_fns(&mut logger, Some(&()));
    check_handler::check_basic_borrowing_handler_fns(&mut logger, Some(&()));
    check_handler::check_basic_shared_handler_fns(&mut logger);
}
