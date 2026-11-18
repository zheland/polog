use polog::{ContextArgs, Discard, Handler, SpanId};

use crate::{ExternalArgs, ExternalDescriptor};

/// A backend or middleware handler for events from foreign facades.
///
/// `ExternalHandler<Context>` is the foreign-facade counterpart of
/// [`Handler<Context>`].
/// It receives events that originate in `log`, `slog`, or `tracing` and are
/// normalized by the delegators in this crate's `delegators` module.
/// The methods mirror [`Handler`]'s event and span methods, but accept
/// [`ExternalDescriptor`] and [`ExternalArgs`] in place of
/// [`polog::Descriptor`] and [`polog::LogArgs`], because foreign facades carry
/// their metadata in their own types.
///
/// # Method families
///
/// - **Event path** (`log` and `slog`): [`log_external`](Self::log_external)
///   after [`try_enable_external`](Self::try_enable_external) has accepted the
///   event. The span methods are not called for these facades. They have no
///   span lifecycle.
/// - **Span lifecycle** (`tracing`): a
///   [`new_span_external`](Self::new_span_external) →
///   [`enter_span_external`](Self::enter_span_external) →
///   [`exit_span_external`](Self::exit_span_external) →
///   [`close_span_external`](Self::close_span_external) sequence.
///   `new_span_external` runs once per `tracing::Span` object.
///   `enter_span_external` and `exit_span_external` run per enter/exit of that
///   object. `close_span_external` runs when the object is destroyed.
///
/// # Enablement
///
/// [`try_enable_external`](Self::try_enable_external) runs first for
/// every event and every span-object, matching [`Handler::try_enable`].
/// A `None` return means the event or span is dropped; the other
/// methods are then not called.
/// `try_enable_external` does not receive [`ExternalArgs`], matching
/// [`ExternalFilterFn`](crate::ExternalFilterFn) and
/// [`FilterFn`](crate::FilterFn), which also decide on the descriptor
/// alone.
///
/// [`polog::Descriptor`]: polog::Descriptor
/// [`polog::LogArgs`]: polog::LogArgs
/// [`polog::DynHandler::exit_span`]: polog::DynHandler::exit_span
pub trait ExternalHandler<Context>: Handler<Context>
where
    Context: ContextArgs,
{
    /// Checks whether the handler accepts the event or span described by
    /// `descriptor`, optionally enriching the incoming `context`.
    ///
    /// Returns `Some(enriched_context)` if the event should be processed,
    /// or `None` if it should be dropped.
    /// The returned context is forwarded to
    /// [`log_external`](Self::log_external),
    /// [`new_span_external`](Self::new_span_external), and
    /// [`enter_span_external`](Self::enter_span_external).
    ///
    /// Callers must invoke this method before constructing log arguments so
    /// that disabled events pay no per‑event cost.
    /// The logging macros perform this two‑step protocol automatically.
    fn try_enable_external<Desc>(
        &self,
        descriptor: &Desc,
        context: Context,
    ) -> Option<Self::EnrichedContext>
    where
        Desc: ExternalDescriptor;

    /// Processes a foreign log event with the given structured arguments
    /// and enriched context.
    ///
    /// This method is only called when
    /// [`try_enable_external`](Self::try_enable_external) has returned
    /// `Some`.
    /// Implementations should not re‑check whether the event is enabled.
    ///
    /// Method arguments are:
    /// - `descriptor`: A foreign-facade metadata (level, target, name,
    ///   location).
    /// - `args`: A key-value argument list. Can be formatted in place, or
    ///   converted with
    /// - `span_id`: A `polog` span ID under which the event runs, or `None`
    ///   when no span is active.
    /// - `context`: The enriched context returned by `try_enable_external`.
    fn log_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        span_id: Option<SpanId>,
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs;

    /// Begins a foreign span.
    ///
    /// Called by a `tracing` adapter once per `tracing::Span` object,
    /// before any [`enter_span_external`](Self::enter_span_external).
    /// `log` and `slog` do not call this method.
    /// Those facades have no span lifecycle.
    ///
    /// `span_id` is the [`SpanId`] the adapter allocated for this span
    /// (via [`BaseHandler::generate_span_id`]), or `None` if the adapter does
    /// not associate a `polog` ID with this span-object call.
    /// The same `span_id` is used in the subsequent
    /// `enter_span_external`, `exit_span_external`, and
    /// `close_span_external` calls.
    ///
    /// This method is only called when [`try_enable_external`] has returned
    /// `Some`.
    /// Implementations must not re-check enablement.
    ///
    /// [`BaseHandler::generate_span_id`]: polog::BaseHandler::generate_span_id
    /// [`try_enable_external`]: Self::try_enable_external
    fn new_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs;

    /// Enters a foreign span.
    ///
    /// Called by a `tracing` adapter when foreign code enters a span.
    /// `log` and `slog` do not call this method; those facades have no
    /// span lifecycle.
    ///
    /// # Re-entrance and span identity
    ///
    /// `polog` spans cannot be re-entered.
    /// A `polog` span is entered exactly once, when it is created, and
    /// exited exactly once, when it is destroyed.
    /// `tracing` does not share this restriction: the same
    /// `tracing::Span` object may be entered, exited, and re-entered
    /// arbitrarily many times, from any thread.
    ///
    /// To bridge the two models, this method treats the `tracing` span
    /// object and its entry regions as separate `polog` spans:
    ///
    /// - [`new_span_external`](Self::new_span_external) created one `polog`
    ///   span for the span object, on a stable [`SpanId`] (`span_object`). That
    ///   span carries the `ExternalNew` / `ExternalClose` lifecycle and is the
    ///   parent of every entry region.
    /// - `enter_span_external` creates a **new** `polog` span for each entry,
    ///   on a fresh [`SpanId`] (`span_enter_k` for the k-th entry). Its parent
    ///   is `span_object`. Every event logged between this call and the
    ///   matching [`exit_span_external`](Self::exit_span_external) is
    ///   attributed to `span_enter_k`.
    /// - [`exit_span_external`](Self::exit_span_external) destroys
    ///   `span_enter_k`.
    /// - [`close_span_external`](Self::close_span_external) destroys
    ///   `span_object`.
    ///
    /// Because `span_object` outlives `span_enter_k` and may be entered from
    /// another thread, every `span_enter_k` is reported as
    /// [`SpanKind::Detached`][polog::SpanKind::Detached]: the span
    /// object is not blocked by the entry region.
    /// This is the only correct `SpanKind` for a foreign entry, even
    /// when the parent and child happen to run on the same thread for
    /// their entire lifetime; the adapter cannot prove containment.
    ///
    /// # Arguments
    ///
    /// - `descriptor`: metadata for the `tracing` span object, identical to the
    ///   descriptor passed to [`new_span_external`](Self::new_span_external).
    /// - `args`: the span's initial attributes, identical to the args passed to
    ///   `new_span_external`. Entries after the first reuse the same arguments;
    ///   `tracing` does not re-supply them.
    /// - `parent_span_id`: `Some(span_object)`, the span object's ID returned
    ///   by `new_span_external`.
    /// - `span_id`: `Some(span_enter_k)`, a fresh ID generated by the adapter
    ///   for this entry via
    ///   [`BaseHandler::generate_span_id`][polog::BaseHandler::generate_span_id].
    ///   A handler that attributes subsequent events to this entry region uses
    ///   this ID; the same ID is passed to the matching
    ///   [`exit_span_external`](Self::exit_span_external).
    /// - `context`: the enriched context returned by
    ///   [`try_enable_external`](Self::try_enable_external).
    ///
    /// This method is only called when
    /// [`try_enable_external`](Self::try_enable_external) has returned
    /// `Some`; implementations must not re-check enablement.
    fn enter_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs;

    /// Exits a foreign span.
    ///
    /// Called by a `tracing` adapter when foreign code exits a span,
    /// after the events that ran inside it.
    /// `log` and `slog` do not call this method.
    ///
    /// `context` is a fresh `Context`, not the enriched context
    /// returned by [`try_enable_external`](Self::try_enable_external) at
    /// span entry.
    /// Middleware may enrich it again.
    /// Because the span has already been created, this call is not a filter
    /// point.
    fn exit_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor;

    /// Closes a foreign span.
    ///
    /// Called by a `tracing` adapter when the last clone of a
    /// `tracing::Span` is dropped, after its last
    /// [`exit_span_external`](Self::exit_span_external).
    /// This is the `tracing::Subscriber::try_close` analogue: the span
    /// object is being destroyed and will not be entered or exited again.
    ///
    /// `context` is a fresh `Context`.
    /// Because the span is being destroyed, this call is not a filter point.
    fn close_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor;

    /// Records new fields on a live foreign span.
    ///
    /// Called by a `tracing` adapter when `Span::record()` or
    /// `Span::record_all()` runs, and by `async-trait`'s `#[instrument]`
    /// for state captured after span creation.
    /// `log` and `slog` do not call this method; their facades have no
    /// equivalent operation.
    ///
    /// `descriptor` is the span object's metadata, identical to the
    /// descriptor passed to [`new_span_external`](Self::new_span_external).
    /// `args` are the recorded fields, in source order.
    /// `span_id` is the span object's ID (the value returned by
    /// `new_span_external`).
    /// `context` is a fresh `Context`, not the [`Self::EnrichedContext`]
    /// returned at span creation.
    ///
    /// This method is called unconditionally for spans that were accepted
    /// by [`try_enable_external`](Self::try_enable_external) at creation
    /// time.
    /// It is not a filter point, matching `tracing::Subscriber`'s contract:
    /// `Subscriber::record` runs without an additional `enabled` check.
    ///
    /// The default implementation discards the recorded fields, because
    /// `polog`'s native span model has no dynamic fields. Backends that
    /// track span state can override it to update the stored span.
    ///
    /// [`Self::EnrichedContext`]: polog::Handler::EnrichedContext
    fn record_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs;

    /// Records a causal "follows from" edge between two foreign spans.
    ///
    /// Called by a `tracing` adapter when `Span::follows_from()` runs.
    /// `log` and `slog` do not call this method.
    ///
    /// `span_id` is the follower; `follows_from_span_id` is the span the
    /// follower is causally related to. `descriptor` is the follower's
    /// metadata.
    /// `context` is a fresh `Context`.
    ///
    /// Like [`record_span_external`], this is not a filter point.
    /// `tracing::Subscriber::record_follows_from` is called unconditionally
    /// on live spans.
    ///
    /// The default implementation discards the edge, because `polog`'s
    /// span model has no causal-link concept. Backends that store a span
    /// graph can override it.
    ///
    /// [`record_span_external`]: Self::record_span_external
    fn link_span_external<Desc>(
        &mut self,
        descriptor: &Desc,
        followed_span_id: SpanId,
        follower_span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor;
}

impl<H, Context> ExternalHandler<Context> for &mut H
where
    H: ExternalHandler<Context>,
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
        H::try_enable_external(&**self, descriptor, context)
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
        H::log_external(&mut **self, descriptor, args, span_id, context);
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
        H::new_span_external(
            &mut **self,
            descriptor,
            args,
            parent_span_id,
            span_id,
            context,
        );
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
        H::enter_span_external(
            &mut **self,
            descriptor,
            args,
            parent_span_id,
            span_id,
            context,
        );
    }

    #[inline]
    fn exit_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        H::exit_span_external(&mut **self, descriptor, span_id, context);
    }

    #[inline]
    fn close_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        H::close_span_external(&mut **self, descriptor, span_id, context);
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
        H::record_span_external(&mut **self, descriptor, args, span_id, context);
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
        H::link_span_external(
            &mut **self,
            descriptor,
            followed_span_id,
            follower_span_id,
            context,
        );
    }
}

impl<Context> ExternalHandler<Context> for Discard
where
    Context: ContextArgs,
{
    #[inline]
    fn try_enable_external<Desc>(
        &self,
        _descriptor: &Desc,
        _context: Context,
    ) -> Option<Self::EnrichedContext>
    where
        Desc: ExternalDescriptor,
    {
        None
    }

    #[inline]
    fn log_external<Desc, Args>(
        &mut self,
        _descriptor: &Desc,
        _args: &Args,
        _span_id: Option<SpanId>,
        _context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
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
        _descriptor: &Desc,
        _args: &Args,
        _span_id: SpanId,
        _context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
    }

    #[inline]
    fn link_span_external<Desc>(
        &mut self,
        _descriptor: &Desc,
        _followed_span_id: SpanId,
        _follower_span_id: SpanId,
        _context: Context,
    ) where
        Desc: ExternalDescriptor,
    {
    }
}

#[cfg(feature = "either-1")]
impl<H1, H2, Context> ExternalHandler<Context> for either_1::Either<H1, H2>
where
    H1: ExternalHandler<Context>,
    H2: ExternalHandler<Context>,
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
        match self {
            Self::Left(handler) => {
                H1::try_enable_external(handler, descriptor, context).map(either_1::Either::Left)
            }
            Self::Right(handler) => {
                H2::try_enable_external(handler, descriptor, context).map(either_1::Either::Right)
            }
        }
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
        match (self, context) {
            (Self::Left(handler), Self::EnrichedContext::Left(context)) => {
                H1::log_external(&mut *handler, descriptor, args, span_id, context);
            }
            (Self::Right(handler), Self::EnrichedContext::Right(context)) => {
                H2::log_external(&mut *handler, descriptor, args, span_id, context);
            }
            (Self::Left(_), Self::EnrichedContext::Right(_))
            | (Self::Right(_), Self::EnrichedContext::Left(_)) => {
                // Silently ignore the wrong combination of handlers and
                // enriched contexts.
            }
        }
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
        match (self, context) {
            (Self::Left(handler), Self::EnrichedContext::Left(context)) => H1::new_span_external(
                &mut *handler,
                descriptor,
                args,
                parent_span_id,
                span_id,
                context,
            ),
            (Self::Right(handler), Self::EnrichedContext::Right(context)) => H2::new_span_external(
                &mut *handler,
                descriptor,
                args,
                parent_span_id,
                span_id,
                context,
            ),
            (Self::Left(_), Self::EnrichedContext::Right(_))
            | (Self::Right(_), Self::EnrichedContext::Left(_)) => {
                // Silently ignore the wrong combination of handlers and
                // enriched contexts.
            }
        }
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
        match (self, context) {
            (Self::Left(handler), Self::EnrichedContext::Left(context)) => H1::enter_span_external(
                &mut *handler,
                descriptor,
                args,
                parent_span_id,
                span_id,
                context,
            ),
            (Self::Right(handler), Self::EnrichedContext::Right(context)) => {
                H2::enter_span_external(
                    &mut *handler,
                    descriptor,
                    args,
                    parent_span_id,
                    span_id,
                    context,
                );
            }
            (Self::Left(_), Self::EnrichedContext::Right(_))
            | (Self::Right(_), Self::EnrichedContext::Left(_)) => {
                // Silently ignore the wrong combination of handlers and
                // enriched contexts.
            }
        }
    }

    #[inline]
    fn exit_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        match self {
            Self::Left(handler) => {
                H1::exit_span_external(&mut *handler, descriptor, span_id, context);
            }
            Self::Right(handler) => {
                H2::exit_span_external(&mut *handler, descriptor, span_id, context);
            }
        }
    }

    #[inline]
    fn close_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        match self {
            Self::Left(handler) => {
                H1::close_span_external(&mut *handler, descriptor, span_id, context);
            }
            Self::Right(handler) => {
                H2::close_span_external(&mut *handler, descriptor, span_id, context);
            }
        }
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
        match self {
            Self::Left(handler) => {
                H1::record_span_external(&mut *handler, descriptor, args, span_id, context);
            }
            Self::Right(handler) => {
                H2::record_span_external(&mut *handler, descriptor, args, span_id, context);
            }
        }
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
        match self {
            Self::Left(handler) => H1::link_span_external(
                &mut *handler,
                descriptor,
                followed_span_id,
                follower_span_id,
                context,
            ),
            Self::Right(handler) => H2::link_span_external(
                &mut *handler,
                descriptor,
                followed_span_id,
                follower_span_id,
                context,
            ),
        }
    }
}
