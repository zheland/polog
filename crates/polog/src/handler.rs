use core::fmt::Debug;

use crate::{
    DebuggableMixedArgsBuilder, Descriptor, DisplayableMixedArgs, IntoSendable, SpanId, SpanKind,
    default_log_related_panic,
};

/// A heterogeneous tuple of log and span arguments that supports inspecting,
/// synchronous formatting and conversion to an owned, sendable form.
///
/// This trait is automatically implemented for any type that is:
/// - Statically displayable via [`DisplayableMixedArgs<'a>`] where each
///   [`Arg<T>`] leaf of `MixedArgs` implements `Display`.
/// - Convertible to a sendable representation via [`IntoSendable`] where the
///   resulting type is itself [`DisplayableMixedArgs<'static>`].
///
/// The `'a` lifetime is the lifetime of any values borrowed by the tuple.
/// It bounds `Self` (`Self: 'a`) and determines the lifetime of the type-erased
/// `&dyn Display` references produced by [`DynMixedArgs::nth_dyn`].
///
/// [`Arg<T>`]: crate::Arg
/// [`DynMixedArgs::nth_dyn`]: crate::DynMixedArgs::nth_dyn
pub trait LogArgs<'a>:
    'a + DisplayableMixedArgs<'a> + IntoSendable<Sendable: DisplayableMixedArgs<'static>>
{
}
impl<'a, T> LogArgs<'a> for T where
    T: 'a
        + ?Sized
        + DisplayableMixedArgs<'a>
        + IntoSendable<Sendable: DisplayableMixedArgs<'static>>
{
}

/// A heterogeneous tuple of log and span arguments that is already in
/// sendable form.
///
/// Satisfied by every type that is `'static + Send + Sync` and implements
/// [`DisplayableMixedArgs<'static>`]. The typical source is
/// [`IntoSendable::into_sendable`]: converting a [`LogArgs<'a>`] yields a
/// `Sendable` value that satisfies `StaticLogArgs`.
///
/// Unlike [`LogArgs`], `StaticLogArgs` does not require [`IntoSendable`].
/// This breaks a trait-resolution cycle for methods that take already-converted
/// arguments: requiring `IntoSendable` there would force every path back to
/// those methods to re-derive the same bound. It also lets middleware share
/// `&Args` across threads without a conversion step.
pub trait StaticLogArgs: 'static + Send + Sync + DisplayableMixedArgs<'static> {}
impl<T> StaticLogArgs for T where T: 'static + Send + Sync + DisplayableMixedArgs<'static> {}

/// A heterogeneous tuple of log and span context arguments.
///
/// Context arguments are supplied by middleware [`Handler`] implementations
/// and flow through the handler chain.
/// They must be `'static + Send`, so they can be shared across threads.
///
/// This trait is automatically implemented for any type that is
/// `'static + Send + DebuggableMixedArgsBuilder<'static>`.
///
/// `DebuggableMixedArgsBuilder` is required so that middleware and backends can
/// format the full context when diagnosing enrichment, at the cost of requiring
/// every context value to implement `Debug`.
/// This is a deliberate trade: context values that cannot be formatted are
/// context values that are hard to diagnose when the middleware chain
/// misbehaves.
pub trait ContextArgs: 'static + Send + Sync + DebuggableMixedArgsBuilder<'static> {}
impl<T> ContextArgs for T where
    T: ?Sized + 'static + Send + Sync + DebuggableMixedArgsBuilder<'static>
{
}

/// Functionality common to all [`Handler`] implementations.
pub trait BaseHandler {
    /// Generates a [`SpanId`].
    ///
    /// Implementations should not reuse an ID for this handler until its span
    /// ID counter wraps.
    /// After wrap-around, previously issued IDs may be reused.
    fn generate_span_id(&self) -> SpanId;

    /// Flushes any buffered records.
    ///
    /// The method may block and is intended for explicit, synchronous flushing.
    ///
    /// # For implementors
    ///
    /// This method is not called automatically by any default implementations
    /// or macros.
    /// It can be called manually on shut-down to ensure any in-flight records
    /// are flushed.
    fn flush_blocking(&mut self);

    /// Flushes any buffered records as part of a failure path.
    ///
    /// Called by [`LoggerExt::flush_and_panic`] and
    /// [`LoggerExt::panic_if_critical`] immediately before
    /// [`BaseHandler::panic`], and by user code from any cooperative failure
    /// context: a panic hook, an `atexit` callback, or a dedicated thread to
    /// which a signal has been routed for handling outside the signal handler.
    ///
    /// # For implementors
    ///
    /// The method runs in normal (non-signal) context, while no other thread
    /// holds a mutable borrow of this handler.
    /// It may allocate, take locks, format, and call `io::Write`.
    ///
    /// **It must not panic.** A panic inside `flush_on_panic` while another
    /// failure is in progress is a double fault, which aborts the process and
    /// loses every buffered record the method was trying to write.
    /// Any fallible operation — lock acquisition, allocation, `write` — must
    /// be handled with `let _ = …;`, `.ok()`, or an equivalent that does not
    /// propagate the failure as a panic.
    ///
    /// **It should be bounded.**
    /// Failure to flush is acceptable.
    /// Blocking indefinitely is not, because the caller is on a deadline.
    ///
    /// # Signal handlers
    ///
    /// `flush_on_panic` **cannot** be called from a real signal handler
    /// (SIGSEGV, SIGBUS, SIGABRT, etc.).
    /// Implementations are not required to use only `async-signal-safe`
    /// operations.
    /// A backend that supports signal-time flushing must expose a different
    /// interface for that purpose.
    ///
    /// [`LoggerExt::flush_and_panic`]: crate::LoggerExt::flush_and_panic
    /// [`LoggerExt::panic_if_critical`]: crate::LoggerExt::panic_if_critical
    fn flush_on_panic(&mut self);

    /// Returns whether a [`Level::Bug`] event should panic after being logged.
    ///
    /// [`LoggerExt::panic_if_critical`] and the `bug!` macro call this once per
    /// event at the [`Level::Bug`] level.
    /// Returning `true` causes the call to invoke [`BaseHandler::panic`] after
    /// the event has been emitted.
    ///
    /// The default implementation returns `cfg!(debug_assertions)`, so `bug!`
    /// panics in dev builds and logs without panicking in release builds when
    /// the default profile is used.
    ///
    /// [`Level::Bug`]: crate::Level::Bug
    /// [`LoggerExt::panic_if_critical`]: crate::LoggerExt::panic_if_critical
    #[inline]
    fn should_panic_on_bug(
        &self,
        _descriptor: &'static Descriptor,
        _span_id: Option<SpanId>,
    ) -> bool {
        cfg!(debug_assertions)
    }

    /// Panics after a critical log event.
    ///
    /// Called by [`LoggerExt::flush_and_panic`] for [`Level::Fatal`] events and
    /// by [`LoggerExt::panic_if_critical`] when the event is at
    /// [`Level::Fatal`] or at [`Level::Bug`] with
    /// [`should_panic_on_bug`] returning `true`.
    ///
    /// [`flush_on_panic`] runs before this method is called, so any buffered
    /// events — including the one that triggered the panic — have had a
    /// best-effort chance to reach the sink.
    ///
    /// The default implementation produces the panic message from `descriptor`
    /// and `span_id` alone.
    /// Backends may override this to customize panic messages or cleanup.
    ///
    /// # Panics
    ///
    /// Always panics.
    ///
    /// [`flush_on_panic`]: Self::flush_on_panic
    /// [`should_panic_on_bug`]: Self::should_panic_on_bug
    /// [`Level::Bug`]: crate::Level::Bug
    /// [`Level::Fatal`]: crate::Level::Fatal
    /// [`LoggerExt::flush_and_panic`]: crate::LoggerExt::flush_and_panic
    /// [`LoggerExt::panic_if_critical`]: crate::LoggerExt::panic_if_critical
    #[inline]
    #[track_caller]
    fn panic(&mut self, descriptor: &'static Descriptor, span_id: Option<SpanId>) -> ! {
        default_log_related_panic(descriptor, span_id);
    }
}

/// An object-safe subset of [`Handler`] covering span lifecycle events.
///
/// `DynHandler<Context>` contains the methods of [`Handler`] that can be
/// dispatched through a trait object: [`DynHandler::exit_span`] and the
/// poll-span callbacks ([`enter_poll_span`], [`pending_poll_span`],
/// [`ready_poll_span`]).
///
/// [`Handler`] extends this trait with the methods that are inherently
/// non-object-safe: [`try_enable`] returns an associated type, and [`log`] and
/// [`enter_span`] are generic over the argument tuple.
/// Those methods require the concrete handler type at the call site and cannot
/// be invoked through a trait object.
///
/// Use `DynHandler` when a handler must be type-erased — for example, to store
/// a span whose concrete backend type cannot be named in the containing
/// struct's type, while still emitting the span's exit event on drop. In every
/// other situation, bound on [`Handler`] or [`SharedHandler`] instead.
///
/// # Root context
///
/// `Context` is the incoming context type at the start of the handler chain.
/// The blanket [`Logger`] impl exists only for `Handler<()>`: the ergonomic
/// call-site API always starts from `()` and lets middleware enrich it through
/// [`Handler::try_enable`].
/// A handler with a non-empty `Context` cannot be used through [`Logger`] and
/// must either be driven manually or wrapped by middleware that supplies that
/// context starting from `()`.
/// The context type is therefore an implementation detail of the backend
/// chain, not a parameter every log statement must name.
///
/// # Object safety
///
/// `DynHandler<Context>` is object-safe: it has no associated types, no
/// generic methods, and no `Self` in return position. This allows trait
/// objects such as `Box<dyn DynHandler<()> + Send + Sync>` to be constructed
/// and stored.
///
/// [`enter_poll_span`]: Self::enter_poll_span
/// [`pending_poll_span`]: Self::pending_poll_span
/// [`ready_poll_span`]: Self::ready_poll_span
/// [`try_enable`]: crate::Handler::try_enable
/// [`Handler::try_enable`]: crate::Handler::try_enable
/// [`log`]: crate::Handler::log
/// [`enter_span`]: crate::Handler::enter_span
/// [`Logger`]: crate::Logger
/// [`Handler::try_enable`]: crate::Handler::try_enable
pub trait DynHandler<Context>: BaseHandler
where
    Context: ContextArgs,
{
    /// Finalizes a span previously created by [`Handler::enter_span`].
    ///
    /// After this call, `span_id` is no longer associated with an active span.
    ///
    /// The span was already accepted by [`Handler::try_enable`] when
    /// it was entered, so this method should not re‑check whether the span is
    /// enabled.
    ///
    /// `context` is a fresh `Context`, not the `EnrichedContext` returned by
    /// [`Handler::try_enable`] at span entry.
    /// Middleware may enrich it again.
    /// Because the span has already been created, this call is not a filter
    /// point.
    ///
    /// `span_kind` is the same value that was passed to [`Handler::enter_span`]
    /// for this span.
    fn exit_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    );

    /// Records that a poll of the future associated with `span_id` has begun.
    ///
    /// This is emitted by [`PollInstrumented`] before calling `Future::poll`.
    ///
    /// The span was already accepted by [`Handler::try_enable`] when
    /// it was entered, so this method should not re‑check whether the span is
    /// enabled.
    ///
    /// `context` is a fresh `Context`, not the `EnrichedContext` returned by
    /// [`Handler::try_enable`] at span entry.
    /// Middleware may enrich it again.
    /// Because the span has already been created, this call is not a filter
    /// point.
    ///
    /// [`PollInstrumented`]: crate::PollInstrumented
    fn enter_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    );

    /// Records that the future associated with `span_id` returned
    /// [`Poll::Pending`].
    ///
    /// The span was already accepted by [`Handler::try_enable`] when
    /// it was entered, so this method should not re‑check whether the span is
    /// enabled.
    ///
    /// `context` is a fresh `Context`, not the `EnrichedContext` returned by
    /// [`Handler::try_enable`] at span entry.
    /// Middleware may enrich it again.
    /// Because the span has already been created, this call is not a filter
    /// point.
    ///
    /// [`Poll::Pending`]: core::task::Poll::Pending
    fn pending_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    );

    /// Records that the future associated with `span_id` returned
    /// [`Poll::Ready`].
    ///
    /// The span was already accepted by [`Handler::try_enable`] when
    /// it was entered, so this method should not re‑check whether the span is
    /// enabled.
    ///
    /// `context` is a fresh `Context`, not the `EnrichedContext` returned by
    /// [`Handler::try_enable`] at span entry.
    /// Middleware may enrich it again.
    /// Because the span has already been created, this call is not a filter
    /// point.
    ///
    /// [`Poll::Ready`]: core::task::Poll::Ready
    fn ready_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    );
}

/// A [`DynHandler`] whose concrete type also implements [`Debug`].
///
/// `DebuggableDynHandler` exists to give a single name to the bound
/// `Debug + DynHandler<Context>`, which cannot be written directly as a trait
/// object (`dyn DynHandler<Context> + Debug`).
/// This alias allows trait objects such as `Box<dyn DebuggableDynHandler<()> +
/// Send + Sync>` to be named and stored.
///
/// See [`DebuggableTypeErasedOwningSpan`] for the corresponding span alias.
///
/// [`DebuggableTypeErasedOwningSpan`]: crate::DebuggableTypeErasedOwningSpan
pub trait DebuggableDynHandler<T>: Debug + DynHandler<T>
where
    T: ContextArgs,
{
}

impl<T, U> DebuggableDynHandler<U> for T
where
    T: Debug + DynHandler<U>,
    U: ContextArgs,
{
}

/// A backend or middleware log handler.
///
/// `Handler<Context>` is the central abstraction for building composable,
/// type‑rich event pipelines.
/// Each implementation may filter, record, or transform events, and may
/// optionally augment or inspect the accompanying context.
///
/// # Context (`Context`)
///
/// The `Context` type parameter is a heterogeneous tuple implementing
/// [`ContextArgs`].
/// It flows through the handler chain and typically holds dynamic metadata such
/// as timestamps, physical core index, or trace information.
///
/// A handler that starts with `Context = ()` can be wrapped by middleware that
/// appends additional data, producing a richer context type for downstream
/// handlers.
///
/// # Enriched context (`EnrichedContext`)
///
/// [`Handler::try_enable`] inspects a descriptor and the incoming `Context`,
/// and returns `Some(EnrichedContext)` when the event is accepted.
/// Middleware may augment the context.
/// Leaf handlers typically set `EnrichedContext = Context`.
///
/// The enriched context is threaded explicitly from
/// [`Handler::try_enable`] into [`Handler::log`] and [`Handler::enter_span`],
/// so each accepted event is enriched exactly once.
///
/// Lifecycle methods that run after the span has been created
/// ([`DynHandler::exit_span`] and the poll events) receive a fresh
/// `Context` instead.
/// A span persists only its `SpanId`, so the enriched context produced at
/// entry is not retained.
/// Middleware that enriches the context runs its enrichment function again for
/// these calls.
/// Because the span has already been created, these calls are not filter
/// points.
///
/// # Root context
///
/// `Context` is the incoming context type at the start of the handler chain.
/// The blanket [`Logger`] impl exists only for `Handler<()>`: the ergonomic
/// call-site API always starts from `()` and lets middleware enrich it through
/// [`Handler::try_enable`].
/// A handler with a non-empty `Context` cannot be used through [`Logger`] and
/// must either be driven manually or wrapped by middleware that supplies that
/// context starting from `()`.
/// The context type is therefore an implementation detail of the backend
/// chain, not a parameter every log statement must name.
///
/// # Dyn compatibility
///
/// `Handler<Context>` is **not** dyn-compatible.
/// [`Handler::try_enable`] returns the associated type [`EnrichedContext`], and
/// [`Handler::log`] and [`Handler::enter_span`] are generic over the argument
/// tuple, so no trait object can be formed from the full trait.
///
/// The object-safe subset — [`DynHandler::exit_span`] and the poll-span
/// callbacks — is exposed as the supertrait [`DynHandler<Context>`].
/// Bound on `Handler` when the concrete backend type is known at the call
/// site.
/// Bound on `DynHandler` (via `dyn DynHandler<C>`) when the handler must
/// be type-erased, for example to store an owning span in a struct that emits
/// its exit event on drop.
///
/// [`Handler::log`]: Self::log
/// [`Handler::enter_span`]: Self::enter_span
/// [`Handler::try_enable`]: Self::try_enable
/// [`Logger`]: crate::Logger
/// [`EnrichedContext`]: crate::Handler::EnrichedContext
pub trait Handler<Context>: DynHandler<Context>
where
    Context: ContextArgs,
{
    /// The context type passed to [`Handler::log`] and [`Handler::enter_span`]
    /// after [`Handler::try_enable`] has accepted the event.
    ///
    /// Middleware that augments the context sets this to the augmented type;
    /// leaf handlers set it to `Context`.
    ///
    /// The trait does not bound this type by [`ContextArgs`].
    /// A multi-branch middleware such as `polog_util::Fanout` represents the
    /// per-branch acceptance state as `(Option<A>, Option<B>)`, which is not a
    /// valid [`ContextArgs`].
    /// Implementations may use any shape that satisfies their own needs.
    /// They must be able to receive it back unchanged from [`Handler::log`] and
    /// [`Handler::enter_span`].
    ///
    /// [`Handler::log`]: Self::log
    /// [`Handler::enter_span`]: Self::enter_span
    /// [`Handler::try_enable`]: Self::try_enable
    type EnrichedContext;

    /// Checks whether the handler accepts the event described by `descriptor`,
    /// optionally enriching the incoming `context`.
    ///
    /// Returns `Some(enriched_context)` if the event should be processed, or
    /// `None` if it should be silently dropped.
    /// The returned context is forwarded to [`Handler::log`] and
    /// [`Handler::enter_span`].
    ///
    /// Callers must invoke this method before constructing log arguments so
    /// that disabled events pay no per‑event cost.
    /// The logging macros perform this two‑step protocol automatically.
    ///
    /// [`Handler::log`]: Self::log
    /// [`Handler::enter_span`]: Self::enter_span
    fn try_enable(
        &self,
        descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext>;

    /// Processes an event with the given structured arguments and enriched
    /// context.
    ///
    /// This method is only called when [`Handler::try_enable`] has
    /// returned `Some`.
    /// Implementations should not re‑check whether the event is enabled.
    ///
    /// Method arguments are:
    /// - `descriptor`: A descriptor with static log metadata.
    /// - `args`: A heterogeneous tuple of [`Arg`] wrappers that implements
    ///   [`LogArgs`]. It can be formatted immediately via
    ///   [`DisplayableMixedArgs`] or converted to a `'static + Send` form via
    ///   [`IntoSendable`] for deferred (off‑thread) formatting.
    /// - `span_id`: An ID that identifies the execution span under which the
    ///   event occurs, or `None` when no span is active.
    /// - `context`: The enriched context returned by [`Handler::try_enable`].
    ///
    /// [`Arg`]: crate::Arg
    fn log<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        context: Self::EnrichedContext,
    ) where
        Args: LogArgs<'a>;

    /// Begins a new execution span.
    ///
    /// This method is only called when [`Handler::try_enable`] has
    /// returned `Some`.
    /// Implementations should not re‑check whether the span is enabled.
    ///
    /// Spans model hierarchical regions of execution.
    /// The returned identifier should be supplied to subsequent calls to
    /// [`log`] and [`DynHandler::exit_span`] so that events can be correlated.
    ///
    /// Method arguments are:
    /// - `descriptor`: A descriptor with static log metadata.
    /// - `args`: A heterogeneous tuple of [`Arg`] wrappers that implements
    ///   [`LogArgs`]. It can be formatted immediately via
    ///   [`DisplayableMixedArgs`] or converted to a `'static + Send` form via
    ///   [`IntoSendable`] for deferred (off‑thread) formatting.
    /// - `parent_span_id`: An immediate parent span id or `None` for the
    ///   unspecified span.
    /// - `span_id`: A span id.
    /// - `span_kind`: The [`SpanKind`] describing the lifetime relationship
    ///   between this span and its parent. See [`SpanKind`] for the contract of
    ///   each variant.
    /// - `context`: The enriched context returned by [`Handler::try_enable`].
    ///
    /// [`log`]: Self::log
    /// [`Arg`]: crate::Arg
    fn enter_span<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Self::EnrichedContext,
    ) where
        Args: LogArgs<'a>;

    /// Processes an event whose argument tuple is already in sendable form.
    ///
    /// This method is only called when [`Handler::try_enable`] has
    /// returned `Some`.
    /// Implementations should not re‑check whether the event is enabled.
    ///
    /// The static counterpart of [`log`].
    /// The parameters are identical.
    /// The difference is the `Args: StaticLogArgs` bound
    /// (`'static + Send + Sync + DisplayableMixedArgs<'static>`).
    ///
    /// The `'static` bound unlocks per-leaf type inspection through
    /// [`MixedArgs::nth_arg_as_dyn_any`], which requires `Self: 'static` and
    /// therefore cannot be called from [`log`], where the argument tuple is
    /// only known to live for the caller's `'a`.
    /// The `Sync` bound lets middleware such as `polog_util::Fanout` share the
    /// tuple across branches through `Arc` without cloning.
    ///
    /// A handler that overrides this method can inspect the concrete type of
    /// each leaf and act on it without going through [`Display`] or [`Debug`]
    /// formatting.
    /// Typical uses:
    /// - Borrow a `&'static str` argument as `&str` instead of allocating a
    ///   `String` for its formatted value (for example, the `@fn`
    ///   meta-attribute).
    /// - Recognize a specific type such as `Duration`, `Instant`, or `bool` and
    ///   render it with a column, unit, or color that `Display` alone cannot
    ///   express.
    /// - Compute a derived value from a typed argument and cache it alongside
    ///   the rendered record.
    ///
    /// The method is called by callers that already hold a sendable argument
    /// tuple — typically an off-thread worker after
    /// [`IntoSendable::into_sendable`] has run, or middleware such as
    /// `polog_util::Fanout` that shares a single converted tuple across
    /// branches.
    ///
    /// [`Display`]: core::fmt::Display
    /// [`Debug`]: core::fmt::Debug
    /// [`log`]: Self::log
    /// [`IntoSendable::into_sendable`]: crate::IntoSendable::into_sendable
    /// [`MixedArgs::nth_arg_as_dyn_any`]: crate::MixedArgs::nth_arg_as_dyn_any
    fn log_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs;

    /// Begins a span whose argument tuple is already in sendable form.
    ///
    /// This method is only called when [`Handler::try_enable`] has
    /// returned `Some`.
    /// Implementations should not re‑check whether the span is enabled.
    ///
    /// The static counterpart of [`enter_span`].
    /// The parameters are identical.
    /// The difference is the `Args: StaticLogArgs` bound
    /// (`'static + Send + Sync + DisplayableMixedArgs<'static>`).
    ///
    /// The `'static` bound unlocks per-leaf type inspection through
    /// [`MixedArgs::nth_arg_as_dyn_any`], which requires `Self: 'static` and
    /// therefore cannot be called from [`enter_span`], where the argument
    /// tuple is only known to live for the caller's `'a`.
    /// The `Sync` bound lets middleware such as `polog_util::Fanout` share the
    /// tuple across branches through `Arc` without cloning.
    ///
    /// A handler that overrides this method can inspect the concrete type of
    /// each leaf and act on it without going through [`Display`] or [`Debug`]
    /// formatting.
    /// Typical uses match [`log_static`].
    ///
    /// The method is called by callers that already hold a sendable argument
    /// tuple — typically an off-thread worker after
    /// [`IntoSendable::into_sendable`] has run, or middleware such as
    /// `polog_util::Fanout` that shares a single converted tuple across
    /// branches.
    ///
    /// [`Display`]: core::fmt::Display
    /// [`Debug`]: core::fmt::Debug
    /// [`enter_span`]: crate::Handler::enter_span
    /// [`log_static`]: crate::Handler::log_static
    /// [`MixedArgs::nth_arg_as_dyn_any`]: crate::MixedArgs::nth_arg_as_dyn_any
    fn enter_span_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs;
}

/// A [`Handler`] that can process an event without consuming its argument
/// tuple.
///
/// `BorrowingHandler` complements [`Handler`] by accepting the argument tuple
/// and the enriched context by shared reference, so the same event can be
/// dispatched more than once — by fanout middleware, by inspection code, or by
/// any other handler that wants to read the arguments without taking ownership.
///
/// # Relation to [`Handler`]
///
/// `BorrowingHandler: Handler`.
/// Every `BorrowingHandler` is also a `Handler`.
/// The borrowed form trades the owned-argument path for a shared reference,
/// which is useful when the same arguments must reach multiple consumers.
/// Handlers that do not need to share arguments can implement only [`Handler`].
///
/// # Off-thread handlers
///
/// `BorrowingHandler` can be implemented by any handler that receives its
/// arguments in already-converted form, which includes every handler running
/// behind an offloader on the worker thread.
/// Such a handler receives [`StaticLogArgs`], whose `'static + Send + Sync +
/// DisplayableMixedArgs<'static>` bounds already satisfy `log_borrowed` and
/// `enter_span_borrowed` with no conversion step.
pub trait BorrowingHandler<Context>: Handler<Context>
where
    Context: ContextArgs,
{
    /// Processes an event with borrowed arguments and context.
    ///
    /// This method is only called when [`Handler::try_enable`] has
    /// returned `Some`.
    /// Implementations should not re‑check whether the event is enabled.
    ///
    /// The borrowed counterpart of [`Handler::log_static`].
    /// The parameters are the same up to the reference mode.
    /// The difference is that both `args` and `context` are taken by shared
    /// reference, so the same event can be dispatched to more than one handler
    /// without cloning either the argument tuple or the context.
    ///
    /// Because `Args: StaticLogArgs` also carries the `'static` bound, per-leaf
    /// type inspection through [`MixedArgs::nth_arg_as_dyn_any`] is available
    /// here as well.
    /// A handler that overrides this method can inspect the concrete type of
    /// each leaf and act on it without going through [`Display`] or [`Debug`]
    /// formatting.
    /// Typical uses match [`Handler::log_static`].
    ///
    /// The method is called by middleware that dispatches one event to
    /// multiple consumers — typically fanout middleware such as
    /// `polog_util::BorrowingFanout` — after the arguments have been converted
    /// to sendable form once.
    ///
    /// This method is only called when [`Handler::try_enable`] has returned
    /// `Some`.
    /// Implementations should not re‑check whether the event is enabled.
    ///
    /// [`Display`]: core::fmt::Display
    /// [`Debug`]: core::fmt::Debug
    /// [`Handler::try_enable`]: crate::Handler::try_enable
    /// [`Handler::log_static`]: crate::Handler::log_static
    /// [`MixedArgs::nth_arg_as_dyn_any`]: crate::MixedArgs::nth_arg_as_dyn_any
    fn log_borrowed<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        span_id: Option<SpanId>,
        context: &Self::EnrichedContext,
    ) where
        Args: StaticLogArgs;

    /// Begins a span with borrowed arguments and context.
    ///
    /// This method is only called when [`Handler::try_enable`] has
    /// returned `Some`.
    /// Implementations should not re‑check whether the span is enabled.
    ///
    /// The borrowed counterpart of [`Handler::enter_span_static`].
    /// The parameters are the same up to the reference mode.
    /// The difference is that both `args` and `context` are taken by shared
    /// reference, so the same span can be dispatched to more than one handler
    /// without cloning either the argument tuple or the context.
    ///
    /// Because `Args: StaticLogArgs` also carries the `'static` bound, per-leaf
    /// type inspection through [`MixedArgs::nth_arg_as_dyn_any`] is available
    /// here as well.
    /// A handler that overrides this method can inspect the concrete type of
    /// each leaf and act on it without going through [`Display`] or [`Debug`]
    /// formatting.
    /// Typical uses match [`log_borrowed`].
    ///
    /// The method is called by middleware that dispatches one span to
    /// multiple consumers — typically fanout middleware such as
    /// `polog_util::BorrowingFanout` — after the arguments have been converted
    /// to sendable form once.
    ///
    /// This method is only called when [`Handler::try_enable`] has returned
    /// `Some`.
    /// Implementations should not re‑check whether the span is enabled.
    ///
    /// [`Display`]: core::fmt::Display
    /// [`Debug`]: core::fmt::Debug
    /// [`Handler::try_enable`]: crate::Handler::try_enable
    /// [`Handler::enter_span_static`]: crate::Handler::enter_span_static
    /// [`log_borrowed`]: crate::BorrowingHandler::log_borrowed
    /// [`MixedArgs::nth_arg_as_dyn_any`]: crate::MixedArgs::nth_arg_as_dyn_any
    fn enter_span_borrowed<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: &Self::EnrichedContext,
    ) where
        Args: StaticLogArgs;
}

/// A shared handle to a backend or middleware log handler.
///
/// A handler handle is used to share a backend or middleware log handler
/// between threads and detached spans.
///
/// # Clone
///
/// The trait requires `Clone` so a handler can be duplicated for multiple
/// concurrent users, and `Send + Sync` to allow sharing across threads.
/// The `Clone` must produce an independent handle that shares the same
/// underlying sink and is safe to send to another thread.
///
/// All methods are monomorphised over the argument types, enabling the compiler
/// to inline and optimize the entire call chain.
///
/// # `Rc<H>` is not a `SharedHandler`
///
/// `Rc<H>` implements [`Handler`] (via the blanket `impl Handler for Rc<H>`)
/// but intentionally cannot implement `SharedHandler`, because `Rc` is
/// neither `Send` nor `Sync`.
/// Since [`Logger`] is blanket-implemented only for `H: SharedHandler<()>`,
/// this means `Rc<H>` cannot be used through the logging macros either.
/// Use `Arc<H>`, which does implement `SharedHandler`.
///
/// The `Rc` `Handler` impl exists so that middleware composed from `Rc`-wrapped
/// pieces still satisfies `Handler` bounds.
/// It is not intended as a logger.
///
/// [`Handler`]: crate::Handler
/// [`Logger`]: crate::Logger
pub trait SharedHandler<Context>: 'static + Send + Sync + Clone + Handler<Context>
where
    Context: ContextArgs,
{
}

macro_rules! propagate_base_handler_impl {
    (
        impl ( $($tt:tt)* ) BaseHandler for $ty:ty where ( $($bounds:tt)* ),
        impl ($via:ty) $self:tt => $expr:expr $(,)?
    ) => {
        impl<$($tt)*> BaseHandler for $ty
        where
            $($bounds)*
        {
            #[inline]
            fn generate_span_id(&$self) -> SpanId {
                <$via as BaseHandler>::generate_span_id(&$expr)
            }

            #[inline]
            fn flush_blocking(&mut $self) {
                <$via as BaseHandler>::flush_blocking(&mut $expr);
            }

            #[inline]
            fn flush_on_panic(&mut $self) {
                <$via as BaseHandler>::flush_on_panic(&mut $expr);
            }

            #[inline]
            fn should_panic_on_bug(
                &$self,
                descriptor: &'static Descriptor,
                span_id: Option<SpanId>,
            ) -> bool {
                <$via as BaseHandler>::should_panic_on_bug(
                    &$expr,
                    descriptor,
                    span_id
                )
            }

            #[inline]
            fn panic(
                &mut $self,
                descriptor: &'static Descriptor,
                span_id: Option<SpanId>
            ) -> ! {
                <$via as BaseHandler>::panic(&mut $expr, descriptor, span_id);
            }
        }
    }
}

macro_rules! propagate_dyn_handler_impl {
    (
        impl ( $($tt:tt)* ) DynHandler<$context:ty> for $ty:ty where ( $($bounds:tt)* ),
        impl ($via:ty) $self:tt => $expr:expr $(,)?
    ) => {
        impl<$($tt)*> DynHandler<$context> for $ty
        where
            $($bounds)*
        {
            #[inline]
            fn exit_span(
                &mut $self,
                descriptor: &'static Descriptor,
                span_id: SpanId,
                span_kind: SpanKind,
                context: Context,
            ) {
                <$via as DynHandler<Context>>::exit_span(
                    &mut $expr,
                    descriptor,
                    span_id,
                    span_kind,
                    context,
                );
            }


            #[inline]
            fn enter_poll_span(
                &mut $self,
                descriptor: &'static Descriptor,
                span_id: SpanId,
                context: Context,
            ) {
                <$via as DynHandler<Context>>::enter_poll_span(
                    &mut $expr,
                    descriptor,
                    span_id,
                    context,
                );
            }

            #[inline]
            fn pending_poll_span(
                &mut $self,
                descriptor: &'static Descriptor,
                span_id: SpanId,
                context: Context,
            ) {
                <$via as DynHandler<Context>>::pending_poll_span(
                    &mut $expr,
                    descriptor,
                    span_id,
                    context,
                );
            }

            #[inline]
            fn ready_poll_span(
                &mut $self,
                descriptor: &'static Descriptor,
                span_id: SpanId,
                context: Context,
            ) {
                <$via as DynHandler<Context>>::ready_poll_span(
                    &mut $expr,
                    descriptor,
                    span_id,
                    context,
                );
            }
        }
    }
}

macro_rules! propagate_handler_impl {
    (
        impl ( $($tt:tt)* ) Handler<$context:ty> for $ty:ty where ( $($bounds:tt)* ),
        type EnrichedContext = $enriched_context:ty,
        impl ($via:ty) $self:tt => $expr:expr $(,)?
    ) => {
        impl<$($tt)*> Handler<$context> for $ty
        where
            $($bounds)*
        {
            type EnrichedContext = $enriched_context;

            #[inline]
            fn try_enable(
                &$self,
                descriptor: &'static Descriptor,
                context: Context,
            ) -> Option<Self::EnrichedContext> {
                <$via as Handler<Context>>::try_enable(&$expr, descriptor, context)
            }

            #[inline]
            fn log<'a, Args>(
                &mut $self,
                descriptor: &'static Descriptor,
                args: Args,
                span_id: Option<SpanId>,
                context: Self::EnrichedContext,
            ) where
                Args: LogArgs<'a>,
            {
                <$via as Handler<Context>>::log(
                    &mut $expr,
                    descriptor,
                    args,
                    span_id,
                    context,
                );
            }

            #[inline]
            fn enter_span<'a, Args>(
                &mut $self,
                descriptor: &'static Descriptor,
                args: Args,
                parent_span_id: Option<SpanId>,
                span_id: SpanId,
                span_kind: SpanKind,
                context: Self::EnrichedContext,
            ) where
                Args: LogArgs<'a>,
            {
                <$via as Handler<Context>>::enter_span(
                    &mut $expr,
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
                &mut $self,
                descriptor: &'static Descriptor,
                args: Args,
                span_id: Option<SpanId>,
                context: Self::EnrichedContext,
            ) where
                Args: StaticLogArgs,
            {
                <$via as Handler<Context>>::log_static(
                    &mut $expr,
                    descriptor,
                    args,
                    span_id,
                    context,
                );
            }

            #[inline]
            fn enter_span_static<Args>(
                &mut $self,
                descriptor: &'static Descriptor,
                args: Args,
                parent_span_id: Option<SpanId>,
                span_id: SpanId,
                span_kind: SpanKind,
                context: Self::EnrichedContext,
            ) where
                Args: StaticLogArgs,
            {
                <$via as Handler<Context>>::enter_span_static(
                    &mut $expr,
                    descriptor,
                    args,
                    parent_span_id,
                    span_id,
                    span_kind,
                    context,
                );
            }
        }
    }
}

macro_rules! propagate_borrowing_handler_impl {
    (
        impl ( $($tt:tt)* ) BorrowingHandler<$context:ty> for $ty:ty where ( $($bounds:tt)* ),
        impl ($via:ty) $self:tt => $expr:expr $(,)?
    ) => {
        impl<$($tt)*> BorrowingHandler<$context> for $ty
        where
            $($bounds)*
        {
            #[inline]
            fn log_borrowed<Args>(
                &mut $self,
                descriptor: &'static Descriptor,
                args: &Args,
                span_id: Option<SpanId>,
                context: &Self::EnrichedContext,
            ) where
                Args: StaticLogArgs,
            {
                <$via as BorrowingHandler<Context>>::log_borrowed(
                    &mut $expr,
                    descriptor,
                    args,
                    span_id,
                    context,
                );
            }

            #[inline]
            fn enter_span_borrowed<Args>(
                &mut $self,
                descriptor: &'static Descriptor,
                args: &Args,
                parent_span_id: Option<SpanId>,
                span_id: SpanId,
                span_kind: SpanKind,
                context: &Self::EnrichedContext,
            ) where
                Args: StaticLogArgs,
            {
                <$via as BorrowingHandler<Context>>::enter_span_borrowed(
                    &mut $expr,
                    descriptor,
                    args,
                    parent_span_id,
                    span_id,
                    span_kind,
                    context,
                );
            }
        }
    }
}

propagate_base_handler_impl!(
    impl (H) BaseHandler for &mut H where (H: ?Sized + BaseHandler),
    impl (H) self => **self
);

propagate_dyn_handler_impl!(
    impl (H, Context) DynHandler<Context> for &mut H
    where (
        H: ?Sized + DynHandler<Context>,
        Context: ContextArgs,
    ),
    impl (H) self => **self
);

propagate_handler_impl!(
    impl (H, Context) Handler<Context> for &mut H
    where (
        H: ?Sized + Handler<Context>,
        Context: ContextArgs,
    ),
    type EnrichedContext = H::EnrichedContext,
    impl (H) self => **self
);

propagate_borrowing_handler_impl!(
    impl (H, Context) BorrowingHandler<Context> for &mut H
    where (
        H: ?Sized + BorrowingHandler<Context>,
        Context: ContextArgs,
    ),
    impl (H) self => **self
);

#[cfg(feature = "alloc")]
mod alloc_impls {
    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::sync::Arc;

    use crate::{
        BaseHandler, BorrowingHandler, ContextArgs, Descriptor, DynHandler, Handler, LogArgs,
        SharedHandler, SpanId, SpanKind, StaticLogArgs,
    };

    propagate_base_handler_impl!(
        impl (H) BaseHandler for Box<H> where (H: ?Sized + BaseHandler),
        impl (H) self => **self
    );

    propagate_dyn_handler_impl!(
        impl (H, Context) DynHandler<Context> for Box<H>
        where (
            H: ?Sized + DynHandler<Context>,
            Context: ContextArgs,
        ),
        impl (H) self => **self
    );

    propagate_handler_impl!(
        impl (H, Context) Handler<Context> for Box<H>
        where (
            H: ?Sized + Handler<Context>,
            Context: ContextArgs,
        ),
        type EnrichedContext = H::EnrichedContext,
        impl (H) self => **self
    );

    propagate_borrowing_handler_impl!(
        impl (H, Context) BorrowingHandler<Context> for Box<H>
        where (
            H: ?Sized + BorrowingHandler<Context>,
            Context: ContextArgs,
        ),
        impl (H) self => **self
    );

    impl<H, Context, EnrichedContext> SharedHandler<Context> for Box<H>
    where
        H: SharedHandler<Context, EnrichedContext = EnrichedContext>,
        Context: ContextArgs,
    {
    }

    propagate_base_handler_impl!(
        impl (H) BaseHandler for Rc<H>
        where (
            H: ?Sized,
            for<'a> &'a H: BaseHandler,
        ),
        impl (&H) self => &**self
    );

    propagate_dyn_handler_impl!(
        impl (H, Context) DynHandler<Context> for Rc<H>
        where (
            H: ?Sized,
            for<'a> &'a H: DynHandler<Context>,
            Context: ContextArgs,
        ),
        impl (&H) self => &**self
    );

    propagate_handler_impl!(
        impl (H, Context, EnrichedContext) Handler<Context> for Rc<H>
        where (
            H: ?Sized,
            for<'a> &'a H: Handler<Context, EnrichedContext = EnrichedContext>,
            Context: ContextArgs,
        ),
        type EnrichedContext = EnrichedContext,
        impl (&H) self => &**self
    );

    propagate_borrowing_handler_impl!(
        impl (H, Context, EnrichedContext) BorrowingHandler<Context> for Rc<H>
        where (
            H: ?Sized,
            for<'a> &'a H: BorrowingHandler<Context, EnrichedContext = EnrichedContext>,
            Context: ContextArgs,
        ),
        impl (&H) self => &**self
    );

    propagate_base_handler_impl!(
        impl (H) BaseHandler for Arc<H> where (
            H: ?Sized,
            for<'a> &'a H: BaseHandler,
        ),
        impl (&H) self => &**self
    );

    propagate_dyn_handler_impl!(
        impl (H, Context) DynHandler<Context> for Arc<H>
        where (
            H: ?Sized,
            for<'a> &'a H: DynHandler<Context>,
            Context: ContextArgs,
        ),
        impl (&H) self => &**self
    );

    propagate_handler_impl!(
        impl (H, Context, EnrichedContext) Handler<Context> for Arc<H>
        where (
            H: ?Sized,
            for<'a> &'a H: Handler<Context, EnrichedContext = EnrichedContext>,
            Context: ContextArgs,
        ),
        type EnrichedContext = EnrichedContext,
        impl (&H) self => &**self
    );

    propagate_borrowing_handler_impl!(
        impl (H, Context, EnrichedContext) BorrowingHandler<Context> for Arc<H>
        where (
            H: ?Sized,
            for<'a> &'a H: BorrowingHandler<Context, EnrichedContext = EnrichedContext>,
            Context: ContextArgs,
        ),
        impl (&H) self => &**self
    );

    impl<H, Context, EnrichedContext> SharedHandler<Context> for Arc<H>
    where
        H: 'static + ?Sized + Send + Sync,
        for<'a> &'a H: Handler<Context, EnrichedContext = EnrichedContext>,
        Context: ContextArgs,
    {
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::sync::Arc;

    use crate::{
        BaseHandler, Descriptor, Discard, Handler, Level, SpanLevel, log, log_descriptor_ref, span,
    };

    #[expect(clippy::mut_mut, reason = "explicitly tested")]
    #[test]
    #[should_panic(expected = "FATAL at root span")]
    fn test_handler_panic() {
        const DESC: &Descriptor = log_descriptor_ref!(Level::Fatal, "Descriptor");
        BaseHandler::panic(&mut &mut Discard, DESC, None);
    }

    #[test]
    fn test_handler_by_ref() {
        let mut logger = Discard;
        log!(&mut logger, Level::Info, "Message");
        let mut span = span!(&mut logger, SpanLevel::Info, "Message");
        log!(&mut span, Level::Info, "Message");
        drop(span);
    }

    #[expect(clippy::mut_mut, reason = "explicitly tested")]
    #[cfg(feature = "alloc")]
    #[test]
    fn test_handler_by_ref_alloc() {
        use crate::{BorrowingHandler, DynHandler, SpanKind};

        const DESC: &Descriptor = log_descriptor_ref!(Level::Info, "Descriptor");

        let logger = Discard;
        log!(&mut Box::new(logger), Level::Info, "Message");
        log!(&mut Arc::new(logger), Level::Info, "Message");

        let mut logger = Box::new(Discard);
        let mut span = span!(&mut logger, SpanLevel::Info, "Message");
        log!(&mut span, Level::Info, "Message");
        drop(span);

        let mut logger = Arc::new(Discard);
        let mut span = span!(&mut logger, SpanLevel::Info, "Message");
        log!(&mut span, Level::Info, "Message");
        drop(span);

        let logger = Discard;
        let logger = Arc::new(logger);
        let mut logger = Box::new(logger);
        let mut logger: &mut Box<Arc<Discard>> = &mut logger;

        BaseHandler::flush_blocking(&mut logger); // No-op.
        BaseHandler::flush_on_panic(&mut logger); // No-op.
        assert_eq!(
            BaseHandler::should_panic_on_bug(&logger, DESC, None),
            cfg!(debug_assertions)
        );
        let span_id1 = BaseHandler::generate_span_id(&logger);
        let span_id2 = BaseHandler::generate_span_id(&logger);
        assert_ne!(span_id1, span_id2);

        assert_eq!(Handler::try_enable(&logger, DESC, ()), None);

        Handler::log(&mut logger, DESC, (), None, ());
        Handler::log_static(&mut logger, DESC, (), None, ());
        BorrowingHandler::log_borrowed(&mut logger, DESC, &(), None, &());

        Handler::enter_span(&mut logger, DESC, (), None, span_id1, SpanKind::Nested, ());
        Handler::enter_span_static(&mut logger, DESC, (), None, span_id1, SpanKind::Nested, ());
        BorrowingHandler::enter_span_borrowed(
            &mut logger,
            DESC,
            &(),
            None,
            span_id1,
            SpanKind::Nested,
            &(),
        );
        DynHandler::exit_span(&mut logger, DESC, span_id1, SpanKind::Nested, ());

        DynHandler::enter_poll_span(&mut logger, DESC, span_id2, ());
        DynHandler::pending_poll_span(&mut logger, DESC, span_id2, ());
        DynHandler::enter_poll_span(&mut logger, DESC, span_id2, ());
        DynHandler::ready_poll_span(&mut logger, DESC, span_id2, ());
    }
}
