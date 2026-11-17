use alloc::sync::Arc;
use core::error::Error;
use core::fmt;
use std::io::{self, BufWriter, Stderr, Stdout};

use parking_lot::RwLock;
use polog::ContextArgs;

use crate::{
    ColorizePreset, Formatter, LoggerConfig, NdJsonFormatter, OffThreadLogger,
    OffThreadLoggerConfig, Pologger, TextFormatter, TextFormatterColorMode, TextFormatterConfig,
    TextFormatterFlags,
};

/// Configures and builds an [`OffThreadLogger`].
///
/// [`OffThreadLoggerBuilder`] shortens the manual composition shown in the
/// crate-level examples by bundling the choices that nearly every binary
/// makes the same way:
/// - `RUST_LOG`-style filter on both sides of the offloader.
/// - Offloading with a bounded channel and `DROP_NEWEST` backpressure.
/// - Periodic flush on the worker.
/// - `SystemTime` enricher on the caller side.
/// - Optional global registration with `log` and/or `tracing`.
///
/// Anything the builder does not expose (formatter flags, channel capacity,
/// backpressure policy, custom enrichers, file rotation) is available by
/// writing the chain manually.
/// The builder is a convenience, not a framework.
///
/// The `'a` lifetime is the lifetime of the filter specification.
/// The builder borrows it so that it does not need to allocate until
/// [`build`](Self::build).
#[derive(Debug)]
pub struct OffThreadLoggerBuilder<'a, Fmt, FmtCfg> {
    formatter: Fmt,
    formatter_config: FmtCfg,
    log_filter: &'a str,
}

impl<'a>
    OffThreadLoggerBuilder<'a, TextFormatter<BufWriter<Stderr>>, Arc<RwLock<TextFormatterConfig>>>
{
    /// Starts building a logger that writes text to `stderr`.
    ///
    /// The formatter uses [`TextFormatterFlags::default`] and
    /// [`ColorizePreset::Auto`], which is the right default for a
    /// terminal-facing CLI.
    #[inline]
    #[must_use]
    pub fn stderr_text(log_filter: &'a str) -> Self {
        let formatter = TextFormatter::stderr(TextFormatterFlags::default(), ColorizePreset::Auto);
        let formatter_config = Arc::clone(formatter.config());
        Self {
            formatter,
            formatter_config,
            log_filter,
        }
    }
}

impl<'a>
    OffThreadLoggerBuilder<'a, TextFormatter<BufWriter<Stdout>>, Arc<RwLock<TextFormatterConfig>>>
{
    /// Starts building a logger that writes text to `stdout`.
    ///
    /// Suitable for services whose `stderr` is reserved for runtime
    /// diagnostics, and for processes whose `stdout` is captured by a
    /// supervisor.
    #[inline]
    #[must_use]
    pub fn stdout_text(log_filter: &'a str) -> Self {
        let formatter = TextFormatter::stdout(TextFormatterFlags::default(), ColorizePreset::Auto);
        let formatter_config = Arc::clone(formatter.config());
        Self {
            formatter,
            formatter_config,
            log_filter,
        }
    }
}

impl<'a, W>
    OffThreadLoggerBuilder<'a, TextFormatter<BufWriter<W>>, Arc<RwLock<TextFormatterConfig>>>
where
    W: io::Write,
{
    /// Starts building a logger that writes text to a caller-supplied writer.
    ///
    /// Colorization is disabled.
    /// Use this when you have an already-opened `io::Write` whose
    /// TTY-ness does not need to be detected, for example a `Vec<u8>`, a
    /// network stream, or a `File` you have opened yourself with a specific
    /// truncate-or-append policy.
    #[inline]
    #[must_use]
    pub fn custom_text(writer: W, log_filter: &'a str) -> Self {
        let formatter_config = Arc::new(RwLock::new(TextFormatterConfig::new(
            TextFormatterFlags::default(),
            TextFormatterColorMode::Off,
        )));
        let formatter = TextFormatter::new(BufWriter::new(writer), Arc::clone(&formatter_config));
        Self {
            formatter,
            formatter_config,
            log_filter,
        }
    }
}

impl<'a, W> OffThreadLoggerBuilder<'a, NdJsonFormatter<BufWriter<W>>, ()>
where
    W: io::Write,
{
    /// Starts building a logger that writes NDJSON to a caller-supplied
    /// writer.
    ///
    /// Each line is one JSON object.
    /// The format is suitable for ingestion by `serde_json`-based pipelines and
    /// by log aggregators that accept newline-delimited JSON.
    ///
    /// `NdJsonFormatter` escape, but does not sanitize content.
    /// See the formatter's type-level documentation for the contract with
    /// respect to a downstream consumer that re-renders decoded strings in
    /// a terminal.
    #[inline]
    #[must_use]
    pub fn custom_ndjson(writer: W, log_filter: &'a str) -> Self {
        let formatter = NdJsonFormatter::new(BufWriter::new(writer));

        Self {
            formatter,
            formatter_config: (),
            log_filter,
        }
    }
}

impl<'a, Fmt> OffThreadLoggerBuilder<'a, Fmt, ()> {
    /// Starts building a logger with an arbitrary [`Formatter`].
    ///
    /// Use this constructor when none of the predefined destinations fits.
    ///
    /// # Formatter config
    ///
    /// The returned builder carries `()` as its formatter-config type, so
    /// [`build`] returns an [`OffThreadLoggerConfig<()>`] whose
    /// `formatter_config` is `()`. The builder cannot extract a formatter
    /// config from an arbitrary [`Formatter`] because the trait does not expose
    /// one. To obtain a runtime handle for a custom formatter, hold the
    /// formatter's own `config()` handle before passing the formatter in.
    ///
    /// ```rust,ignore
    /// let formatter = TextFormatter::stderr(flags, ColorizePreset::Auto);
    /// let formatter_config = Arc::clone(formatter.config());
    /// let (logger, config) =
    ///     OffThreadLoggerBuilder::with_formatter(formatter, "info")?.build()?;
    /// // `formatter_config` and `config` are both live.
    /// ```
    ///
    /// [`build`]: Self::build
    #[inline]
    #[must_use]
    pub const fn with_formatter(formatter: Fmt, log_filter: &'a str) -> Self {
        Self {
            formatter,
            formatter_config: (),
            log_filter,
        }
    }
}

impl<Fmt, FmtCfg> OffThreadLoggerBuilder<'_, Fmt, FmtCfg> {
    /// Builds the logger.
    ///
    /// # Steps
    ///
    /// 1. Parses `log_filter` into a [`polog_util::FilterRules`] rule set.
    /// 2. Builds a [`Pologger`] over the configured formatter.
    /// 3. Wraps the backend in a [`SharedPologger`] with a clone of the rule
    ///    set as the foreign-event filter.
    /// 4. Optionally registers the shared backend as the global `log` logger
    ///    and/or the global `tracing` subscriber.
    /// 5. Wraps the shared backend in an offloader worker and spawns the worker
    ///    thread.
    /// 6. Wraps the offloader in a [`polog_util::Filter`] with the same rule
    ///    set.
    /// 7. Wraps the handler in a [`polog_util::Enricher`] that adds
    ///    `SystemTime`.
    ///
    /// Both the `polog`-side filter and the foreign-event filter use the
    /// same rules, so per-crate overrides (`RUST_LOG`-style
    /// `my_crate=debug,info`) apply uniformly to events coming from `polog`
    /// macros and from `log`/`tracing` facades.
    ///
    /// The returned logger implements [`Debug`] so that applications can log
    /// the logger handle itself during diagnostics.
    /// This is only possible because the whole middleware chain from
    /// `polog-util` derives [`Debug`].
    /// The bound will be preserved across releases as long as
    /// that property holds.
    ///
    /// # Returned handles
    ///
    /// The returned [`OffThreadLoggerConfig`] shares its three
    /// `Arc<RwLock<...>>` handles with the returned logger and with each other.
    /// Mutating a handle from any thread takes effect on the next event the
    /// logger processes.
    /// Dropping the config does not affect the logger.
    /// The logger holds its own `Arc` clones.
    ///
    /// # Errors
    ///
    /// - [`BuildOffThreadLoggerError::ParseFilter`] — the filter specification
    ///   did not parse.
    /// - [`BuildOffThreadLoggerError::GlobalLogAlreadySet`] — the global `log`
    ///   logger was already set by another crate. Only reachable when
    ///   [`register_as_global_log_logger(true)`] was called on the builder.
    /// - [`BuildOffThreadLoggerError::GlobalTracingAlreadySet`] — the global
    ///   `tracing` subscriber was already set. Only reachable when
    ///   [`register_as_global_tracing_subscriber(true)`] was called on the
    ///   builder.
    /// - [`BuildOffThreadLoggerError::SpawnLogger`] — the offloader worker
    ///   thread could not be spawned.
    ///
    /// # Partial registration on failure
    ///
    /// When both registrations are requested and the second one fails, the
    /// first registration is already active and cannot be undone.
    /// This is an inherent limitation of the process-global registration APIs
    /// in `log` and `tracing`.
    /// Neither crate offers an unregister operation. The
    /// builder cannot roll back the first registration either.
    ///
    /// [`register_as_global_log_logger(true)`]: Self::register_as_global_log_logger
    /// [`register_as_global_tracing_subscriber(true)`]: Self::register_as_global_tracing_subscriber
    #[inline]
    pub fn build(
        self,
    ) -> Result<
        (
            OffThreadLogger<
                impl 'static + fmt::Debug + polog::SharedHandler<()> + polog_util::ExternalHandler<()>,
                impl ContextArgs,
            >,
            OffThreadLoggerConfig<FmtCfg>,
        ),
        BuildOffThreadLoggerError,
    >
    where
        Fmt: 'static + Send + Formatter,
    {
        use alloc::sync::Arc;
        use core::time::Duration;

        let span_id_generator = Arc::new(polog::SpanIdGenerator::new());

        let filter = polog_util::FilterRules::parse(self.log_filter)
            .map_err(BuildOffThreadLoggerError::ParseFilter)?
            .to_owning();
        let filter = Arc::new(RwLock::new(filter));

        let logger_config = Arc::new(RwLock::new(LoggerConfig::compact()));

        let backend = Pologger::new(
            Arc::clone(&span_id_generator),
            self.formatter,
            Arc::clone(&logger_config),
        );

        let (offloader, service) = polog_util::new_offloader(
            backend,
            Arc::clone(&span_id_generator),
            polog_util::OffloaderCapacity::Bounded(8192),
            polog_util::OffloaderBackpressurePolicy::DROP_NEWEST,
            Some(Duration::from_millis(1)), // Flush_interval.
            |_, ev: !| match ev {},         // Custom event.
        );

        let worker = service
            .spawn()
            .map_err(BuildOffThreadLoggerError::SpawnLogger)?;

        let handler = polog_util::Filter::new(offloader.clone(), Arc::clone(&filter));

        #[cfg(not(all(test, miri)))]
        let handler = polog_util::Enricher::new(handler, polog_util::SystemTimeEnricherFn);

        let logger = OffThreadLogger::new(handler, offloader, worker);
        let config = OffThreadLoggerConfig::new(filter, logger_config, self.formatter_config);

        Ok((logger, config))
    }
}

/// Error returned by [`OffThreadLoggerBuilder::build`].
#[derive(Debug)]
pub enum BuildOffThreadLoggerError {
    /// The filter specification did not parse.
    ///
    /// The wrapped error is [`polog_util::ParseFilterRulesError`].
    /// It describes which item in the specification was not a valid level name.
    ParseFilter(polog_util::ParseFilterRulesError),

    /// The offloader worker thread could not be spawned.
    ///
    /// The wrapped error is [`std::io::Error`] from
    /// [`std::thread::Builder::spawn`].
    /// The typical cause is a resource exhaustion at the operating system
    /// level.
    SpawnLogger(io::Error),
}

impl fmt::Display for BuildOffThreadLoggerError {
    #[expect(clippy::missing_inline_in_public_items, reason = "cold path")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParseFilter(err) => {
                write!(f, "Failed to parse logger filter: {err:?}")?;
            }
            Self::SpawnLogger(err) => {
                write!(f, "Failed to spawn logger service: {err:?}")?;
            }
        }
        Ok(())
    }
}

impl Error for BuildOffThreadLoggerError {}

impl From<!> for BuildOffThreadLoggerError {
    #[inline]
    fn from(value: !) -> Self {
        match value {}
    }
}

#[expect(clippy::unwrap_used, reason = "okay in tests")]
#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use core::fmt;
    use std::thread;

    use parking_lot::RwLock;
    use polog::LoggerExt;
    use polog_util::{OffloaderHandle, OwningFilterRules};

    use crate::{LoggerConfig, OffThreadLoggerBuilder, TextFormatterConfig};

    #[cfg(all(feature = "log-0-4", feature = "tracing-0-1"))]
    #[derive(Clone, Debug)]
    struct Writer(Arc<polog_util::Mutex<Vec<u8>>>);

    #[cfg(all(feature = "log-0-4", feature = "tracing-0-1"))]
    impl std::io::Write for Writer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.0.lock().flush()
        }
    }

    #[test]
    fn test_off_thread_config() {
        let (_logger, config) = OffThreadLoggerBuilder::stderr_text("warn,foo=debug")
            .build()
            .unwrap();

        let _: &Arc<RwLock<OwningFilterRules>> = &config.filter;
        let _: &Arc<RwLock<LoggerConfig>> = &config.logger_config;
        let _: &Arc<RwLock<TextFormatterConfig>> = &config.formatter_config;

        let (_logger, config) = OffThreadLoggerBuilder::stdout_text("warn,foo=debug")
            .build()
            .unwrap();

        let _: &Arc<RwLock<OwningFilterRules>> = &config.filter;
        let _: &Arc<RwLock<LoggerConfig>> = &config.logger_config;
        let _: &Arc<RwLock<TextFormatterConfig>> = &config.formatter_config;

        let (_logger, config) = OffThreadLoggerBuilder::custom_text(Vec::new(), "warn,foo=debug")
            .build()
            .unwrap();

        let _: &Arc<RwLock<OwningFilterRules>> = &config.filter;
        let _: &Arc<RwLock<LoggerConfig>> = &config.logger_config;
        let _: &Arc<RwLock<TextFormatterConfig>> = &config.formatter_config;

        let (_logger, config) = OffThreadLoggerBuilder::custom_ndjson(Vec::new(), "warn,foo=debug")
            .build()
            .unwrap();

        let _: &Arc<RwLock<OwningFilterRules>> = &config.filter;
        let _: &Arc<RwLock<LoggerConfig>> = &config.logger_config;
        let _: &() = &config.formatter_config;
    }

    #[test]
    fn test_off_thread_builder_logger() {
        let (mut logger, _config) = OffThreadLoggerBuilder::stderr_text("warn,foo=debug")
            .build()
            .unwrap();

        logger.flush_blocking();
        logger.flush_on_panic();

        let _: &dyn fmt::Debug = logger.handler();

        let _: Option<&OffloaderHandle<_, !>> = logger.offloader_handle();
        let _: Option<&thread::JoinHandle<()>> = logger.service_handle();

        let handle: Option<OffloaderHandle<_, !>> = logger.take_offloader_handle();
        handle.unwrap().stop();
        assert!(logger.take_offloader_handle().is_none());
        let handle: Option<thread::JoinHandle<()>> = logger.take_service_handle();
        handle.unwrap().join().unwrap();
        assert!(logger.take_service_handle().is_none());
    }

    #[cfg(all(feature = "log-0-4", feature = "tracing-0-1"))]
    #[test]
    fn test_off_thread_builder_global() {
        use alloc::sync::Arc;
        use core::mem;

        use parking_lot::Mutex;
        use polog::LoggerExt;
        use polog_util::FilterFn;

        use crate::OffThreadLoggerBuilder;
        use crate::test_util::assert_eq_with_wildcards;

        let writer = Writer(Arc::new(Mutex::new(Vec::new())));
        let (mut logger, config) =
            OffThreadLoggerBuilder::custom_text(writer.clone(), "warn,foo=debug")
                .build()
                .unwrap();

        let max_level = config.filter.read().max_level();
        logger.register_as_global_log_logger(max_level).unwrap();
        logger.register_as_global_tracing_subscriber().unwrap();

        polog::warn!(&mut logger, "Message 1");
        polog::info!(&mut logger, "Message 2");
        polog::info!(&mut logger, target: "foo", "Message 3");
        polog::info!(&mut logger, target: "bar", "Message 4");
        logger.flush_blocking();

        assert!(!log_0_4::log_enabled!(log_0_4::Level::Info));
        log_0_4::info!("Hello log");
        tracing_0_1::event!(tracing_0_1::Level::INFO, "Hello tracing");
        logger.flush_blocking();

        let written = mem::take(&mut *writer.0.lock());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "\
????-??-??T??:??:??.??????Z root    root WARN  pologger::off_thread::builder::tests Message 1
????-??-??T??:??:??.??????Z root    root INFO  foo Message 3
",
        );

        // Example of dynamic filter update.
        let filter = polog_util::FilterRules::parse("debug,foo=warn")
            .unwrap()
            .to_owning();
        *config.filter.write() = filter;
        config.update_global_log_level();
        config.update_global_tracing_interest_cache();

        polog::info!(&mut logger, "Message 1");
        polog::info!(&mut logger, "Message 2");
        polog::info!(&mut logger, target: "foo", "Message 3");
        polog::info!(&mut logger, target: "bar", "Message 4");
        logger.flush_blocking();

        assert!(log_0_4::log_enabled!(log_0_4::Level::Info));
        log_0_4::info!("Hello log");
        tracing_0_1::event!(tracing_0_1::Level::INFO, "Hello tracing");
        logger.flush_blocking();

        let written = mem::take(&mut *writer.0.lock());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "\
????-??-??T??:??:??.??????Z root    root INFO  pologger::off_thread::builder::tests Message 1
????-??-??T??:??:??.??????Z root    root INFO  pologger::off_thread::builder::tests Message 2
????-??-??T??:??:??.??????Z root    root INFO  bar Message 4
????-??-??T??:??:??.??????Z root    root INFO  pologger::off_thread::builder::tests Hello log
????-??-??T??:??:??.??????Z root    root INFO  pologger::off_thread::builder::tests Hello tracing
",
        );

        polog::info!(&mut logger, target: "target", "Message"; key = "value");
        logger.flush_blocking();

        log_0_4::info!(target: "target", key = "value"; "Hello log");
        tracing_0_1::event!(
            target: "target", tracing_0_1::Level::INFO, key = "value", "Hello tracing"
        );
        logger.flush_blocking();

        let written = mem::take(&mut *writer.0.lock());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "\
????-??-??T??:??:??.??????Z root    root INFO  target Message: key = value
????-??-??T??:??:??.??????Z root    root INFO  target Hello log: key = \"value\"
????-??-??T??:??:??.??????Z root    root INFO  target Hello tracing: key = value
",
        );

        test_off_thread_builder_global_tracing(&mut logger, &writer);
        test_off_thread_builder_global_tracing_kvs(&mut logger, &writer);
    }

    #[cfg(all(feature = "log-0-4", feature = "tracing-0-1"))]
    fn test_off_thread_builder_global_tracing<L>(logger: &mut L, writer: &Writer)
    where
        L: polog::Logger,
    {
        use core::mem;

        use crate::test_util::assert_eq_with_wildcards;

        let span = tracing_0_1::info_span!(target: "target", "Span1");
        let some_span_id = span.id().unwrap();
        logger.flush_blocking();
        let written = mem::take(&mut *writer.0.lock());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "????-??-??T??:??:??.??????Z root :> #1   INFO  target Span1\n",
        );

        tracing_0_1::event!(tracing_0_1::Level::INFO, "Hello tracing");
        let guard = span.enter();
        tracing_0_1::event!(tracing_0_1::Level::INFO, "Hello tracing");
        logger.flush_blocking();
        let written = mem::take(&mut *writer.0.lock());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "\
????-??-??T??:??:??.??????Z root    root INFO  pologger::off_thread::builder::tests Hello tracing
????-??-??T??:??:??.??????Z   #1 |> #2   INFO  target Span1
????-??-??T??:??:??.??????Z   #1    #2   INFO  pologger::off_thread::builder::tests Hello tracing
",
        );

        tracing_0_1::dispatcher::get_default(|dispatch| {
            assert_eq!(dispatch.current_span().id().unwrap().into_u64(), 1);
        });

        drop(guard);
        logger.flush_blocking();
        let written = mem::take(&mut *writer.0.lock());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "????-??-??T??:??:??.??????Z   #1 |< #2   INFO  target Span1\n",
        );

        thread::scope({
            let mut logger = logger.detach();
            let span = span.clone();
            move |scope| {
                let handle = scope.spawn(move || {
                    let guard = span.enter();
                    logger.flush_blocking();
                    let written = mem::take(&mut *writer.0.lock());
                    assert_eq_with_wildcards(
                        str::from_utf8(&written).unwrap(),
                        "????-??-??T??:??:??.??????Z   #1 |> #3   INFO  target Span1\n",
                    );

                    drop(guard);
                    logger.flush_blocking();
                    let written = mem::take(&mut *writer.0.lock());
                    assert_eq_with_wildcards(
                        str::from_utf8(&written).unwrap(),
                        "????-??-??T??:??:??.??????Z   #1 |< #3   INFO  target Span1\n",
                    );
                });
                handle.join().unwrap();
            }
        });

        drop(span);
        logger.flush_blocking();
        let written = mem::take(&mut *writer.0.lock());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "????-??-??T??:??:??.??????Z root :< #1   INFO  target Span1\n",
        );

        let span1 = tracing_0_1::info_span!(target: "target", "Span1", foo = "bar");
        let span2 = span1.record("foo", true);
        tracing_0_1::dispatcher::get_default(|dispatch| {
            dispatch.record_follows_from(&span2.id().unwrap(), &some_span_id);
        });
        drop(span1);

        logger.flush_blocking();
        let written = mem::take(&mut *writer.0.lock());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "\
????-??-??T??:??:??.??????Z root :> #4   INFO  target Span1: foo = bar
????-??-??T??:??:??.??????Z unkn    #4   INFO  target record: foo = true
????-??-??T??:??:??.??????Z unkn    #4   INFO  target record_follows_from: follows_from_span_id = 1
????-??-??T??:??:??.??????Z root :< #4   INFO  target Span1
",
        );
    }

    #[cfg(all(feature = "log-0-4", feature = "tracing-0-1"))]
    fn test_off_thread_builder_global_tracing_kvs<L>(logger: &mut L, writer: &Writer)
    where
        L: polog::Logger,
    {
        use core::error::Error;
        use core::mem;
        use std::io;

        use crate::test_util::assert_eq_with_wildcards;

        let error: &dyn Error = &io::Error::new(io::ErrorKind::NotFound, "missing");
        let span1 = tracing_0_1::info_span!(
            target: "target",
            "Span1",
            bool = true,
            u64 = 123_u64,
            i64 = -123_i64,
            u128 = 123_u128,
            i128 = -123_i128,
            f64 = 123.456_f64,
            error,
            fmt = format_args!("abc"),
        );
        drop(span1);
        logger.flush_blocking();
        let written = mem::take(&mut *writer.0.lock());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "\
????-??-??T??:??:??.??????Z root :> #5   INFO  target Span1: bool = true, u64 = 123, i64 = -123, \
             u128 = 123, i128 = -123, f64 = 123.456, error = missing, fmt = abc
????-??-??T??:??:??.??????Z root :< #5   INFO  target Span1
",
        );

        tracing_0_1::event!(
            target: "target",
            tracing_0_1::Level::INFO,
            bool = true,
            u64 = 123_u64,
            i64 = -123_i64,
            u128 = 123_u128,
            i128 = -123_i128,
            f64 = 123.456_f64,
            error = error,
            fmt = format_args!("abc"),
            "Message",
        );
        logger.flush_blocking();
        let written = mem::take(&mut *writer.0.lock());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "????-??-??T??:??:??.??????Z root    root INFO  target Message: bool = true, u64 = \
             123, i64 = -123, u128 = 123, i128 = -123, f64 = 123.456, error = missing, fmt = abc\n",
        );
    }

    #[cfg(feature = "slog-2")]
    #[test]
    fn test_off_thread_pologger_slog() {
        use core::error::Error;
        use core::mem;
        use std::io;

        use polog_util::Mutex;
        use slog_2::Drain;

        use crate::test_util::assert_eq_with_wildcards;

        let writer = Writer(Arc::new(Mutex::new(Vec::new())));
        let (mut logger, _config) =
            OffThreadLoggerBuilder::custom_text(writer.clone(), "warn,foo=debug")
                .build()
                .unwrap();

        let slog = logger.to_slog_logger();

        slog_2::info!(slog, "slog::info!"; "a" => 1, "b" => 2);
        {
            let slog = slog.new(slog_2::o!("c" => 3, "d" => 4));
            slog_2::info!(slog, "slog::info!"; "e" => 5, "f" => 6);
        }
        logger.flush_blocking();
        logger.flush_on_panic();
        let written = mem::take(&mut *writer.0.lock());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "\
????-??-??T??:??:??.??????Z root    root INFO  pologger::shared::logger::tests slog::info!: b = 2, \
             a = 1
????-??-??T??:??:??.??????Z root    root INFO  pologger::shared::logger::tests slog::info!: f = 6, \
             e = 5, d = 4, c = 3\n",
        );

        let error: &dyn Error = &io::Error::new(io::ErrorKind::NotFound, "missing");

        slog_2::info!(
            slog,
            "slog::info!";
            "unit" => (),
            "none" => Option::<()>::None,
            "char" => 'a',
            "str" => "abc",
            "u8" => 123_u8,
            "i8" => -123_i8,
            "u16" => 123_u16,
            "i16" => -123_i16,
            "u32" => 123_u32,
            "i32" => -123_i32,
            "u64" => 123_u64,
            "i64" => -123_i64,
            "u128" => 123_u128,
            "i128" => -123_i128,
            "usize" => 123_usize,
            "isize" => -123_isize,
            "f32" => 123_f32,
            "f64" => -123_f64,
            "error" => error,
            "fmt" => format_args!("abc"),
        );
        assert!(slog.is_enabled(slog_2::Level::Info));
        slog.flush().unwrap();
        logger.flush_blocking();
        logger.flush_on_panic();
        let written = mem::take(&mut *writer.0.lock());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "????-??-??T??:??:??.??????Z root    root INFO  pologger::shared::logger::tests \
             slog::info!: fmt = abc, error = missing, f64 = -123, f32 = 123, isize = -123, usize \
             = 123, i128 = -123, u128 = 123, i64 = -123, u64 = 123, i32 = -123, u32 = 123, i16 = \
             -123, u16 = 123, i8 = -123, u8 = 123, str = abc, char = a, none = None, unit = ()\n",
        );
    }
}
