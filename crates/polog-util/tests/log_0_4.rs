#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]
#![expect(clippy::unwrap_used, reason = "okay in tests")]

use polog_test_helpers::check_handler;
use polog_test_helpers::tracker::Tracker;
use polog_util::bridges::log_0_4::LogBridge;
use polog_util::delegators::log_0_4::{ViaLog, ViaLogFlags};

#[test]
fn test_log_global() {
    let tracker = Tracker::default();
    tracker.set_details_enabled(true);
    LogBridge::new(tracker.clone())
        .try_install_global()
        .unwrap();

    let mut logger = ViaLog::new(log_0_4::logger(), ViaLogFlags::all());

    polog::info!(&mut logger, "Info"; key = "value");
    assert_eq!(
        tracker.take_details(),
        &[
            // `LogBridge::try_install_global`.
            "max_level_external",
            // `ViaLog::try_enable`.
            "try_enable_external(None, ())",
            // `LogBridge::log`.
            "try_enable_external(Some(Fmt(Info)), ())",
            "log_external(Some(Fmt(Info)), (key = value), None, ())",
        ]
    );

    let span = polog::info_span!(&mut logger, "Span"; foo = "bar");
    assert_eq!(
        tracker.take_details(),
        &[
            "try_enable_external(None, ())",
            "try_enable_external(Some(Fmt(Span)), ())",
            "log_external(Some(Fmt(Span)), (foo = bar), None, ())"
        ]
    );
    drop(span);

    check_handler::check_basic_dyn_handler_fns(&mut logger);
    check_handler::check_basic_handler_fns(&mut logger, Some(&()));
    check_handler::check_basic_borrowing_handler_fns(&mut logger, Some(&()));
    check_handler::check_basic_shared_handler_fns(&mut logger);
}
