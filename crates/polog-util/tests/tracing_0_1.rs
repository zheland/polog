#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]
#![expect(clippy::unwrap_used, reason = "okay in tests")]

use polog_test_helpers::check_handler;
use polog_test_helpers::tracker::Tracker;
use polog_util::bridges::tracing_0_1::TracingBridge;
use polog_util::delegators::tracing_0_1::ViaTracing;

#[expect(clippy::too_many_lines, reason = "better to keep together")]
#[test]
fn test_tracing_global() {
    let tracker = Tracker::default();
    tracker.set_details_enabled(true);
    TracingBridge::new(tracker.clone())
        .try_install_global()
        .unwrap();

    let mut logger =
        tracing_0_1::dispatcher::get_default(|dispatch| ViaTracing::new(dispatch.clone()));

    for _ in 0..2 {
        polog::info!(&mut logger, "Info"; key = "value");
    }
    assert_eq!(
        tracker.take_details(),
        &[
            // `TracingBridge::register_callsite`.
            "is_enabled_external(Some(Str(Static(\"Info\"))))",
            // The first `TracingBridge::event_enabled`.
            "is_enabled_external(Some(Str(Static(\"Info\"))))",
            "log_external(Some(Str(Static(\"Info\"))), (key = value), None, ())",
            // The second `TracingBridge::event_enabled`.
            "is_enabled_external(Some(Str(Static(\"Info\"))))",
            "log_external(Some(Str(Static(\"Info\"))), (key = value), None, ())"
        ]
    );

    for _ in 0..2 {
        let span = polog::info_span!(&mut logger, "Span"; key = "value");
        drop(span);
    }
    assert_eq!(
        tracker.take_details(),
        &[
            "is_enabled_external(Some(Str(Static(\"Span\"))))",
            "new_span_external(Some(Str(Static(\"Span\"))), (key = value), None, SpanId(1), ())",
            "enter_span_external(Some(Str(Static(\"Span\"))), (), Some(SpanId(1)), SpanId(2), ())",
            "exit_span_external(Some(Str(Static(\"Span\"))), SpanId(2), ())",
            "close_span_external(Some(Str(Static(\"Span\"))), SpanId(1), ())",
            "new_span_external(Some(Str(Static(\"Span\"))), (key = value), None, SpanId(3), ())",
            "enter_span_external(Some(Str(Static(\"Span\"))), (), Some(SpanId(3)), SpanId(4), ())",
            "exit_span_external(Some(Str(Static(\"Span\"))), SpanId(4), ())",
            "close_span_external(Some(Str(Static(\"Span\"))), SpanId(3), ())"
        ]
    );

    tracing_0_1::event!(tracing_0_1::Level::INFO, "`tracing::event!`");
    let span = tracing_0_1::info_span!("`tracing::info_span!` Span A", e = 5, f = 6);
    {
        let _guard = span.enter();
        tracing_0_1::event!(tracing_0_1::Level::INFO, "`tracing::event!` In A");
    }
    assert_eq!(
        tracker.take_details(),
        &[
            "is_enabled_external(Some(Str(Static(\"event \
             crates/polog-util/tests/tracing_0_1.rs:58\"))))",
            "is_enabled_external(Some(Str(Static(\"event \
             crates/polog-util/tests/tracing_0_1.rs:58\"))))",
            "log_external(Some(Str(Static(\"event crates/polog-util/tests/tracing_0_1.rs:58\"))), \
             (`tracing::event!`), None, ())",
            "is_enabled_external(Some(Str(Static(\"`tracing::info_span!` Span A\"))))",
            "new_span_external(Some(Str(Static(\"`tracing::info_span!` Span A\"))), (e = 5, f = \
             6), None, SpanId(5), ())",
            "enter_span_external(Some(Str(Static(\"`tracing::info_span!` Span A\"))), (), \
             Some(SpanId(5)), SpanId(6), ())",
            "is_enabled_external(Some(Str(Static(\"event \
             crates/polog-util/tests/tracing_0_1.rs:62\"))))",
            "is_enabled_external(Some(Str(Static(\"event \
             crates/polog-util/tests/tracing_0_1.rs:62\"))))",
            "log_external(Some(Str(Static(\"event crates/polog-util/tests/tracing_0_1.rs:62\"))), \
             (`tracing::event!` In A), Some(SpanId(6)), ())",
            "exit_span_external(Some(Str(Static(\"`tracing::info_span!` Span A\"))), SpanId(6), \
             ())"
        ]
    );

    tracing_0_1::event!(
        tracing_0_1::Level::INFO,
        u64 = 123_u64,
        i64 = 234_i64,
        u128 = 345_u128,
        i128 = 456_i128,
        f64 = 123.234_f64,
        bool = true,
        str = "str",
        "`tracing::event!`"
    );
    assert_eq!(
        tracker.take_details(),
        &[
            "is_enabled_external(Some(Str(Static(\"event \
             crates/polog-util/tests/tracing_0_1.rs:89\"))))",
            "is_enabled_external(Some(Str(Static(\"event \
             crates/polog-util/tests/tracing_0_1.rs:89\"))))",
            "log_external(Some(Str(Static(\"event crates/polog-util/tests/tracing_0_1.rs:89\"))), \
             (`tracing::event!`, u64 = 123, i64 = 234, u128 = 345, i128 = 456, f64 = 123.234, \
             bool = true, str = str), None, ())"
        ]
    );
    let span = tracing_0_1::info_span!(
        "`tracing::event!`",
        u64 = 123_u64,
        i64 = 234_i64,
        u128 = 345_u128,
        i128 = 456_i128,
        f64 = 123.234_f64,
        bool = true,
        str = "str",
    );
    drop(span);
    assert_eq!(
        tracker.take_details(),
        &[
            "is_enabled_external(Some(Str(Static(\"`tracing::event!`\"))))",
            "new_span_external(Some(Str(Static(\"`tracing::event!`\"))), (u64 = 123, i64 = 234, \
             u128 = 345, i128 = 456, f64 = 123.234, bool = true, str = str), None, SpanId(7), ())",
            "close_span_external(Some(Str(Static(\"`tracing::event!`\"))), SpanId(7), ())"
        ]
    );

    check_handler::check_basic_dyn_handler_fns(&mut logger);
    check_handler::check_basic_handler_fns(&mut logger, Some(&()));
    check_handler::check_basic_borrowing_handler_fns(&mut logger, Some(&()));
    check_handler::check_basic_shared_handler_fns(&mut logger);
}
