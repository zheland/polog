use polog::{ContextArgs, Discard, LevelFilter, SpanId};

use crate::{ExternalArgs, ExternalDescriptor};

/// Functionality common to all [`ExternalHandler`] implementations.
pub trait BaseExternalHandler {
    /// The highest‑severity [`LevelFilter`] this handler will accept.
    ///
    /// Used by the facade bridges like [`LogBridge`] and
    /// provided by the facade delegators like [`ViaLog`]
    ///
    /// [`LogBridge`]: crate::bridges::log_0_4::LogBridge
    /// [`ViaLog`]: crate::delegators::log_0_4::ViaLog
    fn max_level_external(&self) -> LevelFilter;

    /// Checks whether the handler accepts the event or span described by
    /// `descriptor`.
    ///
    /// Returns `true` if the event should be processed, or `false` if it
    /// should be dropped.
    ///
    /// Callers must invoke this method before constructing log arguments so
    /// that disabled events pay no per‑event cost.
    fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor;
}

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
/// A `None` return means the event or span is dropped.
/// The other methods are then not called.
/// `try_enable_external` does not receive [`ExternalArgs`], matching
/// [`ExternalFilterFn`](crate::ExternalFilterFn) and
/// [`FilterFn`](crate::FilterFn), which also decide on the descriptor
/// alone.
///
/// [`polog::Descriptor`]: polog::Descriptor
/// [`polog::LogArgs`]: polog::LogArgs
/// [`polog::DynHandler::exit_span`]: polog::DynHandler::exit_span
pub trait ExternalHandler<Context>: BaseExternalHandler
where
    Context: ContextArgs,
{
    /// Processes a foreign log event with the given structured arguments
    /// and enriched context.
    ///
    /// This method is only called when
    /// [`is_enabled_external`] has returned `true`.
    /// Implementations should not re‑check whether the event is enabled.
    ///
    /// Method arguments are:
    /// - `descriptor`: A foreign-facade metadata (level, target, name,
    ///   location).
    /// - `args`: A key-value argument list. Can be formatted in place, or
    ///   converted with
    /// - `span_id`: A `polog` span ID under which the event runs, or `None`
    ///   when no span is active.
    /// - `context`: The context.
    ///
    /// [`is_enabled_external`]: Self::is_enabled_external
    fn log_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        span_id: Option<SpanId>,
        context: Context,
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
    /// This method is only called when [`is_enabled_external`] has returned
    /// `true`.
    /// Implementations must not re-check enablement.
    ///
    /// [`BaseHandler::generate_span_id`]: polog::BaseHandler::generate_span_id
    /// [`is_enabled_external`]: Self::is_enabled_external
    /// [`enter_span_external`]: Self::enter_span_external
    fn new_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs;

    /// Enters a foreign span.
    ///
    /// Called by a `tracing` adapter when foreign code enters a span.
    /// `log` and `slog` do not call this method.
    /// Those facades have no span lifecycle.
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
    ///   matching [`exit_span_external`] is attributed to `span_enter_k`.
    /// - [`exit_span_external`] destroys `span_enter_k`.
    /// - [`close_span_external`](Self::close_span_external) destroys
    ///   `span_object`.
    ///
    /// Because `span_object` outlives `span_enter_k` and may be entered from
    /// another thread, every `span_enter_k` is reported as
    /// [`SpanKind::Detached`]: the span
    /// object is not blocked by the entry region.
    /// This is the only correct `SpanKind` for a foreign entry, even
    /// when the parent and child happen to run on the same thread for
    /// their entire lifetime.
    /// The adapter cannot prove containment.
    ///
    /// # Arguments
    ///
    /// - `descriptor`: metadata for the `tracing` span object, identical to the
    ///   descriptor passed to [`new_span_external`].
    /// - `args`: the span's initial attributes, identical to the args passed to
    ///   `new_span_external`. Entries after the first reuse the same arguments;
    ///   `tracing` does not re-supply them.
    /// - `parent_span_id`: `Some(span_object)`, the span object's ID returned
    ///   by `new_span_external`.
    /// - `span_id`: `Some(span_enter_k)`, a fresh ID generated by the adapter
    ///   for this entry via [`BaseHandler::generate_span_id`]. A handler that
    ///   attributes subsequent events to this entry region uses this ID. The
    ///   same ID is passed to the matching [`exit_span_external`].
    /// - `context`: the enriched context returned by [`is_enabled_external`].
    ///
    /// This method is only called when [`is_enabled_external`] has returned
    /// `true`.
    /// Implementations must not re-check enablement.
    ///
    /// [`is_enabled_external`]: Self::is_enabled_external
    /// [`new_span_external`]: Self::new_span_external
    /// [`exit_span_external`]: Self::exit_span_external
    /// [`close_span_external`]: Self::close_span_external
    /// [`SpanKind::Detached`]: polog::SpanKind::Detached
    /// [`BaseHandler::generate_span_id`]: polog::BaseHandler::generate_span_id
    fn enter_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs;

    /// Exits a foreign span.
    ///
    /// Called by a `tracing` adapter when foreign code exits a span,
    /// after the events that ran inside it.
    /// `log` and `slog` do not call this method.
    ///
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
    /// Because the span is being destroyed, this call is not a filter point.
    fn close_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor;

    /// Records new fields on a live foreign span.
    ///
    /// Called by a `tracing` adapter when `Span::record()` or
    /// `Span::record_all()` runs, and by `async-trait`'s `#[instrument]`
    /// for state captured after span creation.
    /// `log` and `slog` do not call this method.
    /// Their facades have no equivalent operation.
    ///
    /// `descriptor` is the span object's metadata, identical to the
    /// descriptor passed to [`new_span_external`]. `args` are the recorded
    /// fields, in source order.
    /// `span_id` is the span object's ID (the value returned by
    /// `new_span_external`).
    /// `context` is a fresh `Context`, not the [`Self::EnrichedContext`]
    /// returned at span creation.
    ///
    /// This method is called unconditionally for spans that were accepted
    /// by [`is_enabled_external`] at creation time.
    /// It is not a filter point, matching `tracing::Subscriber`'s contract:
    /// `Subscriber::record` runs without an additional `enabled` check.
    ///
    /// The default implementation discards the recorded fields, because
    /// `polog`'s native span model has no dynamic fields. Backends that
    /// track span state can override it to update the stored span.
    ///
    /// [`Self::EnrichedContext`]: polog::Handler::EnrichedContext
    /// [`new_span_external`]: Self::new_span_external
    /// [`is_enabled_external`]: Self::is_enabled_external
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

macro_rules! propagate_base_external_handler_impl {
    (
        impl ( $($tt:tt)* ) BaseExternalHandler for $ty:ty
        where ( $($bounds:tt)* ),
        impl ($via:ty) $self:tt => $expr:expr $(,)?
    ) => {
        impl<$($tt)*> BaseExternalHandler for $ty
        where
            $($bounds)*
        {
            #[inline]
            fn max_level_external(&$self) -> LevelFilter {
                <$via as BaseExternalHandler>::max_level_external(&$expr)
            }


            #[inline]
            fn is_enabled_external<Desc>(&$self, descriptor: &Desc) -> bool
            where
                Desc: ExternalDescriptor
            {
                <$via as BaseExternalHandler>::is_enabled_external(&$expr, descriptor)
            }
        }
    }
}

macro_rules! propagate_external_handler_impl {
    (
        impl ( $($tt:tt)* ) ExternalHandler<$context:ty> for $ty:ty
        where ( $($bounds:tt)* ),
        impl ($via:ty) $self:tt => $expr:expr $(,)?
    ) => {
        impl<$($tt)*> ExternalHandler<$context> for $ty
        where
            $($bounds)*
        {
            #[inline]
            fn log_external<Desc, Args>(
                &mut $self,
                descriptor: &Desc,
                args: &Args,
                span_id: Option<SpanId>,
                context: Context,
            ) where
                Desc: ExternalDescriptor,
                Args: ExternalArgs,
            {
                <$via as ExternalHandler<Context>>::log_external(&mut $expr, descriptor, args, span_id, context);
            }

            #[inline]
            fn new_span_external<Desc, Args>(
                &mut $self,
                descriptor: &Desc,
                args: &Args,
                parent_span_id: Option<SpanId>,
                span_id: SpanId,
                context: Context,
            ) where
                Desc: ExternalDescriptor,
                Args: ExternalArgs,
            {
                <$via as ExternalHandler<Context>>::new_span_external(
                    &mut $expr,
                    descriptor,
                    args,
                    parent_span_id,
                    span_id,
                    context,
                );
            }

            #[inline]
            fn enter_span_external<Desc, Args>(
                &mut $self,
                descriptor: &Desc,
                args: &Args,
                parent_span_id: Option<SpanId>,
                span_id: SpanId,
                context: Context,
            ) where
                Desc: ExternalDescriptor,
                Args: ExternalArgs,
            {
                <$via as ExternalHandler<Context>>::enter_span_external(
                    &mut $expr,
                    descriptor,
                    args,
                    parent_span_id,
                    span_id,
                    context,
                );
            }

            #[inline]
            fn exit_span_external<Desc>(
                &mut $self,
                descriptor: &Desc,
                span_id: SpanId,
                context: Context,
            ) where
                Desc: ExternalDescriptor,
            {
                <$via as ExternalHandler<Context>>::exit_span_external(&mut $expr, descriptor, span_id, context);
            }

            #[inline]
            fn close_span_external<Desc>(
                &mut $self,
                descriptor: &Desc,
                span_id: SpanId,
                context: Context,
            ) where
                Desc: ExternalDescriptor,
            {
                <$via as ExternalHandler<Context>>::close_span_external(&mut $expr, descriptor, span_id, context);
            }

            #[inline]
            fn record_span_external<Desc, Args>(
                &mut $self,
                descriptor: &Desc,
                args: &Args,
                span_id: SpanId,
                context: Context,
            ) where
                Desc: ExternalDescriptor,
                Args: ExternalArgs,
            {
                <$via as ExternalHandler<Context>>::record_span_external(&mut $expr, descriptor, args, span_id, context);
            }

            #[inline]
            fn link_span_external<Desc>(
                &mut $self,
                descriptor: &Desc,
                followed_span_id: SpanId,
                follower_span_id: SpanId,
                context: Context,
            ) where
                Desc: ExternalDescriptor,
            {
                <$via as ExternalHandler<Context>>::link_span_external(
                    &mut $expr,
                    descriptor,
                    followed_span_id,
                    follower_span_id,
                    context,
                );
            }
        }
    };
}

propagate_base_external_handler_impl!(
    impl (H) BaseExternalHandler for &mut H
    where (H: ?Sized + BaseExternalHandler),
    impl (H) self => **self
);

propagate_external_handler_impl!(
    impl (H, Context) ExternalHandler<Context> for &mut H
    where (
        H: ?Sized + ExternalHandler<Context>,
        Context: ContextArgs,
    ),
    impl (H) self => **self
);

impl BaseExternalHandler for Discard {
    #[inline]
    fn max_level_external(&self) -> LevelFilter {
        LevelFilter::Off
    }

    #[inline]
    fn is_enabled_external<Desc>(&self, _descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor,
    {
        false
    }
}

impl<Context> ExternalHandler<Context> for Discard
where
    Context: ContextArgs,
{
    #[inline]
    fn log_external<Desc, Args>(
        &mut self,
        _descriptor: &Desc,
        _args: &Args,
        _span_id: Option<SpanId>,
        _context: Context,
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
        _context: Context,
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
        _context: Context,
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

#[cfg(feature = "alloc")]
mod alloc_impls {
    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::sync::Arc;

    use polog::{ContextArgs, LevelFilter, SpanId};

    use crate::{BaseExternalHandler, ExternalArgs, ExternalDescriptor, ExternalHandler};

    propagate_base_external_handler_impl!(
        impl (H) BaseExternalHandler for Box<H>
        where (H: ?Sized + BaseExternalHandler),
        impl (H) self => **self
    );

    propagate_external_handler_impl!(
        impl (H, Context) ExternalHandler<Context> for Box<H>
        where (
            H: ?Sized + ExternalHandler<Context>,
            Context: ContextArgs,
        ),
        impl (H) self => **self
    );

    propagate_base_external_handler_impl!(
        impl (H) BaseExternalHandler for Rc<H>
        where (
            H: ?Sized,
            for<'a> &'a H: BaseExternalHandler,
        ),
        impl (&H) self => &**self
    );

    propagate_external_handler_impl!(
        impl (H, Context) ExternalHandler<Context> for Rc<H>
        where (
            H: ?Sized,
            for<'a> &'a H: ExternalHandler<Context>,
            Context: ContextArgs,
        ),
        impl (&H) self => &**self
    );

    propagate_base_external_handler_impl!(
        impl (H) BaseExternalHandler for Arc<H>
        where (
            H: ?Sized,
            for<'a> &'a H: BaseExternalHandler,
        ),
        impl (&H) self => &**self
    );

    propagate_external_handler_impl!(
        impl (H, Context) ExternalHandler<Context> for Arc<H>
        where (
            H: ?Sized,
            for<'a> &'a H: ExternalHandler<Context>,
            Context: ContextArgs,
        ),
        impl (&H) self => &**self
    );
}

#[cfg(feature = "either-1")]
mod either_1_impls {
    use polog::{ContextArgs, LevelFilter, SpanId};

    use crate::{BaseExternalHandler, ExternalArgs, ExternalDescriptor, ExternalHandler};

    impl<H1, H2> BaseExternalHandler for either_1::Either<H1, H2>
    where
        H1: BaseExternalHandler,
        H2: BaseExternalHandler,
    {
        #[inline]
        fn max_level_external(&self) -> LevelFilter {
            match self {
                Self::Left(handler) => H1::max_level_external(handler),
                Self::Right(handler) => H2::max_level_external(handler),
            }
        }

        #[inline]
        fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
        where
            Desc: ExternalDescriptor,
        {
            match self {
                Self::Left(handler) => H1::is_enabled_external(handler, descriptor),
                Self::Right(handler) => H2::is_enabled_external(handler, descriptor),
            }
        }
    }

    impl<H1, H2, Context> ExternalHandler<Context> for either_1::Either<H1, H2>
    where
        H1: ExternalHandler<Context>,
        H2: ExternalHandler<Context>,
        Context: ContextArgs,
    {
        #[inline]
        fn log_external<Desc, Args>(
            &mut self,
            descriptor: &Desc,
            args: &Args,
            span_id: Option<SpanId>,
            context: Context,
        ) where
            Desc: ExternalDescriptor,
            Args: ExternalArgs,
        {
            match self {
                Self::Left(handler) => {
                    H1::log_external(&mut *handler, descriptor, args, span_id, context);
                }
                Self::Right(handler) => {
                    H2::log_external(&mut *handler, descriptor, args, span_id, context);
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
            context: Context,
        ) where
            Desc: ExternalDescriptor,
            Args: ExternalArgs,
        {
            match self {
                Self::Left(handler) => {
                    H1::new_span_external(
                        &mut *handler,
                        descriptor,
                        args,
                        parent_span_id,
                        span_id,
                        context,
                    );
                }
                Self::Right(handler) => {
                    H2::new_span_external(
                        &mut *handler,
                        descriptor,
                        args,
                        parent_span_id,
                        span_id,
                        context,
                    );
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
            context: Context,
        ) where
            Desc: ExternalDescriptor,
            Args: ExternalArgs,
        {
            match self {
                Self::Left(handler) => {
                    H1::enter_span_external(
                        &mut *handler,
                        descriptor,
                        args,
                        parent_span_id,
                        span_id,
                        context,
                    );
                }
                Self::Right(handler) => {
                    H2::enter_span_external(
                        &mut *handler,
                        descriptor,
                        args,
                        parent_span_id,
                        span_id,
                        context,
                    );
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
        fn close_span_external<Desc>(
            &mut self,
            descriptor: &Desc,
            span_id: SpanId,
            context: Context,
        ) where
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
}
