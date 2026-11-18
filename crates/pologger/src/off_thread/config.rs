use alloc::sync::Arc;

use parking_lot::RwLock;
use polog::LevelFilter;
use polog_util::{FilterFn, OwningFilterRules};

use crate::LoggerConfig;

/// Runtime configuration of a logger produced by [`OffThreadLoggerBuilder`].
///
/// Bundles the shared handles that control the logger's dynamic behavior:
/// the `RUST_LOG`-style filter, the logger's own configuration, and the
/// formatter-specific configuration.
/// Each handle is `Arc`-shared with the live logger, so mutating any of
/// them from any thread takes effect on the logger's next event.
///
/// # Sharing
///
/// Every field is a shared handle, not an owned value:
///
/// - [`filter`] and [`logger_config`] are `Arc<RwLock<...>>`   unconditionally.
/// - [`formatter_config`] is whatever the [`OffThreadLoggerBuilder`]
///   constructor chose for `FmtCfg`. The predefined constructors use
///   `Arc<RwLock<...>>`, and [`with_formatter`] uses `()` because the builder
///   cannot extract a shared handle from an arbitrary [`Formatter`].
///
/// Dropping an `OffThreadLoggerConfig` does not affect the logger.
/// The logger holds its own `Arc` clones of the filter and logger config,
/// and the formatter holds its own clone of the formatter config.
/// Dropping the config therefore only discards this struct's extra handles.
/// The logger keeps working.
///
/// Cloning an `OffThreadLoggerConfig` produces an independent handle to the
/// same shared state.
/// Two clones observe each other's writes.
///
/// # When a write takes effect
///
/// Writes are visible to the logger on the next event it processes, on both
/// sides of the offloader.
/// A write through [`filter`], for example, updates the caller-side [`Filter`],
/// because both branches of the pipeline hold clones of the same
/// `Arc<RwLock<OwningFilterRules>>`.
///
/// There is no barrier between a write and a subsequent `polog::info!` call
/// beyond the `RwLock`'s own happens-before edge: a write that returns
/// before the next `info!` call takes effect on that call.
/// A write that races with an in-flight event either lands before that
/// event's filter check or after it.
/// Either outcome is consistent with "takes effect on the next event".
///
/// # Which handle to use
///
/// The fields are public for direct manipulation.
/// See the [`LoggerConfig`] type documentation for the lock-hold constraint
/// that applies to update closures.
///
/// # Filter changes
///
/// The filter is shared between the caller-side middleware and the
/// worker-side foreign-event ingestion, so one write reconfigures both.
/// If the same `OffThreadLoggerConfig` is cloned and one clone drops the
/// filter handle while another keeps it, the shared value remains alive;
/// only the reference count changes.
///
/// [`Filter`]: polog_util::Filter
/// [`filter`]: Self::filter
/// [`formatter_config`]: Self::formatter_config
/// [`Formatter`]: crate::Formatter
/// [`logger_config`]: Self::logger_config
/// [`LoggerConfig`]: crate::LoggerConfig
/// [`OffThreadLoggerBuilder`]: crate::OffThreadLoggerBuilder
/// [`OffThreadLoggerBuilder::build`]: crate::OffThreadLoggerBuilder::build
/// [`OwningFilterRules`]: polog_util::OwningFilterRules
/// [`TextFormatterConfig`]: crate::TextFormatterConfig
/// [`with_formatter`]: crate::OffThreadLoggerBuilder::with_formatter
/// [`filter`]: Self::filter
///
/// [`LoggerConfig`]: crate::LoggerConfig
#[derive(Clone, Debug)]
pub struct OffThreadLoggerConfig<FmtCfg> {
    /// The `RUST_LOG`-style filter applied to both `polog` events and
    /// foreign events.
    ///
    /// A write through this handle takes effect on the next event processed
    /// by either side, but only for facades that read the filter per event.
    /// The recommended way to update it is to replace the whole value with
    /// [`FilterRules::parse`] followed by [`FilterRules::to_owning`]:
    ///
    /// ```rust,ignore
    /// let rules = BorrowingFilterRules::parse("info,my_crate=debug")?
    ///     .to_owning();
    /// *config.filter.write() = rules;
    /// config.update_global_log_level();
    /// config.update_global_tracing_interest_cache();
    /// ```
    ///
    /// # Per-facade propagation
    ///
    /// `polog` and `slog` events read this filter on every event and need no
    /// further action after a write. `log` and `tracing` cache an effective
    /// level at their registration point and do not re-read the filter for
    /// that purpose:
    ///
    /// - **`log`**: after a write, call [`Self::update_global_log_level`] if
    ///   the `log-0-4` feature is enabled and the logger is registered as the
    ///   global `log` logger. Without it, `log!` call sites at newly enabled
    ///   levels stay short-circuited by `log::max_level()` and never reach
    ///   [`Pologger`].
    /// - **`tracing`**: after a write, call
    ///   [`Self::update_global_tracing_interest_cache`] if the `tracing-0-1`
    ///   feature is enabled and the logger is registered as the global
    ///   `tracing` subscriber. Without it, call sites cached as
    ///   `Interest::never` remain disabled and `max_level_hint` is not
    ///   re-queried.
    ///
    /// Neither call is automatic because both write process-global state
    /// that affects every logger in the process, not only this one.
    ///
    /// # Locking
    ///
    /// Mutating the existing value in place is legal, but holds the write
    /// lock for the duration of the mutation.
    /// Replacing the whole value is preferred because it holds the lock for
    /// a single assignment.
    ///
    /// [`Pologger`]: crate::Pologger
    /// [`Filter`]: polog_util::Filter
    /// [`FilterRules::parse`]: polog_util::BorrowingFilterRules::parse
    /// [`FilterRules::to_owning`]: polog_util::FilterRules::to_owning
    pub filter: Arc<RwLock<OwningFilterRules>>,

    /// The logger's own configuration: rendering flags and their setters.
    ///
    /// Shared with the [`Pologger`] backend.
    /// A write through this handle takes effect on the next event rendered
    /// by the backend, which is the next event the worker processes.
    ///
    /// [`Pologger`]: crate::Pologger
    pub logger_config: Arc<RwLock<LoggerConfig>>,

    /// The formatter-specific runtime configuration.
    ///
    /// The concrete type is chosen by the [`OffThreadLoggerBuilder`]
    /// constructor:
    /// - [`stderr_text`], [`stdout_text`], and [`custom_text`] use
    ///   `Arc<RwLock<TextFormatterConfig>>`.
    /// - [`custom_ndjson`] uses `Arc<RwLock<NdJsonFormatterConfig>>`.
    /// - [`with_formatter`] uses `()`, because the builder cannot extract a
    ///   shared handle from an arbitrary [`Formatter`].
    ///
    /// [`Formatter`]: crate::Formatter
    /// [`OffThreadLoggerBuilder`]: crate::OffThreadLoggerBuilder
    /// [`custom_ndjson`]: crate::OffThreadLoggerBuilder::custom_ndjson
    /// [`custom_text`]: crate::OffThreadLoggerBuilder::custom_text
    /// [`stderr_text`]: crate::OffThreadLoggerBuilder::stderr_text
    /// [`stdout_text`]: crate::OffThreadLoggerBuilder::stdout_text
    /// [`with_formatter`]: crate::OffThreadLoggerBuilder::with_formatter
    pub formatter_config: FmtCfg,
}

impl<FmtCfg> OffThreadLoggerConfig<FmtCfg> {
    /// Creates a config handle from its shared parts.
    ///
    /// Intended for [`OffThreadLoggerBuilder::build`] and for callers
    /// constructing the chain by hand.
    /// Most applications obtain a config from the builder.
    ///
    /// # Sharing contract
    ///
    /// This constructor does not clone: each argument is moved into the
    /// struct as-is.
    /// The caller must ensure that:
    ///
    /// - `filter` is the same `Arc` that was passed to [`Filter::new`] on the
    ///   caller side and to [`Pologger::new`] on the worker side, otherwise
    ///   mutations through this config will not reach the filter;
    /// - `logger_config` is the same `Arc` that was passed to
    ///   [`Pologger::new`], otherwise flag updates will not reach the backend;
    /// - `formatter_config` is the same handle that the formatter holds,
    ///   otherwise formatter-config updates will not reach the formatter.
    ///
    /// The builder's `build` method wires all three correctly.
    /// A hand-written chain must reproduce the wiring or accept that some
    /// updates have no effect.
    ///
    /// [`Filter::new`]: polog_util::Filter::new
    /// [`OffThreadLoggerBuilder::build`]: crate::OffThreadLoggerBuilder::build
    /// [`Pologger::new`]: crate::Pologger::new
    /// [`Pologger::new`]: crate::Pologger::new
    #[inline]
    #[must_use]
    pub const fn new(
        filter: Arc<RwLock<OwningFilterRules>>,
        logger_config: Arc<RwLock<LoggerConfig>>,
        formatter_config: FmtCfg,
    ) -> Self {
        Self {
            filter,
            logger_config,
            formatter_config,
        }
    }

    /// The highest‑severity [`LevelFilter`] the filter will accept.
    #[inline]
    pub fn max_level(&self) -> LevelFilter {
        self.filter.read().max_level()
    }

    /// Updates the process-wide `log` max level to match the current filter.
    ///
    /// The [`log`] crate caches a process-global max level and every `log!`
    /// call site checks it before the event is constructed.
    /// A write to [`Self::filter`] is immediately visible to [`Pologger`]'s
    /// `log::Log::enabled` and `log::Log::log`, but `log!` will not reach
    /// either method if the cached max level still excludes the event's level.
    ///
    /// Calling this method propagates the filter's current
    /// [`FilterFn::max_level`] to `log::max_level()`, so subsequent `log!` call
    /// sites observe the new rule:
    ///
    /// - When the filter is **relaxed**, this call is required for correctness.
    ///   Without it, call sites at newly enabled levels stay short-circuited by
    ///   `log!` and never reach [`Pologger`].
    /// - When the filter is **tightened**, this call is a performance
    ///   optimization. Without it, events are still filtered correctly by
    ///   [`Pologger`]'s per-event check, but `log!` still pays the cost of
    ///   constructing the metadata and record before the record is discarded.
    ///
    /// # When to call
    ///
    /// Call this method after every write to [`Self::filter`].
    /// [`OffThreadLoggerBuilder::build`] calls it once at registration time to
    /// match the initial filter.
    /// Later filter changes are the caller's responsibility because this method
    /// writes a process-global value that affects every `log` logger in the
    /// process, not only this one.
    ///
    /// # Scope
    ///
    /// This method only updates the `log` facade.
    /// The other facades have different caching behavior and need separate
    /// handling:
    ///
    /// - [`polog`] and [`slog`] events read the filter on every event and need
    ///   no update.
    /// - [`tracing`] caches `Interest` per call site at registration and
    ///   queries [`Subscriber::max_level_hint`] once. Relaxing the filter after
    ///   registration requires `tracing_core::callsite::rebuild_interest_cache`
    ///   instead, which is deliberately not called here because it re-evaluates
    ///   every registered call site in the process.
    ///
    /// [`FilterFn::max_level`]: polog_util::FilterFn::max_level
    /// [`OffThreadLoggerBuilder::build`]: crate::OffThreadLoggerBuilder::build
    /// [`Self::filter`]: Self::filter
    /// [`Pologger`]: crate::Pologger
    /// [`Subscriber::max_level_hint`]: https://docs.rs/tracing-core/latest/tracing_core/subscriber/trait.Subscriber.html#method.max_level_hint
    /// [`log`]: https://docs.rs/log/latest/log/
    /// [`polog`]: https://docs.rs/polog/latest/polog/
    /// [`slog`]: https://docs.rs/slog/latest/slog/
    /// [`tracing`]: https://docs.rs/tracing/latest/tracing/
    #[cfg(feature = "log-0-4")]
    #[cfg_attr(docsrs, doc(cfg(feature = "log-0-4")))]
    #[inline]
    pub fn update_global_log_level(&self) {
        use polog_util::FilterFn;

        let max_level = self.filter.read().max_level();
        let max_level = polog_util::conv::log_0_4::level_filter_into_foreign_approx(max_level);
        log_0_4::set_max_level(max_level);
    }

    /// Rebuilds the `tracing` callsite interest cache against the current
    /// filter.
    ///
    /// [`tracing`] caches `Interest` per callsite and queries
    /// [`Subscriber::max_level_hint`] once, at subscriber registration.
    /// A write to [`Self::filter`] is visible to [`Pologger`]'s
    /// per-event `enabled` check, but callsites already cached as
    /// `Interest::never` stay disabled and `LevelFilter::current()` keeps
    /// the old hint.
    /// Calling this method re-evaluates both:
    /// - Every registered callsite is re-asked via
    ///   [`Subscriber::register_callsite`].
    /// - [`Subscriber::max_level_hint`] is re-queried and propagated to
    ///   `tracing_core::LevelFilter::current()`.
    ///
    /// # When to call
    ///
    /// Call this method after every write to [`Self::filter`] when the
    /// `tracing` integration is enabled.
    /// The call is expensive: it walks every registered callsite in the
    /// process, so prefer to batch filter updates if possible.
    ///
    /// # Availability
    ///
    /// Available with the `tracing-0-1` feature.
    /// Without it, there is no `tracing` facade to update and the method is not
    /// compiled.
    ///
    /// [`Self::filter`]: Self::filter
    /// [`Pologger`]: crate::Pologger
    /// [`Subscriber::register_callsite`]: https://docs.rs/tracing-core/latest/tracing_core/subscriber/trait.Subscriber.html#method.register_callsite
    /// [`Subscriber::max_level_hint`]: https://docs.rs/tracing-core/latest/tracing_core/subscriber/trait.Subscriber.html#method.max_level_hint
    /// [`tracing`]: https://docs.rs/tracing/latest/tracing/
    #[cfg(feature = "tracing-0-1")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tracing-0-1")))]
    #[inline]
    pub fn update_global_tracing_interest_cache(&self) {
        tracing_core_0_1::callsite::rebuild_interest_cache();
    }
}
