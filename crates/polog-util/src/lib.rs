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
//! sharing, type erasure, inbound bridges from `log`/`slog`/`tracing`, outbound
//! delegators to `log`/`slog`/`tracing`, and small context wrappers that
//! middleware can inject.
//!
//! `polog-util` provides:
//! - Middleware that wraps any [`Handler`]: [`Filter`], [`Fanout`],
//!   [`Enricher`], [`OffloaderHandle`], [`Shared`].
//! - Middleware that wraps a [`BorrowingHandler`]: [`BorrowingFanout`].
//! - Inbound foreign-facade adapters that normalize events from `log`, `slog`,
//!   and `tracing` into the `polog` handler chain: [`LogBridge`],
//!   [`SlogBridge`], [`TracingBridge`], and the [`ExternalHandler`] family.
//! - Delegators to other ecosystems: [`ViaLog`], [`ViaSlog`], [`ViaTracing`].
//! - Type‑erased argument wrappers: [`TypeErasedArgs`],
//!   [`TypeErasedArcLogArgs`], [`TypeErasedArcContextArgs`].
//! - Type-erased foreign-event metadata and arguments:
//!   [`TypeErasedExternalDescriptor`], [`TypeErasedExternalArgs`].
//! - Typed argument access and downcasting via [`MixedArgsExt`].
//! - Conversion functions between `polog::Level`/`LevelFilter` and
//!   `log`/`slog`/`tracing` types under `conv` module.
//! - Context wrappers for middleware‑injected values under `Context`:
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
//! polog = "0.1.0-alpha.1"
//! polog-util = "0.1.0-alpha.1"
//! ```
//!
//! In executables you also need a backend:
//!
//! ```toml
//! [dependencies]
//! polog = "0.1.0-alpha.1"
//! polog-util = "0.1.0-alpha.1"
//! pologger = "0.1.0-alpha.1"
//! ```
//!
//! Enable features for external integrations:
//!
//! ```toml
//! [dependencies]
//! polog-util = { version = "0.1.0-alpha.1", features = [ "log-0-4", "slog-2", "tracing-0-1" ] }
//! ```
//!
//!
//! # Relationship to other crates
//!
//! | Crate | Role |
//! |:--:|:--|
//! | [`polog`] | Facade traits and macros. |
//! | `polog-util` | Optional utilities, middleware and foreign-facade adapters. |
//! | [`pologger`] | Reference logger backend implementation. |
//! | [`log`] / [`slog`] / [`tracing`] | External ecosystems bridged in both directions. |
//!
//!
//! # Composition and placement
//!
//! ## Composition
//!
//! The table compares every chain-shaped item in this crate.
//! - **Accepts from** and **forwards to** place the item on the chain from call
//!   site to backend.
//! - **Fanin** means the item implements [`polog::SharedHandler`]: it can be
//!   cloned, and each clone used as an independent handler by a separate
//!   producer. Every `SharedHandler` is already a fanin.
//! - **Fanout** means the item dispatches one event or span to multiple
//!   handlers.
//! - "**—**" means not applicable.
//!
//! | Item | Kind | Accepts from | Forwards to | Fanin | Fanout | Notes |
//! |:--|:--|:--|:--|:--:|:--:|:--|
//! | [`Filter<H, F>`] | Middleware | Chain | Wrapped `H` if accepted | if `H` | no | Runs inside `try_enable`. Place near the call site. |
//! | [`Enricher<H, F>`] | Middleware | Chain | Wrapped `H`, with one injected context arg | if `H` | no | Placement decides which thread's time/identity is observed. |
//! | [`Shared<H>`] | Middleware | Chain | Wrapped `H` via `Arc<Mutex<H>>` | **yes** | no | Makes any `Handler` usable by `polog` macros and a fanin. The target need not be `SharedHandler`. |
//! | [`OffloaderHandle`] | Middleware | Chain | Wrapped `H`, on the worker thread | **yes** | no | Makes any `Handler` usable by `polog` macros and a fanin. The target need not be `SharedHandler`. |
//! | [`Fanout<Hs, N>`] | Middleware | Chain | Multiple branches | if all `Hs` | **yes** | `Arc`-shares args when more than one branch accepts. Use when at least one branch is not a `BorrowingHandler`. |
//! | [`BorrowingFanout<Hs, N>`] | Middleware | Chain | Multiple branches | if all `Hs` | **yes** | Avoids the `Arc`. **preferred** when every branch implements `BorrowingHandler`. |
//! | [`Either<H1, H2>`] | Middleware | Chain | Exactly one branch | if `H1` and `H2` | no | Runtime selection. Both arms must agree on `EnrichedContext`. |
//! | [`LogBridge<H>`] | Bridge | `log` | Wrapped `H` | — | — | Chain input. Spans become regular records. |
//! | [`SlogBridge<H>`] | Bridge | `slog` | Wrapped `H` | — | — | Chain input. Spans become regular records. |
//! | [`TracingBridge<H>`] | Bridge | `tracing` | Wrapped `H` | — | — | Chain input, including span lifecycle. |
//! | [`ViaLog`] | Delegator | Chain | [`log::Log`] | **yes** | — | Chain output. Spans become regular records. |
//! | [`ViaSlog`] | Delegator | Chain | [`slog::Logger`] | **yes** | — | Chain output. Spans become regular records. |
//! | [`ViaTracing`] | Delegator | Chain | [`tracing::Dispatch`] | **yes** | — | Chain output, including span lifecycle. |
//! | [`Pologger`] from [`pologger`] crate | Backend | Chain | Formatter & I/O | no | — | Reference backend. Not a `SharedHandler`. Wrap in `Shared` or `OffloaderHandle` to get fanin. |
//!
//! Notes:
//!
//! - **Prefer `BorrowingFanout` when every branch implements
//!   `BorrowingHandler`.** It avoids the `Arc` that `Fanout` allocates in the
//!   multi-accept case. Use `Fanout` when at least one branch is not a
//!   `BorrowingHandler`.
//! - **`Shared` and `OffloaderHandle` are the two ways to feed a
//!   non-`SharedHandler` backend from multiple producers.** `Shared` wraps the
//!   target in `Arc<Mutex<H>>` and makes it usable by `polog` macros.
//!   `OffloaderHandle` is itself a `SharedHandler` whose target is owned by
//!   `OffloaderService` and therefore need not be shared.
//! - **Bridges are chain inputs. Delegators and `Pologger` are chain outputs.**
//!   Filters, enrichers, fanouts, offloaders, and `Either` sit in between.
//!
//! ## Placement
//!
//! Order matters along the chain from call site to backend.
//!
//! 1. **`Filter` closest to the call site.** `Filter` runs inside `try_enable`.
//!    Placed before [`OffloaderHandle`], filtered events never reach the
//!    channel and never pay for argument conversion or dispatch. Placed after
//!    `OffloaderHandle`, the event has already been sent by the time the filter
//!    runs.
//! 2. **`Enricher` placement changes what the context represents.** Before
//!    [`OffloaderHandle`]: `SystemTime::now()`, `Instant::now()`,
//!    `thread::current().id()` reflect the calling thread's time and identity.
//!    After [`OffloaderHandle`]: the same calls reflect the worker thread's
//!    time and identity, and the context reflects worker processing time, not
//!    call-site time. Choose deliberately, and note the choice when
//!    interpreting `MonotonicTime` or `ThreadId` in the emitted records.
//! 3. **`OffloaderHandle` closest to the backend.** The whole point is to move
//!    expensive work — formatting, I/O, channel processing — onto a dedicated
//!    worker that owns the backend. `OffloaderHandle` should wrap the backend
//!    directly, or wrap only cheap middleware in front of it.
//! 4. **Prefer `BorrowingFanout`, fall back to `Fanout`.** Use
//!    [`BorrowingFanout`] when every branch implements [`BorrowingHandler`]. It
//!    skips the `Arc` allocation that [`Fanout`] pays in the multi-accept case.
//!    Use `Fanout` when at least one branch is not a `BorrowingHandler`.
//!    `BorrowingFanout` cannot wrap [`OffloaderHandle`], [`Enricher`], or
//!    [`Shared`]. Either use `Fanout`, or place the `BorrowingFanout` on the
//!    worker side of an `OffloaderHandle` — the offloader's inner handler can
//!    be a `BorrowingFanout` of two `BorrowingHandler` branches.
//!
//!
//! # API Cheat Sheet
//!
//! ## Middleware
//!
//! | Item | Behavior |
//! |:--:|:--|
//! | [`Filter<H, F>`] | Filters events and spans using a [`FilterFn`]. |
//! | [`FilterRules`] | `RUST_LOG`‑style filter rules with longest‑prefix‑wins matching. |
//! | [`MaxLevel`], [`ConstMaxLevel<M>`], [`Target`], [`TargetPrefix`], [`All`], [`Any`], [`Always`], [`Never`], [`Not<F>`] | Composable [`FilterFn`] implementations. |
//! | [`Fanout<Hs, N>`], [`Fanout1`]…[`Fanout4`] | Delivers events and spans to up to four handlers, sharing args through `Arc` when more than one accepts. |
//! | [`BorrowingFanout<Hs, N>`], [`BorrowingFanout1`]…[`BorrowingFanout4`] | Like `Fanout`, but dispatches through `BorrowingHandler` and skips the `Arc` allocation. |
//! | [`Enricher<H, F>`] | Injects `F::Output` as a context argument before delegating. |
//! |[ `EnricherFn`] | Trait for enricher output factories. Implemented for `Fn() -> T` closures. |
//! | [`SystemTimeEnricher<H>`], [`MonotonicEnricher<H>`] | Built‑in `Enricher` aliases over [`SystemTimeEnricherFn`] and `MonotonicEnricherFn`. |
//! | [`Shared<H>`] | Makes a non‑shared [`Handler`] shareable across threads via `Arc<Mutex<H>>`. |
//! | [`Either<H1, H2>`] | Runtime selection between two handlers. |
//! | [`OffloaderHandle<Context, CustomEvent>`] | The [`Handler`] and [`SharedHandler`] frontend of the offload middleware. |
//! | [`OffloaderService<H, Context, CustomEvent, CustomHandler>`] | Owns the backend and runs the worker. |
//! | [`new_offloader(...)`] | Constructs the `(OffloaderHandle, OffloaderService)` pair. |
//! | [`OffloaderCapacity`] | `Bounded(usize)` or `Unbounded`. |
//! | [`OffloaderBackpressurePolicy`] | `Block`, `DropNewest { timeout }`, `DropOldest { timeout }`. Shorthands `DROP_NEWEST` and `DROP_OLDEST` use zero timeout. |
//!
//! ## Filtering
//!
//! Filters in this crate implement two related traits:
//!
//! - [`FilterFn`] — receives `&'static Descriptor`. Used by `Filter` at `polog`
//!   call sites.
//! - [`ExternalFilterFn`] — receives `&ExternalDescriptor<'_>`. Used when it
//!   ingests foreign events from `log` / `slog` / `tracing`.
//!
//! Every built‑in filter — [`MaxLevel`], [`ConstMaxLevel<M>`], [`Target`],
//! [`TargetPrefix`], [`All`], [`Any`], [`Not`], [`Always`], [`Never`],
//! [`FilterRules`] — implements both, so one parsed rule set filters both the
//! `polog` chain and the foreign-facade ingestion:
//!
//! ## Inbound foreign-facade adapters
//!
//! These adapters receive events from [`log`], [`slog`], or [`tracing`] and
//! forward them into a [`polog`] handler chain.
//! They are the reverse direction of the outbound delegators below.
//!
//! | Item | Behavior |
//! |:--:|:--|
//! | [`LogBridge<H>`] | [`log::Log`] adapter over a [`polog::Handler`] backend. |
//! | [`SlogBridge<H>`] | [`slog::Drain`] adapter over a [`polog::Handler`] backend. |
//! | [`TracingBridge<H>`] | [`tracing::Dispatch`]/[`tracing::Subscriber`] adapter over a [`polog::Handler`] backend. |
//! | [`ExternalHandler<Context>`] | Backend/middleware trait for foreign-facade events. Extends `Handler<Context>`. |
//! | [`ExternalDescriptor`] | Borrowed description of a foreign event/span: message, level, target, module path, file, line, column. |
//! | [`TypeErasedExternalDescriptor`] | Owned, type-erased [`ExternalDescriptor`] for storing foreign metadata. |
//! | [`ExternalArgs`] | Borrowed view of foreign event arguments; walks fields once through a visitor. |
//! | [`ExternalArgsVisitor`] | Visitor receiving `visit_message` and `visit_arg` callbacks. |
//! | [`ExternalArgValue<'_>`] | Borrowed foreign argument value: primitives, strings, `Display`/`Debug`/`Error` trait objects, and `format_args!`. |
//! | [`TypeErasedExternalArgs`] | Owned, type-erased foreign argument set. |
//!
//!
//! ## Outbound delegators
//!
//! These adapters forward [`polog`] events and spans to foreign facades.
//!
//! | Item | Behavior |
//! |:--:|:--|
//! | [`ViaLog<L>`] | Forwards events and spans to a [`log::Log`] backend. Spans are logged as regular records. |
//! | [`ViaSlog<D>`] | Forwards events and spans to a [`slog::Logger`]. Spans are logged as regular records. |
//! | [`ViaTracing`] | Forwards events and spans to a [`tracing::Dispatch`], including the span lifecycle. |
//!
//! ## Argument utilities
//!
//! | Item | Behavior |
//! |:--:|:--|
//! | [`TypeErasedArgs<T>`] | Transparent wrapper that forwards `MixedArgs` behavior and can be stored behind `Arc`. |
//! | [`TypeErasedArcLogArgs`] | `TypeErasedArgs<Arc<dyn … + DisplayableMixedArgs<'static>>>`. |
//! | [`TypeErasedArcContextArgs`] | `TypeErasedArgs<Arc<dyn … + AnyDebuggableMixedArgs<'static>>>`. |
//! | [`MixedArgsExt`] | Typed downcasting: `try_nth_arg_as::<T>(i)`, `type_filtered_nth_arg::<T>(i)`, `type_filtered_num_args::<T>()`. |
//! | [`AnyDynMixedArgs`], [`AnyDisplayableMixedArgs`], [`AnyDebuggableMixedArgs`] and their `…Builder` counterparts | Marker traits combining `Any` with the argument traits, for downcasting to a concrete tuple. |
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
//! [`FilterRules::parse`] returns a rule set borrowing from the input string.
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
//! [`OffloaderService::spawn`] spawns a dedicated worker thread.
//! [`OffloaderService::run_blocking`] runs the loop on the calling thread.
//! [`OffloaderBackpressurePolicy::Block`] blocks the caller on a full channel.
//! [`DropNewest`] and [`DropOldest`] drop on a full channel, optionally after a
//! timeout.
//!
//! ## Ingesting `log` events
//!
//! ```rust
//! # #[cfg(feature = "log-0-4")] {
//! use polog_util::bridges::log_0_4::LogBridge;
//!
//! # let handler = polog::Discard;
//! LogBridge::new(handler.clone())
//!     .try_install_global()
//!     .unwrap();
//! # }
//! ```
//!
//! [`LogBridge`] forwards `log` records into the `polog` handler chain.
//! `log` events have no span lifecycle, so only the event path is used.
//! [`SlogBridge`] and [`TracingBridge`] are structured the same way.
//!
//! ## Delegating to `log`
//!
//! ```rust
//! # #[cfg(feature = "log-0-4")] {
//! use polog_util::delegators::log_0_4::{ViaLog, ViaLogFlags};
//!
//! # let handler = polog::Discard;
//! let handler = ViaLog::new(handler, ViaLogFlags::empty());
//! # }
//! ```
//!
//! [`ViaLog`], [`ViaSlog`] and [`ViaTracing`] are structured the same way.
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
//! | `alloc` | yes | Enables `alloc`‑based items such as [`TypeErasedArcLogArgs`], [`TypeErasedArcContextArgs`], and [`FilterRules`] | `polog/alloc` |
//! | `std` | yes | Enables `std`‑based middleware and utilities, including some enrichers and the offloader. | `alloc`, `polog/std` |
//! | `fanout` | no | Enables the fanout middleware. | `alloc`, `dep:dashmap` (transitive `std`) |
//! | `offloader` | no | Enables the offloading middleware. | `std`, `dep:crossbeam-channel`, `dep:oneshot`, `oneshot/std` |
//! | `offloader-support-tokio` | no | Enables `tokio::task::block_in_place` during offloader flush‑on‑panic when running on a multi‑thread Tokio runtime. | `offloader`, `dep:tokio`, `tokio/rt`, `tokio/rt-multi-thread` |
//! | `shared` | no | Enables the sharing middleware. | `alloc`, `dep:parking_lot` (transitive `std`) |
//! | `shared-filter` | no | Enables filter implementations for `Arc<RwLock<F>>`. | `alloc`, `dep:parking_lot` (transitive `std`) |
//! | `either-1` | no | Enables integrations for `either = "1"` crate types, including `either::Either` implementations. | `dep:either` (version = 1) |
//! | `log-0-4` | no | Enables integrations for `log = "0.4"` crate types, including `conv::log_0_4`, [`LogBridge`], and [`ViaLog`]. | `dep:log` (version = 0.4), `log/kv` |
//! | `slog-2` | no | Enables integrations for `slog = "2"` crate types, including [`SlogBridge`] and [`ViaSlog`]. | `dep:slog` (version = 2), `slog-2/dynamic-keys` |
//! | `slog-2-semver-unstable` | no | Enables `slog = "2"` integrations that depend on `#[doc(hidden)]` items of dependency crates, items that those crates only intend for their own macro expansions. | `slog-2` |
//! | `tracing-core-0-1` | no | Enables integrations for `tracing_core = "0.1"` crate types, including `conv::tracing_core_0_1`. | `dep:tracing-core` (version = 0.1) |
//! | `tracing-0-1` | no | Enables integrations for `tracing = "0.1"` crate types, including [`TracingBridge`]. | `std`, `tracing-core-0-1`, `dep:tracing` (version = 0.1), `tracing/std`, `dep:dashmap` |
//! | `tracing-0-1-semver-unstable` | no | Enables `tracing = "0.1"` integrations that depend on `#[doc(hidden)]` items of dependency crates, items that those crates only intend for their own macro expansions, including [`ViaTracing`]. | `tracing-0-1`, `dep:smallvec` |
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
//! If this happens, pin the affected dependency, to an exact version known to
//! work with your `polog` version.
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
//! Forward direction — [`polog`] events to a foreign facade — is handled by
//! the delegators in this crate ([`ViaLog`], [`ViaSlog`], [`ViaTracing`]).
//!
//! Reverse direction — foreign events to a [`polog`] backend — is handled by
//! the bridges in this crate ([`LogBridge`], [`SlogBridge`],
//! [`TracingBridge`]).
//!
//! Filters implement both [`FilterFn`] and [`ExternalFilterFn`], so a single
//! parsed rule set can filter both paths.
//!
//! ## Why are there separate [`ExternalDescriptor`] and [`ExternalArgs`] types?
//!
//! Foreign facades do not carry `'static` [`polog::Descriptor`] values, and
//! they differ in which metadata they provide.
//! [`ExternalDescriptor`] is a borrowed, optional-field view over that
//! metadata, and [`ExternalArgs`] walks the foreign argument list once through
//! a visitor.
//! This avoids leaking foreign metadata to obtain `'static` values and avoids
//! building a parallel native `Descriptor` layer.
//! [`TypeErasedExternalDescriptor`] and [`TypeErasedExternalArgs`] provide
//! owned forms for storage and off-thread handling.
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
//!
//! [API Documentation]: https://docs.rs/polog-util/latest/polog_util/
//! [`polog`]: https://docs.rs/polog/latest/polog/
//! [`pologger`]: https://docs.rs/pologger/latest/pologger/
//! [`log`]: https://docs.rs/log/latest/log/
//! [`slog`]: https://docs.rs/slog/latest/slog/
//! [`tracing`]: https://docs.rs/tracing/latest/tracing/
//! [`Handler`]: https://docs.rs/polog/latest/polog/trait.Handler.html
//! [`SharedHandler`]: https://docs.rs/polog/latest/polog/trait.SharedHandler.html
//! [`polog::Descriptor`]: https://docs.rs/polog/latest/polog/struct.Descriptor.html
//! [`polog::Handler`]: https://docs.rs/polog/latest/polog/trait.Handler.html
//! [`polog::SharedHandler`]: https://docs.rs/polog/latest/polog/trait.SharedHandler.html
//! [`BorrowingHandler`]: https://docs.rs/polog/latest/polog/trait.BorrowingHandler.html
//! [`LogBridge`]: bridges::log_0_4::struct.LogBridge.html
//! [`LogBridge<H>`]: bridges::log_0_4::struct.LogBridge.html
//! [`SlogBridge`]: bridges::slog_2::struct.SlogBridge.html
//! [`SlogBridge<H>`]: bridges::slog_2::struct.SlogBridge.html
//! [`TracingBridge`]: bridges::tracing_0_1::struct.TracingBridge.html
//! [`TracingBridge<H>`]: bridges::tracing_0_1::struct.TracingBridge.html
//! [`ViaLog`]: delegators::log_0_4::struct.ViaLog.html
//! [`ViaSlog`]: delegators::slog_2::struct.ViaSlog.html
//! [`ViaTracing`]: delegators::tracing_0_1::struct.ViaTracing.html
//! [`ExternalHandler`]: trait.ExternalHandler.html
//! [`Filter`]: struct.Filter.html
//! [`Filter<H, F>`]: struct.Filter.html
//! [`FilterFn`]: trait.FilterFn.html
//! [`ExternalFilterFn`]: trait.ExternalFilterFn.html
//! [`Enricher`]: struct.Enricher.html
//! [`Enricher<H, F>`]: struct.Enricher.html
//! [`Fanout`]: struct.Fanout.html
//! [`Fanout<Hs, N>`]: struct.Fanout.html
//! [`BorrowingFanout`]: struct.BorrowingFanout.html
//! [`BorrowingFanout<Hs, N>`]: struct.BorrowingFanout.html
//! [`Shared`]: struct.Shared.html
//! [`Shared<H>`]: struct.Shared.html
//! [`OffloaderHandle`]: struct.OffloaderHandle.html
//! [`TypeErasedArgs`]: struct.TypeErasedArgs.html
//! [`TypeErasedArcLogArgs`]: type.TypeErasedArcLogArgs.html
//! [`TypeErasedArcContextArgs`]: type.TypeErasedArcContextArgs.html
//! [`TypeErasedExternalDescriptor`]: struct.TypeErasedExternalDescriptor.html
//! [`TypeErasedExternalArgs`]: struct.TypeErasedExternalArgs.html
//! [`MixedArgsExt`]: trait.MixedArgsExt.html
//! [`FilterRules`]: struct.FilterRules.html
//! [`FilterRules::parse`]: struct.FilterRules.html#method.parse
//! [`MaxLevel`]: filters::struct.MaxLevel.html
//! [`ConstMaxLevel<M>`]: filters::struct.ConstMaxLevel.html
//! [`Target`]: filters::struct.Target.html
//! [`TargetPrefix`]: filters::struct.TargetPrefix.html
//! [`All`]: filters::struct.All.html
//! [`Any`]: filters::struct.Any.html
//! [`Always`]: filters::struct.Always.html
//! [`Never`]: filters::struct.Never.html
//! [`Not`]: filters::struct.Not.html
//! [`Not<F>`]: filters::struct.Not.html
//! [`Fanout1`]: type.Fanout1.html
//! [`Fanout4`]: type.Fanout4.html
//! [`BorrowingFanout1`]: type.BorrowingFanout1.html
//! [`BorrowingFanout4`]: type.BorrowingFanout4.html
//! [`EnricherFn`]: trait.EnricherFn.html
//! [`SystemTimeEnricher<H>`]: type.SystemTimeEnricher.html
//! [`MonotonicEnricher<H>`]: type.MonotonicEnricher.html
//! [`SystemTimeEnricherFn`]: struct.SystemTimeEnricherFn.html
//! [`OffloaderHandle<Context, CustomEvent>`]: struct.OffloaderHandle.html
//! [`OffloaderService<H, Context, CustomEvent, CustomHandler>`]: struct.OffloaderService.html
//! [`new_offloader(...)`]: fn.new_offloader.html
//! [`OffloaderCapacity`]: enum.OffloaderCapacity.html
//! [`OffloaderBackpressurePolicy`]: enum.OffloaderBackpressurePolicy.html
//! [`OffloaderBackpressurePolicy::Block`]: enum.OffloaderBackpressurePolicy.html#variant.Block
//! [`DropNewest`]: enum.OffloaderBackpressurePolicy.html#variant.DropNewest
//! [`DropOldest`]: enum.OffloaderBackpressurePolicy.html#variant.DropOldest
//! [`OffloaderService::spawn`]: struct.OffloaderService.html#method.spawn
//! [`OffloaderService::run_blocking`]: struct.OffloaderService.html#method.run_blocking
//! [`ExternalDescriptor<'_>`]: trait.ExternalDescriptor.html
//! [`ExternalDescriptor`]: trait.ExternalDescriptor.html
//! [`ExternalHandler<Context>`]: trait.ExternalHandler.html
//! [`ExternalArgs`]: trait.ExternalArgs.html
//! [`ExternalArgsVisitor`]: trait.ExternalArgsVisitor.html
//! [`ExternalArgValue<'_>`]: enum.ExternalArgValue.html
//! [`ViaLog<L>`]: delegators::log_0_4::struct.ViaLog.html
//! [`ViaSlog<D>`]: delegators::slog_2::struct.ViaSlog.html
//! [`TypeErasedArgs<T>`]: struct.TypeErasedArgs.html
//! [`AnyDynMixedArgs`]: trait.AnyDynMixedArgs.html
//! [`AnyDisplayableMixedArgs`]: trait.AnyDisplayableMixedArgs.html
//! [`AnyDebuggableMixedArgs`]: trait.AnyDebuggableMixedArgs.html
//! [`Pologger`]: https://docs.rs/pologger/latest/pologger/struct.Pologger.html
//! [`log::Log`]: https://docs.rs/log/latest/log/trait.Log.html
//! [`slog::Drain`]: https://docs.rs/slog/latest/slog/trait.Drain.html
//! [`slog::Logger`]: https://docs.rs/slog/latest/slog/struct.Logger.html
//! [`tracing::Dispatch`]: https://docs.rs/tracing/latest/tracing/struct.Dispatch.html
//! [`tracing::Subscriber`]: https://docs.rs/tracing/latest/tracing/trait.Subscriber.html
//! [`Either<H1, H2>`]: https://docs.rs/either/latest/either/enum.Either.html

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

mod args;
pub mod bridges;
pub mod context;
pub mod conv;
pub mod delegators;
mod external;
mod middleware;
pub mod util;

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
