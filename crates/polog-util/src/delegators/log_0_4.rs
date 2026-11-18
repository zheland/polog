//! Delegators from `polog` to `log` 0.4.x.

use log_0_4 as log;
use polog::{
    BaseHandler, BorrowingHandler, ContextArgs, Descriptor, DisplayableMixedArgs, DynHandler,
    Handler, LevelFilter, LogArgs, SharedHandler, SpanId, SpanIdGenerator, SpanKind, StaticLogArgs,
};

use crate::conv::log_0_4::level_into_foreign_approx;
use crate::util::MaybeStaticStr;
use crate::{
    BaseExternalHandler, ExternalArgValue, ExternalArgs, ExternalArgsVisitor, ExternalDescriptor,
    ExternalHandler, ExternalMessage, conv,
};

/// Fallback ID generator used by `ViaLog`.
///
/// `log` has no notion of span IDs, so the IDs generated here are only used
/// to satisfy the [`BaseHandler::generate_span_id`] contract.
/// They never reach he `log` backend.
static SPAN_ID_GENERATOR: SpanIdGenerator = SpanIdGenerator::new();

/// A `polog` [`Handler`] and [`ExternalHandler`] adapter over a
/// [`log::Log`] backend.
///
/// Forwards `polog` events to [`log::Log::log`] by building a
/// [`log::Record`] from the static [`Descriptor`] and the argument tuple.
///
/// `ViaLog` serves both directions of the `log ↔ polog` bridge:
///
/// - As a native `polog` handler, `ViaLog` accepts events produced by `polog`
///   call sites (through [`Handler::log`], [`Handler::log_static`], and
///   [`BorrowingHandler::log_borrowed`]) and renders them into a `log::Record`.
/// - As an [`ExternalHandler`], `ViaLog` accepts events that originated in
///   `tracing`, `slog`, or a reverse-bridged `log` source and forwards them to
///   the same `log` backend. See [Argument rendering] below for the difference
///   in behavior between the two paths.
///
/// # Argument rendering
///
/// **Native `polog` events** ([`Handler::log`], [`Handler::log_static`],
/// [`BorrowingHandler::log_borrowed`]) forward structured key-value
/// arguments as `log::kv::Value`s that wrap the original `Display`
/// implementations.
/// A `log` backend that walks `record.key_values()` sees the same keys
/// and values a same-thread `polog` backend would see.
///
/// **External events** ([`ExternalHandler::log_external`]) originate in
/// `tracing`, `slog`, or `log` and arrive here through a `Bridge*`
/// adapter (`LogBridge`, `SlogBridge`, `TracingBridge`).
/// `log`'s kv protocol requires `Value<'kvs>` where `'kvs` is the borrow
/// of the record source, and `ExternalArgs` exposes each field with
/// lifetime `'src` bounded by the foreign facade's borrow.
/// `'src: 'kvs` is not a property `ExternalArgs` guarantees.
/// For `Copy`-shaped `ExternalArg` variants there is no `&'src` at all,
/// so `Value<'kvs>` would have to be constructed from `'kvs`-lived
/// storage.
/// Forwarding external arguments as `key_values()` would therefore
/// require a per-event arena of `String`s on the producing thread — a
/// cost that contradicts `polog`'s design goal of zero per-argument work
/// on the producing thread for `Copy + Send + 'static` values.
///
/// This delegator therefore renders the external descriptor's message and
/// every external argument as `key = value` text into `record.args()`,
/// so `log` backends that display `record.args()` still see the full
/// event.
/// `record.key_values()` is left empty for external events.
///
/// The asymmetry is deliberate: it preserves the "no work on the
/// producing thread" property of the native `polog` fast path and
/// accepts the cost of rendering the external event in the bridge
/// itself.
///
/// # Filtering
///
/// [`Handler::try_enable`] calls [`log::Log::enabled`] with a
/// [`log::Metadata`] built from the descriptor;
/// [`ExternalHandler::try_enable_external`] does the same for external
/// events.
/// A `log` backend that returns `false` from `enabled` short-circuits
/// before any arguments are formatted.
///
/// [`Handler::enter_span`]: polog::Handler::enter_span
/// [`Handler::log`]: polog::Handler::log
/// [`Handler::log_static`]: polog::Handler::log_static
/// [`BorrowingHandler::log_borrowed`]: polog::BorrowingHandler::log_borrowed
/// [`ExternalHandler`]: crate::ExternalHandler
/// [`ExternalHandler::log_external`]: crate::ExternalHandler::log_external
/// [`ExternalHandler::enter_span_external`]: crate::ExternalHandler::enter_span_external
/// [`ExternalHandler::try_enable_external`]: crate::ExternalHandler::try_enable_external
/// [`log::Log::enabled`]: log_0_4::Log::enabled
/// [`log::Log::log`]: log_0_4::Log::log
/// [`log::Metadata`]: log_0_4::Metadata
/// [`log::Record`]: log_0_4::Record
/// [Argument rendering]: #argument-rendering
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ViaLog<L> {
    /// The `log` backend to forward to.
    handler: L,
    /// The flags customizing the behavior of the delegator.
    flags: ViaLogFlags,
}

bitflags::bitflags! {
    /// Rendering options for the [`ViaLog`] delegator.
    ///
    /// `log` has no span lifecycle.
    /// A `polog` span, and every operation on it (entry, exit, dynamic-field
    /// `record`, `follows_from` link, cl therefore be rendered as an ordinary
    /// record to be visible at all.
    ///
    /// [`ViaLog`]: crate::ViaLog
    #[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
    pub struct ViaLogFlags: u32 {
        /// Emit span entry as a `log::Record`.
        ///
        /// When set, [`Handler::enter_span`] and
        /// [`ExternalHandler::enter_span_external`] render a record
        /// carrying the span's descriptor and arguments.
        ///
        /// [`Handler::enter_span`]: polog::Handler::enter_span
        /// [`ExternalHandler::enter_span_external`]: crate::ExternalHandler::enter_span_external
        const LOG_ENTER_SPAN = 1 << 0;

        /// Emit dynamic-field `record` and `follows_from` operations on
        /// a span as `log::Record`s.
        ///
        /// When set, [`ExternalHandler::record_span_external`] renders
        /// a record whose message is derived from the recorded fields.
        ///
        /// [`ExternalHandler::record_span_external`]: crate::ExternalHandler::record_span_external
        const LOG_RECORD_SPAN = 1 << 1;
    }
}

impl<L> ViaLog<L> {
    /// Creates a new bridge from `polog` to `log` 0.4.x.
    #[inline]
    #[must_use]
    pub const fn new(handler: L, flags: ViaLogFlags) -> Self {
        Self { handler, flags }
    }

    /// Returns a reference to the wrapped handler.
    #[inline]
    #[must_use]
    pub const fn handler(&self) -> &L {
        &self.handler
    }

    /// Returns a mutable reference to the wrapped handler.
    #[inline]
    #[must_use]
    pub const fn handler_mut(&mut self) -> &mut L {
        &mut self.handler
    }

    /// Consumes the middleware, returning the wrapped handler.
    #[inline]
    #[must_use]
    pub fn into_handler(self) -> L {
        self.handler
    }
}

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
        self.handler.flush();
    }

    #[inline]
    fn flush_on_panic(&mut self) {
        self.handler.flush();
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
        is_enabled(&self.handler, descriptor).then_some(context)
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
        log(&self.handler, descriptor, &args);
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
        if self.flags.contains(ViaLogFlags::LOG_ENTER_SPAN) {
            // `log` has no span lifecycle.
            // Log the span entry as a regular record so the span's message and
            // args are at least visible.
            log(&self.handler, descriptor, &args);
        }
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
        log(&self.handler, descriptor, &args);
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
        log(&self.handler, descriptor, &args);
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
        log(&self.handler, descriptor, args);
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
        log(&self.handler, descriptor, args);
    }
}

impl<L, Context> SharedHandler<Context> for ViaLog<L>
where
    L: 'static + Send + Sync + Clone + log::Log,
    Context: ContextArgs,
{
}

impl<L> BaseExternalHandler for ViaLog<L>
where
    L: 'static + Send + Sync + Clone + log::Log,
{
    #[inline]
    fn max_level_external(&self) -> LevelFilter {
        let level = log::max_level();
        conv::log_0_4::level_filter_from_foreign(level)
    }
}

impl<L, Context> ExternalHandler<Context> for ViaLog<L>
where
    L: 'static + Send + Sync + Clone + log::Log,
    Context: ContextArgs,
{
    #[inline]
    fn try_enable_external<Desc>(
        &self,
        descriptor: &Desc,
        context: Context,
    ) -> Option<Self::EnrichedContext>
    where
        Desc: ExternalDescriptor,
    {
        is_enabled_external(&self.handler, descriptor).then_some(context)
    }

    #[inline]
    fn log_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        _span_id: Option<SpanId>,
        _context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        log_external(&self.handler, &descriptor, &args);
    }

    #[inline]
    fn new_span_external<Desc, Args>(
        &mut self,
        _descriptor: &Desc,
        _args: &Args,
        _parent_span_id: Option<SpanId>,
        _span_id: SpanId,
        _context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
    }

    #[inline]
    fn enter_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        _parent_span_id: Option<SpanId>,
        _span_id: SpanId,
        _context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        if self.flags.contains(ViaLogFlags::LOG_ENTER_SPAN) {
            // `log` has no span lifecycle.
            // Log the span entry as a regular record so the span's message and
            // args are at least visible.
            log_external(&self.handler, &descriptor, &args);
        }
    }

    #[inline]
    fn exit_span_external<Desc>(&mut self, _descriptor: &Desc, _span_id: SpanId, _context: Context)
    where
        Desc: ExternalDescriptor,
    {
    }

    #[inline]
    fn close_span_external<Desc>(&mut self, _descriptor: &Desc, _span_id: SpanId, _context: Context)
    where
        Desc: ExternalDescriptor,
    {
    }

    #[inline]
    fn record_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        _span_id: SpanId,
        _context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        if self.flags.contains(ViaLogFlags::LOG_RECORD_SPAN) {
            // `log` has no span lifecycle.
            // Log the span entry as a regular record so the span's message and
            // args are at least visible.
            log_external(&self.handler, &descriptor, &args);
        }
    }

    #[inline]
    fn link_span_external<Desc>(
        &mut self,
        _descriptor: &Desc,
        _span_id: SpanId,
        _follows_from_span_id: SpanId,
        _context: Context,
    ) where
        Desc: ExternalDescriptor,
    {
    }
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
fn is_enabled_external<L, Desc>(log: &L, descriptor: &Desc) -> bool
where
    L: log::Log,
    Desc: ExternalDescriptor,
{
    let metadata = build_metadata_external(descriptor);
    log.enabled(&metadata)
}

#[inline]
fn log<'a, L, Args>(log: &L, descriptor: &'static Descriptor, args: &Args)
where
    L: log::Log,
    Args: DisplayableMixedArgs<'a>,
{
    let metadata = build_metadata(descriptor);

    let kv = ArgsVisitor {
        keys: descriptor.param_names,
        values: args,
    };
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

#[inline]
fn log_external<L, Desc, Args>(log: &L, descriptor: &Desc, args: &Args)
where
    L: log::Log,
    Desc: ExternalDescriptor,
    Args: ExternalArgs,
{
    let metadata = build_metadata_external(descriptor);

    let mut builder = log::RecordBuilder::new();
    let &mut _ = builder.metadata(metadata);

    if let Some(file) = descriptor.file() {
        let &mut _ = match file {
            MaybeStaticStr::Static(file) => builder.file_static(Some(file)),
            MaybeStaticStr::Borrowed(file) => builder.file(Some(file)),
        };
    }
    if let Some(line) = descriptor.line() {
        let &mut _ = builder.line(Some(line));
    }
    if let Some(module_path) = descriptor.module_path() {
        let &mut _ = match module_path {
            MaybeStaticStr::Static(module_path) => builder.module_path_static(Some(module_path)),
            MaybeStaticStr::Borrowed(module_path) => builder.module_path(Some(module_path)),
        };
    }

    // Structured key-value arguments from an `ExternalArgs` source cannot
    // be forwarded to `log`'s `kv` protocol without a per-event arena.
    //
    // `log::kv::VisitSource<'kvs>::visit_pair` requires `Value<'kvs>`.
    // Every `Value<'kvs>` constructor takes a `&'kvs` reference — a
    // reference whose lifetime is `'kvs`, the borrow of the `Source` for
    // one record rendering.
    // `ExternalArgs` exposes each field with lifetime `'src` (the source's
    // own borrow).
    // `'src: 'kvs` is not a property `ExternalArgs` guarantees.
    //
    // For `Copy` variants (`Bool`, `U8..U128`, `I8..I128`, `F32`, `F64`,
    // `Unit`, `None`) there is no `&'src` at all, so a `&'kvs` must be
    // obtained from storage that lives for `'kvs`. The match arm that
    // produces the value is scoped to one statement, and `Value<'v>` is
    // invariant in `'v`, so no coercion helps.
    // `Stringified*` variants carry an owned `String` but still need a
    // `'kvs`-long borrow of its contents.
    //
    // Fixing this requires a `Vec<Option<String>>` arena in this frame,
    // populated in a first `ExternalArgs::visit` pass and borrowed in a
    // second — one extra visit traversal and one `String` allocation per
    // primitive argument, per event, per bridge hop.
    //
    // `log`'s kv protocol is designed for eager, same-thread rendering:
    // in a `log!` call the arguments are alive in the macro expansion, so
    // `Value<'kvs>` borrows them directly.
    // A `polog` facade whose primary win is "no work on the producing thread"
    // cannot afford a per-event `String` arena for a bridge.
    // This bridge therefore does not set `key_values`.
    // Instead it renders the descriptor message and every argument as `key =
    // value` text into `args` below, so `log` backends that display
    // `record.args()` still see the full event.

    let message = ExternalLogMessage { descriptor, args };
    let message_args = format_args!("{message}");
    let &mut _ = builder.args(message_args);

    log.log(&builder.build());
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
fn build_metadata_external<Desc>(descriptor: &Desc) -> log::Metadata<'_>
where
    Desc: ExternalDescriptor,
{
    let level = level_into_foreign_approx(descriptor.level());
    let mut builder = log::MetadataBuilder::new();
    let &mut _ = builder.level(level);

    if let Some(target) = descriptor.target().or_else(|| descriptor.module_path()) {
        let &mut _ = builder.target(target.as_str());
    }
    builder.build()
}

#[derive(Copy, Clone, Debug)]
struct ArgsVisitor<'a, T> {
    keys: &'static [&'static str],
    values: &'a T,
}

impl<'a, 'b, T> log::kv::Source for ArgsVisitor<'a, T>
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

/// Renders an external event's descriptor message and every argument as
/// a single `Display` value.
///
/// Used by [`log_external`] to feed the combined text into
/// [`log::RecordBuilder::args`]; see the comment there for why external
/// key-value pairs are rendered into the message rather than passed
/// through `key_values()`.
///
/// The rendered form is:
///
/// - descriptor message (if any), followed by
/// - the visitor's message (if any), followed by
/// - `key = value` for each argument,
///
/// separated by single spaces (message parts) and `", "` (argument
/// parts). Errors from the underlying [`core::fmt::Write`] are swallowed
/// because `log` has no fallible rendering path to report them through.
struct ExternalLogMessage<'a, Desc, Args> {
    descriptor: &'a Desc,
    args: &'a Args,
}

impl<Desc, Args> core::fmt::Display for ExternalLogMessage<'_, Desc, Args>
where
    Desc: ExternalDescriptor,
    Args: ExternalArgs,
{
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut visitor = ExternalLogMessageArgsVisitor { f, has_any: false };

        if let Some(msg) = self.descriptor.message() {
            match msg {
                ExternalMessage::Str(value) => {
                    core::fmt::Write::write_str(visitor.f, value.as_str())?;
                    visitor.has_any = true;
                }
                ExternalMessage::Fmt(fmt) => {
                    core::fmt::Display::fmt(fmt, visitor.f)?;
                    visitor.has_any = true;
                }
            }
        }

        self.args.visit(&mut visitor);
        Ok(())
    }
}

struct ExternalLogMessageArgsVisitor<'a, 'b> {
    f: &'a mut core::fmt::Formatter<'b>,
    has_any: bool,
}

impl ExternalArgsVisitor for ExternalLogMessageArgsVisitor<'_, '_> {
    #[inline]
    fn visit_message(&mut self, value: ExternalArgValue<'_>) {
        // `visit_message` is called only when the source distinguishes a
        // message field from the argument list (`tracing` events with a
        // `message = ...` field).
        // When the descriptor already provided a message, the visitor's message
        // follows it with a space.
        let _ = self.try_write_message(&value);
    }

    #[inline]
    fn visit_arg(&mut self, key: MaybeStaticStr<'_>, value: ExternalArgValue<'_>) {
        let _ = self.try_write_arg_value(key, &value);
    }
}

impl ExternalLogMessageArgsVisitor<'_, '_> {
    #[inline]
    fn try_write_message(&mut self, value: &ExternalArgValue<'_>) -> core::fmt::Result {
        if self.has_any {
            core::fmt::Write::write_str(self.f, " ")?;
        }
        self.has_any = true;
        write!(self.f, "{value}")?;
        Ok(())
    }

    #[inline]
    fn try_write_arg_value(
        &mut self,
        key: MaybeStaticStr<'_>,
        value: &ExternalArgValue<'_>,
    ) -> core::fmt::Result {
        if self.has_any {
            core::fmt::Write::write_str(self.f, ", ")?;
        }
        self.has_any = true;
        core::fmt::Write::write_str(self.f, key.as_str())?;
        core::fmt::Write::write_str(self.f, " = ")?;
        write!(self.f, "{value}")?;
        Ok(())
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

    use crate::delegators::log_0_4::{ViaLog, ViaLogFlags};

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

    #[test]
    fn test_delegator_log_accessors() {
        let mut enricher = ViaLog::new(123, ViaLogFlags::all());
        assert_eq!(enricher.handler(), &123);
        assert_eq!(enricher.handler_mut(), &123);
        assert_eq!(enricher.into_handler(), 123);
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
        let mut logger = ViaLog {
            handler: log_0_4::logger(),
            flags: ViaLogFlags::all(),
        };

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

        let mut logger = Fanout2::new((
            ViaLog {
                handler: log_0_4::logger(),
                flags: ViaLogFlags::all(),
            },
            ViaLog {
                handler: log_0_4::logger(),
                flags: ViaLogFlags::all(),
            },
        ));

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

        let mut logger = BorrowingFanout2::new((
            ViaLog {
                handler: log_0_4::logger(),
                flags: ViaLogFlags::all(),
            },
            ViaLog {
                handler: log_0_4::logger(),
                flags: ViaLogFlags::all(),
            },
        ));

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
        let mut logger = ViaLog {
            handler: log_0_4::logger(),
            flags: ViaLogFlags::all(),
        };

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
