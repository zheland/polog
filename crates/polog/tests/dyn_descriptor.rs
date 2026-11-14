#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]

use polog::{Arg, Descriptor, Level, Logger, log_descriptor_ref};
use polog_test_helpers::accept_all::AcceptAll;

fn choose_descriptor_dynamically<L>(logger: &mut L, kind: usize)
where
    L: Logger,
{
    static DESCRIPTOR1: &Descriptor =
        log_descriptor_ref!(Level::Error, "Error 1", keys: &["foo", "bar"]);
    static DESCRIPTOR2: &Descriptor =
        log_descriptor_ref!(Level::Warn, "Warn 1", keys: &["qwe", "asd"]);

    let descriptor = if kind == 0 { DESCRIPTOR1 } else { DESCRIPTOR2 };

    let args = (Arg(1), Arg(2));

    polog::log!(logger, descriptor_ref: descriptor; :r args);
}

#[test]
fn test_choose_descriptor_dynamically() {
    let mut logger = AcceptAll;
    choose_descriptor_dynamically(&mut logger, 0);
    choose_descriptor_dynamically(&mut logger, 1);
}
