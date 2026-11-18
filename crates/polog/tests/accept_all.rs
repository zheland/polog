#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]

use polog_test_helpers::accept_all::AcceptAll;
use polog_test_helpers::check_handler;

#[expect(clippy::mut_mut, reason = "explicitly tested")]
#[test]
fn test_accept_all() {
    check_handler::check_basic_dyn_handler_fns(&mut AcceptAll);
    check_handler::check_basic_handler_fns(&mut AcceptAll, Some(&()));
    check_handler::check_basic_borrowing_handler_fns(&mut AcceptAll, Some(&()));
    check_handler::check_basic_shared_handler_fns(&mut AcceptAll);

    check_handler::check_basic_dyn_handler_fns(&mut &AcceptAll);
    check_handler::check_basic_handler_fns(&mut &AcceptAll, Some(&()));
    check_handler::check_basic_borrowing_handler_fns(&mut &AcceptAll, Some(&()));
    check_handler::check_basic_shared_handler_fns(&mut &AcceptAll);

    check_handler::check_basic_dyn_handler_fns(&mut &mut AcceptAll);
    check_handler::check_basic_handler_fns(&mut &mut AcceptAll, Some(&()));
    check_handler::check_basic_borrowing_handler_fns(&mut &mut AcceptAll, Some(&()));
}
