#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]

use polog::{BaseHandler, Descriptor, Discard, DynHandler, Level, SpanKind, log_descriptor_ref};
use polog_test_helpers::check_handler;

#[test]
fn test_discard() {
    const INFO_DESC: &Descriptor = log_descriptor_ref!(
        Level::Info, "Message", keys: &["foo", "bar"]
    );

    check_handler::check_basic_dyn_handler_fns(&mut Discard);
    check_handler::check_basic_shared_handler_fns(&mut Discard);

    let mut handler = Discard;
    let span_id1 = handler.generate_span_id();
    let span_id2 = handler.generate_span_id();
    assert_ne!(span_id1, span_id2);

    handler.enter_poll_span(INFO_DESC, span_id1, ());
    handler.pending_poll_span(INFO_DESC, span_id1, ());
    handler.enter_poll_span(INFO_DESC, span_id1, ());
    handler.ready_poll_span(INFO_DESC, span_id1, ());

    handler.exit_span(INFO_DESC, span_id1, SpanKind::Nested, ());
}
