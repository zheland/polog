#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]

use polog_test_helpers::tracker::Tracker;
use polog_test_helpers::{check_external_handler, check_handler};
use polog_util::bridges::slog_2::SlogBridge;
use polog_util::delegators::slog_2::{ViaSlog, ViaSlogFlags};

#[test]
fn test_slog_global() {
    let tracker = Tracker::default();
    tracker.set_details_enabled(true);
    let slog_logger = SlogBridge::new(tracker.clone()).into_slog_logger();

    let mut logger = ViaSlog::new(slog_logger.clone(), ViaSlogFlags::all());

    polog::info!(&mut logger, "Info"; key = "value");
    assert_eq!(
        tracker.take_details(),
        &[
            "is_enabled_external(Some(Fmt(Info)))",
            "log_external(Some(Fmt(Info)), (key = value), None, ())"
        ]
    );

    let span = polog::info_span!(&mut logger, "Span"; key = "value");
    assert_eq!(
        tracker.take_details(),
        &[
            "is_enabled_external(Some(Fmt(Span)))",
            "log_external(Some(Fmt(Span)), (key = value), None, ())"
        ]
    );
    drop(span);

    slog_2::info!(slog_logger, "slog::info!"; "k" => 11, "l" => 12);
    {
        let slog_logger = slog_logger.new(slog_2::o!("m" => 13, "n" => 14));
        slog_2::info!(slog_logger, "slog::info!"; "o" => 15, "p" => 16);
    }
    assert_eq!(
        tracker.take_details(),
        &[
            "is_enabled_external(Some(Fmt(slog::info!)))",
            "log_external(Some(Fmt(slog::info!)), (l = 12, k = 11), None, ())",
            "is_enabled_external(Some(Fmt(slog::info!)))",
            "log_external(Some(Fmt(slog::info!)), (p = 16, o = 15, n = 14, m = 13), None, ())"
        ]
    );

    check_handler::check_basic_dyn_handler_fns(&mut logger);
    check_handler::check_basic_handler_fns(&mut logger, Some(&()));
    check_handler::check_basic_borrowing_handler_fns(&mut logger, Some(&()));
    check_handler::check_basic_shared_handler_fns(&mut logger);
    check_external_handler::check_external_handler_fns(&mut logger, &());
}
