use std::thread;

use polog::{Logger, SharedHandler, SpanId, SpanKind};
use polog_util::OffloaderHandle;

/// A logger handle that owns the offloader worker and shuts it down on drop.
///
/// # Logging
///
/// `OffThreadLogger` implements [`polog::Logger`], so every `polog` macro
/// accepts `&mut OffThreadLogger<…>` directly.
/// Events flow through the same middleware chain built by
/// [`OffThreadLoggerBuilder::build`]: filter, offloader, and time enricher.
///
/// # Shutdown
///
/// Dropping an intact `OffThreadLogger` performs three steps in order:
///
/// 1. Sends an `Event::Shutdown` to the worker with the [`Block`] backpressure
///    policy. This is guaranteed to be delivered unless the worker has already
///    exited (in which case the send is a no-op).
/// 2. The worker, on receiving the event, breaks its loop, flushes the inner
///    handler, and returns.
/// 3. `Drop` joins the worker, so the flush from step 2 is guaranteed to
///    complete before `Drop` returns.
///
/// The blocking duration is bounded by "one worker iteration" (drain any
/// queued events, format them, flush).
/// In synchronous contexts this is acceptable.
/// In async runtimes or signal handlers, call
/// [`polog::LoggerExt::flush_blocking`] on the logger before letting it drop
/// so that most of the work is already done by the time `Drop` runs.
///
/// # Decomposing
///
/// Two of the three owned fields can be extracted:
/// [`take_offloader_handle`] removes the handle used to send the shutdown
/// event, and [`take_service_handle`] removes the worker's `JoinHandle`.
/// Extraction is intended for callers who want to control shutdown timing —
/// for example, to avoid blocking inside `Drop` on an async runtime.
/// - Taking only the service handle disables the join.
/// - Taking only the offloader handle disables both shutdown signalling and
///   join.
///
/// # Handler clones after shutdown
///
/// [`handler_ref`] returns `&H` where `H: SharedHandler<()>`.
/// A caller can clone the chain and keep it alive past the `OffThreadLogger`'s
/// drop.
/// Such clones still hold an [`OffloaderHandle`] internally, and events sent
/// through them after shutdown are accepted into the channel but never read.
/// This is a deliberate trade to keep `OffloaderHandle` cheap to clone.
///
/// Callers who must keep handler clones across shutdown should drop them
/// before dropping the `OffThreadLogger`, or accept that late events may be
/// silently lost.
///
/// # Worker panics
///
/// If the worker panics, `Drop`'s [`join`] returns the panic payload.
/// `Drop` discards it.
/// There is no way to observe the panic after the fact because
/// `OffThreadLogger` owns the only `JoinHandle`.
/// Use [`service_handle`] to check the worker's liveness while the logger is
/// still alive if this matters.
///
/// [`Block`]: polog_util::OffloaderBackpressurePolicy::Block
/// [`OffThreadLoggerBuilder::build`]: crate::OffThreadLoggerBuilder::build
/// [`join`]: std::thread::JoinHandle::join
/// [`service_handle`]: Self::service_handle
/// [`take_offloader_handle`]: Self::take_offloader_handle
/// [`take_service_handle`]: Self::take_service_handle
/// [`handler_ref`]: polog::Logger::handler_ref
#[derive(Debug)]
pub struct OffThreadLogger<H, C> {
    handler: H,
    offloader_handle: Option<OffloaderHandle<C, !>>,
    service_handle: Option<thread::JoinHandle<()>>,
}

impl<H, C> OffThreadLogger<H, C> {
    /// Creates an `OffThreadLogger` from its parts.
    ///
    /// `handler` is the fully-enriched logging chain that events flow
    /// through.
    /// `offloader_handle` is a separate handle to the same offloader, used
    /// only to deliver the shutdown event on drop.
    /// `service_handle` is the worker's [`JoinHandle`].
    ///
    /// Callers who need a non-builder configuration can construct the three
    /// parts manually and compose them here.
    /// See the [type-level documentation] for the decomposing pattern that
    /// extracts the parts again.
    ///
    /// [type-level documentation]: Self#decomposing
    /// [`JoinHandle`]: std::thread::JoinHandle
    /// [`OffThreadLoggerBuilder::build`]: crate::OffThreadLoggerBuilder::build
    #[inline]
    #[must_use]
    pub(crate) const fn new(
        handler: H,
        offloader_handle: OffloaderHandle<C, !>,
        service_handle: thread::JoinHandle<()>,
    ) -> Self {
        Self {
            handler,
            offloader_handle: Some(offloader_handle),
            service_handle: Some(service_handle),
        }
    }

    /// Returns the wrapped logging chain.
    ///
    /// Equivalent to [`polog::Logger::handler_ref`] but available without
    /// importing the trait.
    /// Prefer `handler_ref` in generic code.
    /// Use this method when the concrete `OffThreadLogger` type is already in
    /// scope.
    ///
    /// See the [type-level documentation] for the caveat about clones
    /// surviving past shutdown.
    ///
    /// [type-level documentation]: Self#handler-clones-after-shutdown
    #[inline]
    pub const fn handler(&self) -> &H {
        &self.handler
    }

    /// Returns a reference to the shutdown signalling handle, if it has not
    /// been taken.
    ///
    /// The handle is present for the whole lifetime of the logger unless
    /// [`take_offloader_handle`] is called.
    /// See the [type-level documentation] for the decomposing pattern and
    /// its effect on [`Drop`].
    ///
    /// [`take_offloader_handle`]: Self::take_offloader_handle
    /// [type-level documentation]: Self#decomposing
    #[inline]
    pub const fn offloader_handle(&self) -> Option<&OffloaderHandle<C, !>> {
        self.offloader_handle.as_ref()
    }

    /// Removes and returns the shutdown signalling handle.
    ///
    /// Intended for decomposing the logger.
    /// See the [type-level documentation] for the full contract.
    /// In particular, always take both this handle and
    /// [`take_service_handle`] if the goal is to control shutdown timing.
    ///
    /// # Interaction with `Drop`
    ///
    /// After this method returns `Some`, [`Drop`] skips both the shutdown
    /// signal and the join, because the join would deadlock waiting for a
    /// worker that has not been told to exit.
    /// The worker therefore exits only when the taken handle is dropped
    /// (which closes the channel), and is not joined.
    ///
    /// [`take_service_handle`]: Self::take_service_handle
    /// [type-level documentation]: Self#decomposing
    #[inline]
    pub const fn take_offloader_handle(&mut self) -> Option<OffloaderHandle<C, !>> {
        self.offloader_handle.take()
    }

    /// Returns a reference to the worker's `JoinHandle`, if it has not been
    /// taken.
    ///
    /// Useful for liveness monitoring: `is_finished()` reports whether the
    /// worker has already exited (for example, because it panicked).
    ///
    /// The handle is present for the whole lifetime of the logger unless
    /// [`take_service_handle`] is called.
    ///
    /// [`take_service_handle`]: Self::take_service_handle
    #[inline]
    pub const fn service_handle(&self) -> Option<&thread::JoinHandle<()>> {
        self.service_handle.as_ref()
    }

    /// Removes and returns the worker's `JoinHandle`.
    ///
    /// Intended for decomposing the logger.
    /// See the [type-level documentation] for the full contract.
    /// In particular, always take both this handle and
    /// [`take_offloader_handle`] if the goal is to control shutdown timing.
    ///
    /// # Interaction with `Drop`
    ///
    /// After this method returns `Some`, [`Drop`] skips the join.
    /// The shutdown event is still sent by `Drop`, so the worker exits on
    /// its own.
    /// The caller owns the `JoinHandle` and can join it at their chosen time.
    ///
    /// [`take_offloader_handle`]: Self::take_offloader_handle
    /// [type-level documentation]: Self#decomposing
    #[inline]
    pub const fn take_service_handle(&mut self) -> Option<thread::JoinHandle<()>> {
        self.service_handle.take()
    }
}

impl<H, C> Drop for OffThreadLogger<H, C> {
    /// Signals the worker to exit, then joins it.
    ///
    /// See the [type-level documentation] for the full shutdown contract and
    /// for the cost of dropping this logger in an async context.
    ///
    /// If [`take_offloader_handle`] has been called, this method does
    /// nothing: no shutdown event is sent, and no join is attempted.
    /// If only [`take_service_handle`] has been called, this method still
    /// sends the shutdown event but skips the join.
    ///
    /// [`take_offloader_handle`]: Self::take_offloader_handle
    /// [`take_service_handle`]: Self::take_service_handle
    /// [type-level documentation]: Self#shutdown
    #[inline]
    fn drop(&mut self) {
        let Some(offloader_handle) = self.offloader_handle.take() else {
            return;
        };

        offloader_handle.stop();

        let Some(handle) = self.service_handle.take() else {
            return;
        };

        // Best-effort join.
        // A worker panic has already been observed by the runtime.
        // Re-panicking here would abort during unwinding.
        drop(handle.join());
    }
}

impl<H, C> Logger for OffThreadLogger<H, C>
where
    H: SharedHandler<()>,
{
    type Handler = H;

    #[inline]
    fn handler_ref(&self) -> &Self::Handler {
        &self.handler
    }

    #[inline]
    fn handler_mut(&mut self) -> &mut Self::Handler {
        &mut self.handler
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
