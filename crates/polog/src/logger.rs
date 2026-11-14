use crate::{SharedHandler, SpanId, SpanKind};

/// A frontend for emitting events and managing spans.
///
/// `Logger` is the public API through which structured events and spans are
/// produced.
/// It abstracts over the concrete [`Handler`] so that the backend can be
/// swapped or composed without changing call sites.
///
/// All methods are monomorphised over the argument tuple `Args`, allowing the
/// compiler to inline formatting and conversion logic when the tuple structure
/// is statically known.
///
/// # Context
///
/// [`Logger`] is the ergonomic frontend.
/// Every event and span it produces starts from the empty context `()`: the
/// blanket `Logger` impl applies only to `Handler<Context = ()>`, and
/// [`LoggerExt::try_enable`] passes `()` to [`Handler::try_enable`].
/// Middleware attaches per-invocation metadata by *enriching* that empty
/// context, not by requiring callers to construct one.
/// A handler whose `Context` is not `()` therefore cannot be used through
/// [`Logger`] and must be driven manually with [`Handler`].
/// This keeps the call-site API uniform: log statements never name the
/// enriched context type, and swapping middleware does not change call sites.
///
/// Before calling [`LoggerExt::log`], [`LoggerExt::borrowing_span`], or
/// [`LoggerExt::deferred_span`], callers must run the enablement check via
/// [`LoggerExt::try_enable`] with the empty context `()` and pass the returned
/// enriched context to the method.
/// These methods do not call [`Handler::try_enable`] and do not re-check
/// enablement.
///
/// Implementations are provided for raw backends (`Handler<()>`), for spans
/// ([`BorrowingSpan`], [`OwningSpan`]), and for [`DetachedLogger`].
///
/// [`BorrowingSpan`]: crate::BorrowingSpan
/// [`DetachedLogger`]: crate::DetachedLogger
/// [`Handler::try_enable`]: crate::Handler::try_enable
/// [`Handler`]: crate::Handler
/// [`LoggerExt::deferred_span`]: crate::LoggerExt::deferred_span
/// [`LoggerExt::log`]: crate::LoggerExt::log
/// [`LoggerExt::borrowing_span`]: crate::LoggerExt::borrowing_span
/// [`LoggerExt::try_enable`]: crate::LoggerExt::try_enable
/// [`LoggerExt::try_enable`]: crate::LoggerExt::try_enable
/// [`OwningSpan`]: crate::OwningSpan
pub trait Logger {
    /// The concrete shared handler type used by this logger.
    type Handler: SharedHandler<()>;

    /// Returns a shared reference to the underlying handler.
    fn handler_ref(&self) -> &Self::Handler;

    /// Returns a mutable reference to the underlying handler.
    fn handler_mut(&mut self) -> &mut Self::Handler;

    /// Returns the ID of the span that events logged through this logger are
    /// attributed to, or `None` if this logger is not associated with a span.
    ///
    /// For a [`BorrowingSpan`] or [`OwningSpan`], this is the span's own ID.
    /// For a [`DetachedLogger`], this is the ID of the span that was active
    /// when the context was created.
    /// The context is a proxy for that span on another thread or task.
    ///
    /// [`BorrowingSpan`]: crate::BorrowingSpan
    /// [`OwningSpan`]: crate::OwningSpan
    /// [`DetachedLogger`]: crate::DetachedLogger
    fn span_id(&self) -> Option<SpanId>;

    /// Returns the [`SpanKind`] that a child [`BorrowingSpan`] created via
    /// [`LoggerExt::borrowing_span`] or
    /// [`LoggerExt::transparent_borrowing_span`] from this logger will report.
    ///
    /// This is **not** this logger's own kind. It describes the relationship
    /// between a future `BorrowingSpan` child and the span identified by
    /// [`Logger::span_id`] of this logger — the child's parent.
    ///
    /// Other constructors determine their child's kind independently:
    /// [`LoggerExt::owning_span`] always produces [`SpanKind::Detached`],
    /// and [`DeferredSpan::enter`] materializes with [`SpanKind::Detached`].
    ///
    /// [`LoggerExt::borrowing_span`]: crate::LoggerExt::borrowing_span
    /// [`LoggerExt::transparent_borrowing_span`]: crate::LoggerExt::transparent_borrowing_span
    /// [`LoggerExt::owning_span`]: crate::LoggerExt::owning_span
    /// [`BorrowingSpan`]: crate::BorrowingSpan
    /// [`DeferredSpan::enter`]: crate::DeferredSpan::enter
    /// [`Logger::span_id`]: Self::span_id
    /// [`SpanKind::Detached`]: crate::SpanKind::Detached
    fn child_borrowing_span_kind(&self) -> SpanKind;
}

impl<H> Logger for H
where
    H: SharedHandler<()>,
{
    type Handler = H;

    #[inline]
    fn handler_ref(&self) -> &Self::Handler {
        self
    }

    #[inline]
    fn handler_mut(&mut self) -> &mut Self::Handler {
        self
    }

    #[inline]
    fn span_id(&self) -> Option<SpanId> {
        None
    }

    #[inline]
    fn child_borrowing_span_kind(&self) -> SpanKind {
        SpanKind::Nested
    }
}
