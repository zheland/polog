use crate::{
    BaseHandler, BorrowingSpan, DeferredSpan, Descriptor, DetachedLogger, Handler, Level, LogArgs,
    Logger, OwningSpan, SpanKind,
};

/// Convenience methods for [`Logger`] implementations.
///
/// # Context
///
/// Every method here drives the underlying handler from `()` and forwards the
/// [`Handler::EnrichedContext`] returned by [`Handler::try_enable`].
/// A middleware chain therefore exposes the same call-site API as a leaf
/// handler: middleware enriches the empty context on its own, and the caller
/// never names the enriched type.
pub trait LoggerExt: Logger {
    /// Checks whether the handler accepts the event described by `descriptor`,
    /// optionally enriching the incoming `context`.
    ///
    /// Returns `Some(enriched_context)` if the event or span is accepted, or
    /// `None` if it is disabled.
    /// The returned enriched context must be passed to [`LoggerExt::log`],
    /// [`LoggerExt::borrowing_span`], or [`LoggerExt::deferred_span`].
    #[inline]
    fn try_enable(
        &self,
        descriptor: &'static Descriptor,
    ) -> Option<<Self::Handler as Handler<()>>::EnrichedContext> {
        Handler::try_enable(self.handler_ref(), descriptor, ())
    }

    /// Flushes the underlying handler, blocking until buffered records are
    /// written.
    #[inline]
    fn flush_blocking(&mut self) {
        BaseHandler::flush_blocking(self.handler_mut());
    }

    /// Flushes any buffered records as part of a failure path.
    #[inline]
    #[track_caller]
    fn flush_on_panic(&mut self) {
        BaseHandler::flush_on_panic(self.handler_mut());
    }

    /// Emits an event with the given structured arguments.
    ///
    /// Callers must first call [`LoggerExt::try_enable`] with the empty context
    /// `()` and pass the returned enriched context here.
    /// This method does not call [`Handler::try_enable`] and does not re-check
    /// whether the event is enabled.
    #[inline]
    fn log<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        context: <Self::Handler as Handler<()>>::EnrichedContext,
    ) where
        Args: LogArgs<'a>,
    {
        let span_id = self.span_id();
        let handler = self.handler_mut();
        Handler::log(handler, descriptor, args, span_id, context);
    }

    /// Opens a new span that borrows the logger's handler.
    ///
    /// The returned [`BorrowingSpan`] is entered with
    /// [`Logger::child_borrowing_span_kind`] as its [`SpanKind`].
    /// That value describes the relationship between the new span and the span
    /// identified by [`Logger::span_id`] of this logger:
    ///
    /// - [`SpanKind::Nested`] on a raw handler, a real [`BorrowingSpan`] or
    ///   [`OwningSpan`], or a transparent span with `Nested` kind: the new span
    ///   borrows the handler mutably, so the parent cannot emit events until it
    ///   is dropped.
    /// - [`SpanKind::Detached`] on a [`DetachedLogger`] or on a transparent
    ///   span with `Detached` kind: the new span shares only the parent
    ///   [`SpanId`] with the logger, and the span that created the context may
    ///   continue emitting events or be closed while the new span is active.
    ///
    /// See [`Logger::child_borrowing_span_kind`] for the full decision table.
    ///
    /// Callers must first call [`LoggerExt::try_enable`] with the empty
    /// context `()` and pass the returned enriched context here.
    /// This method does not call [`Handler::try_enable`] and does not
    /// re-check whether the event is enabled.
    ///
    /// The returned [`BorrowingSpan`] lives as long as the borrow and
    /// emits [`DynHandler::exit_span`] on drop.
    ///
    /// [`Logger::child_borrowing_span_kind`]: crate::Logger::child_borrowing_span_kind
    /// [`Logger::span_id`]: crate::Logger::span_id
    /// [`SpanId`]: crate::SpanId
    /// [`DynHandler::exit_span`]: crate::DynHandler::exit_span
    #[inline]
    fn borrowing_span<'this, 'a, Args>(
        &'this mut self,
        descriptor: &'static Descriptor,
        args: Args,
        context: <Self::Handler as Handler<()>>::EnrichedContext,
    ) -> BorrowingSpan<'this, Self::Handler>
    where
        Args: LogArgs<'a>,
    {
        let parent_span_id = self.span_id();
        let kind = self.child_borrowing_span_kind();
        let span_id = BaseHandler::generate_span_id(self.handler_ref());
        let handler = self.handler_mut();
        Handler::enter_span(
            handler,
            descriptor,
            args,
            parent_span_id,
            span_id,
            kind,
            context,
        );
        BorrowingSpan::new(handler, Some(descriptor), Some(span_id), kind)
    }

    /// Creates a transparent span that borrows the handler and shares the
    /// current span ID.
    ///
    /// A transparent span emits no enter/exit events.
    /// Only its [`Logger::span_id`] is used, to attribute events and child
    /// spans to the enclosing span.
    /// The returned span's kind is [`Logger::child_borrowing_span_kind`], so
    /// children created from it behave as if created directly on `self`:
    ///
    /// - From a raw handler, a real [`BorrowingSpan`] or [`OwningSpan`], or
    ///   another transparent span with `Nested` kind: children are
    ///   [`SpanKind::Nested`].
    /// - From a [`DetachedLogger`] or a transparent span with `Detached` kind:
    ///   children are [`SpanKind::Detached`], because the transparent span is a
    ///   proxy for a span on another thread or task and does not block it.
    ///
    /// [`Logger::child_borrowing_span_kind`]: crate::Logger::child_borrowing_span_kind
    /// [`Logger::span_id`]: crate::Logger::span_id
    /// [`SpanKind::Nested`]: crate::SpanKind::Nested
    /// [`SpanKind::Detached`]: crate::SpanKind::Detached
    #[inline]
    fn transparent_borrowing_span(&mut self) -> BorrowingSpan<'_, Self::Handler> {
        let span_id = self.span_id();
        let kind = self.child_borrowing_span_kind();
        let handler = self.handler_mut();
        BorrowingSpan::new(handler, None, span_id, kind)
    }

    /// Opens a new span that owns a cloned handler and emits
    /// [`DynHandler::exit_span`] on drop.
    ///
    /// The returned [`OwningSpan`] clones the handler and may be stored in a
    /// struct or moved into another thread or task.
    /// Because the caller retains access to this logger and the span is not
    /// borrow-tied to it, the parent may emit events or be closed while the
    /// span is active.
    /// The span is therefore always reported as [`SpanKind::Detached`].
    ///
    /// Use [`LoggerExt::borrowing_span`] for a [`SpanKind::Nested`] span whose
    /// lifetime the borrow checker ties to the parent.
    ///
    /// Callers must first call [`LoggerExt::try_enable`] with the empty context
    /// `()` and pass the returned enriched context here.
    /// This method does not call [`Handler::try_enable`] and does not re-check
    /// whether the event is enabled.
    ///
    /// [`DynHandler::exit_span`]: crate::DynHandler::exit_span
    #[inline]
    fn owning_span<'a, Args>(
        &self,
        descriptor: &'static Descriptor,
        args: Args,
        context: <Self::Handler as Handler<()>>::EnrichedContext,
    ) -> OwningSpan<Self::Handler>
    where
        Args: LogArgs<'a>,
    {
        let parent_span_id = self.span_id();
        let span_id = BaseHandler::generate_span_id(self.handler_ref());
        let mut handler = self.handler_ref().clone();
        Handler::enter_span(
            &mut handler,
            descriptor,
            args,
            parent_span_id,
            span_id,
            SpanKind::Detached,
            context,
        );
        OwningSpan::new(handler, Some(descriptor), Some(span_id))
    }

    /// Creates a transparent span that owns a cloned handler and shares
    /// the current span ID.
    ///
    /// A transparent span emits no enter/exit events; only its
    /// [`Logger::span_id`] is used, to attribute events and child spans
    /// to the enclosing span.
    /// This is the owning counterpart of
    /// [`LoggerExt::transparent_borrowing_span`] and is useful when only span
    /// ID propagation is needed while the span must be stored or moved.
    ///
    /// Child spans created from the returned span are always reported as
    /// [`SpanKind::Detached`], because the owning span holds its own
    /// handler: the span identified by [`Logger::span_id`] is free to
    /// emit events or be closed while those children are active.
    ///
    /// [`Logger::span_id`]: crate::Logger::span_id
    /// [`SpanKind::Detached`]: crate::SpanKind::Detached
    #[inline]
    fn transparent_owning_span(&self) -> OwningSpan<Self::Handler> {
        let span_id = self.span_id();
        let handler = self.handler_ref().clone();
        OwningSpan::new(handler, None, span_id)
    }

    /// Creates a deferred span that owns a cloned handler and emits no events
    /// until materialized with [`DeferredSpan::enter`].
    ///
    /// The returned [`DeferredSpan`] stores the logger's current [`SpanId`] as
    /// its parent, clones the handler, and captures the descriptor and the
    /// argument factory without evaluating them.
    ///
    /// The materialized span is always reported as [`SpanKind::Detached`],
    /// because the [`DeferredSpan`] owns a cloned handler and does not
    /// block the span that created it.
    /// To obtain a strictly nested span, use [`LoggerExt::borrowing_span`]
    /// directly.
    ///
    /// [`PollInstrument::poll_instrument`] materializes the span as `Detached`
    /// and adds per-poll events on top.
    ///
    /// [`SpanId`]: crate::SpanId
    /// [`PollInstrument::poll_instrument`]: crate::PollInstrument::poll_instrument
    #[inline]
    fn deferred_span<'a, LazyArgs, Args>(
        &self,
        descriptor: &'static Descriptor,
        lazy_args: LazyArgs,
    ) -> DeferredSpan<Self::Handler, LazyArgs>
    where
        LazyArgs: FnOnce() -> Args,
        Args: LogArgs<'a>,
    {
        let parent_span_id = self.span_id();
        let handler = self.handler_ref().clone();
        DeferredSpan::new(handler, descriptor, lazy_args, parent_span_id)
    }

    /// Creates a detached context that owns a cloned handler and the current
    /// [`SpanId`].
    ///
    /// The returned [`DetachedLogger`] implements [`Logger`] and can be moved
    /// across threads or tasks or outlive the logger borrow.
    /// A span created from it with [`LoggerExt::borrowing_span`] is reported as
    /// [`SpanKind::Detached`].
    /// Passing it to [`PollInstrumented::new`] creates a [`SpanKind::Detached`]
    /// span.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::polog::LoggerExt;
    ///
    /// let mut logger = polog::Discard;
    /// let mut logger = polog::info_span!(&mut logger, "root"); // Nested
    /// let mut detached = logger.detach();
    /// let mut child = polog::info_span!(&mut detached, "child"); // Detached
    /// drop(child);
    /// let mut pass = detached.transparent_borrowing_span();
    /// let mut grand = polog::info_span!(&mut pass, "grand"); // Detached
    /// ```
    ///
    /// [`SpanId`]: crate::SpanId
    /// [`SpanKind::Detached`]: crate::SpanKind::Detached
    /// [`PollInstrumented::new`]: crate::PollInstrumented::new
    #[inline]
    fn detach(&self) -> DetachedLogger<Self::Handler> {
        let handler = self.handler_ref();
        DetachedLogger::new(handler.clone(), self.span_id())
    }

    /// Logs a critical event and panics unconditionally.
    ///
    /// Flushes buffered records with [`BaseHandler::flush_on_panic`] before
    /// calling [`BaseHandler::panic`].
    ///
    /// # Panics
    ///
    /// Always panics.
    #[inline]
    #[track_caller]
    fn flush_and_panic(&mut self, descriptor: &'static Descriptor) -> ! {
        let span_id = self.span_id();
        // Ensure buffered fatal logs are written.
        BaseHandler::flush_on_panic(self.handler_mut());
        BaseHandler::panic(self.handler_mut(), descriptor, span_id);
    }

    /// Panics if `descriptor` denotes a critical event.
    ///
    /// A critical event is either [`Level::Fatal`] or [`Level::Bug`] for which
    /// [`BaseHandler::should_panic_on_bug`] returns `true`.
    /// Before panicking, buffered records are flushed with
    /// [`BaseHandler::flush_on_panic`].
    ///
    /// # Panics
    ///
    /// Panics if either condition is fulfilled:
    /// - `descriptor.level == Level::Fatal`.
    /// - `descriptor.level == Level::Bug &&
    ///   logger.should_panic_on_bug(descriptor)`.
    #[inline]
    #[track_caller]
    fn panic_if_critical(&mut self, descriptor: &'static Descriptor) {
        let span_id = self.span_id();
        if descriptor.level == Level::Fatal
            || (descriptor.level == Level::Bug
                && self.handler_ref().should_panic_on_bug(descriptor, span_id))
        {
            // Ensure buffered fatal logs are written.
            BaseHandler::flush_on_panic(self.handler_mut());
            BaseHandler::panic(self.handler_mut(), descriptor, span_id);
        }
    }
}

impl<L> LoggerExt for L where L: ?Sized + Logger {}
