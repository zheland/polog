#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! The `polog-util` crate provides optional utilities, middleware, adapters,
//! and formatting helpers for the [`polog`] logging facade.
//!
//! It is not a logging backend.
//! It composes with any [`polog::Handler`] and is intended to be used together
//! with [`polog`] and a backend such as [`pologger`].
//!
//!
//! # About
//!
//! `polog-util` supplies the pieces that are useful around such a facade but do
//! not belong in the core crate: filtering, enrichment, fanout, offloading,
//! sharing, type erasure, delegators to `log`/`slog`/`tracing`, and small
//! context wrappers that middleware can inject.
//!
//! `polog-util` provides:
//! - Middleware that wraps any [`Handler`]: [`Filter`], [`Fanout`], [`Either`],
//!   [`Enricher`], [`OffloaderHandle`].
//! - Middleware that wraps a [`BorrowingHandler`]: [`BorrowingFanout`].
//! - Middleware that makes a non‑shared handler shareable: [`Shared`].
//! - Type‑erased argument wrappers: [`TypeErasedArgs`],
//!   [`TypeErasedArcLogArgs`], [`TypeErasedArcContextArgs`].
//! - Typed argument access and downcasting via [`MixedArgsExt`].
//! - Delegators to other ecosystems: [`ViaLog`], [`ViaSlog`], [`ViaTracing`].
//! - Conversion functions between `polog::Level`/`LevelFilter` and
//!   `log`/`slog`/`tracing` types under [`conv`].
//! - Context wrappers for middleware‑injected values under [`context`]:
//!   `MonotonicTime`, `ProcessCpuTime`, `ThreadCpuTime`, `ProcessId`,
//!   `ProcessName`, `ThreadId`, `ThreadName`, `TaskId`, `TaskName`, `CpuId`.
//! - Built‑in enricher functions: `SystemTimeEnricherFn`,
//!   `MonotonicEnricherFn`.
//!
//!
//! # Usage
//!
//! In libraries add:
//!
//! ```toml
//! [dependencies]
//! polog = "0.0.1-dev.0"
//! polog-util = "0.0.1-dev.0"
//! ```
//!
//! In executables you also need a backend:
//!
//! ```toml
//! [dependencies]
//! polog = "0.0.1-dev.0"
//! polog-util = "0.0.1-dev.0"
//! pologger = "0.0.1-dev.0"
//! ```
//!
//! Enable features for external integrations:
//!
//! ```toml
//! [dependencies]
//! polog-util = { version = "0.0.1-dev.0", features = [ "log-0-4", "offloader" ] }
//! ```
//!
//!
//! # Relationship to other crates
//!
//! | Crate | Role |
//! |:--:|:--|
//! | [`polog`] | Facade traits and macros. |
//! | `polog-util` | Optional utilities and middleware. |
//! | [`pologger`] | Reference logger backend implementation. |
//! | [`log`] / [`slog`] / [`tracing`] | External ecosystems bridged by delegators. |
//!
//!
//! # Composition and placement
//!
//! ## Placement rules
//!
//! Order matters along the chain from call site to backend.
//!
//! 1. **`Filter` closer to the call site.** `Filter` runs inside `try_enable`.
//!    Placed before [`OffloaderHandle`], filtered events never reach the
//!    channel and never pay for argument conversion or dispatch. Placed after
//!    `OffloaderHandle`, the event has already been sent by the time the filter
//!    runs.
//!
//! 2. **`Enricher` placement changes what the context represents.**
//!    - Before `OffloaderHandle`: `SystemTime::now()`, `Instant::now()`,
//!      `thread::current().id()` reflect the calling thread's time and
//!      identity.
//!    - After `OffloaderHandle`: the same calls reflect the worker thread's
//!      time and identity, and the context reflects worker processing time, not
//!      call‑site time.
//!
//!    Choose deliberately, and note the choice when interpreting
//!    `MonotonicTime` or `ThreadId` in the emitted records.
//!
//! 3. **`OffloaderHandle` closest to the backend.** The whole point is to move
//!    expensive work — formatting, I/O, channel processing — onto a dedicated
//!    worker that owns the backend. `OffloaderHandle` should wrap the backend
//!    directly, or wrap only cheap middleware in front of it.
//!
//! 4. **`BorrowingFanout` where every branch supports it.** `BorrowingFanout`
//!    cannot wrap `OffloaderHandle`, `Enricher`, or `Shared`. Two options:
//!    - Use `Fanout` instead, which accepts any `Handler` branch.
//!    - Place the `BorrowingFanout` on the worker side of an `OffloaderHandle`:
//!      the inner handler of the offloader can be a `BorrowingFanout` of two
//!      `BorrowingHandler` branches.
//!
//! 5. **`Fanout` when at least one branch is not a `BorrowingHandler`.**
//!    `Fanout` allocates an `Arc` only when more than one branch accepts. When
//!    exactly one branch accepts, no allocation is made. `BorrowingFanout`
//!    avoids the `Arc` in the multi‑accepted case and is otherwise equivalent.
//!
//! 6. **For more than four branches, nest.** `Fanout1`…`Fanout4` and
//!    `BorrowingFanout1`…`BorrowingFanout4` cover up to four branches without
//!    nesting. Beyond that, wrap `Fanout` in `Fanout`: each nesting level costs
//!    one extra `Arc::clone` per fanout call. Both families stop at four
//!    because the small sizes cover real use, and nesting composes cleanly past
//!    that. An opt‑in feature that raises the limit can be added later without
//!    a breaking change.
//!
//! ## Recommended chains
//!
//! ```text
//! // Filter at the call site, offload the backend work.
//! let handler = Filter::new(
//!     new_offloader(backend, ...).0,
//!     MaxLevel(LevelFilter::Info),
//! );
//!
//! // Two expensive backends, fanout at the call site.
//! // Both branches are OffloaderHandle, which is not a BorrowingHandler.
//! let handler = Fanout::new((backend_a, backend_b));
//!
//! // Two cheap backends, both BorrowingHandler.
//! // No Arc allocation in the multi‑accepted case.
//! let handler = BorrowingFanout::new((ViaLog(a), ViaLog(b)));
//!
//! // Fanout on the worker side.
//! let inner = BorrowingFanout::new((ViaLog(a), ViaSlog(b)));
//! let (handler, service) = new_offloader(inner, ...);
//! ```
//!
//!
//! # API Cheat Sheet
//!
//! ## Middleware
//!
//! | Item | Behavior |
//! |:--:|:--|
//! | `Filter<H, F>` | Filters events and spans using a `FilterFn`. |
//! | `FilterRules` | `RUST_LOG`‑style filter rules with longest‑prefix‑wins matching. |
//! | `MaxLevel`, `ConstMaxLevel<M>`, `Target`, `TargetPrefix`, `All`, `Any`, `Always`, `Never`, `Not<F>` | Composable `FilterFn` implementations. |
//! | `FilterFnExt` | Combinators `not`, `and`, `or` on any `FilterFn`. |
//! | `Fanout<Hs, N>`, `Fanout1`…`Fanout4` | Delivers events and spans to up to four handlers, sharing args through `Arc` when more than one accepts. |
//! | `BorrowingFanout<Hs, N>`, `BorrowingFanout1`…`BorrowingFanout4` | Like `Fanout`, but dispatches through `BorrowingHandler` and skips the `Arc` allocation. |
//! | `Enricher<H, F>` | Injects `F::Output` as a context argument before delegating. |
//! | `EnricherFn` | Trait for enricher output factories. Implemented for `Fn() -> T` closures. |
//! | `SystemTimeEnricher<H>`, `MonotonicEnricher<H>` | Built‑in `Enricher` aliases over `SystemTimeEnricherFn` and `MonotonicEnricherFn`. |
//! | `Shared<H>` | Makes a non‑shared `Handler` shareable across threads via `Arc<Mutex<H>>`. |
//! | `Either<H1, H2>` | Runtime selection between two handlers. |
//! | `OffloaderHandle<Context, CustomEvent>` | The `Handler` and `SharedHandler` frontend of the offload middleware. |
//! | `OffloaderService<H, Context, CustomEvent, CustomHandler>` | Owns the backend and runs the worker. |
//! | `new_offloader(...)` | Constructs the `(OffloaderHandle, OffloaderService)` pair. |
//! | `OffloaderCapacity` | `Bounded(usize)` or `Unbounded`. |
//! | `OffloaderBackpressurePolicy` | `Block`, `DropNewest { timeout }`, `DropOldest { timeout }`. Shorthands `DROP_NEWEST` and `DROP_OLDEST` use zero timeout. |
//!
//! ## Filtering
//!
//! Filters in this crate implement two related traits:
//!
//! - [`FilterFn`] — receives `&'static Descriptor`. Used by `Filter` at `polog`
//!   call sites.
//! - [`ExternalFilterFn`] — receives `&ExternalDescriptor<'_>`. Used by
//!   `pologger::SharedPologger` when it ingests foreign events.
//!
//! Every built‑in filter — `MaxLevel`, `ConstMaxLevel<M>`, `Target`,
//! `TargetPrefix`, `All`, `Any`, `Not`, `Always`, `Never`, `FilterRules` —
//! implements both, so one parsed rule set filters both the `polog` chain
//! and the foreign-facade ingestion:
//!
//! ```rust,ignore
//! # #[cfg(feature = "shared")] {
//! use core::time::Duration;
//! use std::sync::Arc;
//!
//! use polog_util::{
//!     Filter, FilterRules, OffloaderBackpressurePolicy, OffloaderCapacity, new_offloader,
//! };
//!
//! let filter = FilterRules::parse("info,my_crate=debug")
//!     .expect("invalid filter")
//!     .to_owning();
//!
//! let span_id_generator = Arc::new(polog::SpanIdGenerator::new());
//! let formatter = pologger::TextFormatter::stderr(
//!     pologger::TextFormatterFlags::default(),
//!     pologger::ColorizePreset::Auto,
//! );
//! let handler = pologger::Pologger::new(
//!     Arc::clone(&span_id_generator),
//!     formatter,
//!     pologger::LoggerFlags::compact(),
//! );
//!
//! // Foreign side, inside the backend wiring.
//! let handler = pologger::SharedPologger::new(handler, filter.clone());
//!
//! let (handler, service) = new_offloader(
//!     handler,
//!     span_id_generator,
//!     OffloaderCapacity::Bounded(8192),
//!     OffloaderBackpressurePolicy::DROP_NEWEST,
//!     Some(Duration::from_millis(1)),  // Flush_interval.
//!     |_, ev: !| match ev {}, // Custom_handler.
//! );
//! # let handler: polog_util::OffloaderHandle<(), _> = handler;
//!
//! let _handle = service.spawn();
//!
//! // Polog side.
//! let logger = Filter::new(handler, filter);
//! # }
//! ```
//!
//! ## Delegators
//!
//! | Item | Behavior |
//! |:--:|:--|
//! | `log_0_4::ViaLog` | Forwards events and spans to a `log::Log` backend. Spans are logged as regular records. |
//! | `slog_2::ViaSlog` | Forwards events and spans to a `slog::Logger`. Spans are logged as regular records. |
//! | `tracing_0_1::ViaTracing` | Forwards events and spans to a `tracing::Dispatch`, including the span lifecycle. |
//!
//! ## Argument utilities
//!
//! | Item | Behavior |
//! |:--:|:--|
//! | `TypeErasedArgs<T>` | Transparent wrapper that forwards `MixedArgs` behavior and can be stored behind `Arc`. |
//! | `TypeErasedArcLogArgs` | `TypeErasedArgs<Arc<dyn … + DisplayableMixedArgs<'static>>>`. |
//! | `TypeErasedArcContextArgs` | `TypeErasedArgs<Arc<dyn … + AnyDebuggableMixedArgs<'static>>>`. |
//! | `MixedArgsExt` | Typed downcasting: `try_nth_arg_as::<T>(i)`, `type_filtered_nth_arg::<T>(i)`, `type_filtered_num_args::<T>()`. |
//! | `AnyDynMixedArgs`, `AnyDisplayableMixedArgs`, `AnyDebuggableMixedArgs` and their `…Builder` counterparts | Marker traits combining `Any` with the argument traits, for downcasting to a concrete tuple. |
//!
//! ## Context wrappers
//!
//! Types in `polog_util::context` are injected by `Enricher` middleware and
//! consumed by backends that read `context_fields` (for example
//! `pologger::LoggerFlags`):
//!
//! `MonotonicTime`, `ProcessCpuTime`, `ThreadCpuTime`, `ProcessId`,
//! `ProcessName`, `ThreadId`, `ThreadName`, `TaskId`, `TaskName`, `CpuId`.
//!
//! ## Conversions
//!
//! `polog_util::conv` contains feature‑gated conversion helpers:
//!
//! - `conv::log_0_4` — `log` ↔ `polog` level and filter conversions.
//! - `conv::slog_2` — `slog` ↔ `polog` level and filter conversions.
//! - `conv::tracing_core_0_1` — `tracing_core` ↔ `polog` level and filter
//!   conversions.
//!
//!
//! # Examples
//!
//! ## Filtering
//!
//! ```rust
//! use polog::LevelFilter;
//! use polog_util::Filter;
//! use polog_util::filters::{All, MaxLevel, TargetPrefix};
//!
//! # let handler = polog::Discard;
//! let handler = Filter::new(
//!     handler,
//!     All((MaxLevel(LevelFilter::Info), TargetPrefix("my_app"))),
//! );
//! ```
//!
//! ## Parsed filtering
//!
//! ```rust
//! # #[cfg(feature = "alloc")] {
//! use polog_util::{Filter, FilterRules};
//!
//! # let handler = polog::Discard;
//! let rules = FilterRules::parse("info,my_crate=debug,my_crate::noisy=info")
//!     .expect("invalid filter specification")
//!     .to_owning();
//! let handler = Filter::new(handler, rules);
//! # }
//! ```
//!
//! `FilterRules::parse` returns a rule set borrowing from the input string.
//! Call `.to_owning()` before storing the handler to make it static.
//!
//! ## Enrichment
//!
//! ```rust
//! # #[cfg(feature = "std")] {
//! use polog_util::{Enricher, SystemTimeEnricherFn};
//!
//! # let handler = polog::Discard;
//! let handler = Enricher::new(handler, SystemTimeEnricherFn);
//! # }
//! ```
//!
//! `F::Output` must be `'static + Debug`, and the inner handler must accept
//! `Context::WithDebuggableArg<F::Output>`.
//!
//! ## Offloading
//!
//! ```rust
//! # #[cfg(feature = "offloader")] {
//! use core::time::Duration;
//! use std::sync::Arc;
//!
//! use polog_util::{OffloaderBackpressurePolicy, new_offloader};
//!
//! let span_id_generator = Arc::new(polog::SpanIdGenerator::new());
//! # let handler = polog::Discard;
//!
//! let (handler, service) = new_offloader(
//!     handler,
//!     Arc::clone(&span_id_generator),
//!     polog_util::OffloaderCapacity::Bounded(8192),
//!     polog_util::OffloaderBackpressurePolicy::DROP_NEWEST,
//!     Some(Duration::from_millis(1)), // Flush_interval.
//!     |_, ev: !| match ev {},         // Custom_handler.
//! );
//! # let handler: polog_util::OffloaderHandle<(), _> = handler;
//!
//! let worker = service.spawn().expect("failed to spawn logger worker");
//!
//! // Use `handler` as the handler in polog macros.
//! // Call `handler.stop()` before shutdown if you need an explicit drain.
//! // Send custom events with `handler.send_custom(event, policy)`.
//! # }
//! ```
//!
//! `OffloaderService::spawn` spawns a dedicated worker thread.
//! `OffloaderService::run_blocking` runs the loop on the calling thread.
//! `OffloaderBackpressurePolicy::Block` blocks the caller on a full channel.
//! `DropNewest` and `DropOldest` drop on a full channel, optionally after a
//! timeout.
//!
//!
//! The shared generator is what lets `SharedPologger` interleave foreign and
//! `polog` events in one backend with a single, non‑overlapping span ID
//! space.
//! A fresh `Arc::new(SpanIdGenerator::new())` is fine.
//!
//! ## Delegating to `log`
//!
//! ```rust
//! # #[cfg(feature = "log-0-4")] {
//! use polog_util::delegators::log_0_4::ViaLog;
//!
//! # let handler = polog::Discard;
//! let handler = ViaLog(handler);
//! # }
//! ```
//!
//! `ViaSlog` and `ViaTracing` are structured the same way under
//! `polog_util::slog_2::ViaSlog` and `polog_util::tracing_0_1::ViaTracing`.
//!
//! ## Type-erased arguments
//!
//! Custom handlers that need to store arguments beyond a call boundary — for
//! example to send them through an `mpsc` channel — can use the type‑erased
//! wrappers:
//!
//! ```rust
//! # #[cfg(feature = "alloc")] {
//! use std::sync::Arc;
//!
//! use polog_util::{TypeErasedArcLogArgs, TypeErasedArgs};
//!
//! fn convert(args: impl polog::IntoSendable<Sendable: polog::DisplayableMixedArgs<'static>>) {
//!     let args: TypeErasedArcLogArgs = TypeErasedArgs::new(Arc::new(args.into_sendable()));
//!     // ..
//! }
//! # convert((polog::Arg(123)));
//! # }
//! ```
//!
//! ## Typed argument access in `log_static`
//!
//! The `log_static` method of [`Handler`] receives `Args: StaticLogArgs`,
//! which carries the `'static` bound needed for typed downcasting:
//!
//! ```rust
//! use polog_util::MixedArgsExt;
//!
//! fn convert(args: impl 'static + polog::DisplayableMixedArgs<'static>) {
//!     for i in 0..args.num_args() {
//!         if let Some(name) = args.try_nth_arg_as::<&'static str>(i) {
//!             // `name` is the borrowed `&'static str`, not its formatted form.
//!         }
//!     }
//! }
//! # convert((polog::Arg(123)));
//! # convert((polog::Arg("123")));
//! ```
//!
//! This is how `pologger` recovers the `@fn` meta‑attribute without
//! allocating.
//!
//!
//! # Feature flags
//!
//! | Feature | Default | Description | Enables deps and features |
//! |:--:|:--:|:--:|:--:|
//! | `alloc` | yes | Enables `alloc`‑based items such as `TypeErasedArcLogArgs`, `TypeErasedArcContextArgs`, and `FilterRules` | `polog/alloc` |
//! | `std` | yes | Enables `std`‑based middleware and utilities, including some enrichers and the offloader. | `alloc`, `polog/std` |
//! | `fanout` | no | Enables the fanout middleware. | `alloc`, `dep:dashmap` (transitive `std`) |
//! | `offloader` | no | Enables the offloading middleware. | `std`, `dep:crossbeam-channel`, `dep:oneshot` |
//! | `offloader-support-tokio` | no | Enables `tokio::task::block_in_place` during offloader flush‑on‑panic when running on a multi‑thread Tokio runtime. | `offloader`, `dep:tokio` |
//! | `shared` | no | Enables the sharing middleware. | `alloc`, `dep:parking_lot` (transitive `std`) |
//! | `shared-filter` | no | Enables filter implementations for `Arc<RwLock<F>>`. | `alloc`, `dep:parking_lot` (transitive `std`) |
//! | `log-0-4` | no | Enables integrations for `log = "0.4"` crate types, including `conv::log_0_4` and `log_0_4::ViaLog`. | `dep:log` (version = 0.4), `log/kv` |
//! | `slog-2` | no | Enables integrations for `slog = "2"` crate types, including `slog_2::ViaSlog`. | `dep:slog` (version = 2) |
//! | `slog-2-semver-unstable` | no | Enables `slog = "2"` integrations that depend on `#[doc(hidden)]` items of dependency crates, items that those crates only intend for their own macro expansions. | `slog-2` |
//! | `tracing-core-0-1` | no | Enables integrations for `tracing_core = "0.1"` crate types, including `conv::tracing_core_0_1`. | `dep:tracing-core` (version = 0.1) |
//! | `tracing-0-1-semver-unstable` | no | Enables `tracing = "0.1"` integrations that depend on `#[doc(hidden)]` items of dependency crates, items that those crates only intend for their own macro expansions, including `tracing_0_1::ViaTracing`. | `std`, `tracing-core-0-1`, `dep:tracing` (version = 0.1), `tracing/std`, `dep:dashmap` |
//!
//! ## `*-semver-unstable` feature flags
//!
//! Enabling any of these features can, while unlikely, make a `cargo update`
//! break the build or change runtime behaviour, even without any change on the
//! `polog` side.
//! This is because the gated integrations rely on `#[doc(hidden)]` items of
//! their dependency, which are not covered by that dependency's semver
//! guarantees and may be renamed, changed, or removed at any time.
//!
//! If this happens, pin the affected dependency, currently `tracing` and
//! `tracing-core`, to an exact version known to work with your `polog` version.
//!
//! `tracing-0-1-semver-unstable` feature gates the key-values construction in
//! `polog-util::tracing_0_1::ViaTracing`.
//! The implementation is forced to rely on
//! `tracing_core::field::FieldSet::value_set_all`, a `#[doc(hidden)]`
//! constructor used by tracing's own `event!` / `span!` / `valueset!` macros,
//! because `tracing` exposes no stable way to build a `ValueSet` from outside
//! of its macros.
//!
//! `slog-2-semver-unstable` feature gates the proper location propagation
//! in `polog-util::slog_2::ViaSlog`.
//! The implementation is forced to rely on
//! `slog::RecordLocation` and `slog::RecordStatic`, structs with
//! `#[doc(hidden)]` fields used by slog's own `record_static!` macros,
//! because `slog` exposes no stable way to build a `RecordStatic` with custom
//! location outside of its macros.
//!
//! Don't use these features if you need a fully semver-reliable integrations.
//! The implementations provided are best-effort by design.
//!
//!
//! # FAQ
//!
//! ## How do `log` / `slog` / `tracing` events reach a `polog` backend?
//!
//! Forward direction — `polog` events to a foreign facade — is handled by
//! the delegators in this crate (`ViaLog`, `ViaSlog`, `ViaTracing`).
//!
//! Reverse direction — foreign events to a `polog` backend — is handled by
//! `pologger::SharedPologger`, not by `polog-util`. The two are deliberately
//! separated:
//!
//! - `polog-util` supplies the *filter interface* both directions need
//!   (`ExternalFilterFn`) plus the `ExternalDescriptor<'a>` type that lets a
//!   foreign event be filtered without a `'static` `Descriptor`.
//! - `pologger` supplies the *backend adapter* (`SharedPologger`) that converts
//!   foreign events to `polog`'s rendering model and shares its
//!   `SpanIdGenerator` with the rest of the chain.
//!
//! This split keeps `polog-util` free of `log` / `slog` / `tracing` feature
//! dependencies for users who only need one direction.
//!
//! A user who needs both directions installs `polog-util` and `pologger`
//! together and gets a single filter type (`ExternalFilterFn`) shared by both
//! the foreign-ingest path and the `polog` call-site path.
//!
//!
//! # Documentation
//!
//! [API Documentation]
//!
//!
//! # License
//!
//! Licensed under either of
//!
//! - Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
//! - MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)
//!
//! at your option.
//!
//!
//! # Contribution
//!
//! Unless you explicitly state otherwise, any contribution intentionally
//! submitted for inclusion in the work by you, as defined in the Apache-2.0
//! license, shall be dual licensed as above, without any
//! additional terms or conditions.
//!
//! [API Documentation]: https://docs.rs/polog-util/latest/polog_util/
//! [`polog`]: https://docs.rs/polog/latest/polog/
//! [`pologger`]: https://docs.rs/pologger/latest/pologger/
//! [`log`]: https://docs.rs/log/latest/log/
//! [`slog`]: https://docs.rs/slog/latest/slog/
//! [`tracing`]: https://docs.rs/tracing/latest/tracing/
//! [`Handler`]: https://docs.rs/polog/latest/polog/trait.Handler.html
//! [`BorrowingHandler`]: https://docs.rs/polog/latest/polog/trait.BorrowingHandler.html
//! [`ViaLog`]: struct.ViaLog.html
//! [`ViaSlog`]: struct.ViaSlog.html
//! [`ViaTracing`]: struct.ViaTracing.html

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

mod args;
pub mod context;
pub mod conv;
pub mod delegators;
mod external;
mod middleware;

pub use args::*;
pub use external::*;
pub use middleware::*;
/// A re-export of `parking_lot::RwLock`.
#[cfg(any(feature = "shared", feature = "shared-filter"))]
pub use parking_lot::{Mutex, RwLock};

#[cfg(all(test, not(doc)))]
mod tests_deps {
    #[cfg(not(any(feature = "alloc", feature = "fanout")))]
    use parking_lot as _;
}
