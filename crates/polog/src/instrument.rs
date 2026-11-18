use core::pin::Pin;
use core::task::{Context, Poll};

use crate::{
    DeferredSpan, DetachedLogger, DynHandler, LogArgs, Logger, LoggerExt, OwningSpan, SharedHandler,
};

/// A future wrapper that emits per-poll lifecycle events for a
/// [`SpanKind::Detached`] span.
///
/// [`PollInstrumented`] emits [`Handler::enter_poll_span`] before every poll,
/// then [`Handler::pending_poll_span`] or [`Handler::ready_poll_span`] after
/// the poll returns.
/// The owned [`OwningSpan`] emits the poll span's `enter_span` and
/// `exit_span` events.
///
/// The poll span is always reported as [`SpanKind::Detached`], because the
/// returned future may be spawned and outlive the span that created it.
/// Even if the caller awaits the future in place, `PollInstrumented` cannot
/// prove this from the type system.
/// Reporting `Detached` is the safe, conservative classification.
///
/// If per-poll visibility is not needed, use [`LoggerExt::borrowing_span`] to
/// obtain a [`SpanKind::Nested`] span with strict-ordering guarantees.
///
/// `PollInstrumented` has no `Drop` impl of its own.
/// The poll span exit is driven entirely by [`OwningSpan`]'s drop, which fires
/// when the wrapper is dropped regardless of whether the inner future completed
/// or was cancelled.
///
/// [`SpanKind::Detached`]: crate::SpanKind::Detached
/// [`Handler::enter_poll_span`]: crate::DynHandler::enter_poll_span
/// [`Handler::pending_poll_span`]: crate::DynHandler::pending_poll_span
/// [`Handler::ready_poll_span`]: crate::DynHandler::ready_poll_span
/// [`SpanKind::Nested`]: crate::SpanKind::Nested
#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct PollInstrumented<F, H>
where
    H: DynHandler<()>,
{
    // Drop order matters: future must be dropped before span.
    future: F,
    span: OwningSpan<H>,
}

/// Extension trait for wrapping a future factory with poll instrumentation.
pub trait PollInstrument: Sized {
    /// Wraps this future factory with poll instrumentation.
    ///
    /// `span` is materialized as a [`SpanKind::Detached`] span.
    /// The factory receives a [`DetachedLogger`] that shares the poll span's
    /// handler and [`SpanId`], and returns the future to instrument.
    /// Events and child spans created through the context are attributed to the
    /// poll span.
    ///
    /// # Factory shape
    ///
    /// The factory is `FnOnce(DetachedLogger<H>) -> F`, not a plain
    /// `F: Future`, because the per-poll span's logger only exists *after* the
    /// deferred span has been materialized.
    /// The factory receives a [`DetachedLogger`] sharing the poll span's
    /// handler and [`SpanId`], and returns the future to instrument.
    /// Events logged through that logger and child spans created from it are
    /// attributed to the poll span, and every `poll` call emits an
    /// `enter_poll_span` / `pending_poll_span` / `ready_poll_span` pair around
    /// the inner future's `poll` (see [`PollInstrumented`]).
    ///
    /// This shape lets an ordinary helper that accepts `&mut L where L: Logger`
    /// be driven inside an instrumented future:
    /// the helper is called with the factory's [`DetachedLogger`], and the
    /// instrumented span's identity flows through it.
    /// A plain `F: Future` would not carry the span context into the helper.
    ///
    /// ```rust,ignore
    /// let deferred = polog::deferred_info_span!(&mut logger, "task"; task_id);
    /// tokio::task::spawn(
    ///     (|mut logger| async move {
    ///         do_async_work(&mut logger).await
    ///     })
    ///     .poll_instrument(deferred),
    /// );
    /// ```
    ///
    /// [`DetachedLogger`]: crate::DetachedLogger
    /// [`SpanId`]: crate::SpanId
    /// [`SpanKind::Detached`]: crate::SpanKind::Detached
    #[inline]
    fn poll_instrument<'a, F, H, LazyArgs, Args>(
        self,
        span: DeferredSpan<H, LazyArgs>,
    ) -> PollInstrumented<F, H>
    where
        Self: FnOnce(DetachedLogger<H>) -> F,
        H: SharedHandler<()>,
        LazyArgs: FnOnce() -> Args,
        Args: LogArgs<'a>,
    {
        PollInstrumented::new(span, self)
    }
}

impl<F> PollInstrument for F where F: Sized {}

impl<F, H> PollInstrumented<F, H>
where
    H: SharedHandler<()>,
{
    /// Creates a poll-instrumented future from a deferred span and a future
    /// factory.
    ///
    /// The deferred span is materialized with [`SpanKind::Detached`]:
    /// [`Handler::try_enable`] is called with its descriptor, the lazy
    /// arguments are evaluated, and [`Handler::enter_span`] is emitted if
    /// the span is enabled.
    /// The poll span inherits the deferred span's descriptor and parent
    /// [`SpanId`] and receives a fresh [`SpanId`] so backends can distinguish
    /// concurrent instrumented futures by span.
    ///
    /// The factory receives a [`DetachedLogger`] built from the poll span.
    /// Its [`SpanId`] is the poll span's, so events logged and child spans
    /// created through the context are attributed to the poll span.
    ///
    /// This is the imperative form of [`PollInstrument::poll_instrument`]:
    /// `PollInstrumented::new(span, f)` is equivalent to
    /// `f.poll_instrument(span)` with the arguments reversed.
    /// Use whichever reads more naturally at the call site.
    ///
    /// [`PollInstrument::poll_instrument`]: crate::PollInstrument::poll_instrument
    ///
    /// [`SpanId`]: crate::SpanId
    /// [`SpanKind::Detached`]: crate::SpanKind::Detached
    /// [`Handler::try_enable`]: crate::Handler::try_enable
    /// [`Handler::enter_span`]: crate::Handler::enter_span
    #[inline]
    pub fn new<'a, LazyArgs, Args, Func>(span: DeferredSpan<H, LazyArgs>, future_func: Func) -> Self
    where
        LazyArgs: FnOnce() -> Args,
        Args: LogArgs<'a>,
        Func: FnOnce(DetachedLogger<H>) -> F,
    {
        let span = span.enter();

        // `PollInstrumented` owns an `OwningSpan<H>` whose `Drop` emits
        // `exit_span(SpanKind::Detached, ...)`.
        // `PollInstrumented` itself has no `Drop`.
        // The span exit is driven entirely by `OwningSpan`'s drop, which
        // guarantees `enter_span(Poll)` and `exit_span(Poll)` are paired
        // exactly once regardless of whether the future completed or was
        // cancelled.
        //
        // The factory receives a `DetachedLogger<H>` created from that
        // `OwningSpan`.
        // It clones the handler and shares the poll span's
        // `SpanId`, so any events or child spans created inside the future
        // are correctly attributed to the poll span.

        let detached = span.detach();
        let future = future_func(detached);
        Self { future, span }
    }
}

impl<F, H> Future for PollInstrumented<F, H>
where
    F: Future,
    H: SharedHandler<()>,
{
    type Output = F::Output;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: The data of the `&mut Self` never moved out.
        let this = unsafe { self.get_unchecked_mut() };

        let descriptor = this.span.descriptor();
        let span_id = this.span.id();
        if let Some(descriptor) = descriptor
            && let Some(span_id) = span_id
        {
            this.span
                .handler_mut()
                .enter_poll_span(descriptor, span_id, ());
        }

        // SAFETY: The data of the `&mut Self` never moved out.
        let future = unsafe { Pin::new_unchecked(&mut this.future) };
        let result = future.poll(cx);

        if let Some(descriptor) = descriptor
            && let Some(span_id) = span_id
        {
            let handler = this.span.handler_mut();
            match &result {
                Poll::Pending => handler.pending_poll_span(descriptor, span_id, ()),
                Poll::Ready(_) => handler.ready_poll_span(descriptor, span_id, ()),
            }
        }

        result
    }
}
