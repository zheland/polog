use alloc::sync::Arc;
use core::fmt::Debug;

use parking_lot::Mutex;
use polog::{
    BaseHandler, BorrowingHandler, ContextArgs, DynHandler, Handler, SharedHandler, SpanIdGenerator,
};

use crate::{ExternalArguments, ExternalEntry, Formatter, Pologger};

#[cfg(feature = "tracing-0-1")]
static NEXT_SUBSCRIBER_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// A thread-safe, shareable wrapper around a [`Pologger`].
///
/// `SharedPologger` stores the backend behind `Arc<parking_lot::Mutex<_>>`
/// and clones that `Arc` cheaply, so it can be shared with `log`, `slog`,
/// and `tracing` registrations without moving the underlying `Pologger` out
/// of the caller's control.
///
/// When the corresponding features are enabled, `SharedPologger` implements
/// `log::Log`, `slog::Drain`, and `tracing::Subscriber`.
/// Each implementation locks the mutex for the duration of the call and
/// forwards the event to the inner `Pologger`.
///
/// # Filtering
///
/// Foreign events pass through the [`ExternalFilterFn`] given at
/// construction before reaching the inner `Pologger`.
/// The filter is expected to be the same rule set (or a clone of it) that
/// filters the `polog` side of the chain, so a single configuration covers both
/// directions.
///
/// [`ExternalFilterFn`]: polog_util::ExternalFilterFn
#[derive(Debug)]
pub struct SharedPologger<Fmt, Fff>(Arc<SharedPologgerInner<Fmt, Fff>>);

#[derive(Debug)]
struct SharedPologgerInner<Fmt, Fff> {
    pologger: Mutex<Pologger<Fmt>>,
    foreign_filter: Fff,
    span_id_generator: Arc<SpanIdGenerator>,
    #[cfg(feature = "tracing-0-1")]
    tracing_span_state: SharedPologgerTracingData,
}

/// Per-subscriber state used by the `tracing::Subscriber` adapter.
///
/// `SharedPologger` can be registered as a global `tracing` subscriber.
/// This struct tracks the foreign span objects that subscriber created and the
/// frames of the current thread's entered-span stack, keyed by the
/// subscriber's unique ID.
#[expect(clippy::redundant_pub_crate, reason = "false positive")]
#[cfg(feature = "tracing-0-1")]
#[derive(Debug)]
pub(crate) struct SharedPologgerTracingData {
    /// Live foreign span objects, keyed by polog `SpanId`.
    pub spans: dashmap::DashMap<core::num::NonZero<u64>, SharedPologgerTracingSpan>,

    /// Unique ID of the subscriber that owns this state, so the
    /// thread-local enter stack can tell subscribers apart.
    pub subscriber_id: u64,
}

/// A foreign span object tracked by the `tracing::Subscriber` adapter.
#[expect(clippy::redundant_pub_crate, reason = "false positive")]
#[cfg(feature = "tracing-0-1")]
#[derive(Copy, Clone, Debug)]
pub(crate) struct SharedPologgerTracingSpan {
    /// Static metadata of the tracing span.
    pub metadata: &'static tracing_0_1::Metadata<'static>,

    /// Parent span ID used in the `ForeignNew` emission for this span
    /// object. Persisted so `ForeignClose` can emit the same parent, pairing
    /// open and close of the span object.
    pub parent_span_id: Option<polog::SpanId>,

    /// Current ref-count, as tracked by `clone_span` and `try_close`.
    pub num_refs: usize,
}

impl<Fmt, Fff> SharedPologger<Fmt, Fff> {
    /// Wraps a `Pologger` and the foreign-event filter.
    ///
    /// Clones the backend's `span_id_generator` `Arc` so that the shared
    /// wrapper and the wrapped backend never allocate conflicting span IDs.
    #[inline]
    #[must_use]
    pub fn new(pologger: Pologger<Fmt>, foreign_filter: Fff) -> Self {
        let span_id_generator = Arc::clone(pologger.span_id_generator());
        Self(Arc::new(SharedPologgerInner {
            pologger: Mutex::new(pologger),
            foreign_filter,
            span_id_generator,
            #[cfg(feature = "tracing-0-1")]
            tracing_span_state: SharedPologgerTracingData::default(),
        }))
    }

    /// Returns a shared reference to the mutex that guards the wrapped
    /// handler.
    ///
    /// The reference is to the whole [`parking_lot::Mutex<H>`], not to the
    /// handler inside it.
    /// Acquiring the handler requires the caller to lock the returned mutex
    /// themselves:
    ///
    /// # Locking
    ///
    /// Every [`Handler`] method on `SharedPologger<H>` locks the same mutex for
    /// the duration of the call.
    /// The returned reference does not prevent those methods from blocking on
    /// your guard.
    /// It merely gives you direct access to the underlying lock.
    ///
    /// # Deadlocks
    ///
    /// [`parking_lot::Mutex`] is not reentrant.
    /// Calling any [`Handler`] method on the same `SharedPologger<H>` from
    /// inside a scope that holds a guard obtained from `mutex()` deadlocks.
    ///
    /// The same rule applies to `Clone` handles: `shared.clone()` shares
    /// the `Arc<Mutex<H>>`, so a guard held by one clone blocks every
    /// method on every other clone.
    ///
    /// [`Handler`]: polog::Handler
    #[inline]
    #[must_use]
    pub fn inner(&self) -> &Mutex<Pologger<Fmt>> {
        &self.0.pologger
    }

    /// Returns a reference to the foreign-event filter.
    ///
    /// Only foreign events go through this filter.
    /// Native `polog` events are filtered by whatever middleware wraps the
    /// `SharedPologger` on the `polog` side of the chain.
    #[inline]
    #[must_use]
    pub fn filter(&self) -> &Fff {
        &self.0.foreign_filter
    }

    /// Renders a foreign event through the inner `Pologger`.
    ///
    /// Locks the mutex for the duration of the call.
    /// If another thread is currently rendering, this call waits.
    #[inline]
    pub fn log_external_entry<Args>(&self, entry: &ExternalEntry<'_, Args>)
    where
        Fmt: Formatter,
        Args: ExternalArguments,
    {
        self.0.pologger.lock().log_external_entry(entry);
    }

    /// Registers this `SharedPologger` as the process-wide `log` backend.
    ///
    /// Also sets the global `log` max-level filter to the value reported by
    /// the filter's [`ExternalFilterFn::external_max_level`].
    ///
    /// # Errors
    ///
    /// Returns the same error as `log::set_boxed_logger`: the global
    /// logger has already been set.
    ///
    /// [`ExternalFilterFn::external_max_level`]: polog_util::ExternalFilterFn::external_max_level
    #[cfg(feature = "log-0-4")]
    #[cfg_attr(docsrs, doc(cfg(feature = "log-0-4")))]
    #[inline]
    pub fn register_as_global_log_logger(&self) -> Result<(), log_0_4::SetLoggerError>
    where
        Fmt: 'static + Send + Formatter,
        Fff: 'static + Send + Sync + polog_util::ExternalFilterFn,
    {
        let max_level = self.0.foreign_filter.external_max_level();
        let max_level = polog_util::conv::log_0_4::level_filter_into_foreign_approx(max_level);
        log_0_4::set_max_level(max_level);
        log_0_4::set_boxed_logger(Box::new(self.clone()))
    }

    /// Creates a `slog` logger that drains into this `SharedPologger`.
    ///
    /// The returned `slog::Logger` can be passed to any code that expects a
    /// `slog::Logger`.
    /// Events flow through the `slog::Drain` implementation of `SharedPologger`
    /// and are rendered by the inner `Pologger`.
    #[cfg(feature = "slog-2")]
    #[cfg_attr(docsrs, doc(cfg(feature = "slog-2")))]
    #[inline]
    pub fn make_slog_logger(&self) -> slog_2::Logger
    where
        Fmt: 'static + Send + Formatter,
        Fff: 'static + Send + Sync + polog_util::ExternalFilterFn,
    {
        slog_2::Logger::root(self.clone(), slog_2::o!())
    }

    /// Registers this `SharedPologger` as the process-wide `tracing`
    /// subscriber.
    ///
    /// # Errors
    ///
    /// Returns the same error as `tracing::subscriber::set_global_default`:
    /// the global subscriber has already been set.
    #[cfg(feature = "tracing-0-1")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tracing-0-1")))]
    #[inline]
    pub fn register_as_global_tracing_subscriber(
        &self,
    ) -> Result<(), tracing_0_1::subscriber::SetGlobalDefaultError>
    where
        Fmt: 'static + Send + Formatter,
        Fff: 'static + Send + Sync + polog_util::ExternalFilterFn,
    {
        tracing_0_1::subscriber::set_global_default(self.clone())
    }

    #[cfg(feature = "tracing-0-1")]
    #[inline]
    #[must_use]
    pub(crate) fn tracing_span_state(&self) -> &SharedPologgerTracingData {
        &self.0.tracing_span_state
    }

    /// Flushes the inner backend by shared reference.
    ///
    /// Equivalent to `BaseHandler::flush_blocking` but callable through a
    /// shared `&SharedPologger`, which is what `log::Log::flush` and
    /// `slog::Drain::flush` need.
    #[inline]
    pub fn flush_blocking_by_ref(&self)
    where
        Fmt: Formatter,
    {
        self.0.pologger.lock().flush_blocking();
    }

    /// Flushes the inner backend on a failure path by shared reference.
    ///
    /// The failure-path counterpart of [`flush_blocking_by_ref`].
    ///
    /// [`flush_blocking_by_ref`]: Self::flush_blocking_by_ref
    #[inline]
    pub fn flush_on_panic_by_ref(&self)
    where
        Fmt: Formatter,
    {
        self.0.pologger.lock().flush_on_panic();
    }
}

#[cfg(feature = "tracing-0-1")]
impl SharedPologgerTracingData {
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            spans: dashmap::DashMap::new(),
            subscriber_id: NEXT_SUBSCRIBER_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed),
        }
    }
}

#[cfg(feature = "tracing-0-1")]
impl Default for SharedPologgerTracingData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<Fmt, Fff> BaseHandler for SharedPologger<Fmt, Fff>
where
    Fmt: Formatter,
{
    #[inline]
    fn generate_span_id(&self) -> polog::SpanId {
        self.0.span_id_generator.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {
        self.0.pologger.lock().flush_blocking();
    }

    #[inline]
    fn flush_on_panic(&mut self) {
        self.0.pologger.lock().flush_on_panic();
    }
}

impl<Fmt, Fff, Context> DynHandler<Context> for SharedPologger<Fmt, Fff>
where
    Fmt: Formatter,
    Context: ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        descriptor: &'static polog::Descriptor,
        span_id: polog::SpanId,
        span_kind: polog::SpanKind,
        context: Context,
    ) {
        self.0
            .pologger
            .lock()
            .exit_span(descriptor, span_id, span_kind, context);
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        descriptor: &'static polog::Descriptor,
        span_id: polog::SpanId,
        context: Context,
    ) {
        self.0
            .pologger
            .lock()
            .enter_poll_span(descriptor, span_id, context);
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        descriptor: &'static polog::Descriptor,
        span_id: polog::SpanId,
        context: Context,
    ) {
        self.0
            .pologger
            .lock()
            .pending_poll_span(descriptor, span_id, context);
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        descriptor: &'static polog::Descriptor,
        span_id: polog::SpanId,
        context: Context,
    ) {
        self.0
            .pologger
            .lock()
            .ready_poll_span(descriptor, span_id, context);
    }
}

impl<Fmt, Fff, Context> Handler<Context> for SharedPologger<Fmt, Fff>
where
    Fmt: Formatter,
    Context: ContextArgs,
{
    type EnrichedContext = <Pologger<Fmt> as Handler<Context>>::EnrichedContext;

    #[inline]
    fn try_enable(
        &self,
        descriptor: &'static polog::Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        self.0.pologger.lock().try_enable(descriptor, context)
    }

    #[inline]
    fn log<'a, Args>(
        &mut self,
        descriptor: &'static polog::Descriptor,
        args: Args,
        span_id: Option<polog::SpanId>,
        context: Self::EnrichedContext,
    ) where
        Args: polog::LogArgs<'a>,
    {
        self.0
            .pologger
            .lock()
            .log(descriptor, args, span_id, context);
    }

    #[inline]
    fn enter_span<'a, Args>(
        &mut self,
        descriptor: &'static polog::Descriptor,
        args: Args,
        parent_span_id: Option<polog::SpanId>,
        span_id: polog::SpanId,
        span_kind: polog::SpanKind,
        context: Self::EnrichedContext,
    ) where
        Args: polog::LogArgs<'a>,
    {
        self.0.pologger.lock().enter_span(
            descriptor,
            args,
            parent_span_id,
            span_id,
            span_kind,
            context,
        );
    }

    #[inline]
    fn log_static<Args>(
        &mut self,
        descriptor: &'static polog::Descriptor,
        args: Args,
        span_id: Option<polog::SpanId>,
        context: Self::EnrichedContext,
    ) where
        Args: polog::StaticLogArgs,
    {
        self.0
            .pologger
            .lock()
            .log_static(descriptor, args, span_id, context);
    }

    #[inline]
    fn enter_span_static<Args>(
        &mut self,
        descriptor: &'static polog::Descriptor,
        args: Args,
        parent_span_id: Option<polog::SpanId>,
        span_id: polog::SpanId,
        span_kind: polog::SpanKind,
        context: Self::EnrichedContext,
    ) where
        Args: polog::StaticLogArgs,
    {
        self.0.pologger.lock().enter_span_static(
            descriptor,
            args,
            parent_span_id,
            span_id,
            span_kind,
            context,
        );
    }
}

impl<Fmt, Fff, Context> BorrowingHandler<Context> for SharedPologger<Fmt, Fff>
where
    Fmt: Formatter,
    Context: ContextArgs,
{
    #[inline]
    fn log_borrowed<Args>(
        &mut self,
        descriptor: &'static polog::Descriptor,
        args: &Args,
        span_id: Option<polog::SpanId>,
        context: &Self::EnrichedContext,
    ) where
        Args: polog::StaticLogArgs,
    {
        self.0
            .pologger
            .lock()
            .log_borrowed(descriptor, args, span_id, context);
    }

    #[inline]
    fn enter_span_borrowed<Args>(
        &mut self,
        descriptor: &'static polog::Descriptor,
        args: &Args,
        parent_span_id: Option<polog::SpanId>,
        span_id: polog::SpanId,
        span_kind: polog::SpanKind,
        context: &Self::EnrichedContext,
    ) where
        Args: polog::StaticLogArgs,
    {
        self.0.pologger.lock().enter_span_borrowed(
            descriptor,
            args,
            parent_span_id,
            span_id,
            span_kind,
            context,
        );
    }
}

impl<Fmt, Fff, Context> SharedHandler<Context> for SharedPologger<Fmt, Fff>
where
    Fff: 'static + Send + Sync,
    Fmt: 'static + Send + Sync + Formatter,
    Context: ContextArgs,
{
}

impl<Fmt, Fff> Clone for SharedPologger<Fmt, Fff> {
    #[inline]
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

#[expect(clippy::unwrap_used, reason = "okay in tests")]
#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use alloc::vec::Vec;
    use core::mem;

    use parking_lot::RwLock;
    use polog::{LevelFilter, SpanIdGenerator};
    use polog_util::filters;

    use super::SharedPologger;
    use crate::test_util::{assert_eq_with_wildcards, emit_handler_events};
    use crate::{
        LoggerConfig, Pologger, TextFormatter, TextFormatterColorMode, TextFormatterConfig,
        TextFormatterFlags,
    };

    #[inline]
    fn new_pologger(
        config: Arc<RwLock<LoggerConfig>>,
        external_filter: filters::MaxLevel,
    ) -> SharedPologger<TextFormatter<Vec<u8>>, filters::MaxLevel> {
        let formatter_config =
            TextFormatterConfig::new(TextFormatterFlags::default(), TextFormatterColorMode::Off);
        let formatter_config = Arc::new(RwLock::new(formatter_config));
        let formatter = TextFormatter::new(Vec::<u8>::new(), formatter_config);

        let span_id_generator = Arc::new(SpanIdGenerator::new());
        let logger = Pologger::new(span_id_generator, formatter, config);
        SharedPologger::new(logger, external_filter)
    }

    #[test]
    fn test_shared_pologger_basic() {
        let config = Arc::new(RwLock::new(LoggerConfig::default()));
        let mut logger = new_pologger(config, filters::MaxLevel(LevelFilter::Info));

        polog::info!(&mut logger, "Message");
        logger.flush_blocking_by_ref();
        logger.flush_on_panic_by_ref();
        let written = mem::take(logger.inner().lock().formatter_mut().writer_mut());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "????-??-??T??:??:??.??????Z root    root INFO  pologger::shared::logger::tests \
             Message\n",
        );
    }

    #[test]
    fn test_shared_pologger_low_level() {
        let config = Arc::new(RwLock::new(LoggerConfig::default()));
        let mut logger = new_pologger(config, filters::MaxLevel(LevelFilter::Info));

        emit_handler_events(&mut logger);
        let written = mem::take(logger.inner().lock().formatter_mut().writer_mut());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "\
????-??-??T??:??:??.??????Z root    root INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root    root INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root    root INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root    root INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root >> #1   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root << #1   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root |> #1   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root |< #1   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root >> #1   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root << #1   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root >> #1   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root << #1   INFO  pologger::test_util Message
",
        );
    }

    #[cfg(feature = "slog-2")]
    #[test]
    fn test_shared_pologger_slog() {
        use core::error::Error;
        use std::io;

        use slog_2::Drain;

        let config = Arc::new(RwLock::new(LoggerConfig::default()));
        let logger = new_pologger(config, filters::MaxLevel(LevelFilter::Info));
        let slog = logger.make_slog_logger();

        slog_2::info!(slog, "slog::info!"; "a" => 1, "b" => 2);
        {
            let slog = slog.new(slog_2::o!("c" => 3, "d" => 4));
            slog_2::info!(slog, "slog::info!"; "e" => 5, "f" => 6);
        }
        logger.flush_blocking_by_ref();
        logger.flush_on_panic_by_ref();
        let written = mem::take(logger.inner().lock().formatter_mut().writer_mut());
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
        logger.flush_blocking_by_ref();
        logger.flush_on_panic_by_ref();
        let written = mem::take(logger.inner().lock().formatter_mut().writer_mut());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "????-??-??T??:??:??.??????Z root    root INFO  pologger::shared::logger::tests \
             slog::info!: fmt = abc, error = missing, f64 = -123, f32 = 123, isize = -123, usize \
             = 123, i128 = -123, u128 = 123, i64 = -123, u64 = 123, i32 = -123, u32 = 123, i16 = \
             -123, u16 = 123, i8 = -123, u8 = 123, str = abc, char = a, none = None, unit = ()\n",
        );
    }
}
