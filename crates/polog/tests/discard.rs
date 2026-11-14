#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]

use polog::Discard;
use polog_test_helpers::check_handler;

#[test]
fn test_discard() {
    check_handler::check_basic_dyn_handler_fns(&mut Discard);
    check_handler::check_basic_handler_fns(&mut Discard, Some(&()));
    check_handler::check_basic_borrowing_handler_fns(&mut Discard, Some(&()));
    check_handler::check_basic_shared_handler_fns(&mut Discard);
}
