#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]
#![expect(clippy::unwrap_used, reason = "okay in tests")]

use polog_test_helpers::check_handler;
use polog_test_helpers::tracker::Tracker;
use polog_util::bridges::tracing_0_1::TracingBridge;
use polog_util::delegators::tracing_0_1::ViaTracing;

#[test]
fn test_tracing_global() {
    let tracker = Tracker::default();
    tracker.set_details_enabled(true);
    TracingBridge::new(tracker.clone())
        .try_install_global()
        .unwrap();

    let mut logger =
        tracing_0_1::dispatcher::get_default(|dispatch| ViaTracing::new(dispatch.clone()));

    polog::info!(&mut logger, "Info"; key = "value");
    assert_eq!(
        tracker.take_details(),
        &[
            // `TracingBridge::register_callsite`.
            "try_enable_external(Some(Str(Static(\"Info\"))), ())",
            // `TracingBridge::event_enabled`.
            "try_enable_external(Some(Str(Static(\"Info\"))), ())",
            // `TracingBridge::event` (necessary for context).
            "try_enable_external(Some(Str(Static(\"Info\"))), ())",
            "log_external(Some(Str(Static(\"Info\"))), (key = value), None, ())"
        ]
    );

    let span = polog::info_span!(&mut logger, "Span"; key = "value");
    assert_eq!(
        tracker.take_details(),
        &[
            "try_enable_external(Some(Str(Static(\"Span\"))), ())",
            "try_enable_external(Some(Str(Static(\"Span\"))), ())",
            "try_enable_external(Some(Str(Static(\"Span\"))), ())",
            "new_span_external(Some(Str(Static(\"Span\"))), (key = value), None, SpanId(1), ())",
            "try_enable_external(Some(Str(Static(\"Span\"))), ())",
            "enter_span_external(Some(Str(Static(\"Span\"))), (), Some(SpanId(1)), SpanId(2), ())"
        ]
    );
    drop(span);

    check_handler::check_basic_dyn_handler_fns(&mut logger);
    check_handler::check_basic_handler_fns(&mut logger, Some(&()));
    check_handler::check_basic_borrowing_handler_fns(&mut logger, Some(&()));
    check_handler::check_basic_shared_handler_fns(&mut logger);
}
