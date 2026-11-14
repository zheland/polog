//! Delegators from `polog` to `log` 0.4.x.

use log_0_4 as log;
use polog::{
    BaseHandler, BorrowingHandler, ContextArgs, Descriptor, DisplayableMixedArgs, DynHandler,
    Handler, LogArgs, SharedHandler, SpanId, SpanIdGenerator, SpanKind, StaticLogArgs,
};

use crate::conv::log_0_4::level_into_foreign_approx;

/// Fallback ID generator used by `ViaLog`.
///
/// `log` has no notion of span IDs, so the IDs generated here are only used
/// to satisfy the [`BaseHandler::generate_span_id`] contract.
/// They never reach he `log` backend.
static SPAN_ID_GENERATOR: SpanIdGenerator = SpanIdGenerator::new();

/// A `polog` [`Handler`] adapter over a [`log::Log`] backend.
///
/// Forwards `polog` events to [`log::Log::log`] by building a [`log::Record`]
/// from the static [`Descriptor`] and the argument tuple.
///
/// # Span handling
///
/// `log` has no span lifecycle.
/// Span entry ([`Handler::enter_span`]) is logged as an ordinary record so
/// that the span's descriptor and arguments are at least visible.
/// Span exit and all poll events are silently dropped.
///
/// # Argument rendering
///
/// Arguments reach the backend as `log::kv::Value`s that wrap the original
/// `Display` implementations, so a backend that supports structured
/// key‑value pairs sees the same keys and values that a same‑thread `polog`
/// backend would see.
/// If`log`'s `kv` visitor then renders each value through `Display`, matching
/// how `polog` treats arguments elsewhere.
///
/// # Filtering
///
/// `try_enable` calls [`log::Log::enabled`] with a [`log::Metadata`] built
/// from the descriptor, so a `log` backend that returns `false` from
/// `enabled` short‑circuits before any arguments are formatted.
///
/// [`Handler::enter_span`]: polog::Handler::enter_span
/// [`log::Log::enabled`]: log_0_4::Log::enabled
/// [`log::Log::log`]: log_0_4::Log::log
/// [`log::Metadata`]: log_0_4::Metadata
/// [`log::Record`]: log_0_4::Record
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct ViaLog<L>(
    /// The `log` backend to forward to.
    pub L,
);

impl<L> BaseHandler for ViaLog<L>
where
    L: log::Log,
{
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        SPAN_ID_GENERATOR.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {
        self.0.flush();
    }

    #[inline]
    fn flush_on_panic(&mut self) {
        self.0.flush();
    }
}

impl<L, Context> DynHandler<Context> for ViaLog<L>
where
    L: log::Log,
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

impl<L, Context> Handler<Context> for ViaLog<L>
where
    L: log::Log,
    Context: ContextArgs,
{
    type EnrichedContext = Context;

    #[inline]
    fn try_enable(
        &self,
        descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        is_enabled(&self.0, descriptor).then_some(context)
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
        // `log` has no span lifecycle.
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
        // `log` has no span lifecycle.
        // Log the span entry as a regular record so the span's message and
        // args are at least visible.
        log(&self.0, descriptor, &args);
    }
}

impl<L, Context> BorrowingHandler<Context> for ViaLog<L>
where
    L: log::Log,
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
        // `log` has no span lifecycle.
        // Log the span entry as a regular record so the span's message and
        // args are at least visible.
        log(&self.0, descriptor, args);
    }
}

impl<L, Context> SharedHandler<Context> for ViaLog<L>
where
    L: 'static + Send + Sync + Clone + log::Log,
    Context: ContextArgs,
{
}

#[inline]
fn build_metadata(descriptor: &Descriptor) -> log::Metadata<'_> {
    let level = level_into_foreign_approx(descriptor.level);
    let target = descriptor.target.unwrap_or(descriptor.module_path);
    log::MetadataBuilder::new()
        .level(level)
        .target(target)
        .build()
}

#[inline]
fn is_enabled<L>(log: &L, descriptor: &Descriptor) -> bool
where
    L: log::Log,
{
    let metadata = build_metadata(descriptor);
    log.enabled(&metadata)
}

#[inline]
fn log<'a, L, T>(log: &L, descriptor: &'static Descriptor, args: &T)
where
    L: log::Log,
    T: DisplayableMixedArgs<'a>,
{
    let metadata = build_metadata(descriptor);

    let kv = Kv::new(descriptor.param_names, args);
    let message = format_args!("{}", descriptor.message);
    let mut builder = log::RecordBuilder::new();

    let _: &mut _ = builder
        .metadata(metadata)
        .line(Some(descriptor.location.line()))
        .file(Some(descriptor.location.file()))
        .module_path_static(Some(descriptor.module_path))
        .args(message);

    let _: &mut _ = builder.key_values(&kv);

    let record = builder.build();
    log.log(&record);
}

#[derive(Copy, Clone, Debug)]
struct Kv<'a, T> {
    keys: &'static [&'static str],
    values: &'a T,
}

impl<'a, T> Kv<'a, T> {
    #[inline]
    #[must_use]
    const fn new(keys: &'static [&'static str], values: &'a T) -> Self {
        Self { keys, values }
    }
}

impl<'a, 'b, T> log::kv::Source for Kv<'a, T>
where
    'b: 'a,
    T: DisplayableMixedArgs<'b>,
{
    fn visit<'kvs>(
        &'kvs self,
        visitor: &mut dyn log::kv::VisitSource<'kvs>,
    ) -> Result<(), log::kv::Error> {
        for (i, key) in self.keys.iter().take(self.values.num_args()).enumerate() {
            let key = log::kv::Key::from_str_static(key);
            let Some(value) = self.values.nth_dyn(i) else {
                continue; // Ignore invalid `DisplayableMixedArgs` impl.
            };
            let value = log::kv::Value::from_dyn_display(value);
            visitor.visit_pair(key, value)?;
        }
        Ok(())
    }

    fn get(&self, key: log::kv::Key<'_>) -> Option<log::kv::Value<'_>> {
        let i = self.keys.iter().position(|other| *other == key.as_str())?;
        let Some(value) = self.values.nth_dyn(i) else {
            return None; // Ignore invalid `DisplayableMixedArgs` impl.
        };
        let value = log::kv::Value::from_dyn_display(value);
        Some(value)
    }

    fn count(&self) -> usize {
        self.keys.len().min(self.values.num_args())
    }
}

#[cfg(feature = "alloc")]
#[cfg(test)]
mod tests {
    use alloc::borrow::ToOwned;
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec::Vec;
    use alloc::{format, vec};
    use core::mem;

    use parking_lot::Mutex;
    use polog::{Logger, LoggerExt, PollInstrument};

    use crate::delegators::log_0_4::ViaLog;

    #[derive(Clone, Default, Debug)]
    pub struct Tracker {
        events: Arc<Mutex<Vec<Event>>>,
    }

    #[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
    pub struct FieldVisitor(Vec<(String, String)>);

    #[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
    pub enum Event {
        Enabled,
        Log(String, Vec<(String, String)>),
        Flush,
    }

    impl Tracker {
        #[inline]
        #[must_use]
        pub fn take_events(&self) -> Vec<Event> {
            mem::take(&mut *self.events.lock())
        }
    }

    impl log_0_4::Log for Tracker {
        fn enabled(&self, _metadata: &log_0_4::Metadata<'_>) -> bool {
            self.events.lock().push(Event::Enabled);
            true
        }

        #[expect(clippy::unwrap_used, reason = "okay in tests")]
        fn log(&self, record: &log_0_4::Record<'_>) {
            let mut visitor = FieldVisitor::default();
            record.key_values().visit(&mut visitor).unwrap();

            assert_eq!(record.key_values().count(), visitor.0.len());
            for (key, expected) in &visitor.0 {
                let key = log_0_4::kv::Key::from_str(key.as_str());
                let value = record.key_values().get(key).unwrap();
                assert_eq!(format!("{value}"), *expected);
            }

            self.events
                .lock()
                .push(Event::Log(format!("{}", record.args()), visitor.0));
        }

        fn flush(&self) {
            self.events.lock().push(Event::Flush);
        }
    }

    impl<'a> log_0_4::kv::VisitSource<'a> for FieldVisitor {
        fn visit_pair(
            &mut self,
            key: log_0_4::kv::Key<'a>,
            value: log_0_4::kv::Value<'a>,
        ) -> Result<(), log_0_4::kv::Error> {
            self.0.push((key.as_str().to_owned(), format!("{value}")));
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_delegator_log() {
        let tracker = Tracker::default();
        log_0_4::set_logger(Box::leak(Box::new(tracker.clone())))
            .expect("log library supports only one logger setup");
        log_0_4::set_max_level(log_0_4::LevelFilter::Trace);

        test_log_basic(&tracker);
        test_log_static(&tracker);
        test_log_borrowed(&tracker);
        test_log_async(&tracker).await;
    }

    fn test_log_basic(tracker: &Tracker) {
        let mut logger = ViaLog(log_0_4::logger());

        // Test: `Handler::log`.
        polog::info!(&mut logger, "Message");
        assert_eq!(
            tracker.take_events(),
            [Event::Enabled, Event::Log("Message".to_owned(), vec![])]
        );

        // Test: `Handler::log` with arguments.
        polog::info!(&mut logger, "With args"; foo = 1, bar = 2);
        assert_eq!(
            tracker.take_events(),
            [
                Event::Enabled,
                Event::Log(
                    "With args".to_owned(),
                    vec![
                        ("foo".to_owned(), "1".to_owned()),
                        ("bar".to_owned(), "2".to_owned())
                    ],
                )
            ]
        );

        // Test: `BaseHandler::generate_span_id` and `Handler::enter_span`.
        let mut span = polog::info_span!(&mut logger, "Span");
        assert_eq!(
            tracker.take_events(),
            [Event::Enabled, Event::Log("Span".to_owned(), vec![])]
        );

        // Test: `Handler::log` inside a span.
        polog::info!(&mut span, "Inside");
        assert_eq!(
            tracker.take_events(),
            [Event::Enabled, Event::Log("Inside".to_owned(), vec![])]
        );

        // Test: `DynHandler::exit_span`.
        drop(span);
        assert_eq!(tracker.take_events(), []);

        // Test `BaseHandler::flush_blocking`.
        logger.flush_blocking();
        assert_eq!(tracker.take_events(), [Event::Flush]);

        // Test `BaseHandler::flush_on_panic`.
        logger.flush_on_panic();
        assert_eq!(tracker.take_events(), [Event::Flush]);
    }

    #[cfg(feature = "fanout")]
    fn test_log_static(tracker: &Tracker) {
        use crate::Fanout2;

        let mut logger = Fanout2::new((ViaLog(log_0_4::logger()), ViaLog(log_0_4::logger())));

        // Test: `Handler::log_static`.
        polog::info!(&mut logger, "Message");
        assert_eq!(
            tracker.take_events(),
            [
                Event::Enabled,
                Event::Enabled,
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
                Event::Enabled,
                Event::Enabled,
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
            BorrowingFanout2::new((ViaLog(log_0_4::logger()), ViaLog(log_0_4::logger())));

        // Test: `Handler::log_borrowed`.
        polog::info!(&mut logger, "Message");
        assert_eq!(
            tracker.take_events(),
            [
                Event::Enabled,
                Event::Enabled,
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
                Event::Enabled,
                Event::Enabled,
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
        let mut logger = ViaLog(log_0_4::logger());

        let deferred = polog::deferred_info_span!(&mut logger, "Polled");
        let task = tokio::task::spawn(example_async_fn.poll_instrument(deferred));
        task.await.unwrap();

        assert_eq!(
            tracker.take_events(),
            [
                Event::Enabled,
                Event::Log("Polled".to_owned(), vec![]),
                Event::Enabled,
                Event::Log(
                    "Some warn!".to_owned(),
                    vec![("3 * 4".to_owned(), "12".to_owned())]
                ),
                Event::Enabled,
                Event::Log(
                    "Another warn!".to_owned(),
                    vec![("4 * 5".to_owned(), "20".to_owned())]
                )
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
