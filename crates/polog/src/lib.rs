#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! The `polog` crate provides a performance‑oriented logging and span API.
//!
//! It supports both sync (blocking, same‑thread) formatting and deferred async
//! (non‑blocking, off‑thread) formatting of values of custom types.
//! It eliminates dynamic formatting for sendable values and defers formatting
//! for async loggers.
//!
//!
//! # Introduction
//!
//! Existing logging facades like [`log`], [`tracing`], or [`slog`] force early
//! stringification of non‑primitive value types when logs are sent off‑thread.
//! Also all these crates use dyn‑heavy APIs with unpredictable
//! devirtualization.
//!
//! [`polog`] logging API eliminates these overheads by moving work to compile
//! time: static descriptors, monomorphized calls for sync loggers,
//! into‑sendable conversion called once for all arguments for async loggers.
//! Formatting can be deferred until the logs are actually consumed.
//!
//!
//! # About
//!
//! `polog` is a performance‑oriented structured logging facade.
//! It separates the call‑site API from the backend [`Handler`].
//!
//! Every log or span call site creates a static [`Descriptor`] and passes a
//! heterogeneous tuple of key‑value arguments to the handler.
//! A same‑thread logger can format those arguments directly.
//! An off‑thread logger can convert them into a `'static + Send + Any` form and
//! defer formatting until the event is consumed.
//!
//!
//! # When to use `polog` and when not to
//!
//! Choose `polog` if:
//! - You prefer explicit key‑value structured logs.
//! - You need minimal logging overhead, especially in hot paths.
//! - You are comfortable threading the logger explicitly through the call
//!   graph.
//! - You want the ability to log to be visible in function signatures, with no
//!   hidden thread‑local or global state.
//! - You log from many async tasks and want to avoid formatting before sending.
//! - You can afford slightly higher compile times for better runtime
//!   performance.
//! - You are building an application (not a library) where you control the
//!   logger.
//! - You want the macro layer to be replaceable: every call site expansion is
//!   composed from public functions and traits.
//!
//! Avoid `polog` if:
//! - You rely on ambient span context: thread‑locals, a global logger, or
//!   `#[instrument]`-style automatic spans.
//! - You need compile‑time level filtering through Cargo features like
//!   `max_level_*` or `release_max_level_*`.
//! - You prefer an ergonomic "printf‑like" single‑message logging style.
//! - You are writing a library that must integrate with the existing
//!   `log`/`tracing` ecosystem.
//! - You do not want to thread a logger parameter through every function that
//!   may log or whose children may log.
//! - You need a structured-output backend (JSON, logfmt, protobuf) that keeps
//!   primitive types typed by default. `polog` delivers every argument as its
//!   `Display`-rendered text, so `123` arrives as `"123"`.
//! - You cannot accept the compile‑time and binary‑size impact of
//!   monomorphization.
//! - You need a mature ecosystem of log processors, formatters, and transports
//!   right now.
//!
//!
//! # Usage
//!
//! In libraries add the following to your `Cargo.toml`:
//! ```toml
//! [dependencies]
//! polog = "0.1.0-alpha.1"
//! ```
//!
//! Note that `polog` itself is a logging facade.
//! In executables you need a logger backend that implements the [`Handler`]
//! trait. You can use [`pologger`] as a reference logger backend
//! implementation, and compose optional middleware/utilities from
//! [`polog-util`].
//!
//! ```toml
//! [dependencies]
//! polog = "0.1.0-alpha.1"       # Facade traits and macros.
//! polog-util = "0.1.0-alpha.1"  # Optional utilities and middleware.
//! pologger = "0.1.0-alpha.1"    # Reference logger backend implementation.
//! ```
//!
//!
//! # Example
//!
//! In libraries, pass the logger argument between the functions that use it:
//! ```rust
//! # use polog_test_helpers::accept_all::AcceptAll;
//! # use core::ops::Range;
//! #
//! # let rt = tokio::runtime::Runtime::new().unwrap();
//! # rt.block_on(async move { do_smth(&mut AcceptAll).await });
//! #
//! async fn do_smth<L>(logger: &mut L)
//! where
//!     L: polog::Logger,
//! {
//! #     const ARG2_NAME: &str = "arg2";
//! #
//! #     let string_for_refs = String::from("abc") + "abc";
//! #     let vec_for_refs = vec![1, 2, 3];
//! #
//! #     let int_type = 123;
//! #     let string = String::from("abc");
//! #     let lazy_evaluated = || 123;
//! #     let static_str_ref = "abc";
//! #     let non_static_str_ref = string_for_refs.as_str();
//! #     let slice_ref = vec_for_refs.as_slice();
//! #     let any_type_impls_debug = 1..=10;
//! #     let value1 = 123;
//! #     let value2 = 123;
//! #     let value3 = 123;
//! #     let arg1 = 123;
//! #     let arg2_value = 456;
//! #     let param1 = (123, 234);
//! #     let param2 = 123;
//! #
//!     // ...
//!
//!     use polog::LoggerExt;
//!
//!     polog::info!(logger, "Message 1"; int_type, string, key = lazy_evaluated() + 123);
//!     polog::info!(logger, "Message 2"; static_str_ref, :t non_static_str_ref);
//!     polog::warn!(logger, "Message 3"; :t ? slice_ref, :s ? any_type_impls_debug);
//!
//!     polog::debug!(logger; value1, value2, value3);
//!
//!     let logger = polog::info_span!(logger, "Span example"; arg1, (ARG2_NAME) = arg2_value);
//!
//!     let handle1 = std::thread::spawn({
//!         // `.detach()` clones the handler and captures the current `SpanId`
//!         // without emitting any span events.
//!         // The span itself is created lazily on the spawned thread, so its
//!         // arguments can depend on the thread's state and its enter timestamp is
//!         // accurate.
//!         let mut logger = logger.detach();
//!         move || {
//!             let mut logger = polog::info_span!(&mut logger, "Thread"; ? param1, param2);
//!             some_func(param1, param2, &mut logger)
//!         }
//!     });
//!
//!     let future1 = tokio::task::spawn({
//!         let mut logger = logger.detach();
//!         async move {
//!             let mut logger = polog::info_span!(&mut logger, "Task 1"; ? param1, param2);
//!             some_async_func1(param1, param2, &mut logger).await
//!         }
//!     });
//!
//!     let deferred = polog::deferred_info_span!(&logger, "Task 2"; ? param1, param2);
//!     let future2 = tokio::task::spawn({
//!         // `PollInstrumented` wraps the future so its polls are traced through
//!         // `enter_poll_span`, `pending_poll_span`, and `ready_poll_span` events.
//!         // The instrumented span inherits `logger`'s descriptor and parent
//!         // `SpanId`.
//!         use polog::PollInstrument;
//!         (|mut logger| async move { some_async_func2(param1, param2, &mut logger).await })
//!             .poll_instrument(deferred)
//!     });
//!
//!     // `owning_span!` returns an `OwningSpan` that can be stored.
//!     // Dropping it emits `exit_span`.
//!     // Useful for RAII-style lifetime detection.
//!     let _guard = polog::owning_debug_span!(&logger, "Guard");
//!
//!     future1.await.unwrap();
//!     future2.await.unwrap();
//!     handle1.join().unwrap();
//! }
//! #
//! # fn some_func<L>(param1: (i32, i32), param2: i32, logger: &mut L)
//! # where
//! #     L: polog::Logger,
//! # {
//! # }
//! #
//! # async fn some_async_func1<L>(param1: (i32, i32), param2: i32, logger: &mut L)
//! # where
//! #     L: polog::Logger,
//! # {
//! # }
//! #
//! # async fn some_async_func2<L>(param1: (i32, i32), param2: i32, logger: &mut L)
//! # where
//! #     L: polog::Logger,
//! # {
//! # }
//! ```
//!
//! To create a logger in executables, use logger backend, for example
//! [`pologger`].
//!
//!
//! # Comparison with Existing Crates
//!
//! | Feature / Aspect | [`polog`] | [`log`] | [`tracing`] | [`slog`] |
//! |:----------------:|:---------:|:-------:|:-----------:|:--------:|
//! | Maturity | ⚠️ Not yet mature | ✅ Mature | ✅ Mature | ✅ Mature |
//! | Structured kv (key‑value) support | ✅ Supported | ✅ Supported | ✅ Supported | ✅ Supported |
//! | Span support | ✅ Span | ⛔️ No | ✅ Span | ⛔️ No |
//! | Context support | ✅ Context | ⛔️ No | ✅ Extensions | ✅ Context |
//! | Thread‑local span context | ⛔️ No | ⛔️ No | ✅ `Span::current()` | ⛔️ No |
//! | Dynamic span fields | ⛔️ No | ⛔️ No | ✅ `Span::record` | ⛔️ No |
//! | `#[instrument]` attribute | ⛔️ No | ⛔️ No | ✅ Yes | ⛔️ No |
//! | Deferred formatting of custom types | ✅ Supported | ⛔️ No | ⛔️ No | ⛔️ No |
//! | Single formatted message | ⛔️ No | ✅ Supported | ✅ Supported | ✅ Supported |
//! | Value bounds (without modifiers) | ✅ `IntoSendable + Display` | ✅ `log::kv::ToValue` | ⚠️ Sealed `tracing::Value` | ✅ `slog::Value` |
//! | Supports formatting modifiers | `%`, `?`, `#%`, `#?` | `:%`, `:?` | `%`, `?` | `%`, `?`, `#%`, `#?` |
//! | Structured value output | ⚠️ `Display` text<br>by default | ⚠️ Primitives | ⚠️ Primitives | ✅ `Serializer` |
//! | Sync logger overhead | ✅ None for generic logger | ⚠️ `&dyn VisitSource` | ⚠️ `&dyn Visit` | ⚠️ `&dyn KV` |
//! | Async logger overhead<br>for non‑primitive types| ✅ `IntoSendable` (can be no‑op) | ⚠️ `format!` | ⚠️ `format!` | ⚠️ `format!` |
//! | Static descriptor | ✅ Location, Level,<br>Message, Key names | ⛔️ No | ✅ Location, Level,<br>Message, Key names | ✅ Location, Level |
//! | Compile‑time filtering<br>via Cargo features | ⛔️ No | ✅ `max_level_*` | ✅ `max_level_*` | ✅ `slog‑max‑level-*` |
//! | Disabled events<br>eliminated at compile time | ⚠️ Via optimizer const‑folding | ✅ Always | ✅ Always | ✅ Always |
//! | Non‑const log level | ⚠️ Emulated by the macro codegen | ✅ Yes | ⛔️ No | ⛔️ No |
//! | Global logger support | ⛔️ No | ✅ Yes | ✅ Yes | ⛔️ No |
//! | Parameterized logger support | ✅ Yes | ✅ Yes | ⚠️ `with_dispatch` | ✅ Yes |
//! | API Complexity | ⚠️ Moderate<br>(send behavior modifiers) | ✅ Simple | ✅ Simple | ✅ Simple |
//! | Monomorphization | ✅ Full | ⚠️ `dyn`‑heavy API | ⚠️ `dyn`‑heavy API | ⚠️ `dyn`‑heavy API |
//! | Compile‑time impact | ⚠️ Potentially high,<br/>depends on monomorphization | ✅ Low | ⚠️ Moderate,<br/>complex macro codegen | ✅ Simple |
//! | Ecosystem | ⚠️ Only [`polog`], [`polog-util`]<br/>and [`pologger`] | ✅ Huge | ✅ Huge | ⚠️ Moderate |
//!
//! Table of the approximately closest concepts in log‑related crates:
//!
//! | [`polog`] | [`log`] | [`tracing`] | [`slog`] |
//! |:--:|:--:|:--:|:--:|
//! | `Logger` | `log!` macros / global `log::logger()` | `tracing::Span` / `tracing::Dispatch` | `slog::Logger` |
//! | `SharedHandler<Context>` | `&'static dyn log::Log` / `Arc<dyn Log>` | `tracing::Dispatch` | `slog::Logger` |
//! | `Handler<Context>` | `log::Log` | `tracing::Subscriber` | `slog::Drain` |
//! | `Descriptor` | `log::Metadata` / `log::Record` | `tracing::Metadata` / `tracing::Event` | `slog::RecordStatic` / `slog::Record` |
//! | `LogArgs` / `Arg` / `MixedArgs` | `log::kv::Value` / `log::kv::Source` | `tracing::field::Value` / `ValueSet` | `slog::Value` / `slog::KV` |
//! | `ContextArgs` | — | — | — |
//! | `BorrowingSpan` | — | `tracing::EnteredSpan` | — |
//! | `OwningSpan` | — | — | — |
//! | `DeferredSpan` | — | `tracing::Span` | — |
//! | `DetachedLogger` | — | — | — |
//! | `PollInstrumented` | — | `tracing::Instrumented<F>` | — |
//! | `SpanId` | — | `tracing::span::Id` | — |
//!
//! Span features:
//!
//! | Span feature / Span aspect | [`polog`] | [`tracing`] |
//! |:--------------------------:|:---------:|:-----------:|
//! | Nested sync code Span ID propagation | Explicit borrowing `Span` | Thread‑locals |
//! | Detached sync Span ID propagation | `DetachedLogger` + `span!` | Explicit detached parent span |
//! | Async code Span ID propagation | `DetachedLogger` + `span!` | `Future` wrapping with per‑poll events |
//! | Async Future Instrumenting | `PollInstrumented` wrapping with per‑poll events | `Future` wrapping with per‑poll events |
//! | Sync lifecycle events | `Enter` + `Exit` | `New` + `Enter` + `Exit` + `Close` |
//! | Async lifecycle events | `Enter(Detached)` + `TaskPoll` + `TaskPending` + `TaskReady` + `Exit(Detached)` | `New` + `Enter` + `Exit` + `Close` |
//!
//! Span events usage:
//!
//! | Lifecycle point | Span creation | Sync span entry  | Async poll entry | Async poll pending | Async poll ready | Sync span exit  | Span destruction |
//! |:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|
//! | Called | once | repeatable | per poll | per poll | per poll | repeatable | on drop |
//! | `polog` regular spans | `Enter(Nested/Detached)` | — | — | — | — | — | `Exit(Nested/Detached)` |
//! | `polog` instrumented futures | `Enter(Detached)` | — | `TaskPoll` | `TaskPending` | `TaskReady` | — | `Exit(Detached)` |
//! | `tracing` | `New` | `Enter` | `Enter` | `Exit` | `Exit` | `Exit` | `Close` |
//!
//! `polog` intentionally uses a stricter span lifecycle than [`tracing`].
//! A span is entered exactly once, immediately when it is created, and exited
//! exactly once when it is destroyed.
//! There are no separate New/Close events.
//! For poll‑instrumented futures, the wrapper emits a `SpanKind::Detached`
//! enter/exit pair around the future's lifetime, and per‑poll
//! TaskPoll/TaskPending/TaskReady events between them, so repeated polling is
//! never conflated with re‑entering the span.
//!
//! Additional `polog` features:
//! - [`Level::Fatal`] log level that always panics after logging, with the log
//!   record flushed to the sink before the panic begins.
//! - [`Level::Bug`] log level that conditionally panics after logging if
//!   enabled by the logger, with the log record flushed before the panic
//!   begins.
//! - Macros unifying logging and panic logic like: `log!` with `FATAL` or `BUG`
//!   log levels, `fatal!`, `bug!`, `fatal_assert!`, `fatal_assert_eq!`,
//!   `fatal_assert_ne!`, `bug_assert!`, `bug_assert_eq!`, `bug_assert_ne!`.
//!
//!
//! # API Cheat Sheet
//!
//! Primary logging macros:
//! | Level | Log | Spans | Inspects and mapping | Assertions and unwraps |
//! |:--:|:--:|:--:|:--:|:--:|
//! | passed as argument | [`log!`] | [`span!`],<br>[`owning_span!`],<br>[`deferred_span!`] | [`some_or_log`],<br>[`none_or_log`],<br>[`ok_or_log!`],<br>[`err_or_log!`] | - |
//! | [`Level::Trace`] | [`trace!`] | [`trace_span!`],<br>[`owning_trace_span!`],<br>[`deferred_trace_span!`] | [`some_or_trace!`],<br>[`none_or_trace!`],<br>[`ok_or_trace!`],<br>[`err_or_trace!`] | - |
//! | [`Level::Debug`] | [`debug!`] | [`debug_span!`],<br>[`owning_debug_span!`],<br>[`deferred_debug_span!`] | [`some_or_debug!`],<br>[`none_or_debug!`],<br>[`ok_or_debug!`],<br>[`err_or_debug!`] | - |
//! | [`Level::Info`] | [`info!`] | [`info_span!`],<br>[`owning_info_span!`],<br>[`deferred_info_span!`] | [`some_or_info!`],<br>[`none_or_info!`],<br>[`ok_or_info!`],<br>[`err_or_info!`] | - |
//! | [`Level::Warn`] | [`warn!`] | [`warn_span!`],<br>[`owning_warn_span!`],<br>[`deferred_warn_span!`] | [`some_or_warn!`],<br>[`none_or_warn!`],<br>[`ok_or_warn!`],<br>[`err_or_warn!`] | - |
//! | [`Level::Error`] | [`error!`] | [`error_span!`],<br>[`owning_error_span!`],<br>[`deferred_error_span!`] | [`some_or_error!`],<br>[`none_or_error!`],<br>[`ok_or_error!`],<br>[`err_or_error!`] | - |
//! | [`Level::Bug`] | [`bug!`] | - | - | [`bug_assert!`],<br>[`bug_assert_eq!`],<br>[`bug_assert_ne!`] |
//! | [`Level::Fatal`] | [`fatal!`] | - | - | [`fatal_assert!`],<br>[`fatal_assert_eq!`],<br>[`fatal_assert_ne!`],<br>[`unwrap_some_or_fatal!`],<br>[`unwrap_none_or_fatal!`],<br>[`unwrap_ok_or_fatal!`],<br>[`unwrap_err_or_fatal!`] |
//!
//! Additional helper macros:
//! - [`log_descriptor!`], [`span_descriptor!`], [`log_descriptor_ref!`],
//!   [`span_descriptor_ref!`], [`parse_keys!`], [`parse_values!`],
//!   [`locate_and_parse_values!`], [`uninstantiated_fn_name!`],
//!   [`instantiated_fn_name!`].
//!
//! Primary traits:
//! - [`Logger`]: A frontend for emitting events and managing spans.
//! - [`Handler`]: A backend or middleware log handler.
//! - [`BorrowingHandler`]: A [`Handler`] that can process an event without
//!   consuming its argument tuple. Its methods take `&Args` where `Args:
//!   StaticLogArgs` and `&Self::EnrichedContext`, enabling fanout middleware to
//!   dispatch one converted tuple to multiple branches without cloning.
//! - [`SharedHandler`]: A shared handle to a backend or middleware log handler.
//! - [`DynHandler`]: An object‑safe subset of [`Handler`] covering span
//!   lifecycle events (`exit_span` and the poll callbacks). Used for type
//!   erasure.
//! - [`DebuggableDynHandler`]: A [`DynHandler`] whose concrete type also
//!   implements [`Debug`].
//! - [`IntoSendable`]: A trait to convert a value into sendable static form.
//! - [`LogArgs<'a>`]: A trait for heterogeneous tuples that is used for log
//!   arguments. Implemented for `DisplayableMixedArgs<'a> +
//!   IntoSendable<Sendable: DisplayableMixedArgs<'static>>`.
//! - [`StaticLogArgs`]: The already-converted form of a log argument tuple
//!   (`'static + Send + Sync + DisplayableMixedArgs<'static>`). Accepted by
//!   [`Handler::log_static`] and [`Handler::enter_span_static`]; shareable as
//!   `Arc<Args>` across branches.
//! - [`DisplayableMixedArgs<'a>`]: A trait for heterogeneous tuples of
//!   displayable values that is used for log arguments. Subtrait of
//!   `DynMixedArgs<dyn Display + 'a>`.
//! - [`DynMixedArgs<T>`]: A heterogeneous tuples that is used for log arguments
//!   and log context to access fields as a trait object `&dyn U`. Subtrait of
//!   `MixedArgs`.
//! - [`MixedArgs`]: A heterogeneous tuples that is used for log arguments and
//!   log context.
//! - [`PollInstrument`]: An extension trait for wrapping a future factory with
//!   poll instrumentation.
//!
//! Primary structs:
//! - [`Descriptor`]: Metadata describing a span or event.
//! - [`BorrowingSpan`]: An active span that borrows a handler and emits an exit
//!   event on drop. Created by [`LoggerExt::borrowing_span`] and
//!   [`LoggerExt::transparent_borrowing_span`].
//! - [`OwningSpan`]: An active span that owns a handler and emits an exit event
//!   on drop. Created by materializing [`DeferredSpan`] with
//!   [`DeferredSpan::enter`].
//! - [`DeferredSpan`]: A span that owns a cloned handler and arguments builder
//!   and emits no events until materialized. Created by
//!   [`LoggerExt::deferred_span`].
//! - [`DetachedLogger`]: A logger handle that owns a cloned handler and carries
//!   a parent [`SpanId`], but does not represent a span itself. Created by
//!   [`LoggerExt::detach`].
//! - [`PollInstrumented`]: A future wrapper that emits per‑poll events on a
//!   `Detached` span.
//! - [`SpanId`]: The unique span identifier managed by the logger handler.
//! - [`SpanIdGenerator`]: A thread‑safe generator of [`SpanId`] values.
//! - [`TypeErasedOwningSpan`]: An [`OwningSpan`] whose handler is erased to
//!   `Box<dyn DynHandler<()> + Send + Sync>`, for storing a span in a struct
//!   without a handler‑type parameter. Created by [`OwningSpan::into_erased`].
//! - [`DebuggableTypeErasedOwningSpan`]: Like [`TypeErasedOwningSpan`] but the
//!   erased handler also implements [`Debug`]. Created by
//!   [`OwningSpan::into_debuggable_erased`].
//!
//! Primary enums:
//! - [`Level`]: Severity level for a log entry.
//! - [`LevelFilter`]: Logging filtering level.
//! - [`SpanKind`]: The kind of span: `Nested` or `Detached`.
//!
//!
//! # Macro syntax
//!
//! All logging and span macros share one call‑site grammar:
//! ```rust,ignore
//! $macro!( $logger, $level $(, target: $target)? $(, $message)? $(; $($arg),* $(,)? )? )
//! ```
//!
//! Level‑specific macros fix the level in the macro name and use the shorter
//! form
//! ```rust,ignore
//! $macro!( $logger $(, target: $target)? $(, $message)? $(; $($arg),* $(,)? )? )
//! ```
//!
//! See the [API Cheat Sheet](#api-cheat-sheet) for the full list.
//!
//! ## Dynamic log levels
//!
//! The generic macros `log!`, `span!`, `deferred_span!` accept an optional
//! `:dyn` modifier before the level:
//! ```rust
//! # use polog_test_helpers::accept_all::AcceptAll;
//! # let mut logger = AcceptAll;
//! # let level = polog::Level::Info;
//! # let arg = 123;
//! #
//! polog::log!(&mut logger, :dyn level, "Message"; arg);
//! ```
//!
//! Without `:dyn`, the level must be a constant expression, and a single static
//! descriptor is generated at the call site.
//! With `:dyn`, the macro generates one static descriptor per built‑in level
//! (seven for `log!`, five for `span!` and `deferred_span!`), selects the
//! descriptor whose `level` matches the runtime level value, and runs the
//! enablement check against the selected descriptor.
//!
//! The `:dyn` form therefore trades a runtime `match` for the ability to pass a
//! non‑constant level.
//! Without it, disabled events cost no more than a constant comparison.
//!
//!
//! ## Argument syntax
//!
//! The logger requires every argument to implement
//! `IntoSendable<Sendable: Display> + Display` to be compatible with any
//! logger. A sync logger that stays on the same thread simply ignores
//! `into_sendable()` and uses `Display` to format the value.
//! An async/off‑thread logger can use `IntoSendable::into_sendable()` to obtain
//! arguments implementing `'static + Send + Any + Display` and therefore is not
//! forced to format everything eagerly on the log‑producing thread.
//!
//!
//! ### Argument Keys
//!
//! Keys are static `&'static str` names stored in [`Descriptor`].
//! They identify structured values and are intended to be displayed by
//! backends. Keys must precede send and formatting modifiers.
//!
//! | Syntax                | Resulting key         | Notes |
//! |:---------------------:|:---------------------:|:----------------------------------------------------------|
//! | `foo = expr`          | `"foo"`               | Identifier keys are stringified.                          |
//! | `"foo" = expr`        | `"foo"`               | String literal keys are used verbatim.                    |
//! | `(CONST_EXPR) = expr` | value of `CONST_EXPR` | Must resolve to `&'static str`.                           |
//! | `expr`                | `stringify!(expr)`    | Implicit key derived from the value expression.           |
//! | `@meta = expr`        | `"@meta"`             | Meta‑attribute. Not displayed by default.                 |
//! | `@fn`                 | `"@fn"`               | Shorthand for `@fn = ::polog::uninstantiated_fn_name!()`. |
//!
//! Examples:
//!
//! ```rust
//! # use polog_test_helpers::accept_all::AcceptAll;
//! # use core::time::Duration;
//! # const KEY_CONST: &str = "key_const";
//! # let mut logger = AcceptAll;
//! # let foo = 123;
//! #
//! polog::info!(
//!     &mut logger,
//!     "Message";
//!     @fn,                // key = "@fn", meta-attributes won't be displayed
//!     foo,                // key = "foo"
//!     bar = 123,          // key = "bar"
//!     "baz" = 456,        // key = "baz"
//!     (KEY_CONST) = 789,  // key = KEY_CONST
//! );
//! ```
//!
//! ### Values
//!
//! Values are ordinary Rust expressions.
//! They can be wrapped with send modifiers and formatting modifiers.
//!
//! #### Send modifiers
//!
//! Send modifiers control how a value is converted for off‑thread loggers.
//! They are no‑op for same‑thread loggers.
//!
//! | Macro modifier | Type/Wrapper | Behavior for async/off‑thread loggers |
//! |:--------------:|:------------:|:--------------------------------------|
//! | (none) | `T` (no wrapper) | Compiles only for most `T: 'static + Send + Display` std types<br>and other types implementing `IntoSendable`.<br>Zero‑cost, unless explicitly wrapped with a sendable modifier wrapper. |
//! | `:a` | `AlreadySendable<T>` | Compiles for any `T: 'static + Send + Display`. Zero‑cost. |
//! | `:t` | `Temporary<T>` | Calls `TemporaryIntoSendable::temp_into_sendable()`.<br>Usually uses `clone`, `to_owned`, `collect` and other similar methods recursively. |
//! | `:s` | `Stringifiable<T>` | Formats via `into_string()` into `String`. Affected by formatting modifiers.<br>Formatted using `Display` or `Debug` depending on formatting modifiers.<br> |
//!
//! You can also use `value.sendable_with(|v| v.custom_to_owning_fn())` to wrap
//! a value into `SendableWith`, or implement `IntoSendable` /
//! `TemporaryIntoSendable` for your own types.
//!
//! #### Formatting modifiers
//!
//! Formatting modifiers control how a value is rendered without affecting
//! sendability.
//! They wrap values into the wrappers that delegate their `Display` and `Debug`
//! implementations to specific formatting implementations.
//!
//! | Macro modifier | Wrapper | Behavior |
//! |:--------------:|:-------:|:---------|
//! | (none) | `T` (none) | Uses `Display` formatting |
//! | `%` | `ViaDisplay<T>` | Uses `Display` formatting. |
//! | `?` | `ViaDebug<T>` | Uses `Debug` formatting. |
//! | `#%` | `ViaAltDisplay<T>` | Uses alternative `Display` formatting. |
//! | `#?` | `ViaAltDebug<T>` | Uses alternative `Debug` formatting. |
//!
//! The default formatting using `Display` is subject to change in future major
//! releases.
//!
//! #### Special value syntaxes
//!
//! - `:f(fmt_string, ..args)` formats the argument using a Rust formatting
//!   string. `:f` cannot be combined with send or format modifiers.
//! - `:r(..args)` passes multiple log arguments as a single `MixedArgs` tuple.
//!   `:r` cannot be combined with send or format modifiers and can be used only
//!   with a separately specified descriptor.
//!
//! Examples:
//!
//! ```rust
//! # use polog_test_helpers::accept_all::AcceptAll;
//! # use polog::DecorateSendable;
//! # use std::borrow::Cow;
//! # let mut logger = AcceptAll;
//! # let custom_static_sendable_displayable = core::net::Ipv4Addr::LOCALHOST;
//! # let some_string = String::from("abc") + "abc";
//! # let any_display_impl_type = &core::net::Ipv4Addr::LOCALHOST;
//! # let any_debug_impl_type = &(1, 2, 3);
//! # let smth = Cow::Borrowed("abc");
//! # let hi = 0x12;
//! # let lo = 0x34;
//! # let arg1 = 123;
//! # let arg2 = 234;
//! # let arg3 = 345;
//! #
//! polog::info!(&mut logger, "Message"; str = "static str", int = 12345, float = 12.34);
//! polog::info!(&mut logger, "Message"; :a custom_static_sendable_displayable);
//! polog::info!(&mut logger, "Message"; :t some_string.as_str() );
//! polog::info!(&mut logger, "Message"; :s % any_display_impl_type );
//! polog::info!(&mut logger, "Message"; :s ? any_debug_impl_type );
//! polog::info!(&mut logger, "Message"; smth.sendable_with(|smth: Cow<'_, str>| smth.into_owned()) );
//!
//! polog::info!(&mut logger, "Hexadecimal fraction"; value = :f("{:02x}.{:02x}", hi, lo));
//!
//! const DESCRIPTOR_REF: &polog::Descriptor =
//!     polog::log_descriptor_ref!(polog::Level::Debug, "Raw args"; arg1, arg2, arg3);
//! let args_as_single_tuple = (polog::Arg(arg1), polog::Arg(arg2), polog::Arg(arg3));
//! polog::log!(&mut logger, descriptor_ref: DESCRIPTOR_REF; :r(args_as_single_tuple));
//! ```
//!
//! ### Meta‑attributes
//!
//! Meta‑attributes are key‑value arguments whose key starts with `@`.
//! They are not intended for display.
//! Backends may use them to alter how an event or span is processed.
//! Unknown meta‑attributes should be ignored, so a backend may define its own
//! beyond the canonical set below.
//!
//! Canonical meta‑attribute:
//! - `@fn`: The current function name. Backends may use this value instead of
//!   an empty message. Type: `&'static str`. Has special shorthand syntax `@fn`
//!   which expands to `@fn = polog::uninstantiated_fn_name!()`.
//!
//! Values that describe the execution context — OS thread ID and name, task ID,
//! wall-clock and monotonic timestamps, CPU ID — are not meta‑attributes.
//! They are per‑event context data and belong in context args, where middleware
//! can populate them from the environment actually visible on the producing
//! thread.
//!
//! Example:
//!
//! ```rust
//! # use polog_test_helpers::accept_all::AcceptAll;
//! # foo_bar_baz(&mut AcceptAll);
//! # abc_def_ghi(&mut AcceptAll);
//! #
//! fn foo_bar_baz(logger: &mut impl polog::Logger) {
//! #     let request_id = 123;
//! #     let method = "GET";
//!     let logger = polog::info_span!(logger; @fn, request_id, % method);
//!     // Log an entry with the message derived from function name: "foo_bar_baz".
//! }
//!
//! fn abc_def_ghi(logger: &mut impl polog::Logger) {
//! #     let request_id = 123;
//! #     let method = "GET";
//!     let logger = polog::info_span!(logger; @fn, request_id, % method);
//!     // Log an entry with the message derived from function name: "abc_def_ghi".
//! }
//! ```
//!
//! ## Macro and macro expansion example
//!
//! ```rust
//! # use polog_test_helpers::accept_all::AcceptAll;
//! # struct Expr;
//! # impl Expr {
//! #     fn can(&self) -> i32 {
//! #         123
//! #     }
//! # }
//! # const KEY_VALUE: &str = "key";
//! # let mut logger = AcceptAll;
//! # let std_print = 123;
//! # let std_display = 123;
//! # let std_debug = [1, 2, 3];
//! # let owned_prim_print = core::net::Ipv4Addr::LOCALHOST;
//! # let owned_prim_display = core::net::Ipv4Addr::LOCALHOST;
//! # let owned_prim_debug = std::ffi::OsStr::new("foo");
//! # let temp_prim_print = 123;
//! # let temp_prim_display = 123;
//! # let temp_prim_debug = [1, 2, 3];
//! # let any_prim_print = &core::net::Ipv4Addr::LOCALHOST;
//! # let any_prim_display = &core::net::Ipv4Addr::LOCALHOST;
//! # let any_prim_debug = &std::ffi::OsStr::new("foo");
//! # let expressions = Expr;
//! # let be = 123;
//! # let used = 234;
//! # let as_ = 345;
//! # let well = 456;
//! # let keys_are_optional = 567;
//! # let for_most_values = 678;
//! # let explicit_keys_may_be_identifiers = 123;
//! # let explicit_keys_may_be_string_literals = 234;
//! # let explicit_keys_may_be_const_exprs = 345;
//! # let hex = 0x1234;
//! # let float = 12.34;
//! # let (var1, var2, var3, var4) = (123, 234, 345, 456);
//! #
//! polog::error!(
//!     &mut logger,
//!     "Oh no!";
//!          std_print,           // Most owned stdlib types like `i32` or `&'static str`.
//!        % std_display,         // Most owned stdlib `Display` types like `i32` or `&'static str`.
//!        ? std_debug,           // Most owned stdlib `Debug` types like `[u8; 16]` or `Option<bool>`.
//!
//!     :a   owned_prim_print,    // Any owned type impl `Display`.
//!     :a % owned_prim_display,  // Any owned type impl `Display` like `mpsc::TrySendError<T>`.
//!     :a ? owned_prim_debug,    // Any owned type impl `Debug` like `[u8; 16]`.
//!
//!     :t   temp_prim_print,     // Some borrowed type impl `Display`.
//!     :t % temp_prim_display,   // Some borrowed type impl `Display` like `&str`.
//!     :t ? temp_prim_debug,     // Some borrowed type impl `Debug` like `&[u8]`.
//!
//!     :s   any_prim_print,     // Any borrowed type impl `Display`.
//!     :s % any_prim_display,   // Any borrowed type impl `Display` like `str::EscapeUnicode<'a>`.
//!     :s ? any_prim_debug,     // Any borrowed type impl `Debug` like `&[u8]`.
//!
//!     expressions.can() - (be + used) + as_ * well,
//!     keys_are_optional + for_most_values,
//!     key1 = explicit_keys_may_be_identifiers,
//!     "key2" = explicit_keys_may_be_string_literals,
//!     (KEY_VALUE) = explicit_keys_may_be_const_exprs,
//!     key3 = :f(
//!         "keys is mandatory for `:f` exprs, {hex:02x}{:3}{float:.5e}{:3}",
//!         var1 + var2,
//!         var3 + var4,
//!     ),
//! );
//! ```
//!
//! <details>
//! <summary>Macro expansion (click to show)</summary>
//!
//! ```rust
//! # use polog_test_helpers::accept_all::AcceptAll;
//! #
//! # struct Expr;
//! # impl Expr {
//! #     fn can(&self) -> i32 {
//! #         123
//! #     }
//! # }
//! # const KEY_VALUE: &str = "key";
//! # let mut logger = AcceptAll;
//! # let std_print = 123;
//! # let std_display = 123;
//! # let std_debug = [1, 2, 3];
//! # let owned_prim_print = core::net::Ipv4Addr::LOCALHOST;
//! # let owned_prim_display = core::net::Ipv4Addr::LOCALHOST;
//! # let owned_prim_debug = std::ffi::OsStr::new("foo");
//! # let temp_prim_print = 123;
//! # let temp_prim_display = 123;
//! # let temp_prim_debug = [1, 2, 3];
//! # let any_prim_print = &core::net::Ipv4Addr::LOCALHOST;
//! # let any_prim_display = &core::net::Ipv4Addr::LOCALHOST;
//! # let any_prim_debug = &std::ffi::OsStr::new("foo");
//! # let expressions = Expr;
//! # let be = 123;
//! # let used = 234;
//! # let as_ = 345;
//! # let well = 456;
//! # let keys_are_optional = 567;
//! # let for_most_values = 678;
//! # let explicit_keys_may_be_identifiers = 123;
//! # let explicit_keys_may_be_string_literals = 234;
//! # let explicit_keys_may_be_const_exprs = 345;
//! # let hex = 0x1234;
//! # let float = 12.34;
//! # let (var1, var2, var3, var4) = (123, 234, 345, 456);
//! #
//! {
//!     let descriptor: &'static polog::Descriptor = {
//!         static __DESCRIPTOR: polog::Descriptor = polog::Descriptor {
//!             kind: polog::DescriptorKind::Log,
//!             level: polog::Level::Error,
//!             module_path: ::core::module_path!(),
//!             target: None,
//!             message: "Oh no!",
//!             param_names: &[
//!                 "std_print",
//!                 "std_display",
//!                 "std_debug",
//!                 "owned_prim_print",
//!                 "owned_prim_display",
//!                 "owned_prim_debug",
//!                 "temp_prim_print",
//!                 "temp_prim_display",
//!                 "temp_prim_debug",
//!                 "any_prim_print",
//!                 "any_prim_display",
//!                 "any_prim_debug",
//!                 "(expressions . can () - (be + used) + as_ * well)",
//!                 "keys_are_optional + for_most_values",
//!                 "key1",
//!                 "key2",
//!                 KEY_VALUE,
//!                 "key3",
//!             ],
//!             location: *::core::panic::Location::caller(),
//!         };
//!         &__DESCRIPTOR
//!     };
//!     let logger = &mut *(&mut logger);
//!     if let Some(context) = polog::LoggerExt::try_enable(logger, descriptor) {
//!         polog::LoggerExt::log(
//!             logger,
//!             descriptor,
//!             // Internally passes arguments as a nested tuple.
//!             (
//!                 (
//!                     polog::Arg(std_print),
//!                     polog::Arg(polog::ViaDisplay(std_display)),
//!                     polog::Arg(polog::ViaDebug(std_debug)),
//!                     polog::Arg(polog::AlreadySendable(owned_prim_print)),
//!                     polog::Arg(polog::AlreadySendable(polog::ViaDisplay(
//!                         owned_prim_display,
//!                     ))),
//!                     polog::Arg(polog::AlreadySendable(polog::ViaDebug(owned_prim_debug))),
//!                     polog::Arg(polog::Temporary(temp_prim_print)),
//!                     polog::Arg(polog::Temporary(polog::ViaDisplay(temp_prim_display))),
//!                     polog::Arg(polog::Temporary(polog::ViaDebug(temp_prim_debug))),
//!                     polog::Arg(polog::Stringifiable(any_prim_print)),
//!                     polog::Arg(polog::Stringifiable(polog::ViaDisplay(any_prim_display))),
//!                     polog::Arg(polog::Stringifiable(polog::ViaDebug(any_prim_debug))),
//!                 ),
//!                 polog::Arg(expressions.can() - (be + used) + as_ * well),
//!                 polog::Arg(keys_are_optional + for_most_values),
//!                 polog::Arg(explicit_keys_may_be_identifiers),
//!                 polog::Arg(explicit_keys_may_be_string_literals),
//!                 polog::Arg(explicit_keys_may_be_const_exprs),
//!                 polog::Arg(polog::Stringifiable(core::format_args!(
//!                     "keys is mandatory for `:f` exprs, {hex:02x}{:3}{float:.5e}{:3}",
//!                     var1 + var2,
//!                     var3 + var4,
//!                 ))),
//!             ),
//!             context,
//!         );
//!     }
//!     polog::LoggerExt::panic_if_critical(logger, descriptor);
//! }
//! ```
//!
//! </details>
//!
//!
//! # Span support
//!
//!
//! Spans track hierarchical regions of execution.
//! `polog` uses explicit parent span borrowing or detaching, giving the caller
//! full control over span ancestry and lifetime.
//!
//! ## Span types
//!
//! `polog` exposes three span constructors, each producing a distinct type:
//!
//! | Macro | Returns | Handler ownership | Emits enter/exit | Cross‑thread |
//! |:--:|:--:|:--:|:--:|:--:|
//! | [`span!`] | [`BorrowingSpan<'a, H>`] | borrowed (`&'a mut H`) | yes | no |
//! | [`owning_span!`] | [`OwningSpan<H>`] | owned (`H`) | yes | yes (if `H: Send`) |
//! | [`deferred_span!`] | [`DeferredSpan<H, _>`] | owned (cloned) | no (until materialized) | yes (if `H: Send`) |
//!
//! In addition, [`LoggerExt::detach`] creates a [`DetachedLogger<H>`]: a
//! lightweight logger handle that carries a cloned handler and the current
//! [`SpanId`] but does not represent a span itself.
//! A span created from a `DetachedLogger` (for example, with `span!`) is
//! reported as [`SpanKind::Detached`], because the original logger is not
//! blocked by it.
//!
//! ## Choosing a span type
//!
//! - **Use [`span!`] when the span lives within the same scope as the parent.**
//!   The returned [`BorrowingSpan`] borrows the handler, so the borrow checker
//!   enforces that the parent cannot emit events until the child is dropped.
//!   This is the default choice and corresponds to `tracing::EnteredSpan` /
//!   `slog::Logger` guards.
//!
//! - **Use [`owning_span!`] when the span must be stored or moved.** The
//!   returned [`OwningSpan`] owns a cloned handler and can be placed in a
//!   struct field, returned from a function, or sent to another thread. The
//!   parent is no longer blocked, so the span is reported as
//!   [`SpanKind::Detached`]. The canonical use case is RAII: store the span in
//!   a struct whose `Drop` marks the end of the struct's lifetime.
//!
//! - **Use [`deferred_span!`] when the span's arguments or its entry point are
//!   not yet known.** The returned [`DeferredSpan`] captures the descriptor, a
//!   lazy argument factory, and a cloned handler without emitting any events.
//!   Call [`DeferredSpan::enter`] later to materialize an [`OwningSpan`], or
//!   pass it to [`PollInstrument::poll_instrument`] to wrap a future with
//!   per‑poll events.
//!
//! - **Use [`LoggerExt::detach`] when a span context must cross a thread or
//!   task boundary** but the span itself does not need to be stored as a value.
//!   The returned [`DetachedLogger`] is a lightweight logger; create spans from
//!   it inside the spawned thread or task where their arguments are known.
//!
//! ## Type erasure for library authors
//!
//! A library that stores an owning span in a struct must otherwise carry
//! `H: Handler<()>` through every type that touches that struct.
//! [`TypeErasedOwningSpan`] erases the handler to
//! `Box<dyn DynHandler<()> + Send + Sync>`, so the struct no longer needs the
//! handler parameter and remains `Send + Sync`.
//!
//! ```rust
//! # use polog_test_helpers::accept_all::AcceptAll;
//! # let mut logger = AcceptAll;
//! #
//! use polog::TypeErasedOwningSpan;
//!
//! pub struct Guard {
//!     span: TypeErasedOwningSpan,
//! }
//!
//! impl Guard {
//!     pub fn new(logger: &impl polog::Logger) -> Self {
//!         let span = polog::owning_info_span!(logger, "Guard"; acquired = true)
//!             .into_erased();
//!         Self { span }
//!     }
//! }
//!
//! let guard = Guard::new(&logger);
//! // ...
//! drop(guard);
//! // Dropping `Guard` drops the erased span, which emits `exit_span` on the
//! // underlying handler.
//! ```
//!
//! [`DebuggableTypeErasedOwningSpan`] is the same, but the erased handler also
//! implements `Debug`, so the span can be formatted for diagnostics.
//! See [`DynHandler`] for the object‑safe subset of [`Handler`] that makes
//! erasure possible.
//!
//! ## Example
//!
//! Example with `polog`:
//! ```rust
//! # use polog_test_helpers::accept_all::AcceptAll;
//! # let rt = tokio::runtime::Runtime::new().unwrap();
//! # rt.block_on(async move { example(&mut AcceptAll).await });
//! #
//! use polog::LoggerExt;
//!
//! async fn example<L>(logger: &mut L)
//! where
//!     L: polog::Logger,
//! {
//!     let mut logger = polog::trace_span!(logger, "Example fn.");
//!     polog::info!(&mut logger, "Some info 1.");
//!     polog::warn!(&mut logger, "Some warn 1.");
//!
//!     sync_fn(&mut logger);
//!     async_fn(&mut logger).await;
//!
//!     let handle1 = std::thread::spawn({
//!         let mut logger = logger.detach();
//!         move || {
//!             let mut logger = polog::trace_span!(&mut logger, "Detached Sync Fn.");
//!             sync_fn(&mut logger)
//!         }
//!     });
//!
//!     let future1 = tokio::task::spawn({
//!         let mut logger = logger.detach();
//!         async move {
//!             let mut logger = polog::trace_span!(&mut logger, "Detached Async Fn.");
//!             async_fn(&mut logger).await
//!         }
//!     });
//!
//!     let deferred = polog::deferred_trace_span!(&mut logger, "Instrumented Async Fn.");
//!     // `poll_instrument` takes a `DeferredSpan` and materializes it as a
//!     // `Detached` span with per-poll events on top.
//!     let future2 = tokio::task::spawn({
//!         use polog::PollInstrument;
//!         (|mut logger| async move { async_fn(&mut logger).await }).poll_instrument(deferred)
//!     });
//!
//!     future1.await.unwrap();
//!     future2.await.unwrap();
//!     handle1.join().unwrap();
//! }
//!
//! fn sync_fn<L>(logger: &mut L)
//! where
//!     L: polog::Logger,
//! {
//!     let mut logger = polog::trace_span!(logger, "Sync Fn.");
//!     polog::info!(&mut logger, "Some info 2.");
//!     polog::warn!(&mut logger, "Some warn 2.");
//! }
//!
//! async fn async_fn<L>(logger: &mut L)
//! where
//!     L: polog::Logger,
//! {
//!     let mut logger = polog::trace_span!(logger, "Async Fn.");
//!     polog::info!(&mut logger, "Some info 3.");
//!     polog::warn!(&mut logger, "Some warn 3.");
//! }
//! ```
//!
//! <details>
//! <summary>Similar example using tracing crate for comparison</summary>
//!
//! ```rust
//! use tracing::Instrument;
//! #
//! # let rt = tokio::runtime::Runtime::new().unwrap();
//! # rt.block_on(example());
//!
//! async fn example() {
//!     let mut _guard = tracing::trace_span!("Example fn.").entered();
//!     tracing::info!("Some info 1.");
//!     tracing::warn!("Some warn 1.");
//!
//!     sync_fn();
//!     async_fn().await;
//!
//!     std::thread::spawn({
//!         let span = tracing::trace_span!("Detached Sync Fn");
//!         move || {
//!             let _guard = span.entered();
//!             sync_fn()
//!         }
//!     });
//!
//!     tokio::task::spawn({
//!         let span = tracing::trace_span!("Instrumented Async Fn");
//!         async { async_fn().await }.instrument(span)
//!     });
//! }
//!
//! fn sync_fn() {
//!     let mut _guard = tracing::trace_span!("Sync Fn.").entered();
//!     tracing::info!("Some info 2.");
//!     tracing::warn!("Some warn 2.");
//! }
//!
//! async fn async_fn() {
//!     let mut _guard = tracing::trace_span!("Async Fn.").entered();
//!     tracing::info!("Some info 3.");
//!     tracing::warn!("Some warn 3.");
//! }
//! ```
//!
//! </details>
//!
//!
//! # Data categories
//!
//! Each event and each span carries three orthogonal categories of data with
//! distinct lifetimes, ownership, and visibility:
//!
//! | Category | Type | Origin | Purpose |
//! |:--:|:--:|:--:|:--:|
//! | Descriptor | [`Descriptor`] | Macro‑generated `'static` | Level, module path, optional target,<br>message, key names, source location. |
//! | Log args | `impl `[`LogArgs<'a>`] | Call site | User‑supplied structured values. |
//! | Context args | `impl `[`ContextArgs`] | Middleware `Handler` | Metadata injected by the middleware<br>around a call site. |
//!
//! ## Descriptor
//!
//! Statically generated at compile time per call site (for each macro
//! expansion). Contains severity level, module path, optional target, message
//! string, the parameter names, and the source location.
//! Because it is `'static`, the whole `try_enable` filter path can run against
//! it before any per‑event data is constructed.
//!
//! ## Log args
//!
//! Values supplied by the caller at the macro call site.
//! Their types and order are statically known, which is what enables `polog` to
//! convert them into a `'static + Send` form once for deferred loggers, and to
//! format them without dynamic dispatch for sync loggers.
//!
//! ### Argument views
//!
//! The same argument tuple passes through the handler chain in one of three
//! views:
//!
//! | View | Type | Bounds | Sharable |
//! |:--:|:--:|:--:|:--:|
//! | Caller | `impl LogArgs<'a>` | `'a + DisplayableMixedArgs<'a> + IntoSendable<Sendable: DisplayableMixedArgs<'static>>` | No |
//! | Static | `impl StaticLogArgs` | `'static + Send + Sync + DisplayableMixedArgs<'static>` | `Arc<Args>` |
//! | Borrowed | `&impl StaticLogArgs` | `'static + Send + Sync + DisplayableMixedArgs<'static>` | `&Args` |
//!
//! `Handler::log` and `Handler::enter_span` take the caller view.
//! The handler can format it in place or consume it to produce the static
//! view.
//! `Handler::log_static` and `Handler::enter_span_static` take the static
//! view, which is what an offloader worker holds after the conversion has run.
//! `BorrowingHandler::log_borrowed` and `enter_span_borrowed` take the borrowed
//! view, which fanout middleware passes to each branch without allocating.
//!
//! Two properties matter for correct composition:
//! - `IntoSendable::into_sendable()` consumes the tuple, so a caller view can
//!   be static only once. Middleware that dispatches the same event to more
//!   than one branch converts once and shares the result.
//! - The `Send + Sync` bounds on `StaticLogArgs` are what allow `Arc<Args>` to
//!   be shared across branches.
//!
//! ## Context args
//!
//! Values injected by a middleware [`Handler`] that wraps another `Handler`.
//! They are the mechanism by which middleware attaches per‑invocation metadata
//! that is:
//! - not user data and not derivable at the call site (otherwise it would be a
//!   log arg),
//! - not stable enough to belong in a span (otherwise it would be a log arg),
//! - not stable enough to belong in a descriptor (otherwise it would be in the
//!   descriptor).
//!
//! Because a middleware sees *every* event and span that flows through it,
//! context should be data that changes often.
//! Examples of the good context values: `SystemTime`, `Instant`, current CPU
//! core.
//!
//! Static information belongs in a span, not in context.
//!
//!
//! # Logger backend examples
//!
//! If you do not need to implement a backend manually, use [`pologger`].
//! If you need off‑thread handling, filtering, enrichment, or type erasure, see
//! [`polog-util`].
//!
//! Same‑thread logger:
//! ```rust
//! use std::io::{self, Write};
//!
//! use polog::{Descriptor, SpanId, SpanKind};
//!
//! #[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
//! pub struct SameThreadLogger;
//!
//! static SPAN_ID_GENERATOR: polog::SpanIdGenerator = polog::SpanIdGenerator::new();
//!
//! impl SameThreadLogger {
//!     #[inline]
//!     fn log_impl<'a, Args>(
//!         &mut self,
//!         descriptor: &'static Descriptor,
//!         args: &Args,
//!     ) where
//!         Args: polog::DisplayableMixedArgs<'a>,
//!     {
//!         let stdout = io::stdout();
//!         let mut stdout = stdout.lock();
//!         write!(
//!             stdout,
//!             "{} {} {}",
//!             descriptor.level.as_lowercase_str(),
//!             descriptor.target.unwrap_or(descriptor.module_path),
//!             descriptor.message,
//!         );
//!         for arg_no in (0..args.num_args()).take(descriptor.param_names.len()) {
//!             write!(
//!                 &mut stdout,
//!                 " {} = {}",
//!                 descriptor.param_names[arg_no],
//!                 core::fmt::from_fn(|f| args.display_fmt_nth(f, arg_no).unwrap()),
//!             );
//!         }
//!         writeln!(stdout);
//!     }
//! }
//!
//! impl polog::BaseHandler for SameThreadLogger {
//!     fn generate_span_id(&self) -> SpanId {
//!         SPAN_ID_GENERATOR.generate()
//!     }
//!
//!     fn flush_blocking(&mut self) {}
//!     fn flush_on_panic(&mut self) {}
//! }
//!
//! impl<Context> polog::DynHandler<Context> for SameThreadLogger
//! where
//!     Context: polog::ContextArgs,
//! {
//!     #[inline]
//!     fn exit_span(&mut self, _: &'static Descriptor, _: SpanId, _: SpanKind, _: Context) {
//!         // ...
//!     }
//!
//!     #[inline]
//!     fn enter_poll_span(&mut self, _: &'static Descriptor, _: SpanId, _: Context) {
//!         // ...
//!     }
//!
//!     #[inline]
//!     fn pending_poll_span(&mut self, _: &'static Descriptor, _: SpanId, _: Context) {
//!         // ...
//!     }
//!
//!     #[inline]
//!     fn ready_poll_span(&mut self, _: &'static Descriptor, _: SpanId, _: Context) {
//!         // ...
//!     }
//! }
//!
//! impl<Context> polog::Handler<Context> for SameThreadLogger
//! where
//!     Context: polog::ContextArgs,
//! {
//!     type EnrichedContext = Context;
//!
//!     #[inline]
//!     fn try_enable(
//!         &self,
//!         descriptor: &'static Descriptor,
//!         context: Context,
//!     ) -> Option<Self::EnrichedContext> {
//!         (descriptor.level <= polog::Level::Info).then_some(context)
//!     }
//!
//!     #[inline]
//!     fn log<'a, Args>(
//!         &mut self,
//!         descriptor: &'static Descriptor,
//!         args: Args,
//!         _span_id: Option<SpanId>,
//!         _context: Context,
//!     ) where
//!         Args: polog::LogArgs<'a>,
//!     {
//!         self.log_impl(descriptor, &args);
//!     }
//!
//!     fn enter_span<'a, Args>(
//!         &mut self,
//!         descriptor: &'static Descriptor,
//!         args: Args,
//!         parent_span_id: Option<SpanId>,
//!         span_id: SpanId,
//!         span_kind: SpanKind,
//!         context: Self::EnrichedContext,
//!     ) where
//!         Args: polog::LogArgs<'a>,
//!     {
//!         // ...
//!     }
//!
//!     fn log_static<Args>(
//!         &mut self,
//!         descriptor: &'static Descriptor,
//!         args: Args,
//!         span_id: Option<SpanId>,
//!         context: Self::EnrichedContext,
//!     ) where
//!         Args: polog::StaticLogArgs
//!     {
//!         self.log_impl(descriptor, &args);
//!     }
//!
//!     fn enter_span_static<Args>(
//!         &mut self,
//!         descriptor: &'static Descriptor,
//!         args: Args,
//!         parent_span_id: Option<SpanId>,
//!         span_id: SpanId,
//!         span_kind: SpanKind,
//!         context: Self::EnrichedContext,
//!     ) where
//!         Args: polog::StaticLogArgs
//!     {
//!         // ...
//!     }
//! }
//!
//! impl<Context> polog::BorrowingHandler<Context> for SameThreadLogger
//! where
//!     Context: polog::ContextArgs
//! {
//!     fn log_borrowed<Args>(
//!         &mut self,
//!         descriptor: &'static Descriptor,
//!         args: &Args,
//!         _span_id: Option<SpanId>,
//!         _context: &Self::EnrichedContext,
//!     ) where
//!         Args: polog::StaticLogArgs
//!     {
//!         self.log_impl(descriptor, args);
//!     }
//!
//!     fn enter_span_borrowed<Args>(
//!         &mut self,
//!         descriptor: &'static Descriptor,
//!         args: &Args,
//!         parent_span_id: Option<SpanId>,
//!         span_id: SpanId,
//!         span_kind: SpanKind,
//!         context: &Self::EnrichedContext,
//!     ) where
//!         Args: polog::StaticLogArgs
//!     {
//!         // ...
//!     }
//! }
//!
//! impl<Context> polog::SharedHandler<Context> for SameThreadLogger where Context: polog::ContextArgs {}
//!
//! let mut logger = SameThreadLogger;
//! polog::info!(&mut logger, "Message"; arg1 = "value1");
//! let span = polog::debug_span!(&mut logger, "Span"; arg2 = 123);
//! drop(span);
//! #
//! # use polog_test_helpers::check_handler;
//! # check_handler::check_basic_dyn_handler_fns(&mut SameThreadLogger);
//! # check_handler::check_basic_handler_fns(&mut SameThreadLogger, None);
//! # check_handler::check_basic_borrowing_handler_fns(&mut SameThreadLogger, None);
//! # check_handler::check_basic_shared_handler_fns(&mut SameThreadLogger);
//! ```
//!
//! Off‑thread logger:
//! ```rust
//! use std::io::{self, Write};
//!
//! use polog::{Descriptor, SpanId, SpanKind};
//!
//! #[derive(Clone, Debug)]
//! pub struct OffThreadLogger(
//!     tokio::sync::mpsc::UnboundedSender<(
//!         &'static Descriptor,
//!         Box<dyn 'static + Send + polog::DynDisplayableMixedArgs<'static>>,
//!     )>,
//! );
//!
//! static SPAN_ID_GENERATOR: polog::SpanIdGenerator = polog::SpanIdGenerator::new();
//!
//! impl polog::BaseHandler for OffThreadLogger {
//!     fn generate_span_id(&self) -> SpanId {
//!         SPAN_ID_GENERATOR.generate()
//!     }
//!
//!     fn flush_blocking(&mut self) {}
//!     fn flush_on_panic(&mut self) {}
//! }
//!
//! impl<Context> polog::DynHandler<Context> for OffThreadLogger
//! where
//!     Context: polog::ContextArgs,
//! {
//!     #[inline]
//!     fn exit_span(&mut self, _: &'static Descriptor, _: SpanId, _: SpanKind, _: Context) {
//!         // ...
//!     }
//!
//!     #[inline]
//!     fn enter_poll_span(&mut self, _: &'static Descriptor, _: SpanId, _: Context) {
//!         // ...
//!     }
//!
//!     #[inline]
//!     fn pending_poll_span(&mut self, _: &'static Descriptor, _: SpanId, _: Context) {
//!         // ...
//!     }
//!
//!     #[inline]
//!     fn ready_poll_span(&mut self, _: &'static Descriptor, _: SpanId, _: Context) {
//!         // ...
//!     }
//! }
//!
//! impl<Context> polog::Handler<Context> for OffThreadLogger
//! where
//!     Context: polog::ContextArgs,
//! {
//!     type EnrichedContext = Context;
//!
//!     #[inline]
//!     fn try_enable(
//!         &self,
//!         descriptor: &'static Descriptor,
//!         context: Context,
//!     ) -> Option<Self::EnrichedContext> {
//!         (descriptor.level <= polog::Level::Info).then_some(context)
//!     }
//!
//!     #[inline]
//!     fn log<'a, Args>(
//!         &mut self,
//!         descriptor: &'static Descriptor,
//!         args: Args,
//!         _span_id: Option<SpanId>,
//!         _context: Context,
//!     ) where
//!         Args: polog::LogArgs<'a>,
//!     {
//!         self.0.send((descriptor, Box::new(args.into_sendable())));
//!         //                                     ^^^^^^^^^^^^^^^
//!     }
//!
//!     fn enter_span<'a, Args>(
//!         &mut self,
//!         descriptor: &'static Descriptor,
//!         args: Args,
//!         parent_span_id: Option<SpanId>,
//!         span_id: SpanId,
//!         span_kind: SpanKind,
//!         context: Self::EnrichedContext,
//!     ) where
//!         Args: polog::LogArgs<'a>,
//!     {
//!         // ...
//!     }
//!
//!     fn log_static<Args>(
//!         &mut self,
//!         descriptor: &'static Descriptor,
//!         args: Args,
//!         span_id: Option<SpanId>,
//!         context: Self::EnrichedContext,
//!     ) where
//!         Args: polog::StaticLogArgs
//!     {
//!         self.0.send((descriptor, Box::new(args)));
//!     }
//!
//!     fn enter_span_static<Args>(
//!         &mut self,
//!         descriptor: &'static Descriptor,
//!         args: Args,
//!         parent_span_id: Option<SpanId>,
//!         span_id: SpanId,
//!         span_kind: SpanKind,
//!         context: Self::EnrichedContext,
//!     ) where
//!         Args: polog::StaticLogArgs
//!     {
//!         // ...
//!     }
//! }
//!
//! impl<Context> polog::SharedHandler<Context> for OffThreadLogger
//! where
//!     Context: polog::ContextArgs
//! {}
//!
//! async fn async_logger_handler(
//!     mut events: tokio::sync::mpsc::UnboundedReceiver<(
//!         &'static polog::Descriptor,
//!         Box<(dyn 'static + Send + polog::DynDisplayableMixedArgs<'static>)>,
//!     )>,
//! ) {
//!     while let Some((descriptor, args)) = events.recv().await {
//!         let stdout = io::stdout();
//!         let mut stdout = stdout.lock();
//!         write!(
//!             stdout,
//!             "{} {} {}",
//!             descriptor.level.as_lowercase_str(),
//!             descriptor.target.unwrap_or(descriptor.module_path),
//!             descriptor.message,
//!         );
//!         for arg_no in (0..args.num_args()).take(descriptor.param_names.len()) {
//!             write!(
//!                 &mut stdout,
//!                 " {} = {}",
//!                 descriptor.param_names[arg_no],
//!                 core::fmt::from_fn(|f| args.dyn_display_fmt_nth(f, arg_no).unwrap()),
//!             );
//!         }
//!         writeln!(stdout);
//!     }
//! }
//! #
//! # use polog_test_helpers::check_handler;
//! # let (sender, recv) = tokio::sync::mpsc::unbounded_channel();
//! # let mut logger = OffThreadLogger(sender);
//! # check_handler::check_basic_dyn_handler_fns(&mut logger);
//! # check_handler::check_basic_handler_fns(&mut logger, None);
//! # check_handler::check_basic_shared_handler_fns(&mut logger);
//! # drop(logger);
//! # let rt = tokio::runtime::Runtime::new().unwrap();
//! # rt.block_on(async move { async_logger_handler(recv).await });
//! ```
//!
//! ## Foreign facades
//!
//! The two interoperability directions are independent, and the workspace
//! ships both.
//!
//! To send polog events to `log`, `slog`, or `tracing`, wrap the target
//! backend in `polog_util::log_0_4::ViaLog`, `polog_util::slog_2::ViaSlog`,
//! or `polog_util::tracing_0_1::ViaTracing`.
//!
//! To receive log, slog, or tracing events in a polog backend, wrap
//! the backend in `pologger::SharedPologger` and register it as the global
//! sink for the chosen facade.
//! `SharedPologger` shares the same `SpanIdGenerator` with the backend, so span
//! IDs remain consistent across both directions.
//!
//! `polog` itself has no knowledge of either direction.
//! Both adapters live in the downstream crates so that `polog` stays focused on
//! the facade contract.
//!
//!
//! # FAQ
//!
//! ## What is the current state of the crate?
//!
//! This crate is under active development.
//! The API is subject to change.
//!
//! Currently, `polog` provides the facade traits, macros and the [`Discard`]
//! logger. Related crates also include:
//! - [`polog-util`] — Optional utilities and middleware: `TypeErased`,
//!   `Enricher`, `Filter` (with combinators and parser), `Offload`, and many
//!   other utility items.
//! - [`pologger`] — A reference logger backend implementation.
//!
//! There are likely no other logger implementations, adapters for
//! `log` / `tracing`, or integrations with OpenTelemetry / JSON / etc.
//!
//! If the crate gains community interest, I will happily invest more time on
//! it. Contributions are very welcome!
//!
//! ## Why choose `polog` over `log`?
//!
//! `log` is the mature, established facade with a large ecosystem.
//! `polog` differs in three ways that matter in performance-sensitive code:
//!
//! - **Spans.** `log` has no span concept. `polog` provides borrowing, owning,
//!   deferred, and detached spans with `SpanKind` propagation and per-poll
//!   lifecycle events.
//! - **Deferred formatting.** `log` cannot send non-primitive values across a
//!   thread boundary, so async loggers stringify them on the producing thread.
//!   `polog`'s `IntoSendable` converts arguments once per event and is
//!   zero-cost for `'static + Send` values.
//! - **Monomorphized dispatch.** A sync `polog` handler receives arguments by
//!   value with fully inlined formatting. `log` pays an `&dyn VisitSource`
//!   indirection per event.
//!
//! If you need the `log` ecosystem, a global logger, or `max_level_*` Cargo
//! features, stay with `log`.
//! The two facades are not mutually exclusive: `polog-util::ViaLog` and
//! `pologger::SharedPologger` bridge in both directions.
//!
//! ## Why choose `polog` over `tracing`?
//!
//! `tracing` is the mature, ergonomic span API with a large ecosystem and
//! automatic spans via `#[instrument]`.
//! `polog` differs in three ways that matter in performance-sensitive code:
//!
//! - **No thread-locals.** `tracing` keeps the current span in a thread-local
//!   and looks it up on every event. `polog` threads the logger explicitly, so
//!   span context is visible in every function signature and there is no
//!   thread-local lookup.
//! - **Deferred formatting.** `tracing` passes values as `&dyn
//!   tracing::field::Value` and cannot send non-primitive values across a
//!   thread boundary, so async loggers stringify them on the producing thread.
//!   `polog`'s `IntoSendable` converts arguments once per event and is
//!   zero-cost for `'static + Send` values.
//! - **Monomorphized dispatch.** `polog`'s sync logger receives arguments by
//!   value with fully inlined formatting. `tracing`'s `&dyn Visit` indirection
//!   is not inlined, and its macros expand to more code at each call site.
//!
//! If you rely on ambient span context, `#[instrument]`, `Span::record`,
//! `Span::current()`, or a global subscriber, stay with `tracing`.
//! The two are not mutually exclusive: `polog-util::ViaTracing` and
//! `pologger::SharedPologger` bridge in both directions.
//!
//! ## Why choose `polog` over `slog`?
//!
//! `slog` is a mature, explicit‑logger facade with structured typed output
//! and a moderate ecosystem of drains.
//! It shares `polog`'s philosophy of explicit context propagation: there is
//! no thread‑local ambient logger, and the logger must be threaded through
//! the call graph.
//! `polog` differs in three ways that matter in performance‑sensitive code:
//!
//! - **Spans.** `slog` has no span lifecycle. Its closest analogue is
//!   `Logger::new()`, which merges the parent's `OwnedKVList` into a fresh list
//!   on every call: no enter/exit event, no parent `SpanId`, no per‑poll
//!   signal. `polog` provides borrowing spans (zero‑allocation), owning spans
//!   (shared `Arc` clone), and full lifecycle events with `SpanKind`
//!   propagation and per‑poll lifecycle events for instrumented futures.
//! - **Deferred formatting.** `slog-async` stringifies values on the producing
//!   thread, because `Record` borrows its key‑values and cannot cross a thread
//!   boundary. `polog`'s `IntoSendable` converts arguments once per event and
//!   is zero‑cost for `'static + Send` values.
//! - **Monomorphized dispatch.** `slog`'s sync path serializes every value
//!   through `&mut dyn slog::Serializer`. `polog`'s sync path is fully
//!   monomorphized: the concrete `Display` implementation for each argument is
//!   inlined at the call site.
//!
//! If you need typed structured output via `slog::Serializer`, a mature
//! drain ecosystem, or `serde` integration, stay with `slog`.
//! The two facades are not mutually exclusive: `polog-util::ViaSlog` and
//! `pologger::SharedPologger` bridge in both directions.
//!
//! ## Why not standard Rust formatting?
//!
//! Many logging crates allow and encourage writing something like:
//! `log::error!("Failed to read file {path:?}, reason: {os_err}")`.
//!
//! While convenient, that syntax comes with hidden costs:
//! - It forces immediate formatting (or at least the capture of a
//!   `fmt::Arguments` structure).
//! - It encourages inlining formatting logic into the log site.
//! - It provides formatted output that is hard to decompose into structured
//!   key‑value pairs.
//!
//! This library deliberately makes that style **hard** to use, steering you
//! toward explicit **key‑value** arguments:
//! ```rust
//! # #[cfg(feature = "std")] {
//! # use polog_test_helpers::accept_all::AcceptAll;
//! # let mut logger = AcceptAll;
//! # let path = std::path::Path::new("foo");
//! # let os_err = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
//! polog::error!(&mut logger, "Failed to read file"; ? path, reason = % os_err); // For owned args.
//! # let path = &std::path::Path::new("foo");
//! # let os_err = &std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
//! polog::error!(&mut logger, "Failed to read file"; :t ? path, reason = :t % os_err); // For local args.
//! # }
//! ```
//!
//! Key‑value logging keeps the log site lightweight, defers all formatting
//! to the logger, and preserves structured data for downstream consumers.
//!
//! For cases where you truly need a Rust‑like formatting, the library
//! provides a **formatted value** syntax:
//! ```rust
//! # use polog_test_helpers::accept_all::AcceptAll;
//! # let mut logger = AcceptAll;
//! # let high = 0x1234;
//! # let low = 0x5678;
//! # let frac = 0.1234;
//! polog::error!(&mut logger, "Failed to read file"; code = :f("{:04x}{:04x}{:.8}", high, low, frac));
//! ```
//!
//! ## Why no global logger support?
//!
//! The library is intentionally an explicit‑logger facade.
//! A global logger is not just a convenience trade‑off.
//! It changes the dispatch model that `polog` is built around.
//!
//! At each `polog::info!(logger, ...)` call site, the concrete logger type is
//! statically known.
//! That lets the compiler monomorphize and inline `is_enabled`/`log`,
//! constant‑fold filters, eliminate disabled events, and format sync args with
//! fully monomorphized `Display` calls.
//! Async loggers still get `IntoSendable` conversion once per argument, but the
//! handler dispatch remains static.
//!
//! A global logger must be selected at runtime.
//! In Rust that normally means an object‑safe erased handler behind
//! `OnceLock<Box<dyn ...>>` or `OnceLock<&'static dyn ...>`.
//! That has three effects:
//! - every event pays an indirect call instead of a static, inlinable call;
//! - the compiler cannot generally prove which concrete handler is behind the
//!   global, so it cannot inline, const‑fold, or specialize for it;
//! - if the global can be replaced, or even if it is only set once, the
//!   load/branch is opaque enough that LLVM is much less confident about the
//!   callee type and lifetime.
//!
//! The argument tuple cannot be passed generically through a runtime‑selected
//! global without monomorphizing at the call site.
//! It must be erased (`&dyn ...`, boxed, or stringified).
//! That discards the main win for sync loggers and weakens it for async
//! loggers. A global `polog` can still expose static descriptors and structured
//! keys, and can still defer async formatting via `IntoSendable`, but it is no
//! longer the zero‑overhead path.
//!
//! Also while convenient, this encourages a poor practice of using global
//! logger in every single non‑binary crate.
//!
//! Global loggers are a form of singleton.
//! Rust idioms favor explicit state propagation over implicit globals.
//! Avoiding a global logger enables local or scoped loggers in binary crates
//! and prevents libraries from inadvertently depending on a single logging
//! instance.
//!
//! So `polog` has no global logger by design.
//!
//! `polog` call sites are not global.
//! If you need a process‑wide sink for `log` / `slog` / `tracing`,
//! `pologger::SharedPologger` can be registered as the global default for those
//! facades.
//! That is a property of the backend, not of the facade.
//!
//! ## Are there `#[instrument]`-like macros?
//!
//! Currently the `#[instrument]` macro isn't implemented.
//!
//! The transformation is not a shortcut over the explicit macros:
//! the attribute sees the signature, not the call site, and several decisions
//! it must make have no correct automatic answer:
//! - Send and format modifiers. For arguments macro cannot tell whether the
//!   caller wants to move it, clone it, stringify, stringify with debug, etc.
//! - Sync and async: `polog` spans already produces a correct single
//!   `enter_span` and `exit_span` for async functions. `.poll_instrument` adds
//!   per‑poll events.
//!
//! The recommended workaround is a single explicit span at the top of the body:
//!
//! ```rust
//! # use polog_test_helpers::accept_all::AcceptAll;
//! # do_smth(&mut AcceptAll, 123, "abc");
//! # let rt = tokio::runtime::Runtime::new().unwrap();
//! # rt.block_on(async move { do_smth_async(&mut AcceptAll, 123, "abc").await });
//! #
//! fn do_smth<L>(logger: &mut L, foo: u32, bar: &str)
//! where
//!     L: polog::Logger,
//! {
//!     let mut logger = polog::debug_span!(logger; @fn, foo, :t bar);
//!     // `@fn` expands to `polog::uninstantiated_fn_name!()` which produces the
//!     // name of the current function.
//!     // The backend logger is expected to use this name instead of omitted
//!     // message.
//! }
//!
//! async fn do_smth_async<L>(logger: &mut L, foo: u32, bar: &str)
//! where
//!     L: polog::Logger,
//! {
//!     let mut logger = polog::debug_span!(logger; @fn, foo, :t bar);
//!     // ...
//! }
//! ```
//!
//! Unlike an attribute macro, this makes the send and format choices explicit
//! at the same place they are made everywhere else in polog.
//!
//! An `#[instrument]`-style macro may be added in a future release once the
//! explicit macros are stable enough that a thin attribute form is clearly
//! useful.
//!
//! ## Why no thread-local span context?
//!
//! `tracing` keeps the current span in a thread-local, so `tracing::info!()` in
//! a function that never receives a logger still logs into the enclosing span.
//! `polog` replaces that with an explicit logger parameter: the logger *is* the
//! current span.
//!
//! The trade is deliberate.
//! No thread-local lookup per event, and the span context is visible from every
//! function signature.
//! Detaching across a thread or task boundary is an explicit
//! `LoggerExt::detach`, not an inheritance from a thread-local that may or may
//! not be set.
//!
//! ## Why are there no dynamic span fields?
//!
//! A span's fields are the argument tuple passed at the call site, and the
//! tuple's type is monomorphized into `Handler::enter_span`.
//! There is no `Span::record` equivalent: adding fields after creation would
//! require erasing the tuple, losing monomorphization for every span that uses
//! it — the exact overhead `polog` is built to avoid.
//!
//! If a field is not known at span creation, either:
//! - Open a child span whose arguments include the field, or
//! - Pass a custom key-value container as a single argument and record new
//!   entries into it. The backend sees the container, not individual fields.
//!
//! ## Can a backend emit structured output such as JSON?
//!
//! By default, no.
//! Arguments reach the backend as values implementing `Display`, and the
//! backend sees only rendered text: `123` arrives as `"123"`, `None` as
//! `"null"`.
//! This follows from two `polog` guarantees — monomorphized `Display` calls for
//! same-thread handlers, and `IntoSendable` conversion for off-thread handlers
//! — both of which would be lost to a visitor protocol like
//! `tracing::field::Visit` or `slog::Serializer`.
//!
//! A structured backend has two opt-in paths:
//!
//! 1. **Off-thread handler using `Handler::log_static`.** Its `Args: 'static`
//!    bound unlocks `MixedArgs::nth_arg_as_dyn_any_ref`, so the handler can
//!    downcast recognized `'static` types (`i64`, `bool`, `&'static str`,
//!    `serde_json::Value`) and fall back to text otherwise.
//! 2. **Wrapper at the call site.** Any `Display + IntoSendable` value reaches
//!    the backend unchanged, so `serde_json::Value` or a user-defined `Json<T>`
//!    wrapper survives conversion and can be downcast in `log_static`.
//!
//! Neither path is free: (1) requires an off-thread handler, (2) requires
//! per-argument opt-in.
//! Both preserve the design; the visitor protocol used by `tracing` and `slog`
//! would not.
//!
//! ## What is meant by saying that it is "impossible to send non‑primitive values without formatting" in other crates?
//!
//! [`log`] passes values to logger internally as a slice of private
//! `kv::inner::Inner` enum.
//! It has variants for: `bool`, `&str`, `i64`, `u64`, `u128`, `i128`, `f64`,
//! `&dyn Debug`, `&dyn Display`.
//! It implies using `log::kv::VisitValue` on the logger for value visiting.
//! Async loggers would have to stringify non‑primitive types on the log
//! producing thread.
//!
//! [`tracing`] passes values to logger internally as a
//! `&dyn tracing::field::Value`.
//! `Value` is implemented for most common stdlib types however sealed.
//! It implies using `tracing::field::Visit` on the logger for value visiting.
//! Visitor supports matching to limited subset of types: `f64`, `i64`, `u64`,
//! `i128`, `u128`, `bool`, `&str`, `&[u8]`, `&(dyn Error + 'static)`, and
//! `&dyn Debug`.
//! Async loggers would have to stringify non‑primitive types on the log
//! producing thread.
//!
//! [`slog`] passes values to logger internally as a `&dyn` cons‑style tuples
//! (`(T1, (T2, (T3, ()))`) of raw values with each value implementing
//! `slog::Value` serializable with `slog::Serializer`.
//! Serializer supports matching to limited subset of stdlib types:
//! `bool`, `char`, `any stdlib integer`, `f32`, `f64`, `&str`, `()`, `none`,
//! `&[u8]`, `&dyn SerdeValue`, `&(dyn StdError + 'static)`, and
//! `&fmt::Arguments<'_>`.
//! Async loggers (like [`slog-async`]) would have to stringify non‑primitive
//! types on the log producing thread.
//!
//! ## Why does the log descriptor not contain the full function path?
//!
//! While unstable technically it is almost possible to add `function_path` to
//! `Descriptor` using something like:
//! ```rust,ignore
//! #[cfg(feature = "unstable")]
//! macro_rules! __function_path {
//!     () => {{
//!         fn type_name_of_val<T>(_: &T) -> &'static str {
//!             core::any::type_name::<T>()
//!         }
//!         // Closure used to have "instantiated generics" in the resulting path.
//!         let name = &type_name_of_val(&|| {});
//!         Some(name.strip_suffix("::{{closure}}").unwrap_or(name))
//!     }};
//! }
//! ```
//! The problem, however, is that the `core::any::type_name` is not a const
//! function (as of `Rust 1.99.0`) so it cannot be used to build static
//! descriptor.
//!
//! You can use `@fn` meta‑attribute to pass function name as an string
//! argument. Backends may use this value instead of an empty message.
//!
//! ## Why no `max_level_*` Cargo features?
//!
//! `polog` does not ship `max_level_*`-style features for three reasons.
//!
//! First, they violate Cargo's additivity guideline: they remove code, and
//! feature unification means the most permissive setting wins, so a single
//! crate that enables `max_level_trace` defeats the application's filter for
//! the whole dependency graph.
//!
//! Second, they impose a single global decision. `polog` supports
//! per‑middleware filtering, so a library's recommended level can differ from
//! the application's; `max_level_*` cannot express this.
//!
//! Third, they are not necessary. `polog` call sites are generic over the
//! logger and are monomorphized in the application's codegen unit.
//! A handler whose `try_enable` is transparent and whose filter is const‑known
//! — for example a const‑generic `polog_util::ConstMaxLevel` — folds to a
//! constant in any optimized build, eliminating the event together with its
//! argument construction, without a Cargo feature and without a global setting.
//!
//! ## Does it work with Tokio / async-std / smol / any async runtime?
//!
//! Yes. `polog` is runtime‑agnostic:
//!
//! - The `Logger` parameter is ordinary `&mut L where L: polog::Logger`; no
//!   logging or span signature mentions a runtime type.
//! - `LoggerExt::detach` and `LoggerExt::borrowing_span` produce
//!   `DetachedLogger` and `OwningSpan` that are `Send` when the handler is
//!   `Send`, so a span can be moved into `tokio::spawn`,
//!   `async_std::task::spawn`, `smol::spawn`, or a hand‑rolled future.
//! - `PollInstrumented` wraps any `F: Future` and polls it without assuming any
//!   reactor or timer.
//! - The `OffloaderService` worker runs on a `std::thread`, not on a runtime
//!   thread pool; it does not need a runtime at all.
//! - The `offloader-support-tokio` feature only gates
//!   `tokio::task::block_in_place` during flush‑on‑panic; without it, the same
//!   flush runs on the caller thread.
//!
//! The examples use Tokio because it is convenient for demonstrating async
//! spans and instrumented futures; nothing about the facade depends on it.
//!
//! ## Where did `RUST_LOG` filtering go?
//!
//! `polog` is filter‑free by design: every `Handler` decides independently
//! whether an event is enabled, and `polog` exposes no global filter to
//! configure.
//!
//! The filtering layer lives in `polog-util`:
//!
//! - [`MaxLevel`] for the simple case.
//! - [`ConstMaxLevel<MAX_LEVEL>`] for a const‑generic filter that the optimizer
//!   folds away entirely.
//! - [`FilterRules`] for a `RUST_LOG`‑style rule set with longest‑prefix‑wins
//!   matching: `FilterRules::parse("info,my_crate=debug")?.to_owning()`.
//! - [`FilterFnExt`] for combinators such as
//!   `MaxLevel(..).and(TargetPrefix(..))`.
//!
//! All three types implement both `FilterFn` (for the `polog` side) and
//! `ExternalFilterFn` (for foreign events in `pologger::SharedPologger`), so
//! a single parsed rule set filters both paths.
//!
//! Wrap any handler in `polog_util::Filter` and place the filter on the
//! caller side of an offloader so disabled call sites never construct their
//! arguments:
//!
//! ```rust,ignore
//! let handler = polog_util::Filter::new(offloader_handle, rules);
//! ```
//!
//! `pologger::OffThreadLoggerBuilder` performs this wiring automatically from
//! a single `RUST_LOG` string.
//!
//! ## Why doesn't `polog` parameterize sendable arguments over custom allocators?
//!
//! `IntoSendable` converts a borrowed value into a `'static + Send + Sync` form
//! that can cross a thread boundary. Custom allocator support would require
//! `Sendable` types to be parameterized over an allocator:
//!
//! ```rust,ignore
//! # use core::alloc::Allocator;
//! trait IntoSendable<A: Allocator> {
//!     type Sendable: 'static + Send + Sync;
//!     fn into_sendable(self, alloc: &A) -> Self::Sendable;
//! }
//! ```
//!
//! `Box<str, A>` and `Box<CStr, A>` make this shape workable at the
//! `Sendable` level.
//! The problem is everything above it.
//!
//! Every trait that names `IntoSendable` gains a parameter:
//! - `LogArgs<'a, A>` bounds the argument tuple.
//! - `Handler<Context, A>` gains a parameter, because `log` and `enter_span`
//!   need `Args: LogArgs<'a, A>`.
//! - `Logger<A>` and `LoggerExt<A>` gain a parameter, so every function that
//!   accepts `&mut L where L: Logger` becomes allocator-generic.
//! - Every middleware implementation must propagate `A` through its own trait
//!   bounds, its `EnrichedContext`, and its forwarding methods.
//! - Every `polog::info!` call must have `A` inferred through the entire chain.
//!
//! The propagation is unavoidable because the allocator cannot be quantified
//! away in the trait bound.
//! What would be needed is something like:
//! ```rust,compile_fail
//! trait LogArgs<'a>:
//!     IntoSendable<for<A: Allocator> Sendable<A>: DisplayableMixedArgs<'static>>
//! {}
//! ```
//!
//! Non-lifetime binders are not supported in Rust.
//! The tracking issue is [rust-lang/rust#108185].
//! Without them, there is no way to state "the argument converts to a
//! `Sendable` for any allocator" and still name the resulting type, so `A` must
//! appear on every trait that mentions the conversion.
//!
//! A user who needs a custom allocator can supply a wrapper that converts to
//! them via `TemporaryIntoSendable` and use the `:t` modifier at the call site:
//!
//! ```rust,ignore
//! # use polog_test_helpers::accept_all::AcceptAll;
//! # use core::alloc::Allocator;
//! # use core::fmt;
//! # use polog::TemporaryIntoSendable;
//! # let mut logger = AcceptAll;
//! # let my_str = "abc";
//! # let my_alloc = std::alloc::Global;
//! #
//! struct BoxedStr<'a, A: Allocator>(&'a str, A);
//!
//! impl<A: 'static + Allocator + Send + Sync + Clone> TemporaryIntoSendable for BoxedStr<'_, A> {
//!     type Sendable = Box<str, A>;
//!
//!     fn temp_into_sendable(self) -> Self::Sendable {
//!         todo!()
//!     }
//! }
//!
//! impl<A: Allocator> fmt::Display for BoxedStr<'_, A> {
//!     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//!         todo!()
//!     }
//! }
//!
//! polog::info!(&mut logger, "..."; :t BoxedStr(my_str, my_alloc.clone()));
//! ```
//!
//! The conversion is user-defined, the framework's `Sendable` for `&str` stays
//! `String`, and no handler or middleware changes.
//!
//! ## How does `polog` perform compared to `log`, `tracing`, and `slog`?
//!
//! The `fixtures/codegen` benchmark builds and runs the same recursive workload
//! under each facade, in three modes: no logging, format-to-buffer (no I/O),
//! and send-only (simulating an off-thread backend by making the event `Send`
//! and stashing it in a thread-local slot).
//! Both dev and release profiles are measured.
//!
//! The benchmark compiles with `complex,more-args`, one facade feature, and
//! optionally `span`.
//!
//! Events are emitted with four payload shapes roughly equally often:
//! - `(&'static str, u64)`,
//! - `(&'static str, &'local str, &'local [u8])`,
//! - `(&'static str, ([u8; 8], char, f32, bool), &'local ([u8; 8], char, f32,
//!   bool))`,
//! - `(&'static str, u64, u64)`
//!
//! and spans with:
//! - `(&'static str, &'static str, usize, &'local str, &'local [u8])`.
//!
//! This mix exercises owned primitives, owned `Display` values, borrowed
//! strings, borrowed bytes, and owned and borrowed `Debug`-formatted compounds,
//! which are the characteristics that actually move a logging facade's cost.
//! Diversity beyond a handful of shapes depends entirely on the application and
//! would require a much larger fixture, so the benchmark fixes one
//! representative mix.
//! Different payload-mix ratios were checked during development and the
//! relative results were approximately the same.
//!
//! The foreign backends apply a best-effort send optimization: keys and value
//! descriptors live in separate `SmallVec`s, while rendered/string/byte
//! payloads are concatenated into one or more owned buffers and referenced by
//! length.
//! Primitives stay as typed variants.
//! This avoids formatting every value separately, but it is not one flat
//! buffer.
//! `polog` instead uses `IntoSendable` and type-erased args.
//!
//! All run-time numbers are ratios against the same-workload-with-no-logging
//! baseline in the same profile.
//!
//! **Compile time** (relative to no logging in the same profile, lower is
//! better):
//!
//! | Facade            | Dev Format | Dev Send   | Rel Format | Rel Send   |
//! |:-----------------:|:----------:|:----------:|:----------:|:----------:|
//! | baseline          |     4.3s   |     4.4s   |     9.2s   |     9.2s   |
//! | `log`             |     +20%   |     +19%   |      +9%   |      +8%   |
//! | `slog`            |     +38%   |     +34%   |     +12%   |     +12%   |
//! | `polog`           |     +31%   |     +30%   |     +23%   |     +24%   |
//! | `tracing`         |     +76%   |     +75%   |     +28%   |     +28%   |
//! | `polog` + spans   |     +44%   |     +42%   |     +28%   |     +30%   |
//! | `tracing` + spans |    +106%   |    +104%   |     +40%   |     +40%   |
//!
//! In dev, `polog` adds roughly 40% of the compile-time overhead that
//! `tracing` adds (+31% vs +76%), and approximately the same as `slog`
//! (+38%).
//! With spans the ratio is the same (+44% vs +106%).
//!
//! In release all four are much closer: `polog` adds +23% vs `tracing`'s +28%
//! for events and +28% vs +40% with spans — a few percentage points apart, so
//! the two are close to tied within noise.
//! `log` remains the cheapest throughout.
//! `slog` is cheaper than `polog` in release but not in dev.
//!
//! The dev/release split comes from `tracing`'s macro expansion.
//! Its event and span macros emit a callsite registration, an interest-cache
//! check, and a `Dispatch` call at every call site.
//! Those expand to substantial code that release builds inline away but dev
//! builds do not.
//! `polog`'s macros are simpler — a static descriptor, a `try_enable` check, a
//! monomorphized handler call — so its dev and release compile times are closer
//! to each other (+31% vs +23%) than `tracing`'s (+76% vs +28%).
//!
//! **Run time** (relative to no logging in the same profile, lower is better):
//!
//! | Facade            | Dev Format | Dev Send   | Rel Format | Rel Send   |
//! |:-----------------:|:----------:|:----------:|:----------:|:----------:|
//! | Baseline          |   104.1s   |   103.6s   |     4.1s   |     4.2s   |
//! | `log`             |     2.1×   |     2.2×   |     5.8×   |     6.6×   |
//! | `slog`            |     1.9×   |     1.8×   |     6.1×   |     5.3×   |
//! | `polog`           |     2.0×   |   **1.2×** |     5.9×   |   **1.4×** |
//! | `tracing`         |     2.1×   |     2.0×   |     6.2×   |     5.2×   |
//! | `polog` + spans   |     2.5×   |   **1.2×** |     8.2×   |   **1.6×** |
//! | `tracing` + spans |     3.1×   |     2.7×   |    11.2×   |     9.2×   |
//!
//!
//! **In format mode, all four facades are within ~10% of each other in both
//! profiles** (1.9×–2.1× in dev, 5.8×–6.2× in release).
//! Every one walks the arguments and produces text, and with the mixed payloads
//! the argument walk dominates whatever per-facade overhead remains.
//! Facade choice makes no meaningful difference here.
//!
//! **In send-only mode, `polog` costs roughly half of the next-cheapest facade
//! in dev and roughly a quarter in release** — 1.2× of baseline vs 1.8×–2.2×
//! in dev, and 1.4× vs 5.2×–6.6× in release.
//! The gap is wider in release because the richer argument mix makes the
//! foreign facades' unconditional visit-and-copy work more expensive, while
//! `polog`'s conversion is identity for `'static + Send` arguments and a single
//! owned conversion (`:t` clone or `:s` format) for locals.
//! Spans follow the same pattern: `polog` + spans is ~2× faster than
//! `tracing` + spans in dev (1.2× vs 2.7×) and ~5–6× in release (1.6× vs 9.2×).
//!
//! **Where the fixture still favors `polog`.**
//! The mix is realistic rather than adversarial: most arguments are
//! `'static + Send`, because real code prefers primitives and static names, and
//! only per-invocation IDs and compound context are borrowed.
//! `polog`'s `IntoSendable::into_sendable` is identity for those static
//! arguments, whereas the visitor-based facades copy or format every argument
//! on the producer thread regardless.
//! A workload dominated by call-site `:t`/`:s` conversions moves `polog`'s
//! send-only cost toward `log` / `slog` / `tracing`'s — never above them,
//! because `polog`'s conversion is at worst the same work those facades do
//! unconditionally, but not as far below as the mixed-payload numbers show.
//! The same caveat applies to compile time: the fixture has a bounded number of
//! distinct call sites and argument-type combinations; a real application with
//! hundreds of distinct combinations will see a larger release build.
//!
//! The benchmark source is in `fixtures/codegen` and the bench script
//! generating the tables is `fixtures/codegen/bench.sh`.
//! The numbers above are from a single run on a development machine.
//! You can reproduce them locally before drawing conclusions.
//!
//! ## What about combinatorial code bloat because of extreme monomorphization?
//!
//! Theoretically this can happen, however:
//! - All log invocations with same‑type same‑ordered argument will produce
//!   single set of monomorphized functions.
//! - The binary crate developers may use off‑thread logger for optimal
//!   performance that'll require dynamic dispatch and therefore will send
//!   arguments with their types erased as `&dyn Args`.
//!
//! The fixtures/codegen benchmark measures this empirically.
//! Even in a workload with very few distinct argument-type combinations,
//! polog's release compile time is above log/slog and close to tracing's.
//! Workloads with more variety will be higher.
//!
//!
//! # Documentation
//!
//! [API Documentation]
//!
//!
//! # Feature flags
//!
//! - `alloc` (implied by `std`, enabled by default): enables implementations
//!   for `alloc` types, like `Box`, `Arc`, `String`, `Vec`, `BTreeMap` etc.
//! - `std` (enabled by default): enables `alloc` feature and implementations
//!   for `std` types, like `OsStr`, `Path`, `PathBuf`, `Instant`, `HashMap`
//!   etc.
//! - `test-util` (disabled by default): exposes helpers that let downstream
//!   test suites set up specific internal states, for example to place the
//!   `SpanIdGenerator` counter at a specific value. Intended for tests only.
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
//! [API Documentation]: https://docs.rs/polog/latest/polog/
//! [`polog`]: https://docs.rs/polog/latest/polog/
//! [`log`]: https://docs.rs/log/latest/log/
//! [`tracing`]: https://docs.rs/tracing/latest/tracing/
//! [`slog`]: https://docs.rs/slog/latest/slog/
//! [`slog-async`]: https://docs.rs/slog-async/latest/slog-async/
//! [`pologger`]: https://docs.rs/pologger/latest/pologger/
//! [`polog-util`]: https://docs.rs/polog-util/latest/polog_util/
//! [`Debug`]: https://doc.rust-lang.org/stable/core/fmt/trait.Debug.html
//! [`Logger`]: trait.Logger.html
//! [`DynHandler`]: trait.DynHandler.html
//! [`DebuggableDynHandler`]: trait.DebuggableDynHandler.html
//! [`Handler`]: trait.Handler.html
//! [`Handler::log_static`]: trait.Handler.html#method.log_static
//! [`Handler::enter_span_static`]: trait.Handler.html#method.enter_span_static
//! [`BorrowingHandler`]: trait.BorrowingHandler.html
//! [`SharedHandler`]: trait.SharedHandler.html
//! [`LoggerExt::transparent_borrowing_span`]: trait.LoggerExt.html#method.transparent_span
//! [`LogArgs<'a>`]: trait.LogArgs.html
//! [`StaticLogArgs`]: trait.StaticLogArgs.html
//! [`ContextArgs`]: trait.ContextArgs.html
//! [`IntoSendable`]: trait.IntoSendable.html
//! [`MixedArgs`]: trait.MixedArgs.html
//! [`DynMixedArgs<T>`]: trait.DynMixedArgs.html
//! [`DisplayableMixedArgs<'a>`]: trait.DisplayableMixedArgs.html
//! [`PollInstrument`]: trait.PollInstrument.html
//! [`PollInstrument::poll_instrument`]: trait.PollInstrument.html#method.poll_instrument
//! [`DetachedLogger`]: struct.DetachedLogger.html
//! [`DetachedLogger<H>`]: struct.DetachedLogger.html
//! [`Discard`]: struct.Discard.html
//! [`Descriptor`]: struct.Descriptor.html
//! [`BorrowingSpan`]: struct.BorrowingSpan.html
//! [`BorrowingSpan<'a, H>`]: struct.BorrowingSpan.html
//! [`OwningSpan`]: struct.OwningSpan.html
//! [`OwningSpan<H>`]: struct.OwningSpan.html
//! [`OwningSpan::into_erased`]: struct.OwningSpan.html#method.into_erased
//! [`OwningSpan::into_debuggable_erased`]: struct.OwningSpan.html#method.into_debuggable_erased
//! [`DeferredSpan<H, _>`]: struct.DeferredSpan.html
//! [`DeferredSpan`]: struct.DeferredSpan.html
//! [`DeferredSpan::enter`]: struct.DeferredSpan.html#method.enter
//! [`DebuggableTypeErasedOwningSpan`]: struct.DebuggableTypeErasedOwningSpan.html
//! [`SpanId`]: struct.SpanId.html
//! [`SpanIdGenerator`]: struct.SpanIdGenerator.html
//! [`TypeErasedOwningSpan`]: struct.TypeErasedOwningSpan.html
//! [`PollInstrumented`]: struct.PollInstrumented.html
//! [`Level`]: enum.Level.html
//! [`Level::Trace`]: enum.Level.html#variant.Trace
//! [`Level::Debug`]: enum.Level.html#variant.Debug
//! [`Level::Info`]: enum.Level.html#variant.Info
//! [`Level::Warn`]: enum.Level.html#variant.Warn
//! [`Level::Error`]: enum.Level.html#variant.Error
//! [`Level::Bug`]: enum.Level.html#variant.Bug
//! [`Level::Fatal`]: enum.Level.html#variant.Fatal
//! [`LevelFilter`]: enum.LevelFilter.html
//! [`SpanKind`]: enum.SpanKind.html
//! [`SpanKind::Detached`]: enum.SpanKind.html#variant.Detached
//! [`log!`]: macro.log.html
//! [`trace!`]: macro.trace.html
//! [`debug!`]: macro.debug.html
//! [`info!`]: macro.info.html
//! [`warn!`]: macro.warn.html
//! [`error!`]: macro.error.html
//! [`bug!`]: macro.bug.html
//! [`fatal!`]: macro.fatal.html
//! [`span!`]: macro.span.html
//! [`trace_span!`]: macro.trace_span.html
//! [`debug_span!`]: macro.debug_span.html
//! [`info_span!`]: macro.info_span.html
//! [`warn_span!`]: macro.warn_span.html
//! [`error_span!`]: macro.error_span.html
//! [`owning_span!`]: macro.owning_span.html
//! [`owning_trace_span!`]: macro.owning_trace_span.html
//! [`owning_debug_span!`]: macro.owning_debug_span.html
//! [`owning_info_span!`]: macro.owning_info_span.html
//! [`owning_warn_span!`]: macro.owning_warn_span.html
//! [`owning_error_span!`]: macro.owning_error_span.html
//! [`deferred_span!`]: macro.deferred_span.html
//! [`deferred_trace_span!`]: macro.deferred_trace_span.html
//! [`deferred_debug_span!`]: macro.deferred_debug_span.html
//! [`deferred_info_span!`]: macro.deferred_info_span.html
//! [`deferred_warn_span!`]: macro.deferred_warn_span.html
//! [`deferred_error_span!`]: macro.deferred_error_span.html
//! [`some_or_log!`]: macro.some_or_log.html
//! [`some_or_trace!`]: macro.some_or_trace.html
//! [`some_or_debug!`]: macro.some_or_debug.html
//! [`some_or_info!`]: macro.some_or_info.html
//! [`some_or_warn!`]: macro.some_or_warn.html
//! [`some_or_error!`]: macro.some_or_error.html
//! [`none_or_log!`]: macro.none_or_log.html
//! [`none_or_trace!`]: macro.none_or_trace.html
//! [`none_or_debug!`]: macro.none_or_debug.html
//! [`none_or_info!`]: macro.none_or_info.html
//! [`none_or_warn!`]: macro.none_or_warn.html
//! [`none_or_error!`]: macro.none_or_error.html
//! [`ok_or_log!`]: macro.ok_or_log.html
//! [`ok_or_trace!`]: macro.ok_or_trace.html
//! [`ok_or_debug!`]: macro.ok_or_debug.html
//! [`ok_or_info!`]: macro.ok_or_info.html
//! [`ok_or_warn!`]: macro.ok_or_warn.html
//! [`ok_or_error!`]: macro.ok_or_error.html
//! [`err_or_log!`]: macro.err_or_log.html
//! [`err_or_trace!`]: macro.err_or_trace.html
//! [`err_or_debug!`]: macro.err_or_debug.html
//! [`err_or_info!`]: macro.err_or_info.html
//! [`err_or_warn!`]: macro.err_or_warn.html
//! [`err_or_error!`]: macro.err_or_error.html
//! [`bug_assert!`]: macro.bug_assert.html
//! [`bug_assert_eq!`]: macro.bug_assert_eq.html
//! [`bug_assert_ne!`]: macro.bug_assert_ne.html
//! [`fatal_assert!`]: macro.fatal_assert.html
//! [`fatal_assert_eq!`]: macro.fatal_assert_eq.html
//! [`fatal_assert_ne!`]: macro.fatal_assert_ne.html
//! [`unwrap_some_or_fatal!`]: macro.unwrap_some_or_fatal.html
//! [`unwrap_none_or_fatal!`]: macro.unwrap_none_or_fatal.html
//! [`unwrap_ok_or_fatal!`]: macro.unwrap_ok_or_fatal.html
//! [`unwrap_err_or_fatal!`]: macro.unwrap_err_or_fatal.html
//! [`log_descriptor!`]: macro.log_descriptor.html
//! [`span_descriptor!`]: macro.span_descriptor.html
//! [`log_descriptor_ref!`]: macro.log_descriptor_ref.html
//! [`span_descriptor_ref!`]: macro.span_descriptor_ref.html
//! [`parse_keys!`]: macro.parse_keys.html
//! [`parse_values!`]: macro.parse_values.html
//! [`locate_and_parse_values!`]: macro.locate_and_parse_values.html
//! [`uninstantiated_fn_name!`]: macro.uninstantiated_fn_name.html
//! [`instantiated_fn_name!`]: macro.instantiated_fn_name.html
//! [`MaxLevel`]: https://docs.rs/polog-util/latest/polog_util/struct.MaxLevel.html
//! [`ConstMaxLevel<MAX_LEVEL>`]: https://docs.rs/polog-util/latest/polog_util/struct.ConstMaxLevel.html
//! [`FilterRules`]: https://docs.rs/polog-util/latest/polog_util/struct.FilterRules.html
//! [`FilterFnExt`]: https://docs.rs/polog-util/latest/polog_util/trait.FilterFnExt.html
//! [rust-lang/rust#108185]: https://github.com/rust-lang/rust/issues/108185

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

mod args;
mod descriptor;
mod fmt;
mod handler;
mod instrument;
mod level;
mod logger;
mod logger_ext;
mod loggers;
mod macros;
mod panic;
mod private;
mod send;
mod span;

pub use args::*;
pub use descriptor::*;
pub use fmt::*;
pub use handler::*;
pub use instrument::*;
pub use level::*;
pub use logger::*;
pub use logger_ext::*;
pub use loggers::*;
pub use macros::*;
pub use panic::*;
pub use send::*;
pub use span::*;

#[cfg(test)]
mod integration_tests_deps {
    use polog_test_helpers as _;
    use tokio as _;
}

#[cfg(all(test, not(doc)))]
mod doc_tests_deps {
    use tracing as _;
}
