#![expect(missing_docs, reason = "okay in examples")]
#![expect(clippy::unwrap_used, reason = "okay in examples")]

use core::time::Duration;
use std::sync::Arc;

use either::Either;
use parking_lot::RwLock;
use polog::{Logger, LoggerExt, PollInstrument};
use polog_util::bridges::log_0_4::BridgeLog;
use polog_util::bridges::slog_2::BridgeSlog;
use polog_util::bridges::tracing_0_1::BridgeTracing;

fn main() {
    let preset = std::env::args().nth(1);
    let filter = std::env::args().nth(2);
    let mut logger = new_logger_handler(preset.as_deref(), filter.as_deref());

    BridgeLog::new(logger.clone())
        .try_install_global(polog::LevelFilter::Trace)
        .unwrap();
    BridgeTracing::new(logger.clone())
        .try_install_global()
        .unwrap();
    let slog = BridgeSlog::new(logger.clone()).into_slog_logger();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    polog::info!(&mut logger, "Tokio Runtime Started");

    rt.block_on(app(&mut logger.detach(), &slog));

    logger.flush_blocking();
}

#[inline]
async fn app<L>(logger: &mut L, slog: &slog::Logger)
where
    L: Logger,
{
    let mut logger = polog::info_span!(logger; @fn);

    polog::info!(&mut logger, "Information Message"; key = "value");
    polog::warn!(&mut logger, "Warning Message"; 123, 234, 345);
    polog::error!(&mut logger, "Error Message"; ? ("a", 'b', 3, true), 1 - 2);

    nested(&mut logger, 3);

    let handle1 = std::thread::spawn({
        let mut logger = logger.detach();
        move || sync_fn(&mut logger, 1, 2)
    });

    let handle2 = std::thread::spawn({
        let mut logger = logger.detach();
        move || sync_fn(&mut logger, 3, 4)
    });

    handle1.join().unwrap();
    handle2.join().unwrap();

    let _guard = polog::owning_debug_span!(&logger, "Movable Owned Guard");

    let handle1 = tokio::task::spawn({
        let mut logger = logger.detach();
        async move { async_fn(&mut logger, "foo").await }
    });

    let handle2 = tokio::task::spawn({
        let mut logger = logger.detach();
        async move { async_fn(&mut logger, "abcdefghi").await }
    });

    handle1.await.unwrap();
    handle2.await.unwrap();

    let deferred = polog::deferred_trace_span!(&mut logger, "Instrumented In Place");
    {
        (|mut logger| async move { async_fn(&mut logger, "something").await })
            .poll_instrument(deferred)
    }
    .await;

    let deferred = polog::deferred_trace_span!(&mut logger, "Instrumented Detached");
    tokio::task::spawn({
        (|mut logger| async move { async_fn(&mut logger, "uh").await }).poll_instrument(deferred)
    })
    .await
    .unwrap();

    foreign(&mut logger, slog);

    sanitized(&mut logger);
}

#[inline]
fn nested<L>(logger: &mut L, remained: u32)
where
    L: Logger,
{
    let mut logger = polog::debug_span!(logger; @fn, remained);
    polog::trace!(&mut logger, "Message in Nested Function"; remained);

    if let Some(remained) = remained.checked_sub(1) {
        nested(&mut logger, remained);
    }
}

fn foreign<L>(logger: &mut L, slog_logger: &slog::Logger)
where
    L: Logger,
{
    let mut logger = polog::debug_span!(logger; @fn);
    logger.flush_blocking();

    log::info!(a = 1, b = 2; "`log::info!`");
    log::info!(c = 3, d = 4; "`log::info!`");

    tracing::event!(tracing::Level::INFO, "`tracing::event!`");
    let span = tracing::info_span!("`tracing::info_span!` Span A", e = 5, f = 6);
    {
        let _guard = span.enter();
        tracing::event!(tracing::Level::INFO, "`tracing::event!` In A");
    }
    {
        let _guard = span.enter();
        tracing::event!(
            tracing::Level::INFO,
            g = 7,
            h = 8,
            "`tracing::event!`` In A",
        );

        let span = tracing::info_span!("`tracing::info_span!` Nested Span B", i = 9, j = 10);
        {
            let _guard = span.enter();
            tracing::event!(tracing::Level::INFO, "`tracing::event!` In B");
        }
        {
            let _guard = span.enter();
            tracing::event!(
                tracing::Level::INFO,
                message = "another message arg",
                "`tracing::event! tail arg` In B",
            );
        }
    }
    drop(span);

    slog::info!(slog_logger, "slog::info!"; "k" => 11, "l" => 12);
    {
        let slog_logger = slog_logger.new(slog::o!("m" => 13, "n" => 14));
        slog::info!(slog_logger, "slog::info!"; "o" => 15, "p" => 16);
    }
}

fn sanitized<L>(logger: &mut L)
where
    L: Logger,
{
    let mut logger = polog::debug_span!(
        logger,
        target: "abc\x7fdef",
        "Check\x7f sanitization\x07.";
        "key\x7f\r." = "value\x7f\r.",
        "\x1b[34mANSI\x1b[0m",
    );
    polog::debug!(
        &mut logger,
        target: "abc\x7fdef",
        "A\x7f B\x07.";
        "key\x7f\r." = "value\x7f\r.",
        "\x1b[34mANSI\x1b[0m" = "\x1b[34mANSI\x1b[0m",
    );
}

#[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
#[inline]
fn sync_fn<L>(logger: &mut L, param1: u32, param2: u32)
where
    L: Logger,
{
    let mut logger = polog::debug_span!(logger; @fn, param1, param2);
    polog::trace!(&mut logger, "Message in Sync Function"; sum = param1 + param2);
}

#[inline]
async fn async_fn<L>(logger: &mut L, param: &str)
where
    L: Logger,
{
    let mut logger = polog::debug_span!(logger; @fn, :t param);
    polog::trace!(&mut logger, "First Message in Async Function"; len = param.len());
    tokio::task::yield_now().await;
    polog::trace!(&mut logger, "Second Message in Async Function"; len = param.len());
}

#[inline]
fn new_logger_handler(
    preset: Option<&str>,
    filter: Option<&str>,
) -> impl polog::SharedHandler<()> + polog_util::ExternalHandler<()> {
    let span_id_generator = Arc::new(polog::SpanIdGenerator::new());

    let filter = polog_util::FilterRules::parse(filter.unwrap_or("trace"))
        .unwrap()
        .to_owning();

    let backend = new_logger_backend(Arc::clone(&span_id_generator), preset);

    let (handler, service) = polog_util::new_offloader(
        backend,
        span_id_generator,
        polog_util::OffloaderCapacity::Bounded(8192),
        polog_util::OffloaderBackpressurePolicy::DROP_NEWEST,
        Some(Duration::from_millis(1)), // Flush_interval.
        |_, ev: !| match ev {},         // Custom_handler.
    );

    let _handle = service.spawn();

    let handler = polog_util::Filter::new(handler, filter);

    // Optionally use can wrap handler into enrichers.
    // If wrapped, the log events would enriched by the time on caller side.
    // If not wrapped, the time would be captured by the backend handler.
    polog_util::Enricher::new(handler, polog_util::SystemTimeEnricherFn)
}

#[inline]
fn new_logger_backend(
    span_id_generator: Arc<polog::SpanIdGenerator>,
    preset: Option<&str>,
) -> pologger::Pologger<impl 'static + pologger::Formatter> {
    enum Formatter {
        Text,
        Json,
    }
    enum Format {
        Compact,
        SpanPath,
        CompactAndPoll,
        SpanPathAndPoll,
    }

    let (formatter, format) = match preset {
        Some("text-compact") | None => (Formatter::Text, Format::Compact),
        Some("text-span-path") => (Formatter::Text, Format::SpanPath),
        Some("text-compact-poll") => (Formatter::Text, Format::CompactAndPoll),
        Some("text-span-path-poll") => (Formatter::Text, Format::SpanPathAndPoll),

        Some("ndjson-compact") => (Formatter::Json, Format::Compact),
        Some("ndjson-span-path") => (Formatter::Json, Format::SpanPath),
        Some("ndjson-compact-poll") => (Formatter::Json, Format::CompactAndPoll),
        Some("ndjson-span-path-poll") => (Formatter::Json, Format::SpanPathAndPoll),

        #[expect(clippy::panic, reason = "okay in examples")]
        Some(other) => panic!("Unknown format {other}"),
    };

    let formatter = match formatter {
        Formatter::Text => Either::Left(pologger::TextFormatter::stderr(
            pologger::TextFormatterFlags::default(),
            pologger::ColorizePreset::Auto,
        )),
        Formatter::Json => Either::Right(pologger::NdJsonFormatter::stderr()),
    };

    let config = match format {
        Format::Compact => pologger::LoggerConfig::compact(),
        Format::SpanPath => pologger::LoggerConfig::compact_span_path(),
        Format::CompactAndPoll => pologger::LoggerConfig::compact().with_task_poll_events(),
        Format::SpanPathAndPoll => {
            pologger::LoggerConfig::compact_span_path().with_task_poll_events()
        }
    };

    pologger::Pologger::new(span_id_generator, formatter, Arc::new(RwLock::new(config)))
}
