//! Offloading middleware: moves all logging work onto a dedicated worker
//! thread.

use alloc::boxed::Box;
use core::fmt::{self, Debug, Formatter};
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::ops::ControlFlow;
use core::sync::atomic::{self, AtomicBool, AtomicUsize};
use core::time::Duration;
// Hack to prevent `clippy::std_instead_of_core` failure.
// `std::io::ErrorKind` moved to `core` in nightly.
// `clippy::std_instead_of_core` works only if disabled on crate level.
// See: https://github.com/rust-lang/rust/issues/154046.
use std as core_or_std;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use polog::{
    BaseHandler, ContextArgs, Descriptor, DynHandler, Handler, Level, LevelFilter, LogArgs,
    SharedHandler, SpanId, SpanIdGenerator, SpanKind, StaticLogArgs,
};

use crate::args::{TypeErasedArcLogArgs, TypeErasedArgs};
use crate::{
    BaseExternalHandler, ExternalArgs, ExternalDescriptor, ExternalHandler, TypeErasedExternalArgs,
    TypeErasedExternalDescriptor,
};

/// A [`Handler`] middleware that offloads all logging work onto a dedicated
/// worker thread.
///
/// The worker thread owns the wrapped handler exclusively and performs all
/// real work: the full-context `is_enabled` check, argument formatting, I/O.
///
/// # Filtering
///
/// [`Handler::try_enable`] on `Offload` always accepts the event.
/// Compose with a filter *outside* the offload so disabled call sites never pay
/// for argument construction or a channel round-trip:
///
/// ```rust,ignore
/// let handler = Offload::new(backend);
/// let handler = Filter::new(handler, MaxLevel(LevelFilter::Debug));
/// ```
///
/// The *full* `is_enabled` check — with whatever the enrichers added on the
/// worker side — is still performed inside the worker, before the job is
/// forwarded to the wrapped handler.
/// This matches the semantics of a non-offloaded chain.
///
/// # Shutdown
///
/// Dropping the last clone of this middleware closes the channel, drains any
/// remaining jobs, invokes [`BaseHandler::flush_blocking`] on the inner
/// handler, and joins the worker thread.
/// If the caller panics, [`BaseHandler::flush_on_panic`] is processed
/// synchronously (blocking, per request) so the inner handler gets a
/// best-effort flush before the process aborts.
///
/// # Panic behavior
///
/// [`BaseHandler::should_panic_on_bug`] and [`BaseHandler::panic`] use
/// the default implementations, not the wrapped backend's overrides.
///
/// This is intentional. Both methods run on the **caller** thread, not the
/// worker thread, and a backend running on the worker cannot influence
/// the caller's panic decision.
pub struct OffloaderHandle<Context, CustomEvent> {
    sender: crossbeam_channel::Sender<Event<Context, CustomEvent>>,
    inner: Arc<OffloaderInner<Context, CustomEvent>>,
}

/// An internal state container for one [`OffloaderHandle`] /
/// [`OffloaderService`] pair.
///
/// Holds the channel endpoints so that dropping the last clone of
/// [`OffloaderHandle`] closes the sender and lets the worker exit.
struct OffloaderInner<Context, CustomEvent> {
    receiver: crossbeam_channel::Receiver<Event<Context, CustomEvent>>,
    num_events_dropped: AtomicUsize,
    is_shutdown: AtomicBool,
    span_id_generator: Arc<SpanIdGenerator>,
    backpressure_policy: OffloaderBackpressurePolicy,
    flush_interval: Option<Duration>,
    _context: PhantomData<fn(Context)>,
}

/// The service side of the offload middleware.
///
/// `OffloaderService` owns the wrapped handler and runs the worker loop.
/// The [`OffloaderHandle`] returned alongside it is the frontend that call
/// sites use.
/// The service never touches the caller's thread except through the channel.
///
/// # Startup
///
/// Move the service to a dedicated thread with [`OffloaderService::spawn`],
/// or drive it on the current thread with
/// [`OffloaderService::run_blocking`].
/// Both methods consume the service and will not return until the worker
/// stops.
///
/// # Shutdown
///
/// When every [`OffloaderHandle`] clone has been dropped, the channel closes,
/// the worker drains any remaining events, invokes
/// [`BaseHandler::flush_blocking`] on the wrapped handler, and returns.
/// If the returned `JoinHandle` from `spawn` is retained and joined, the caller
/// can observe the worker's completion and panic status.
/// Otherwise the thread detaches.
///
/// [`BaseHandler::flush_blocking`]: polog::BaseHandler::flush_blocking
pub struct OffloaderService<H, Context, CustomEvent, CustomHandler> {
    handler: H,
    inner: Arc<OffloaderInner<Context, CustomEvent>>,
    custom_handler: CustomHandler,
}

/// The bounded or unbounded channel capacity of an [`OffloaderHandle`].
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum OffloaderCapacity {
    /// A bounded channel with the given capacity.
    ///
    /// Producers may block or drop according to the configured
    /// [`OffloaderBackpressurePolicy`] when the channel is full.
    Bounded(usize),

    /// An unbounded channel.
    ///
    /// Producers never block.
    /// Memory grows with the backlog.
    /// Use only when the producer is guaranteed to outpace the consumer, or
    /// when dropping events is unacceptable and the workload is bounded.
    Unbounded,
}

/// A policy producers follow when a bounded channel is full.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum OffloaderBackpressurePolicy {
    /// Drop the incoming event if the channel is still full after waiting
    /// `timeout`.
    ///
    /// `Duration::ZERO` drops immediately.
    DropNewest {
        /// How long to wait for the channel to free a slot before dropping
        /// the incoming event.
        timeout: Duration,
    },

    /// Drop the oldest queued event if the channel is still full after
    /// waiting `timeout`.
    ///
    /// `Duration::ZERO` drops the oldest immediately.
    DropOldest {
        /// How long to wait for the channel to free a slot before dropping
        /// the oldest queued event.
        timeout: Duration,
    },

    /// Block the producer until the channel has room.
    ///
    /// Prefer this when dropped events are worse than a slow producer.
    Block,
}

/// Creates an offload middleware pair: a frontend [`OffloaderHandle`] and a
/// backend [`OffloaderService`].
///
/// The `span_id_generator` is shared with the wrapped handler so that span
/// IDs remain unique across the `polog` chain.
///
/// `capacity` and `backpressure_policy` configure the channel.
/// `flush_interval` controls periodic flushing on the worker.
/// `None` disables it.
/// `custom_handler` receives every `CustomEvent` sent through
/// [`OffloaderHandle::send_custom`].
///
/// See [`OffloaderService`] for the worker lifecycle.
#[inline]
pub fn new_offloader<H, Context, CustomEvent, CustomHandler>(
    handler: H,
    span_id_generator: Arc<SpanIdGenerator>,
    capacity: OffloaderCapacity,
    backpressure_policy: OffloaderBackpressurePolicy,
    flush_interval: Option<Duration>,
    custom_handler: CustomHandler,
) -> (
    OffloaderHandle<Context, CustomEvent>,
    OffloaderService<H, Context, CustomEvent, CustomHandler>,
)
where
    CustomHandler: FnMut(&mut H, CustomEvent) -> ControlFlow<(), ()>,
{
    let (sender, receiver) = match capacity {
        OffloaderCapacity::Bounded(capacity) => crossbeam_channel::bounded(capacity),
        OffloaderCapacity::Unbounded => crossbeam_channel::unbounded(),
    };
    let inner = Arc::new(OffloaderInner::new(
        receiver,
        span_id_generator,
        backpressure_policy,
        flush_interval,
    ));
    let handle = OffloaderHandle::new(sender, Arc::clone(&inner));
    let service = OffloaderService::new(handler, inner, custom_handler);
    (handle, service)
}

impl<H, Context, CustomEvent, CustomHandler>
    OffloaderService<H, Context, CustomEvent, CustomHandler>
{
    #[inline]
    const fn new(
        handler: H,
        inner: Arc<OffloaderInner<Context, CustomEvent>>,
        custom_handler: CustomHandler,
    ) -> Self {
        Self {
            handler,
            inner,
            custom_handler,
        }
    }

    /// Returns a reference to the wrapped handler.
    #[inline]
    #[must_use]
    pub const fn handler(&self) -> &H {
        &self.handler
    }

    /// Returns a mutable reference to the wrapped handler.
    #[inline]
    #[must_use]
    pub const fn handler_mut(&mut self) -> &mut H {
        &mut self.handler
    }

    /// Consumes the middleware, returning the wrapped handler.
    #[inline]
    #[must_use]
    pub fn drop_into_handler(self) -> H {
        self.inner
            .is_shutdown
            .store(true, atomic::Ordering::Release);

        // Do not add new field here without properly dropping them below.
        let Self {
            handler: _,
            inner: _,
            custom_handler: _,
        } = &self;

        let mut this = ManuallyDrop::new(self);

        // SAFETY: `ManuallyDrop` never runs the inner destructor.
        // SAFETY: The read is performed once per field.
        unsafe {
            let handler = core::ptr::read(&raw const this.handler);
            core::ptr::drop_in_place(&raw mut this.inner);
            core::ptr::drop_in_place(&raw mut this.custom_handler);
            handler
        }
    }

    /// # Errors
    ///
    /// Returns an error if spawning the task fails.
    #[inline]
    pub fn spawn(mut self) -> Result<std::thread::JoinHandle<()>, core_or_std::io::Error>
    where
        H: 'static + Send + ExternalHandler<Context> + Handler<()>,
        Context: ContextArgs,
        CustomEvent: 'static + Send,
        CustomHandler: 'static + Send + FnMut(&mut H, CustomEvent) -> ControlFlow<(), ()>,
    {
        std::thread::Builder::new()
            .name("polog-offload".into())
            .spawn(move || self.run_blocking())
    }

    /// Runs the worker loop on the calling thread.
    ///
    /// Blocks until every [`OffloaderHandle`] clone has been dropped or
    /// [`OffloaderHandle::stop`] has been called, and then returns.
    /// The wrapped handler is flushed with [`BaseHandler::flush_blocking`]
    /// both periodically (if `flush_interval` is set) and once on shutdown.
    ///
    /// Use [`OffloaderService::spawn`] to run the loop on a dedicated
    /// thread instead.
    ///
    /// [`BaseHandler::flush_blocking`]: polog::BaseHandler::flush_blocking
    #[expect(clippy::too_many_lines, reason = "mostly one long match")]
    #[inline]
    pub fn run_blocking(&mut self)
    where
        H: ExternalHandler<Context> + Handler<()>,
        Context: ContextArgs,
        CustomHandler: FnMut(&mut H, CustomEvent) -> ControlFlow<(), ()>,
    {
        let mut disabled_spans = HashSet::new();

        let mut optional_flush_timeout = self
            .inner
            .flush_interval
            .and_then(|interval| Instant::now().checked_add(interval));

        loop {
            if self.inner.is_shutdown.load(atomic::Ordering::Acquire) {
                break;
            }

            let event = match optional_flush_timeout {
                None => match self.inner.receiver.recv() {
                    Ok(event) => event,
                    Err(crossbeam_channel::RecvError {}) => break,
                },
                Some(flush_timeout) => match self.inner.receiver.recv_deadline(flush_timeout) {
                    Ok(event) => event,
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        optional_flush_timeout = self
                            .inner
                            .flush_interval
                            .and_then(|interval| Instant::now().checked_add(interval));
                        H::flush_blocking(&mut self.handler);
                        continue;
                    }
                },
            };

            self.check_dropped_events();

            match event {
                Event::Log {
                    descriptor,
                    span_id,
                    args,
                    context,
                } => {
                    if let Some(context) = self.handler.try_enable(descriptor, context) {
                        <H as Handler<Context>>::log_static(
                            &mut self.handler,
                            descriptor,
                            args,
                            span_id,
                            context,
                        );
                    }
                }

                Event::EnterSpan {
                    descriptor,
                    args,
                    parent_span_id,
                    span_id,
                    span_kind,
                    context,
                } => {
                    if let Some(context) = self.handler.try_enable(descriptor, context) {
                        <H as Handler<Context>>::enter_span_static(
                            &mut self.handler,
                            descriptor,
                            args,
                            parent_span_id,
                            span_id,
                            span_kind,
                            context,
                        );
                    } else {
                        let _is_newly_inserted = disabled_spans.insert(span_id);
                    }
                }

                Event::ExitSpan {
                    descriptor,
                    span_id,
                    span_kind,
                    context,
                } => {
                    if !disabled_spans.remove(&span_id) {
                        self.handler
                            .exit_span(descriptor, span_id, span_kind, context);
                    }
                }

                Event::EnterPollSpan {
                    descriptor,
                    span_id,
                    context,
                } => {
                    if !disabled_spans.contains(&span_id) {
                        self.handler.enter_poll_span(descriptor, span_id, context);
                    }
                }

                Event::PendingPollSpan {
                    descriptor,
                    span_id,
                    context,
                } => {
                    if !disabled_spans.contains(&span_id) {
                        self.handler.pending_poll_span(descriptor, span_id, context);
                    }
                }

                Event::ReadyPollSpan {
                    descriptor,
                    span_id,
                    context,
                } => {
                    if !disabled_spans.contains(&span_id) {
                        self.handler.ready_poll_span(descriptor, span_id, context);
                    }
                }

                Event::LogExternal {
                    descriptor,
                    args,
                    span_id,
                    context,
                } => {
                    if let Some(context) = self.handler.try_enable_external(&*descriptor, context) {
                        <H as ExternalHandler<Context>>::log_external(
                            &mut self.handler,
                            &*descriptor,
                            &args,
                            span_id,
                            context,
                        );
                    }
                }

                Event::NewSpanExternal {
                    descriptor,
                    args,
                    parent_span_id,
                    span_id,
                    context,
                } => {
                    if let Some(context) = self.handler.try_enable_external(&*descriptor, context) {
                        <H as ExternalHandler<Context>>::new_span_external(
                            &mut self.handler,
                            &*descriptor,
                            &args,
                            parent_span_id,
                            span_id,
                            context,
                        );
                    } else {
                        let _is_newly_inserted = disabled_spans.insert(span_id);
                    }
                }

                Event::EnterSpanExternal {
                    descriptor,
                    args,
                    parent_span_id,
                    span_id,
                    context,
                } => {
                    if let Some(context) = self.handler.try_enable_external(&*descriptor, context) {
                        <H as ExternalHandler<Context>>::enter_span_external(
                            &mut self.handler,
                            &*descriptor,
                            &args,
                            parent_span_id,
                            span_id,
                            context,
                        );
                    } else {
                        let _is_newly_inserted = disabled_spans.insert(span_id);
                    }
                }

                Event::ExitSpanExternal {
                    descriptor,
                    span_id,
                    context,
                } => {
                    if !disabled_spans.remove(&span_id) {
                        self.handler
                            .exit_span_external(&*descriptor, span_id, context);
                    }
                }

                Event::CloseSpanExternal {
                    descriptor,
                    span_id,
                    context,
                } => {
                    if !disabled_spans.remove(&span_id) {
                        self.handler
                            .close_span_external(&*descriptor, span_id, context);
                    }
                }

                Event::RecordSpanExternal {
                    descriptor,
                    args,
                    span_id,
                    context,
                } => {
                    if !disabled_spans.contains(&span_id) {
                        self.handler
                            .record_span_external(&*descriptor, &args, span_id, context);
                    }
                }

                Event::LinkSpanExternal {
                    descriptor,
                    followed_span_id,
                    follower_span_id,
                    context,
                } => {
                    if !disabled_spans.contains(&follower_span_id) {
                        self.handler.link_span_external(
                            &*descriptor,
                            followed_span_id,
                            follower_span_id,
                            context,
                        );
                    }
                }

                Event::FlushBlocking { flushed } => {
                    self.handler.flush_blocking();
                    match flushed.send(()) {
                        Ok(()) | Err(oneshot::SendError { .. }) => {}
                    }
                }

                Event::FlushOnPanic { flushed } => {
                    self.handler.flush_on_panic();
                    match flushed.send(()) {
                        Ok(()) | Err(oneshot::SendError { .. }) => {}
                    }
                }

                Event::Shutdown => break,

                Event::Custom(event) => match (self.custom_handler)(&mut self.handler, event) {
                    ControlFlow::Continue(()) => {}
                    ControlFlow::Break(()) => break,
                },
            }
        }
        self.check_dropped_events();

        // Graceful shutdown: drain the inner handler's own buffers too.
        self.handler.flush_blocking();
    }

    /// Reports any events dropped since the last call as a single
    /// synthesized `ERROR` record, then resets the drop counter.
    ///
    /// Called automatically by [`OffloaderService::run_blocking`].
    /// Manual invocation is not required.
    #[inline]
    pub fn check_dropped_events(&mut self)
    where
        H: Handler<()>,
    {
        let num_events_dropped = self
            .inner
            .num_events_dropped
            .swap(0, atomic::Ordering::Relaxed);

        if num_events_dropped > 0 {
            // `H` doesn't implement SharedHandler, so it is not implementing
            // `Logger`, so we can't use `polog::warn` here.
            let descriptor = polog::log_descriptor_ref!(
                Level::Error,
                "Log events dropped in offloader",
                keys: &["num_events_dropped"],
            );
            let args = polog::locate_and_parse_values!(num_events_dropped);
            if let Some(context) = self.handler.try_enable(descriptor, ()) {
                self.handler.log(descriptor, args, None, context);
            }
        }
    }
}

impl<Context, CustomEvent> OffloaderHandle<Context, CustomEvent> {
    #[inline]
    const fn new(
        sender: crossbeam_channel::Sender<Event<Context, CustomEvent>>,
        inner: Arc<OffloaderInner<Context, CustomEvent>>,
    ) -> Self {
        Self { sender, inner }
    }

    /// Signals the worker to stop after draining queued events.
    ///
    /// Does not block: the worker may still be running when this returns.
    /// To synchronize with the worker's completion, join the `JoinHandle`
    /// returned by [`OffloaderService::spawn`].
    #[inline]
    pub fn stop(&self) {
        // `is_shutdown` set to true ensures that the service will stop on next
        // iteration.
        self.inner
            .is_shutdown
            .store(true, atomic::Ordering::Release);
        // `try_send` ensures that the service is woken when it is already
        // waiting for an event.
        let result = self.sender.try_send(Event::Shutdown);
        #[expect(clippy::match_same_arms, reason = "comments are important")]
        match result {
            // Shutdown message is sent.
            Ok(()) => {}
            // The worker is gone. Nothing to do.
            Err(crossbeam_channel::TrySendError::Disconnected(..)) => {}
            // The queue is full. The `is_shutdown` would trigger shutdown.
            Err(crossbeam_channel::TrySendError::Full(..)) => {}
        }
    }

    /// Sends a `CustomEvent` to the worker.
    ///
    /// The worker invokes the `custom_handler` closure passed to
    /// [`new_offloader`] with the wrapped handler and the event.
    ///
    /// `backpressure_policy` overrides the handle's default policy for this
    /// send only.
    /// `None` uses the default.
    #[inline]
    pub fn send_custom(
        &self,
        event: CustomEvent,
        backpressure_policy: Option<OffloaderBackpressurePolicy>,
    ) {
        self.send_with_backpressure(Event::Custom(event), backpressure_policy);
    }

    #[inline]
    fn notify_event_dropped(&self) {
        let _ = self
            .inner
            .num_events_dropped
            .fetch_add(1, atomic::Ordering::Relaxed);
    }

    #[inline]
    fn send_with_backpressure(
        &self,
        event: Event<Context, CustomEvent>,
        backpressure_policy: Option<OffloaderBackpressurePolicy>,
    ) {
        enum Outcome<Context, CustomEvent> {
            Sent,
            DropNewest,
            BumpOldest(Event<Context, CustomEvent>),
            Disconnected,
        }

        // Prevent filling the channel if the worker is gone.
        if self.inner.is_shutdown.load(atomic::Ordering::Acquire) {
            self.notify_event_dropped();
            return;
        }

        // Special case for direct handoff channel.
        if self.sender.capacity() == Some(0) {
            let result = self.sender.send(event);
            match result {
                Ok(()) => {}
                // The worker is gone. Drop the result silently.
                Err(crossbeam_channel::SendError(..)) => self.notify_event_dropped(),
            }
            return;
        }

        let backpressure_policy = backpressure_policy.unwrap_or(self.inner.backpressure_policy);
        let action = match backpressure_policy {
            OffloaderBackpressurePolicy::DropNewest {
                timeout: Duration::ZERO,
            } => {
                let result = self.sender.try_send(event);
                match result {
                    Ok(()) => Outcome::Sent,
                    // The worker is gone. Drop the result silently.
                    Err(crossbeam_channel::TrySendError::Disconnected(..)) => Outcome::Disconnected,
                    // The queue is full. Recover the event back.
                    Err(crossbeam_channel::TrySendError::Full(..)) => Outcome::DropNewest,
                }
            }

            OffloaderBackpressurePolicy::DropOldest {
                timeout: Duration::ZERO,
            } => {
                let result = self.sender.try_send(event);
                match result {
                    Ok(()) => Outcome::Sent,
                    // The worker is gone. Drop the result silently.
                    Err(crossbeam_channel::TrySendError::Disconnected(..)) => Outcome::Disconnected,
                    // The queue is full. Recover the event back.
                    Err(crossbeam_channel::TrySendError::Full(event)) => Outcome::BumpOldest(event),
                }
            }

            OffloaderBackpressurePolicy::DropNewest { timeout } => {
                let result = self.sender.send_timeout(event, timeout);
                match result {
                    Ok(()) => Outcome::Sent,
                    // The worker is gone. Drop the result silently.
                    Err(crossbeam_channel::SendTimeoutError::Disconnected(..)) => {
                        Outcome::Disconnected
                    }
                    // The queue is full. Recover the event back.
                    Err(crossbeam_channel::SendTimeoutError::Timeout(..)) => Outcome::DropNewest,
                }
            }

            OffloaderBackpressurePolicy::DropOldest { timeout } => {
                let result = self.sender.send_timeout(event, timeout);
                match result {
                    Ok(()) => Outcome::Sent,
                    // The worker is gone. Drop the result silently.
                    Err(crossbeam_channel::SendTimeoutError::Disconnected(..)) => {
                        Outcome::Disconnected
                    }
                    // The queue is full. Recover the event back.
                    Err(crossbeam_channel::SendTimeoutError::Timeout(event)) => {
                        Outcome::BumpOldest(event)
                    }
                }
            }

            OffloaderBackpressurePolicy::Block => {
                let result = self.sender.send(event);
                match result {
                    Ok(()) => Outcome::Sent,
                    // The worker is gone. Drop the result silently.
                    Err(crossbeam_channel::SendError(..)) => Outcome::Disconnected,
                }
            }
        };

        match action {
            Outcome::Sent => {}
            Outcome::DropNewest | Outcome::Disconnected => self.notify_event_dropped(),
            Outcome::BumpOldest(mut event) => {
                loop {
                    let oldest = self.inner.receiver.try_recv();
                    if oldest.is_ok() {
                        self.notify_event_dropped();
                    }
                    let result = self.sender.try_send(event);
                    #[expect(clippy::match_same_arms, reason = "behavior comments")]
                    match result {
                        Ok(()) => return,
                        // The worker is gone. Drop the result silently.
                        Err(crossbeam_channel::TrySendError::Disconnected(..)) => return,
                        // Someone just stole the place we've just freed.
                        // Try again.
                        Err(crossbeam_channel::TrySendError::Full(recovered)) => {
                            event = recovered;
                        }
                    };
                }
            }
        }
    }

    #[inline]
    fn flush_blocking_by_ref(&self) {
        // Prevent filling the channel if the worker is gone.
        if self.inner.is_shutdown.load(atomic::Ordering::Acquire) {
            self.notify_event_dropped();
            return;
        }

        let (flushed_tx, flushed_rx) = oneshot::channel();
        let result = self.sender.send(Event::FlushBlocking {
            flushed: flushed_tx,
        });
        #[expect(clippy::single_match_else, reason = "explicit error matching")]
        match result {
            Ok(()) => {}
            // The worker is gone. Drop the result silently.
            Err(crossbeam_channel::SendError(..)) => {
                self.notify_event_dropped();
                return;
            }
        }

        let result = flushed_rx.recv();
        #[expect(clippy::single_match, reason = "explicit error matching")]
        #[expect(clippy::match_same_arms, reason = "behavior comments")]
        match result {
            Ok(()) => {}
            // The worker is gone. Nothing to wait.
            Err(oneshot::RecvError { .. }) => {}
        }
    }

    #[inline]
    fn flush_on_panic_by_ref(&self) {
        // Prevent filling the channel if the worker is gone.
        if self.inner.is_shutdown.load(atomic::Ordering::Acquire) {
            self.notify_event_dropped();
            return;
        }

        #[cfg(feature = "offloader-support-tokio")]
        if can_block_in_place() {
            tokio::task::block_in_place(|| self.flush_on_panic_impl());
            return;
        }

        self.flush_on_panic_impl();
    }

    #[inline]
    fn flush_on_panic_impl(&self) {
        let (flushed_tx, flushed_rx) = oneshot::channel();
        let mut msg = Event::FlushOnPanic {
            flushed: flushed_tx,
        };

        if self.sender.capacity() == Some(0) {
            let result = self.sender.send(msg);
            match result {
                Ok(()) => {}
                // The worker is gone. Drop the result silently.
                Err(crossbeam_channel::SendError(..)) => self.notify_event_dropped(),
            }
            return;
        }

        loop {
            let result = self.sender.try_send(msg);
            match result {
                Ok(()) => break,
                // The worker is gone. Drop the result silently.
                Err(crossbeam_channel::TrySendError::Disconnected(..)) => {
                    self.notify_event_dropped();
                    return;
                }
                // Free queue and try again.
                Err(crossbeam_channel::TrySendError::Full(recovered)) => {
                    msg = recovered;
                    let oldest = self.inner.receiver.try_recv();
                    if oldest.is_ok() {
                        self.notify_event_dropped();
                    }
                }
            }
        }

        let result = flushed_rx.recv();
        #[expect(clippy::single_match, reason = "explicit error matching")]
        #[expect(clippy::match_same_arms, reason = "behavior comments")]
        match result {
            Ok(()) => {}
            // The worker is gone. Nothing to wait.
            Err(oneshot::RecvError { .. }) => {}
        }
    }

    #[inline]
    fn exit_span_by_ref(
        &self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    ) {
        self.send_with_backpressure(
            Event::ExitSpan {
                descriptor,
                span_id,
                span_kind,
                context,
            },
            None,
        );
    }

    #[inline]
    fn enter_poll_span_by_ref(
        &self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        self.send_with_backpressure(
            Event::EnterPollSpan {
                descriptor,
                span_id,
                context,
            },
            None,
        );
    }

    #[inline]
    fn pending_poll_span_by_ref(
        &self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        self.send_with_backpressure(
            Event::PendingPollSpan {
                descriptor,
                span_id,
                context,
            },
            None,
        );
    }

    #[inline]
    fn ready_poll_span_by_ref(
        &self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        self.send_with_backpressure(
            Event::ReadyPollSpan {
                descriptor,
                span_id,
                context,
            },
            None,
        );
    }

    #[inline]
    fn log_by_ref<'a, Args>(
        &self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        context: Context,
    ) where
        Args: LogArgs<'a>,
    {
        let args: TypeErasedArcLogArgs = TypeErasedArgs(Arc::new(args.into_sendable()));
        self.send_with_backpressure(
            Event::Log {
                descriptor,
                args,
                span_id,
                context,
            },
            None,
        );
    }

    #[inline]
    fn enter_span_by_ref<'a, Args>(
        &self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    ) where
        Args: LogArgs<'a>,
    {
        let args: TypeErasedArcLogArgs = TypeErasedArgs(Arc::new(args.into_sendable()));
        self.send_with_backpressure(
            Event::EnterSpan {
                descriptor,
                args,
                parent_span_id,
                span_id,
                span_kind,
                context,
            },
            None,
        );
    }

    #[inline]
    fn log_static_by_ref<Args>(
        &self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        context: Context,
    ) where
        Args: StaticLogArgs,
    {
        let args: TypeErasedArcLogArgs = TypeErasedArgs(Arc::new(args));
        self.send_with_backpressure(
            Event::Log {
                descriptor,
                args,
                span_id,
                context,
            },
            None,
        );
    }

    #[inline]
    fn enter_span_static_by_ref<Args>(
        &self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    ) where
        Args: StaticLogArgs,
    {
        let args: TypeErasedArcLogArgs = TypeErasedArgs(Arc::new(args));
        self.send_with_backpressure(
            Event::EnterSpan {
                descriptor,
                args,
                parent_span_id,
                span_id,
                span_kind,
                context,
            },
            None,
        );
    }

    #[inline]
    fn log_external_by_ref<Desc, Args>(
        &self,
        descriptor: &Desc,
        args: &Args,
        span_id: Option<SpanId>,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let descriptor = TypeErasedExternalDescriptor::from_descriptor(&descriptor);
        let args = TypeErasedExternalArgs::from_args(&args);
        self.send_with_backpressure(
            Event::LogExternal {
                descriptor: Box::new(descriptor),
                args,
                span_id,
                context,
            },
            None,
        );
    }

    #[inline]
    fn new_span_external_by_ref<Desc, Args>(
        &self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let descriptor = TypeErasedExternalDescriptor::from_descriptor(&descriptor);
        let args = TypeErasedExternalArgs::from_args(&args);
        self.send_with_backpressure(
            Event::NewSpanExternal {
                descriptor: Box::new(descriptor),
                args,
                parent_span_id,
                span_id,
                context,
            },
            None,
        );
    }

    #[inline]
    fn enter_span_external_by_ref<Desc, Args>(
        &self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let descriptor = TypeErasedExternalDescriptor::from_descriptor(&descriptor);
        let args = TypeErasedExternalArgs::from_args(&args);
        self.send_with_backpressure(
            Event::EnterSpanExternal {
                descriptor: Box::new(descriptor),
                args,
                parent_span_id,
                span_id,
                context,
            },
            None,
        );
    }

    #[inline]
    fn exit_span_external_by_ref<Desc>(&self, descriptor: Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        let descriptor = TypeErasedExternalDescriptor::from_descriptor(&descriptor);
        self.send_with_backpressure(
            Event::ExitSpanExternal {
                descriptor: Box::new(descriptor),
                span_id,
                context,
            },
            None,
        );
    }

    #[inline]
    fn close_span_external_by_ref<Desc>(&self, descriptor: Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        let descriptor = TypeErasedExternalDescriptor::from_descriptor(&descriptor);
        self.send_with_backpressure(
            Event::CloseSpanExternal {
                descriptor: Box::new(descriptor),
                span_id,
                context,
            },
            None,
        );
    }

    #[inline]
    fn record_span_external_by_ref<Desc, Args>(
        &self,
        descriptor: &Desc,
        args: &Args,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let descriptor = TypeErasedExternalDescriptor::from_descriptor(&descriptor);
        let args = TypeErasedExternalArgs::from_args(&args);
        self.send_with_backpressure(
            Event::RecordSpanExternal {
                descriptor: Box::new(descriptor),
                args,
                span_id,
                context,
            },
            None,
        );
    }

    #[expect(clippy::similar_names, reason = "better than alternatives")]
    #[inline]
    fn link_span_external_by_ref<Desc>(
        &self,
        descriptor: Desc,
        followed_span_id: SpanId,
        follower_span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
    {
        let descriptor = TypeErasedExternalDescriptor::from_descriptor(&descriptor);
        self.send_with_backpressure(
            Event::LinkSpanExternal {
                descriptor: Box::new(descriptor),
                followed_span_id,
                follower_span_id,
                context,
            },
            None,
        );
    }
}

impl OffloaderBackpressurePolicy {
    /// Shorthand for `DropNewest { timeout: Duration::ZERO }`.
    pub const DROP_NEWEST: Self = Self::DropNewest {
        timeout: Duration::ZERO,
    };

    /// Shorthand for `DropOldest { timeout: Duration::ZERO }`
    pub const DROP_OLDEST: Self = Self::DropOldest {
        timeout: Duration::ZERO,
    };
}

impl<Context, CustomEvent> OffloaderInner<Context, CustomEvent> {
    #[inline]
    fn new(
        receiver: crossbeam_channel::Receiver<Event<Context, CustomEvent>>,
        span_id_generator: Arc<SpanIdGenerator>,
        backpressure_policy: OffloaderBackpressurePolicy,
        flush_interval: Option<Duration>,
    ) -> Self {
        Self {
            receiver,
            num_events_dropped: AtomicUsize::new(0),
            is_shutdown: AtomicBool::new(false),
            span_id_generator,
            backpressure_policy,
            flush_interval,
            _context: PhantomData,
        }
    }
}

impl<H, Context, CustomEvent, CustomHandler> Drop
    for OffloaderService<H, Context, CustomEvent, CustomHandler>
{
    #[inline]
    fn drop(&mut self) {
        self.inner
            .is_shutdown
            .store(true, atomic::Ordering::Release);
    }
}

impl<Context, CustomEvent> BaseHandler for OffloaderHandle<Context, CustomEvent>
where
    Context: ContextArgs,
{
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        self.inner.span_id_generator.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {
        self.flush_blocking_by_ref();
    }

    #[inline]
    fn flush_on_panic(&mut self) {
        self.flush_on_panic_by_ref();
    }

    // `should_panic_on_bug` and `panic` methods are intentionally left at their
    // defaults.
}

impl<Context, CustomEvent> DynHandler<Context> for OffloaderHandle<Context, CustomEvent>
where
    Context: ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    ) {
        self.exit_span_by_ref(descriptor, span_id, span_kind, context);
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        self.enter_poll_span_by_ref(descriptor, span_id, context);
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        self.pending_poll_span_by_ref(descriptor, span_id, context);
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        self.ready_poll_span_by_ref(descriptor, span_id, context);
    }
}

impl<Context, CustomEvent> Handler<Context> for OffloaderHandle<Context, CustomEvent>
where
    Context: ContextArgs,
{
    type EnrichedContext = Context;

    #[inline]
    fn try_enable(
        &self,
        _descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        // [`Handler::try_enable`] on `Offload` always accepts the event.
        //
        // The full `try_enable` check is still performed inside the worker,
        // before the job is forwarded to the wrapped handler.
        // This matches the semantics of a non-offloaded chain.
        Some(context)
    }

    #[inline]
    fn log<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        context: Self::EnrichedContext,
    ) where
        Args: LogArgs<'a>,
    {
        self.log_by_ref(descriptor, args, span_id, context);
    }

    #[inline]
    fn enter_span<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Self::EnrichedContext,
    ) where
        Args: LogArgs<'a>,
    {
        self.enter_span_by_ref(
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
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        self.log_static_by_ref(descriptor, args, span_id, context);
    }

    #[inline]
    fn enter_span_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        self.enter_span_static_by_ref(
            descriptor,
            args,
            parent_span_id,
            span_id,
            span_kind,
            context,
        );
    }
}

impl<Context, CustomEvent> SharedHandler<Context> for OffloaderHandle<Context, CustomEvent>
where
    Context: ContextArgs,
    CustomEvent: 'static + Send,
{
}

impl<Context, CustomEvent> BaseExternalHandler for OffloaderHandle<Context, CustomEvent>
where
    Context: ContextArgs,
{
    #[inline]
    fn max_level_external(&self) -> LevelFilter {
        LevelFilter::ALL
    }
}

impl<Context, CustomEvent> ExternalHandler<Context> for OffloaderHandle<Context, CustomEvent>
where
    Context: ContextArgs,
{
    #[inline]
    fn try_enable_external<Desc>(
        &self,
        _descriptor: &Desc,
        context: Context,
    ) -> Option<Self::EnrichedContext>
    where
        Desc: ExternalDescriptor,
    {
        // [`Handler::try_enable_external`] on `Offload` always accepts the
        // event.
        //
        // The full `try_enable` check is still performed inside the worker,
        // before the job is forwarded to the wrapped handler.
        // This matches the semantics of a non-offloaded chain.
        Some(context)
    }

    #[inline]
    fn log_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        span_id: Option<SpanId>,
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        self.log_external_by_ref(descriptor, args, span_id, context);
    }

    #[inline]
    fn new_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        self.new_span_external_by_ref(descriptor, args, parent_span_id, span_id, context);
    }

    #[inline]
    fn enter_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        self.enter_span_external_by_ref(descriptor, args, parent_span_id, span_id, context);
    }

    #[inline]
    fn exit_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        self.exit_span_external_by_ref(descriptor, span_id, context);
    }

    #[inline]
    fn close_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        self.close_span_external_by_ref(descriptor, span_id, context);
    }

    #[inline]
    fn record_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        self.record_span_external_by_ref(descriptor, args, span_id, context);
    }

    #[expect(clippy::similar_names, reason = "better than alternatives")]
    #[inline]
    fn link_span_external<Desc>(
        &mut self,
        descriptor: &Desc,
        followed_span_id: SpanId,
        follower_span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
    {
        self.link_span_external_by_ref(descriptor, followed_span_id, follower_span_id, context);
    }
}

impl<Context, CustomEvent> BaseHandler for &OffloaderHandle<Context, CustomEvent>
where
    Context: ContextArgs,
{
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        OffloaderHandle::generate_span_id(&**self)
    }

    #[inline]
    fn flush_blocking(&mut self) {
        self.flush_blocking_by_ref();
    }

    #[inline]
    fn flush_on_panic(&mut self) {
        self.flush_on_panic_by_ref();
    }

    // `should_panic_on_bug` and `panic` methods are intentionally left at their
    // defaults.
}

impl<Context, CustomEvent> DynHandler<Context> for &OffloaderHandle<Context, CustomEvent>
where
    Context: ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    ) {
        self.exit_span_by_ref(descriptor, span_id, span_kind, context);
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        self.enter_poll_span_by_ref(descriptor, span_id, context);
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        self.pending_poll_span_by_ref(descriptor, span_id, context);
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        self.ready_poll_span_by_ref(descriptor, span_id, context);
    }
}

impl<Context, CustomEvent> Handler<Context> for &OffloaderHandle<Context, CustomEvent>
where
    Context: ContextArgs,
{
    type EnrichedContext = Context;

    #[inline]
    fn try_enable(
        &self,
        descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        OffloaderHandle::try_enable(&**self, descriptor, context)
    }

    #[inline]
    fn log<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        context: Self::EnrichedContext,
    ) where
        Args: LogArgs<'a>,
    {
        self.log_by_ref(descriptor, args, span_id, context);
    }

    #[inline]
    fn enter_span<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Self::EnrichedContext,
    ) where
        Args: LogArgs<'a>,
    {
        self.enter_span_by_ref(
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
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        self.log_static_by_ref(descriptor, args, span_id, context);
    }

    #[inline]
    fn enter_span_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        self.enter_span_static_by_ref(
            descriptor,
            args,
            parent_span_id,
            span_id,
            span_kind,
            context,
        );
    }
}

impl<Context, CustomEvent> SharedHandler<Context> for &'static OffloaderHandle<Context, CustomEvent>
where
    Context: ContextArgs,
    CustomEvent: Send,
{
}

impl<Context, CustomEvent> BaseExternalHandler for &OffloaderHandle<Context, CustomEvent>
where
    Context: ContextArgs,
{
    #[inline]
    fn max_level_external(&self) -> LevelFilter {
        LevelFilter::ALL
    }
}

impl<Context, CustomEvent> ExternalHandler<Context> for &OffloaderHandle<Context, CustomEvent>
where
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
        OffloaderHandle::try_enable_external(&**self, descriptor, context)
    }

    #[inline]
    fn log_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        span_id: Option<SpanId>,
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        self.log_external_by_ref(descriptor, args, span_id, context);
    }

    #[inline]
    fn new_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        self.new_span_external_by_ref(descriptor, args, parent_span_id, span_id, context);
    }

    #[inline]
    fn enter_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        self.enter_span_external_by_ref(descriptor, args, parent_span_id, span_id, context);
    }

    #[inline]
    fn exit_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        self.exit_span_external_by_ref(descriptor, span_id, context);
    }

    #[inline]
    fn close_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        self.close_span_external_by_ref(descriptor, span_id, context);
    }

    #[inline]
    fn record_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        self.record_span_external_by_ref(descriptor, args, span_id, context);
    }

    #[expect(clippy::similar_names, reason = "better than alternatives")]
    #[inline]
    fn link_span_external<Desc>(
        &mut self,
        descriptor: &Desc,
        followed_span_id: SpanId,
        follower_span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
    {
        self.link_span_external_by_ref(descriptor, followed_span_id, follower_span_id, context);
    }
}

/// One job in the offloader's channel.
///
/// Every variant that can be filtered by the wrapped handler's `try_enable`
/// (all of them except `StopJob`, `FlushBlocking`, `FlushOnPanic`, and
/// `Custom`) is checked on the worker side, not at the call site.
/// The call site's `try_enable` is a no‑op.
enum Event<Context, CustomEvent> {
    Log {
        descriptor: &'static Descriptor,
        args: TypeErasedArcLogArgs,
        span_id: Option<SpanId>,
        context: Context,
    },
    EnterSpan {
        descriptor: &'static Descriptor,
        args: TypeErasedArcLogArgs,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    },
    ExitSpan {
        descriptor: &'static Descriptor,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    },
    EnterPollSpan {
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    },
    ReadyPollSpan {
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    },
    PendingPollSpan {
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    },

    LogExternal {
        descriptor: Box<TypeErasedExternalDescriptor>,
        args: TypeErasedExternalArgs,
        span_id: Option<SpanId>,
        context: Context,
    },
    NewSpanExternal {
        descriptor: Box<TypeErasedExternalDescriptor>,
        args: TypeErasedExternalArgs,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Context,
    },
    EnterSpanExternal {
        descriptor: Box<TypeErasedExternalDescriptor>,
        args: TypeErasedExternalArgs,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Context,
    },
    ExitSpanExternal {
        descriptor: Box<TypeErasedExternalDescriptor>,
        span_id: SpanId,
        context: Context,
    },
    CloseSpanExternal {
        descriptor: Box<TypeErasedExternalDescriptor>,
        span_id: SpanId,
        context: Context,
    },
    RecordSpanExternal {
        descriptor: Box<TypeErasedExternalDescriptor>,
        args: TypeErasedExternalArgs,
        span_id: SpanId,
        context: Context,
    },
    LinkSpanExternal {
        descriptor: Box<TypeErasedExternalDescriptor>,
        followed_span_id: SpanId,
        follower_span_id: SpanId,
        context: Context,
    },

    FlushBlocking {
        flushed: oneshot::Sender<()>,
    },
    FlushOnPanic {
        flushed: oneshot::Sender<()>,
    },

    Shutdown,
    Custom(CustomEvent),
}

impl<Context, CustomEvent> Clone for OffloaderHandle<Context, CustomEvent> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<Context, CustomEvent> Debug for OffloaderHandle<Context, CustomEvent> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("OffloaderHandle")
            .field("sender", &self.sender)
            .field("inner", &self.inner)
            .finish()
    }
}

impl<Context, CustomEvent> Debug for OffloaderInner<Context, CustomEvent> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("OffloaderInner")
            .field("receiver", &self.receiver)
            .field("num_events_dropped", &self.num_events_dropped)
            .field("is_shutdown", &self.is_shutdown)
            .field("span_id_generator", &self.span_id_generator)
            .field("backpressure_policy", &self.backpressure_policy)
            .field("flush_interval", &self.flush_interval)
            .field("_context", &PhantomData::<Context>)
            .finish()
    }
}

impl<H, Context, CustomEvent, CustomHandler> Debug
    for OffloaderService<H, Context, CustomEvent, CustomHandler>
where
    H: Debug,
{
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let custom_handler = fmt::from_fn(|f| f.debug_tuple("FnMut").finish_non_exhaustive());
        f.debug_struct("OffloaderService")
            .field("handler", &self.handler)
            .field("inner", &self.inner)
            .field("custom_handler", &custom_handler)
            .finish()
    }
}

#[cfg(feature = "offloader-support-tokio")]
#[inline]
fn can_block_in_place() -> bool {
    // block_in_place is only valid on a multi-threaded runtime.
    tokio::runtime::Handle::try_current()
        .is_ok_and(|handle| handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread)
}

#[cfg(test)]
mod tests {
    use alloc::sync::{Arc, Weak};
    use core::ops::ControlFlow;

    use polog::{Discard, SpanIdGenerator};

    use crate::{
        OffloaderBackpressurePolicy, OffloaderCapacity, OffloaderHandle, OffloaderService,
        new_offloader,
    };

    #[expect(clippy::type_complexity, reason = "okay in tests")]
    #[inline]
    fn new_test_offloader(
        discard: Discard,
    ) -> (
        OffloaderHandle<(), !>,
        OffloaderService<Discard, (), !, impl FnMut(&mut Discard, !) -> ControlFlow<(), ()>>,
    ) {
        new_offloader(
            discard,
            Arc::new(SpanIdGenerator::new()),
            OffloaderCapacity::Unbounded,
            OffloaderBackpressurePolicy::Block,
            None,
            |_, ev: !| match ev {},
        )
    }

    #[test]
    fn test_offloader_accessors() {
        let (logger, mut service) = new_test_offloader(Discard);

        let inner = Arc::downgrade(&service.inner);
        assert_eq!(Weak::strong_count(&inner), 2);

        let _unused = service.handler();
        let _unused = service.handler_mut();
        let _unused = service.drop_into_handler();
        assert_eq!(Weak::strong_count(&inner), 1);

        logger.stop();
        assert_eq!(Weak::strong_count(&inner), 1);
        drop(logger);
        assert_eq!(Weak::strong_count(&inner), 0);
    }
}
