# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Initial public release. The workspace is published as three crates:
- `polog` — the logging and span facade.
- `polog-util` — optional middleware, delegators, and utilities.
- `pologger` — reference logger backend.

### Added

- `polog`: Structured key-value logging with a static `Descriptor` per call
  site.
- `polog`: Monomorphized sync formatting (`Handler::log`) and deferred async
  formatting (`IntoSendable`, `Handler::log_static`).
- `polog`: Send modifiers `:a`, `:t`, `:s`, format modifiers `%`, `?`, `#%`,
  `#?`, and dynamic levels via `:dyn`.
- `polog`: Spans: `BorrowingSpan`, `OwningSpan`, `DeferredSpan`,
  `DetachedLogger`, `SpanKind`.
- `polog`: Per-poll instrumentation via `PollInstrumented`.
- `polog`: `Level::Fatal` and `Level::Bug` with panic integration (`fatal!`,
  `bug!`, assert and unwrap variants).
- `polog`: `Discard` handler, `test-util` feature, feature-gated `alloc`/`std`.
- `polog-util`: Middleware: `Filter`, `Fanout`, `BorrowingFanout`, `Enricher`,
  `Shared`, `Either`, offloader with configurable capacity and backpressure.
- `polog-util`: `RUST_LOG`-style filters via `FilterRules`, plus `MaxLevel`,
  `ConstMaxLevel`, `Target`, `TargetPrefix`, and combinators.
- `polog-util`: Type-erased argument wrappers, typed argument access via
  `MixedArgsExt`.
- `polog-util`: Delegators to `log` 0.4, `slog` 2, and `tracing` 0.1.
- `polog-util`: Level and filter conversions between `polog` and each foreign
  facade.
- `polog-util`: Context wrappers and enricher functions.
- `pologger`: `Pologger` backend with `TextFormatter` and `NdJsonFormatter`.
- `pologger`: `TextFormatter` with ANSI colors, dynamic column padding,
  timestamp prefix highlighting, and control-character sanitization.
- `pologger`: `NdJsonFormatter` with RFC 8259 escaping.
- `pologger`: `OffThreadLoggerBuilder` for one-call chain construction and live
  reconfiguration via `OffThreadLoggerConfig`.
- `pologger`: `SharedPologger` with `log`, `slog`, and `tracing` sink
  implementations.
- `pologger`: `LoggerConfig` bitflags covering event kinds, descriptor, site,
  context, and derived fields, plus behavior switches.

[Unreleased]: https://github.com/zheland/pologger/compare/v0.0.0...HEAD
