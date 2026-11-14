//! Delegators from `polog` to `slog` 2.x.x.

use core::marker::PhantomData;

use polog::{
    BaseHandler, BorrowingHandler, ContextArgs, Descriptor, DisplayableMixedArgs, DynHandler,
    Handler, LogArgs, SharedHandler, SpanId, SpanIdGenerator, SpanKind, StaticLogArgs,
};
use slog_2 as slog;

use crate::conv::slog_2::level_into_foreign_approx;

static SPAN_ID_GENERATOR: SpanIdGenerator = SpanIdGenerator::new();

/// A `polog` [`Handler`] adapter over a [`slog::Logger`].
///
/// Forwards `polog` events to [`slog::Logger::log`] by building a
/// [`slog::Record`] from the static [`Descriptor`] and the argument tuple.
///
/// # Drain requirements
///
/// The wrapped drain must be `Send + Sync + slog::Drain<Ok = (), Err = !>`.
/// The `!` error bound lets `ViaSlog` forward records with `logger.log(..)`
/// without producing a `Result` that no `polog` caller is prepared to handle.
///
/// # Caveats
///
/// - `slog` has no native span concept: [`Handler::enter_span`] logs the span
///   descriptor as an ordinary record, and span exit plus all per-poll
///   lifecycle events are silently ignored.
/// - `slog::Drain` has no `is_enabled` hook, so [`Handler::try_enable`] always
///   returns `Some`. Per-drain filtering (for example `LevelFilter`) is applied
///   inside `Drain::log`.
/// - `slog::Level` has no equivalent of `polog`'s `Bug` / `Fatal` levels. Both
///   are mapped to `Level::Error`.
/// - [`slog::Logger::log`] returns `Result<D::Ok, D::Err>`. `D::Ok` may be a
///   future for async drains. The result is dropped (fire-and-forget).
/// - Source-location fidelity depends on the `slog-2-semver-unstable` feature.
///
/// # Source-location fidelity
///
/// There are two mutually exclusive ways to build a [`slog::RecordStatic`]:
///
/// - **Semver-reliable path** (default). Uses [`slog::record_static!`], which
///   is the only construction route slog documents as supported. The resulting
///   `RecordLocation` points at *this* function's expansion site, so every
///   forwarded record reports the same `file` / `line` / `column` / `module`,
///   not the polog call site.
///
/// - **semver-unstable path** (`slog-2-semver-unstable` feature). Constructs
///   [`slog::RecordStatic`] and [`slog::RecordLocation`] directly from their
///   public-but-not-semver-stable fields so that the descriptor's real source
///   location and module path reach the drain. This mirrors the
///   `FieldSet::value_set_all` hack in the tracing adapter: it is the only way
///   to get the correct data, and it is fenced off behind a feature flag
///   because a minor slog release may break it.
///
/// [`Handler::enter_span`]: polog::Handler::enter_span
/// [`slog::Logger::log`]: slog_2::Logger::log
/// [`slog::Record`]: slog_2::Record
#[derive(Clone, Debug)]
pub struct ViaSlog<D>(pub slog::Logger<D>)
where
    D: Send + Sync + slog::Drain<Ok = (), Err = !>;

impl<D> BaseHandler for ViaSlog<D>
where
    D: Send + Sync + slog::Drain<Ok = (), Err = !>,
{
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        SPAN_ID_GENERATOR.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {
        // `slog::Drain` has no portable flush hook.
    }

    #[inline]
    fn flush_on_panic(&mut self) {}
}

impl<D, Context> DynHandler<Context> for ViaSlog<D>
where
    D: Send + Sync + slog::Drain<Ok = (), Err = !>,
    Context: ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _span_kind: SpanKind,
        _context: Context,
    ) {
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
    }
}

impl<D, Context> Handler<Context> for ViaSlog<D>
where
    D: Send + Sync + slog::Drain<Ok = (), Err = !>,
    Context: ContextArgs,
{
    type EnrichedContext = Context;

    #[inline]
    fn try_enable(
        &self,
        _descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        // `slog::Drain` has no `is_enabled` hook, so per-drain filtering is
        // applied when the record is actually logged.
        Some(context)
    }

    #[inline]
    fn log<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        _span_id: Option<SpanId>,
        _context: Context,
    ) where
        Args: LogArgs<'a>,
    {
        log(&self.0, descriptor, &args);
    }

    #[inline]
    fn enter_span<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        _parent_span_id: Option<SpanId>,
        _span_id: SpanId,
        _span_kind: SpanKind,
        _context: Context,
    ) where
        Args: LogArgs<'a>,
    {
        // `slog` has no span lifecycle.
        // Log the span entry as a regular record so the span's message and
        // args are at least visible.
        log(&self.0, descriptor, &args);
    }

    #[inline]
    fn log_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        _span_id: Option<SpanId>,
        _context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        log(&self.0, descriptor, &args);
    }

    #[inline]
    fn enter_span_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        _parent_span_id: Option<SpanId>,
        _span_id: SpanId,
        _span_kind: SpanKind,
        _context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        // `slog` has no span lifecycle.
        // Log the span entry as a regular record so the span's message and
        // args are at least visible.
        log(&self.0, descriptor, &args);
    }
}

impl<D, Context> BorrowingHandler<Context> for ViaSlog<D>
where
    D: Send + Sync + slog::Drain<Ok = (), Err = !>,
    Context: ContextArgs,
{
    #[inline]
    fn log_borrowed<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        _span_id: Option<SpanId>,
        _context: &Context,
    ) where
        Args: StaticLogArgs,
    {
        log(&self.0, descriptor, args);
    }

    #[inline]
    fn enter_span_borrowed<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        _parent_span_id: Option<SpanId>,
        __span_id: SpanId,
        _span_kind: SpanKind,
        _context: &Context,
    ) where
        Args: StaticLogArgs,
    {
        // `slog` has no span lifecycle.
        // Log the span entry as a regular record so the span's message and
        // args are at least visible.
        log(&self.0, descriptor, args);
    }
}

impl<D, Context> SharedHandler<Context> for ViaSlog<D>
where
    D: 'static + Send + Sync + Clone + slog::Drain<Ok = (), Err = !>,
    Context: ContextArgs,
{
}

/// Emits one `slog::Record` built from a `polog` descriptor and argument
/// tuple.
///
/// # Source-location fidelity
///
/// There are two mutually exclusive ways to build a [`slog::RecordStatic`]:
///
/// - **Semver-reliable path** (default). Uses [`slog::record_static!`], which
///   is the only construction route slog documents as supported. The resulting
///   `RecordLocation` points at *this* function's expansion site, so every
///   forwarded record reports the same `file` / `line` / `column` / `module`,
///   not the polog call site.
///
/// - **semver-unstable path** (`slog-2-semver-unstable` feature). Constructs
///   [`slog::RecordStatic`] and [`slog::RecordLocation`] directly from their
///   public-but-not-semver-stable fields so that the descriptor's real source
///   location and module path reach the drain. This mirrors the
///   `FieldSet::value_set_all` hack in the tracing adapter: it is the only way
///   to get the correct data, and it is fenced off behind a feature flag
///   because a minor slog release may break it.
#[inline]
fn log<'a, D, Args>(logger: &slog::Logger<D>, descriptor: &Descriptor, args: &Args)
where
    D: Send + Sync + slog::Drain<Ok = (), Err = !>,
    Args: DisplayableMixedArgs<'a>,
{
    let level = level_into_foreign_approx(descriptor.level);
    let tag = descriptor.target.unwrap_or(descriptor.module_path);
    let kv = Kv::new(descriptor.param_names, args);
    let msg = format_args!("{}", descriptor.message);

    // # UNRELIABLE API USAGE (feature-gated).
    //
    // `RecordStatic` and `RecordLocation` fields are public but the docs mark
    // `record_static!` as the supported constructor. Building them by hand is
    // the only way to plumb the polog call-site location through. Any field
    // addition or rename in slog is a semver-compatible break for this code.
    //
    // `location` is declared in the outer scope so that `record_static`
    // (which holds `&location`) does not outlive it.
    #[cfg(feature = "slog-2-semver-unstable")]
    let location = slog::RecordLocation {
        file: descriptor.location.file(),
        line: descriptor.location.line(),
        column: descriptor.location.column(),
        // Keep empty as `slog` currently does not record it as well.
        function: "",
        module: descriptor.module_path,
    };

    #[cfg(feature = "slog-2-semver-unstable")]
    let record_static = slog::RecordStatic {
        location: &location,
        level,
        tag,
    };

    // Semver-reliable path. Note that `record_static!` produces a
    // `RecordStatic<'_>` borrowing a function-local `static LOC`, so the
    // resulting `record_static` binding cannot outlive this function either.
    #[cfg(not(feature = "slog-2-semver-unstable"))]
    let record_static = slog::record_static!(level, tag);

    let record = slog::Record::new(&record_static, &msg, slog::BorrowedKV(&kv));

    logger.log(&record);
}

/// Bridges a heterogeneous tuple of `Display` values to `slog::KV`.
///
/// Plays the same role as `log_0_4::Kv`: it walks the leaf arguments in order
/// and emits each as a `Key` / `Display` pair. Uses
/// `Serializer::emit_arguments` so no per-argument `slog::Value` bound is
/// required.
#[derive(Copy, Clone, Debug)]
struct Kv<'a, T> {
    keys: &'static [&'static str],
    values: &'a T,
    _lt: PhantomData<&'a ()>,
}

impl<'a, T> Kv<'a, T> {
    #[inline]
    #[must_use]
    const fn new(keys: &'static [&'static str], values: &'a T) -> Self {
        Self {
            keys,
            values,
            _lt: PhantomData,
        }
    }
}

impl<'a, 'b, T> slog::KV for Kv<'a, T>
where
    'b: 'a,
    T: DisplayableMixedArgs<'b>,
{
    fn serialize(
        &self,
        _record: &slog::Record<'_>,
        serializer: &mut dyn slog::Serializer,
    ) -> slog::Result {
        for (i, key) in self.keys.iter().take(self.values.num_args()).enumerate() {
            if let Some(value) = self.values.nth_dyn(i) {
                serializer.emit_arguments(slog::Key::from(*key), &format_args!("{value}"))?;
            }
        }
        Ok(())
    }
}

#[cfg(feature = "alloc")]
#[cfg(test)]
mod tests {
    use alloc::borrow::ToOwned;
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec::Vec;
    use alloc::{format, vec};
    use core::{fmt, mem};

    use parking_lot::Mutex;
    use polog::{Logger, LoggerExt, PollInstrument};
    use slog_2 as slog;
    use slog_2::KV;

    use crate::delegators::slog_2::ViaSlog;

    #[derive(Clone, Default, Debug)]
    pub struct Tracker {
        events: Arc<Mutex<Vec<Event>>>,
    }

    #[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
    struct FieldVisitor(Vec<(&'static str, String)>);

    #[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
    pub enum Event {
        Log(String, Vec<(&'static str, String)>),
        IsEnabled,
    }

    impl Tracker {
        #[inline]
        #[must_use]
        pub fn take_events(&self) -> Vec<Event> {
            mem::take(&mut *self.events.lock())
        }

        #[inline]
        fn logger(&self) -> slog::Logger {
            slog::Logger::root(self.clone(), slog::o!())
        }
    }

    impl slog::Serializer for FieldVisitor {
        fn emit_arguments(&mut self, key: slog::Key, val: &fmt::Arguments<'_>) -> slog::Result {
            self.0.push((key, format!("{val}")));
            Ok(())
        }
    }

    impl slog::Drain for Tracker {
        type Ok = ();
        type Err = !;

        fn log(
            &self,
            record: &slog::Record<'_>,
            values: &slog::OwnedKVList,
        ) -> Result<Self::Ok, Self::Err> {
            let mut visitor = FieldVisitor::default();
            let _ = record.kv().serialize(record, &mut visitor);
            let _ = values.serialize(record, &mut visitor);

            self.events
                .lock()
                .push(Event::Log(format!("{}", record.msg()), visitor.0));

            Ok(())
        }

        fn is_enabled(&self, _level: slog::Level) -> bool {
            self.events.lock().push(Event::IsEnabled);
            true
        }
    }

    #[tokio::test]
    async fn test_delegator_slog() {
        let tracker = Tracker::default();
        test_log_basic(&tracker);
        test_log_static(&tracker);
        test_log_borrowed(&tracker);
        test_log_async(&tracker).await;
    }

    fn test_log_basic(tracker: &Tracker) {
        let mut logger = ViaSlog(tracker.logger());

        assert!(slog_2::Drain::is_enabled(&logger.0, slog_2::Level::Info));
        assert_eq!(tracker.take_events(), [Event::IsEnabled]);

        // Test: `Handler::log`.
        polog::info!(&mut logger, "Message");
        assert_eq!(
            tracker.take_events(),
            [Event::Log("Message".to_owned(), vec![])]
        );

        // Test: `Handler::log` with arguments.
        polog::info!(&mut logger, "With args"; foo = 1, bar = 2);
        assert_eq!(
            tracker.take_events(),
            [Event::Log(
                "With args".to_owned(),
                vec![("foo", "1".to_owned()), ("bar", "2".to_owned())],
            )]
        );

        // Test: `Handler::enter_span` (logged as regular record).
        let mut span = polog::info_span!(&mut logger, "Span");
        assert_eq!(
            tracker.take_events(),
            [Event::Log("Span".to_owned(), vec![])]
        );

        // Test: `Handler::log` inside a span.
        polog::info!(&mut span, "Inside");
        assert_eq!(
            tracker.take_events(),
            [Event::Log("Inside".to_owned(), vec![])]
        );

        // Test: `DynHandler::exit_span`.
        drop(span);
        assert_eq!(tracker.take_events(), []);

        // Test: `BaseHandler::flush_blocking`.
        logger.flush_blocking();
        assert_eq!(tracker.take_events(), []);

        // Test: `BaseHandler::flush_on_panic`.
        logger.flush_on_panic();
        assert_eq!(tracker.take_events(), []);
    }

    #[cfg(feature = "fanout")]
    fn test_log_static(tracker: &Tracker) {
        use crate::Fanout2;

        let mut logger = Fanout2::new((ViaSlog(tracker.logger()), ViaSlog(tracker.logger())));

        // Test: `Handler::log_static`.
        polog::info!(&mut logger, "Message");
        assert_eq!(
            tracker.take_events(),
            [
                Event::Log("Message".to_owned(), vec![]),
                Event::Log("Message".to_owned(), vec![]),
            ]
        );

        // Test: `BaseHandler::generate_span_id` and
        // `Handler::enter_span_static`.
        let span = polog::info_span!(&mut logger, "Span");
        assert_eq!(
            tracker.take_events(),
            [
                Event::Log("Span".to_owned(), vec![]),
                Event::Log("Span".to_owned(), vec![]),
            ]
        );

        // Test: `DynHandler::exit_span`.
        drop(span);
        assert_eq!(tracker.take_events(), []);
    }

    #[cfg(not(feature = "fanout"))]
    fn test_log_static(_tracker: &Tracker) {}

    #[cfg(feature = "fanout")]
    fn test_log_borrowed(tracker: &Tracker) {
        use crate::BorrowingFanout2;

        let mut logger =
            BorrowingFanout2::new((ViaSlog(tracker.logger()), ViaSlog(tracker.logger())));

        // Test: `Handler::log_borrowed`.
        polog::info!(&mut logger, "Message");
        assert_eq!(
            tracker.take_events(),
            [
                Event::Log("Message".to_owned(), vec![]),
                Event::Log("Message".to_owned(), vec![]),
            ]
        );

        // Test: `BaseHandler::generate_span_id` and
        // `Handler::enter_span_borrowed`.
        let span = polog::info_span!(&mut logger, "Span");
        assert_eq!(
            tracker.take_events(),
            [
                Event::Log("Span".to_owned(), vec![]),
                Event::Log("Span".to_owned(), vec![]),
            ]
        );

        // Test: `DynHandler::exit_span`.
        drop(span);
        assert_eq!(tracker.take_events(), []);
    }

    #[cfg(not(feature = "fanout"))]
    fn test_log_borrowed(_tracker: &Tracker) {}

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    async fn test_log_async(tracker: &Tracker) {
        let mut logger = ViaSlog(tracker.logger());

        let deferred = polog::deferred_info_span!(&mut logger, "Polled");
        let task = tokio::task::spawn(example_async_fn.poll_instrument(deferred));
        task.await.unwrap();

        assert_eq!(
            tracker.take_events(),
            [
                Event::Log("Polled".to_owned(), vec![]),
                Event::Log("Some warn!".to_owned(), vec![("3 * 4", "12".to_owned())],),
                Event::Log("Another warn!".to_owned(), vec![("4 * 5", "20".to_owned())],),
            ]
        );
    }

    async fn example_async_fn<L>(mut logger: L)
    where
        L: Logger,
    {
        polog::info!(&mut logger, "Some warn!"; 3 * 4);
        tokio::task::yield_now().await;
        polog::info!(&mut logger, "Another warn!"; 4 * 5);
    }
}
