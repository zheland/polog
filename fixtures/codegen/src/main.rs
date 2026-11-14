#![expect(missing_docs, reason = "okay in fixture")]
#![allow(unused_crate_dependencies, reason = "okay in fixture")]
#![expect(clippy::allow_attributes, reason = "a lot of codegen")]
#![expect(clippy::arithmetic_side_effects, reason = "okay in fixture")]
#![expect(clippy::unwrap_used, reason = "okay in fixture")]

use core::cmp::Ordering;

mod backends;
mod common;
mod work;

#[allow(unused_mut, reason = "codegen")]
fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut root = common::DetachedLogger::new();

    let mut span = nested_span!(
        &mut root,
        "Compute datacenters cost using sync computations"
    );
    let sync_cost = work::datacenters::compute_cost_sync(&mut span);
    drop(span);

    let span = into_detached!(&root);
    let async_cost = rt.block_on(work::datacenters::compute_cost_async(span));

    drop(root);

    println!("Sync  cost: ${sync_cost}");
    println!("Async cost: ${async_cost}");
    let winner = match sync_cost.cmp(&async_cost) {
        Ordering::Less => "The cheapest datacenters are sync computed",
        Ordering::Equal => "Both datacenters computational methods are equal",
        Ordering::Greater => "The cheapest datacenters are async computed",
    };

    println!("{winner}");
}
