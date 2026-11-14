use polog::{BorrowingHandler, Descriptor, Level, SpanKind, log_descriptor_ref};

/// Builds a [`NonZero`] value at compile time.
#[macro_export]
macro_rules! non_zero {
    ($value:expr) => {
        const { ::core::num::NonZero::new($value).unwrap() }
    };
}

#[expect(clippy::arithmetic_side_effects, reason = "okay in tests")]
pub fn eq_with_wildcards(mut given: &str, expected: &str) -> bool {
    if given.len() != expected.len() {
        return false;
    }

    for expected in expected.split('?') {
        if &given[..expected.len()] != expected {
            return false;
        }
        given = given.get(expected.len() + 1..).unwrap_or("");
    }
    true
}

#[track_caller]
pub fn assert_eq_with_wildcards(given: &str, expected: &str) {
    assert!(
        eq_with_wildcards(given, expected),
        "given:\n{given}, expected:\n{expected}"
    );
}

pub fn emit_handler_events<L>(logger: &mut L)
where
    L: BorrowingHandler<(), EnrichedContext = ()>,
{
    const INFO_DESC: &Descriptor = log_descriptor_ref!(Level::Info, "Message");

    let span_id = logger.generate_span_id();

    logger.log(INFO_DESC, (), None, ());
    logger.log_static(INFO_DESC, (), None, ());
    logger.log_borrowed(INFO_DESC, &(), None, &());
    logger.log(INFO_DESC, (), None, ());
    logger.enter_span(INFO_DESC, (), None, span_id, SpanKind::Nested, ());
    logger.exit_span(INFO_DESC, span_id, SpanKind::Nested, ());
    logger.enter_span_static(INFO_DESC, (), None, span_id, SpanKind::Detached, ());
    logger.exit_span(INFO_DESC, span_id, SpanKind::Detached, ());
    logger.enter_span_borrowed(INFO_DESC, &(), None, span_id, SpanKind::Nested, &());
    logger.exit_span(INFO_DESC, span_id, SpanKind::Nested, ());

    logger.enter_span(INFO_DESC, (), None, span_id, SpanKind::Nested, ());
    logger.enter_poll_span(INFO_DESC, span_id, ());
    logger.pending_poll_span(INFO_DESC, span_id, ());
    logger.enter_poll_span(INFO_DESC, span_id, ());
    logger.ready_poll_span(INFO_DESC, span_id, ());
    logger.exit_span(INFO_DESC, span_id, SpanKind::Nested, ());
}

#[test]
fn test_eq_with_wildcards() {
    assert!(eq_with_wildcards("abc", "a?c"));
    assert!(!eq_with_wildcards("abbc", "a?c"));
    assert!(!eq_with_wildcards("ac", "a?c"));

    assert!(eq_with_wildcards("aa-bb-cc-dd-ee", "aa?bb?cc?dd?ee"));
    assert!(!eq_with_wildcards("aa-b-ccc-dd-ee", "aa?bb?cc?dd?ee"));
    assert!(!eq_with_wildcards("aa-bbb-c-dd-ee", "aa?bb?cc?dd?ee"));
}
