use core::array;

use polog::{
    BaseHandler, BorrowingHandler, ContextArgs, Descriptor, DynHandler, Handler, LevelFilter,
    LogArgs, SharedHandler, SpanId, SpanKind, StaticLogArgs,
};

use crate::{BaseExternalHandler, ExternalArgs, ExternalDescriptor, ExternalHandler};

/// A [`Handler`] that forwards every accepted event and span to all wrapped
/// handlers without allocating to share arguments.
///
/// `BorrowingFanout<(T1, T2, ..., Tn), N>` requires every branch to implement
/// [`BorrowingHandler`] and dispatches through
/// [`BorrowingHandler::log_borrowed`] /
/// [`BorrowingHandler::enter_span_borrowed`]. When more than one branch
/// accepts, arguments are converted once and shared by reference, with no `Arc`
/// allocation.
/// When exactly one branch accepts, arguments pass to that branch unchanged.
///
/// # Arity
///
/// `N` must equal the number of elements in `Hs`.
/// Aliases [`BorrowingFanout1`] … [`BorrowingFanout4`] cover arities 1
/// through 4 without needing the second parameter.
/// Larger arities are supported by nesting.
///
/// # Relation to `Fanout`
///
/// [`Fanout`] is the general form, accepting any [`Handler`] branch and using
/// an `Arc` to share arguments when more than one branch accepts.
/// Use `BorrowingFanout` when every branch implements `BorrowingHandler`.
/// The resulting wrapper is strictly faster when more than one branch accepts
/// because the `Arc` allocation is skipped.
/// When exactly one branch accepts, both wrappers are equivalent.
///
/// # Panic and span‑ID behavior
///
/// `generate_span_id` uses the first branch's generator.
/// All branches see the same `SpanId` for the same event.
///
/// `should_panic_on_bug` returns `true` if any branch returns `true`.
/// `panic` is delegated only to the first branch.
///
/// [`Fanout`]: crate::Fanout
#[derive(Clone, Debug)]
pub struct BorrowingFanout<Hs, const N: usize> {
    handlers: Hs,
    disabled_spans: [dashmap::DashSet<SpanId>; N],
}

/// One‑branch [`BorrowingFanout`]: `BorrowingFanout<(H1,), 1>`.
pub type BorrowingFanout1<H1> = BorrowingFanout<(H1,), 1>;

/// Two‑branch [`BorrowingFanout`]: `BorrowingFanout<(H1, H2), 2>`.
pub type BorrowingFanout2<H1, H2> = BorrowingFanout<(H1, H2), 2>;

/// Three‑branch [`BorrowingFanout`]: `BorrowingFanout<(H1, H2, H3), 3>`.
pub type BorrowingFanout3<H1, H2, H3> = BorrowingFanout<(H1, H2, H3), 3>;

/// Four‑branch [`BorrowingFanout`]: `BorrowingFanout<(H1, H2, H3, H4), 4>`.
pub type BorrowingFanout4<H1, H2, H3, H4> = BorrowingFanout<(H1, H2, H3, H4), 4>;

impl<Hs, const N: usize> BorrowingFanout<Hs, N> {
    /// Creates a `BorrowingFanout` from a tuple of handlers.
    ///
    /// The tuple's arity must equal `N`.
    /// Aliases [`BorrowingFanout1`] … [`BorrowingFanout4`] cover arities 1
    /// through 4 without requiring the second parameter.
    #[inline]
    #[must_use]
    pub fn new(handlers: Hs) -> Self {
        Self {
            handlers,
            disabled_spans: array::from_fn(|_| dashmap::DashSet::new()),
        }
    }
}

impl<Hs, const N: usize> Default for BorrowingFanout<Hs, N>
where
    Hs: Default,
{
    #[inline]
    fn default() -> Self {
        Self {
            handlers: Hs::default(),
            disabled_spans: array::from_fn(|_| dashmap::DashSet::new()),
        }
    }
}

impl<T1> BaseHandler for BorrowingFanout<(T1,), 1>
where
    T1: BaseHandler,
{
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        <T1>::generate_span_id(&self.handlers.0)
    }

    #[inline]
    fn flush_blocking(&mut self) {
        <T1>::flush_blocking(&mut self.handlers.0);
    }

    #[inline]
    fn flush_on_panic(&mut self) {
        <T1>::flush_on_panic(&mut self.handlers.0);
    }

    #[inline]
    fn should_panic_on_bug(
        &self,
        descriptor: &'static Descriptor,
        span_id: Option<SpanId>,
    ) -> bool {
        <T1>::should_panic_on_bug(&self.handlers.0, descriptor, span_id)
    }

    #[inline]
    fn panic(&mut self, descriptor: &'static Descriptor, span_id: Option<SpanId>) -> ! {
        <T1>::panic(&mut self.handlers.0, descriptor, span_id)
    }
}

impl<T1, Context> DynHandler<Context> for BorrowingFanout<(T1,), 1>
where
    T1: DynHandler<Context>,
    Context: Clone + ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    ) {
        let was_enabled = [self.disabled_spans[0].remove(&span_id).is_none()];

        if was_enabled[0] {
            <T1>::exit_span(
                &mut self.handlers.0,
                descriptor,
                span_id,
                span_kind,
                context,
            );
        }
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        let is_enabled = [!self.disabled_spans[0].contains(&span_id)];

        if is_enabled[0] {
            <T1>::enter_poll_span(&mut self.handlers.0, descriptor, span_id, context);
        }
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        let is_enabled = [!self.disabled_spans[0].contains(&span_id)];

        if is_enabled[0] {
            <T1>::pending_poll_span(&mut self.handlers.0, descriptor, span_id, context);
        }
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        let is_enabled = [!self.disabled_spans[0].contains(&span_id)];

        if is_enabled[0] {
            <T1>::ready_poll_span(&mut self.handlers.0, descriptor, span_id, context);
        }
    }
}

impl<T1, Context> Handler<Context> for BorrowingFanout<(T1,), 1>
where
    T1: BorrowingHandler<Context>,
    Context: Clone + ContextArgs,
{
    type EnrichedContext = (Option<T1::EnrichedContext>,);
    #[inline]
    fn try_enable(
        &self,
        descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        let context = (<T1>::try_enable(&self.handlers.0, descriptor, context),);
        (context.0.is_some()).then_some(context)
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
        if let Some(context) = context.0 {
            <T1>::log(&mut self.handlers.0, descriptor, args, span_id, context);
        }
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
        if context.0.is_none() {
            let _is_newly_inserted: bool = self.disabled_spans[0].insert(span_id);
        }

        if let Some(context) = context.0 {
            <T1>::enter_span(
                &mut self.handlers.0,
                descriptor,
                args,
                parent_span_id,
                span_id,
                span_kind,
                context,
            );
        }
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
        if let Some(context) = context.0 {
            <T1>::log_static(&mut self.handlers.0, descriptor, args, span_id, context);
        }
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
        if context.0.is_none() {
            let _is_newly_inserted: bool = self.disabled_spans[0].insert(span_id);
        }

        if let Some(context) = context.0 {
            <T1>::enter_span_static(
                &mut self.handlers.0,
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

impl<T1, Context> BorrowingHandler<Context> for BorrowingFanout<(T1,), 1>
where
    T1: BorrowingHandler<Context>,
    Context: Clone + ContextArgs,
{
    #[inline]
    fn log_borrowed<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        span_id: Option<SpanId>,
        context: &Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        if let Some(context) = &context.0 {
            <T1>::log_borrowed(&mut self.handlers.0, descriptor, args, span_id, context);
        }
    }

    #[inline]
    fn enter_span_borrowed<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: &Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        if context.0.is_none() {
            let _is_newly_inserted: bool = self.disabled_spans[0].insert(span_id);
        }

        if let Some(context) = &context.0 {
            <T1>::enter_span_borrowed(
                &mut self.handlers.0,
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

impl<T1, Context> SharedHandler<Context> for BorrowingFanout<(T1,), 1>
where
    T1: BorrowingHandler<Context> + SharedHandler<Context>,
    Context: Clone + ContextArgs,
{
}

impl<T1> BaseExternalHandler for BorrowingFanout<(T1,), 1>
where
    T1: BaseExternalHandler,
{
    #[inline]
    fn max_level_external(&self) -> LevelFilter {
        T1::max_level_external(&self.handlers.0)
    }
}

impl<T1, Context> ExternalHandler<Context> for BorrowingFanout<(T1,), 1>
where
    T1: BorrowingHandler<Context> + ExternalHandler<Context>,
    Context: Clone + ContextArgs,
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
        let context = (<T1>::try_enable_external(
            &self.handlers.0,
            descriptor,
            context,
        ),);
        (context.0.is_some()).then_some(context)
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
        if let Some(context) = context.0 {
            <T1>::log_external(&mut self.handlers.0, descriptor, args, span_id, context);
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
        if context.0.is_none() {
            let _is_newly_inserted: bool = self.disabled_spans[0].insert(span_id);
        }

        if let Some(context) = context.0 {
            <T1>::new_span_external(
                &mut self.handlers.0,
                descriptor,
                args,
                parent_span_id,
                span_id,
                context,
            );
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
        if context.0.is_none() {
            let _is_newly_inserted: bool = self.disabled_spans[0].insert(span_id);
        }

        if let Some(context) = context.0 {
            <T1>::enter_span_external(
                &mut self.handlers.0,
                descriptor,
                args,
                parent_span_id,
                span_id,
                context,
            );
        }
    }

    #[inline]
    fn exit_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        let was_enabled = self.disabled_spans[0].remove(&span_id).is_none();

        if was_enabled {
            <T1>::exit_span_external(&mut self.handlers.0, descriptor, span_id, context);
        }
    }

    #[inline]
    fn close_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        let was_enabled = self.disabled_spans[0].remove(&span_id).is_none();

        if was_enabled {
            <T1>::close_span_external(&mut self.handlers.0, descriptor, span_id, context);
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
        let is_enabled = !self.disabled_spans[0].contains(&span_id);

        if is_enabled {
            <T1>::record_span_external(&mut self.handlers.0, descriptor, args, span_id, context);
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
        let is_enabled = !self.disabled_spans[0].contains(&follower_span_id);

        if is_enabled {
            <T1>::link_span_external(
                &mut self.handlers.0,
                descriptor,
                followed_span_id,
                follower_span_id,
                context,
            );
        }
    }
}

macro_rules! impl_variadics {
    ( [ $( $args:tt )* ] [] ) => {};
    ( [ $( $args:tt )* ] [ $next:tt $( $rest:tt )* ] ) => {
        impl_variadic!( [ $( $args )* ] [ $next ] [ $( $rest )* ] );
        impl_variadics!( [ $( $args )* $next ] [ $( $rest )* ] );
    };
}

macro_rules! impl_variadic {
    (
        []
        [ ( $field_n:tt $type_n:ident ) ]
        [ $( $rest:tt )* ]
    ) => {
    };
    (
        [ ( $field_1:tt $type_1:ident ) $( ( $field_i:tt $type_i:ident ) )* ]
        [ ( $field_n:tt $type_n:ident ) ]
        [ $( $rest:tt )* ]
    ) => {
        impl< $type_1 $(, $type_i )*, $type_n > BaseHandler
            for BorrowingFanout<
                ( $type_1, $( $type_i, )* $type_n, ),
                { count!( $field_1 $($field_i)* $field_n ) }
            >
        where
            $type_1: BaseHandler,
            $( $type_i: BaseHandler, )*
            $type_n: BaseHandler,
        {
            #[inline]
            fn generate_span_id(&self) -> SpanId {
                <$type_1>::generate_span_id(&self.handlers.$field_1)
            }

            #[inline]
            fn flush_blocking(&mut self) {
                <$type_1>::flush_blocking(&mut self.handlers.$field_1);
                $( <$type_i>::flush_blocking(&mut self.handlers.$field_i); )*
                <$type_n>::flush_blocking(&mut self.handlers.$field_n);
            }

            #[inline]
            fn flush_on_panic(&mut self) {
                <$type_1>::flush_on_panic(&mut self.handlers.$field_1);
                $( <$type_i>::flush_on_panic(&mut self.handlers.$field_i); )*
                <$type_n>::flush_on_panic(&mut self.handlers.$field_n);
            }

            #[inline]
            fn should_panic_on_bug(
                &self,
                descriptor: &'static Descriptor,
                span_id: Option<SpanId>,
            ) -> bool {
                <$type_1>::should_panic_on_bug(
                    &self.handlers.$field_1,
                    descriptor, span_id
                )
                $( || <$type_i>::should_panic_on_bug(
                    &self.handlers.$field_i,
                    descriptor, span_id
                ) )*
                || <$type_n>::should_panic_on_bug(
                    &self.handlers.$field_n,
                    descriptor, span_id
                )
            }

            #[inline]
            fn panic(&mut self, descriptor: &'static Descriptor, span_id: Option<SpanId>) -> ! {
                <$type_1>::panic(&mut self.handlers.$field_1, descriptor, span_id)
            }
        }

        impl< $type_1 $(, $type_i )*, $type_n , Context> DynHandler<Context>
            for BorrowingFanout<
                ( $type_1, $( $type_i, )* $type_n, ),
                { count!( $field_1 $($field_i)* $field_n ) }
            >
        where
            $type_1: DynHandler<Context>,
            $( $type_i: DynHandler<Context>, )*
            $type_n: DynHandler<Context>,
            Context: Clone + ContextArgs,
        {
            #[inline]
            fn exit_span(
                &mut self,
                descriptor: &'static Descriptor,
                span_id: SpanId,
                span_kind: SpanKind,
                context: Context,
            ) {
                let was_enabled = [
                    self.disabled_spans[ $field_1 ].remove(&span_id).is_none(),
                    $( self.disabled_spans[ $field_i ].remove(&span_id).is_none(), )*
                    self.disabled_spans[ $field_n ].remove(&span_id).is_none(),
                ];

                #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
                let num_enabled = (
                    usize::from(was_enabled[$field_1])
                    $( + usize::from(was_enabled[$field_i]) )*
                    + usize::from(was_enabled[$field_n])
                );

                if num_enabled == 1 {
                    if was_enabled[$field_1] {
                        <$type_1>::exit_span(
                            &mut self.handlers.$field_1,
                            descriptor,
                            span_id,
                            span_kind,
                            context
                        );
                    }
                    $( else if was_enabled[$field_i] {
                        <$type_i>::exit_span(
                            &mut self.handlers.$field_i,
                            descriptor,
                            span_id,
                            span_kind,
                            context
                        );
                    } )*
                    else if was_enabled[$field_n] {
                        <$type_n>::exit_span(
                            &mut self.handlers.$field_n,
                            descriptor,
                            span_id,
                            span_kind,
                            context
                        );
                    }
                } else if num_enabled > 1 {
                    if was_enabled[$field_1] {
                        let context = context.clone();
                        <$type_1>::exit_span(
                            &mut self.handlers.$field_1,
                            descriptor,
                            span_id,
                            span_kind,
                            context
                        );
                    }
                    $( if was_enabled[$field_i] {
                        let context = context.clone();
                        <$type_i>::exit_span(
                            &mut self.handlers.$field_i,
                            descriptor,
                            span_id,
                            span_kind,
                            context
                        );
                    } )*
                    if was_enabled[$field_n] {
                        <$type_n>::exit_span(
                            &mut self.handlers.$field_n,
                            descriptor,
                            span_id,
                            span_kind,
                            context
                        );
                    }
                }
            }

            #[inline]
            fn enter_poll_span(
                &mut self,
                descriptor: &'static Descriptor,
                span_id: SpanId,
                context: Context,
            ) {
                let is_enabled = [
                    !self.disabled_spans[ $field_1 ].contains(&span_id),
                    $( !self.disabled_spans[ $field_i ].contains(&span_id), )*
                    !self.disabled_spans[ $field_n ].contains(&span_id),
                ];

                #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
                let num_enabled = (
                    usize::from(is_enabled[$field_1])
                    $( + usize::from(is_enabled[$field_i]) )*
                    + usize::from(is_enabled[$field_n])
                );

                if num_enabled == 1 {
                    if is_enabled[$field_1] {
                        <$type_1>::enter_poll_span(
                            &mut self.handlers.$field_1,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                    $( else if is_enabled[$field_i] {
                        <$type_i>::enter_poll_span(
                            &mut self.handlers.$field_i,
                            descriptor,
                            span_id,
                            context
                        );
                    } )*
                    else if is_enabled[$field_n] {
                        <$type_n>::enter_poll_span(
                            &mut self.handlers.$field_n,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                } else if num_enabled > 1 {
                    if is_enabled[$field_1] {
                        let context = context.clone();
                        <$type_1>::enter_poll_span(
                            &mut self.handlers.$field_1,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                    $( if is_enabled[$field_i] {
                        let context = context.clone();
                        <$type_i>::enter_poll_span(
                            &mut self.handlers.$field_i,
                            descriptor,
                            span_id,
                            context
                        );
                    } )*
                    if is_enabled[$field_n] {
                        <$type_n>::enter_poll_span(
                            &mut self.handlers.$field_n,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                }
            }

            #[inline]
            fn pending_poll_span(
                &mut self,
                descriptor: &'static Descriptor,
                span_id: SpanId,
                context: Context,
            ) {
                let is_enabled = [
                    !self.disabled_spans[ $field_1 ].contains(&span_id),
                    $( !self.disabled_spans[ $field_i ].contains(&span_id), )*
                    !self.disabled_spans[ $field_n ].contains(&span_id),
                ];

                #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
                let num_enabled = (
                    usize::from(is_enabled[$field_1])
                    $( + usize::from(is_enabled[$field_i]) )*
                    + usize::from(is_enabled[$field_n])
                );

                if num_enabled == 1 {
                    if is_enabled[$field_1] {
                        <$type_1>::pending_poll_span(
                            &mut self.handlers.$field_1,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                    $( else if is_enabled[$field_i] {
                        <$type_i>::pending_poll_span(
                            &mut self.handlers.$field_i,
                            descriptor,
                            span_id,
                            context
                        );
                    } )*
                    else if is_enabled[$field_n] {
                        <$type_n>::pending_poll_span(
                            &mut self.handlers.$field_n,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                } else if num_enabled > 1 {
                    if is_enabled[$field_1] {
                        let context = context.clone();
                        <$type_1>::pending_poll_span(
                            &mut self.handlers.$field_1,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                    $( if is_enabled[$field_i] {
                        let context = context.clone();
                        <$type_i>::pending_poll_span(
                            &mut self.handlers.$field_i,
                            descriptor,
                            span_id,
                            context
                        );
                    } )*
                    if is_enabled[$field_n] {
                        <$type_n>::pending_poll_span(
                            &mut self.handlers.$field_n,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                }
            }

            #[inline]
            fn ready_poll_span(
                &mut self,
                descriptor: &'static Descriptor,
                span_id: SpanId,
                context: Context,
            ) {
                let is_enabled = [
                    !self.disabled_spans[ $field_1 ].contains(&span_id),
                    $( !self.disabled_spans[ $field_i ].contains(&span_id), )*
                    !self.disabled_spans[ $field_n ].contains(&span_id),
                ];

                #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
                let num_enabled = (
                    usize::from(is_enabled[$field_1])
                    $( + usize::from(is_enabled[$field_i]) )*
                    + usize::from(is_enabled[$field_n])
                );

                if num_enabled == 1 {
                    if is_enabled[$field_1] {
                        <$type_1>::ready_poll_span(
                            &mut self.handlers.$field_1,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                    $( else if is_enabled[$field_i] {
                        <$type_i>::ready_poll_span(
                            &mut self.handlers.$field_i,
                            descriptor,
                            span_id,
                            context
                        );
                    } )*
                    else if is_enabled[$field_n] {
                        <$type_n>::ready_poll_span(
                            &mut self.handlers.$field_n,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                } else if num_enabled > 1 {
                    if is_enabled[$field_1] {
                        let context = context.clone();
                        <$type_1>::ready_poll_span(
                            &mut self.handlers.$field_1,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                    $( if is_enabled[$field_i] {
                        let context = context.clone();
                        <$type_i>::ready_poll_span(
                            &mut self.handlers.$field_i,
                            descriptor,
                            span_id,
                            context
                        );
                    } )*
                    if is_enabled[$field_n] {
                        <$type_n>::ready_poll_span(
                            &mut self.handlers.$field_n,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                }
            }
        }

        impl< $type_1 $(, $type_i )*, $type_n , Context> Handler<Context>
            for BorrowingFanout<
                ( $type_1, $( $type_i, )* $type_n, ),
                { count!( $field_1 $($field_i)* $field_n ) }
            >
        where
            $type_1: BorrowingHandler<Context>,
            $( $type_i: BorrowingHandler<Context>, )*
            $type_n: BorrowingHandler<Context>,
            Context: Clone + ContextArgs,
        {
            type EnrichedContext = (
                Option<$type_1::EnrichedContext>,
                $( Option<$type_i::EnrichedContext>, )*
                Option<$type_n::EnrichedContext>,
            );

            #[inline]
            fn try_enable(
                &self,
                descriptor: &'static Descriptor,
                context: Context,
            ) -> Option<Self::EnrichedContext> {
                let context = (
                    <$type_1>::try_enable(
                        &self.handlers.$field_1,
                        descriptor,
                        context.clone()
                    ),
                    $( <$type_i>::try_enable(
                        &self.handlers.$field_i,
                        descriptor,
                        context.clone()
                    ), )*
                    <$type_n>::try_enable(
                        &self.handlers.$field_n,
                        descriptor,
                        context
                    ),
                );
                (
                    context.$field_1.is_some()
                    $( || context.$field_i.is_some() )*
                    || context.$field_n.is_some()
                ).then_some(context)
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
                #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
                let num_active = (
                    usize::from(context.$field_1.is_some())
                    $( + usize::from(context.$field_i.is_some()) )*
                    + usize::from(context.$field_n.is_some())
                );

                if num_active == 1 {
                    if let Some(context) = context.$field_1 {
                        <$type_1>::log(
                            &mut self.handlers.$field_1,
                            descriptor,
                            args,
                            span_id,
                            context,
                        );
                    }
                    $( else if let Some(context) = context.$field_i {
                        <$type_i>::log(
                            &mut self.handlers.$field_i,
                            descriptor,
                            args,
                            span_id,
                            context,
                        );
                    } )*
                    else if let Some(context) = context.$field_n {
                        <$type_n>::log(
                            &mut self.handlers.$field_n,
                            descriptor,
                            args,
                            span_id,
                            context,
                        );
                    }
                } else if num_active > 1 {
                    let args = args.into_sendable();
                    if let Some(context) = context.$field_1 {
                        <$type_1>::log_borrowed(
                            &mut self.handlers.$field_1,
                            descriptor,
                            &args,
                            span_id,
                            &context,
                        );
                    }
                    $( if let Some(context) = context.$field_i {
                        <$type_i>::log_borrowed(
                            &mut self.handlers.$field_i,
                            descriptor,
                            &args,
                            span_id,
                            &context,
                        );
                    } )*
                    if let Some(context) = context.$field_n {
                        <$type_n>::log_borrowed(
                            &mut self.handlers.$field_n,
                            descriptor,
                            &args,
                            span_id,
                            &context,
                        );
                    }
                }
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
                if context.$field_1.is_none() {
                    let _is_newly_inserted: bool =
                        self.disabled_spans[$field_1].insert(span_id);
                }
                $(
                    if context.$field_i.is_none() {
                        let _is_newly_inserted: bool =
                            self.disabled_spans[$field_i].insert(span_id);
                    }
                )*
                if context.$field_n.is_none() {
                    let _is_newly_inserted: bool =
                        self.disabled_spans[$field_n].insert(span_id);
                }

                #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
                let num_active = (
                    usize::from(context.$field_1.is_some())
                    $( + usize::from(context.$field_i.is_some()) )*
                    + usize::from(context.$field_n.is_some())
                );

                if num_active == 1 {
                    if let Some(context) = context.$field_1 {
                        <$type_1>::enter_span(
                            &mut self.handlers.$field_1,
                            descriptor,
                            args,
                            parent_span_id,
                            span_id,
                            span_kind,
                            context,
                        );
                    }
                    $( else if let Some(context) = context.$field_i {
                        <$type_i>::enter_span(
                            &mut self.handlers.$field_i,
                            descriptor,
                            args,
                            parent_span_id,
                            span_id,
                            span_kind,
                            context,
                        );
                    } )*
                    else if let Some(context) = context.$field_n {
                        <$type_n>::enter_span(
                            &mut self.handlers.$field_n,
                            descriptor,
                            args,
                            parent_span_id,
                            span_id,
                            span_kind,
                            context,
                        );
                    }
                } else if num_active > 1 {
                    let args = args.into_sendable();
                    if let Some(context) = context.$field_1 {
                        <$type_1>::enter_span_borrowed(
                            &mut self.handlers.$field_1,
                            descriptor,
                            &args,
                            parent_span_id,
                            span_id,
                            span_kind,
                            &context,
                        );
                    }
                    $( if let Some(context) = context.$field_i {
                        <$type_i>::enter_span_borrowed(
                            &mut self.handlers.$field_i,
                            descriptor,
                            &args,
                            parent_span_id,
                            span_id,
                            span_kind,
                            &context,
                        );
                    } )*
                    if let Some(context) = context.$field_n {
                        <$type_n>::enter_span_borrowed(
                            &mut self.handlers.$field_n,
                            descriptor,
                            &args,
                            parent_span_id,
                            span_id,
                            span_kind,
                            &context,
                        );
                    }
                }
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

                #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
                let num_active = (
                    usize::from(context.$field_1.is_some())
                    $( + usize::from(context.$field_i.is_some()) )*
                    + usize::from(context.$field_n.is_some())
                );

                if num_active == 1 {
                    if let Some(context) = context.$field_1 {
                        <$type_1>::log_static(
                            &mut self.handlers.$field_1,
                            descriptor,
                            args,
                            span_id,
                            context,
                        );
                    }
                    $( else if let Some(context) = context.$field_i {
                        <$type_i>::log_static(&mut self.handlers.$field_i,
                            descriptor,
                            args,
                            span_id,
                            context,
                        );
                    } )*
                    else if let Some(context) = context.$field_n {
                        <$type_n>::log_static(
                            &mut self.handlers.$field_n,
                            descriptor,
                            args,
                            span_id,
                            context,
                        );
                    }
                } else if num_active > 1 {
                    if let Some(context) = context.$field_1 {
                        <$type_1>::log_borrowed(&mut self.handlers.$field_1,
                            descriptor,
                            &args,
                            span_id,
                            &context,
                        );
                    }
                    $( if let Some(context) = context.$field_i {
                        <$type_i>::log_borrowed(&mut self.handlers.$field_i,
                            descriptor,
                            &args,
                            span_id,
                            &context,
                        );
                    } )*
                    if let Some(context) = context.$field_n {
                        <$type_n>::log_borrowed(
                            &mut self.handlers.$field_n,
                            descriptor,
                            &args,
                            span_id,
                            &context,
                        );
                    }
                }
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
                if context.$field_1.is_none() {
                    let _is_newly_inserted: bool =
                        self.disabled_spans[$field_1].insert(span_id);
                }
                $(
                    if context.$field_i.is_none() {
                        let _is_newly_inserted: bool =
                            self.disabled_spans[$field_i].insert(span_id);
                    }
                )*
                if context.$field_n.is_none() {
                    let _is_newly_inserted: bool =
                        self.disabled_spans[$field_n].insert(span_id);
                }

                #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
                let num_active = (
                    usize::from(context.$field_1.is_some())
                    $( + usize::from(context.$field_i.is_some()) )*
                    + usize::from(context.$field_n.is_some())
                );

                if num_active == 1 {
                    if let Some(context) = context.$field_1 {
                        <$type_1>::enter_span_static(
                            &mut self.handlers.$field_1,
                            descriptor,
                            args,
                            parent_span_id,
                            span_id,
                            span_kind,
                            context,
                        );
                    }
                    $( else if let Some(context) = context.$field_i {
                        <$type_i>::enter_span_static(
                            &mut self.handlers.$field_i,
                            descriptor,
                            args,
                            parent_span_id,
                            span_id,
                            span_kind,
                            context,
                        );
                    } )*
                    else if let Some(context) = context.$field_n {
                        <$type_n>::enter_span_static(
                            &mut self.handlers.$field_n,
                            descriptor,
                            args,
                            parent_span_id,
                            span_id,
                            span_kind,
                            context,
                        );
                    }
                } else if num_active > 1 {
                    if let Some(context) = context.$field_1 {
                        <$type_1>::enter_span_borrowed(
                            &mut self.handlers.$field_1,
                            descriptor,
                            &args,
                            parent_span_id,
                            span_id,
                            span_kind,
                            &context,
                        );
                    }
                    $( if let Some(context) = context.$field_i {
                        <$type_i>::enter_span_borrowed(
                            &mut self.handlers.$field_i,
                            descriptor,
                            &args,
                            parent_span_id,
                            span_id,
                            span_kind,
                            &context,
                        );
                    } )*
                    if let Some(context) = context.$field_n {
                        <$type_n>::enter_span_borrowed(
                            &mut self.handlers.$field_n,
                            descriptor,
                            &args,
                            parent_span_id,
                            span_id,
                            span_kind,
                            &context,
                        );
                    }
                }
            }
        }

        impl< $type_1 $(, $type_i )*, $type_n , Context> BorrowingHandler<Context>
            for BorrowingFanout<
                ( $type_1, $( $type_i, )* $type_n, ),
                { count!( $field_1 $($field_i)* $field_n ) }
            >
        where
            $type_1: BorrowingHandler<Context>,
            $( $type_i: BorrowingHandler<Context>, )*
            $type_n: BorrowingHandler<Context>,
            Context: Clone + ContextArgs,
        {
            #[inline]
            fn log_borrowed<Args>(
                &mut self,
                descriptor: &'static Descriptor,
                args: &Args,
                span_id: Option<SpanId>,
                context: &Self::EnrichedContext,
            ) where
                Args: StaticLogArgs,
            {
                if let Some(context) = &context.$field_1 {
                    <$type_1>::log_borrowed(
                        &mut self.handlers.$field_1,
                        descriptor,
                        args,
                        span_id,
                        context,
                    );
                }
                $( if let Some(context) = &context.$field_i {
                    <$type_i>::log_borrowed(
                        &mut self.handlers.$field_i,
                        descriptor,
                        args,
                        span_id,
                        context,
                    );
                } )*
                if let Some(context) = &context.$field_n {
                    <$type_n>::log_borrowed(
                        &mut self.handlers.$field_n,
                        descriptor,
                        args,
                        span_id,
                        context,
                    );
                }
            }

            #[inline]
            fn enter_span_borrowed<Args>(
                &mut self,
                descriptor: &'static Descriptor,
                args: &Args,
                parent_span_id: Option<SpanId>,
                span_id: SpanId,
                span_kind: SpanKind,
                context: &Self::EnrichedContext,
            ) where
                Args: StaticLogArgs,
            {
                if context.$field_1.is_none() {
                    let _is_newly_inserted: bool =
                        self.disabled_spans[$field_1].insert(span_id);
                }
                $(
                    if context.$field_i.is_none() {
                        let _is_newly_inserted: bool =
                            self.disabled_spans[$field_i].insert(span_id);
                    }
                )*
                if context.$field_n.is_none() {
                    let _is_newly_inserted: bool =
                        self.disabled_spans[$field_n].insert(span_id);
                }

                if let Some(context) = &context.$field_1 {
                    <$type_1>::enter_span_borrowed(
                        &mut self.handlers.$field_1,
                        descriptor,
                        args,
                        parent_span_id,
                        span_id,
                        span_kind,
                        context,
                    );
                }
                $( if let Some(context) = &context.$field_i {
                    <$type_i>::enter_span_borrowed(
                        &mut self.handlers.$field_i,
                        descriptor,
                        args,
                        parent_span_id,
                        span_id,
                        span_kind,
                        context,
                    );
                } )*
                if let Some(context) = &context.$field_n {
                    <$type_n>::enter_span_borrowed(
                        &mut self.handlers.$field_n,
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

        impl< $type_1 $(, $type_i )*, $type_n , Context> SharedHandler<Context>
            for BorrowingFanout<
                ( $type_1, $( $type_i, )* $type_n, ),
                { count!( $field_1 $($field_i)* $field_n ) }
            >
        where
            $type_1: BorrowingHandler<Context> + SharedHandler<Context>,
            $( $type_i: BorrowingHandler<Context> + SharedHandler<Context>, )*
            $type_n: BorrowingHandler<Context> + SharedHandler<Context>,
            Context: Clone + ContextArgs,
        {
        }

        impl< $type_1 $(, $type_i )*, $type_n> BaseExternalHandler
            for BorrowingFanout<
                ( $type_1, $( $type_i, )* $type_n, ),
                { count!( $field_1 $($field_i)* $field_n ) }
            >
        where
            $type_1: BaseExternalHandler,
            $( $type_i: BaseExternalHandler, )*
            $type_n: BaseExternalHandler,
        {
            #[inline]
            fn max_level_external(&self) -> LevelFilter {
                <$type_1>::max_level_external(&self.handlers.$field_1)
                $( .max(<$type_i>::max_level_external(&self.handlers.$field_i)) )*
                .max(<$type_n>::max_level_external(&self.handlers.$field_n))
            }
        }

        impl< $type_1 $(, $type_i )*, $type_n , Context> ExternalHandler<Context>
            for BorrowingFanout<
                ( $type_1, $( $type_i, )* $type_n, ),
                { count!( $field_1 $($field_i)* $field_n ) }
            >
        where
            $type_1: BorrowingHandler<Context> + ExternalHandler<Context>,
            $( $type_i: BorrowingHandler<Context> + ExternalHandler<Context>, )*
            $type_n: BorrowingHandler<Context> + ExternalHandler<Context>,
            Context: Clone + ContextArgs,
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
                let context = (
                    <$type_1>::try_enable_external(
                        &self.handlers.$field_1,
                        descriptor,
                        context.clone()
                    ),
                    $( <$type_i>::try_enable_external(
                        &self.handlers.$field_i,
                        descriptor,
                        context.clone()
                    ), )*
                    <$type_n>::try_enable_external(
                        &self.handlers.$field_n,
                        descriptor,
                        context
                    ),
                );
                (
                    context.$field_1.is_some()
                    $( || context.$field_i.is_some() )*
                    || context.$field_n.is_some()
                ).then_some(context)
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
                if let Some(context) = context.$field_1 {
                    <$type_1>::log_external(
                        &mut self.handlers.$field_1,
                        descriptor,
                        args,
                        span_id,
                        context
                    );
                }
                $( if let Some(context) = context.$field_i {
                    <$type_i>::log_external(
                        &mut self.handlers.$field_i,
                        descriptor,
                        args,
                        span_id,
                        context
                    );
                } )*
                if let Some(context) = context.$field_n {
                    <$type_n>::log_external(
                        &mut self.handlers.$field_n,
                        descriptor,
                        args,
                        span_id,
                        context
                    );
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
                if context.$field_1.is_none() {
                    let _is_newly_inserted: bool =
                        self.disabled_spans[$field_1].insert(span_id);
                }
                $( if context.$field_i.is_none() {
                    let _is_newly_inserted: bool =
                        self.disabled_spans[$field_i].insert(span_id);
                } )*
                if context.$field_n.is_none() {
                    let _is_newly_inserted: bool =
                        self.disabled_spans[$field_n].insert(span_id);
                }

                if let Some(context) = context.$field_1 {
                    <$type_1>::new_span_external(
                        &mut self.handlers.$field_1,
                        descriptor,
                        args,
                        parent_span_id,
                        span_id,
                        context,
                    );
                }
                $( if let Some(context) = context.$field_i {
                    <$type_i>::new_span_external(
                        &mut self.handlers.$field_i,
                        descriptor,
                        args,
                        parent_span_id,
                        span_id,
                        context,
                    );
                } )*
                if let Some(context) = context.$field_n {
                    <$type_n>::new_span_external(
                        &mut self.handlers.$field_n,
                        descriptor,
                        args,
                        parent_span_id,
                        span_id,
                        context,
                    );
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
                if context.$field_1.is_none() {
                    let _is_newly_inserted: bool =
                        self.disabled_spans[$field_1].insert(span_id);
                }
                $( if context.$field_i.is_none() {
                    let _is_newly_inserted: bool =
                        self.disabled_spans[$field_i].insert(span_id);
                } )*
                if context.$field_n.is_none() {
                    let _is_newly_inserted: bool =
                        self.disabled_spans[$field_n].insert(span_id);
                }

                if let Some(context) = context.$field_1 {
                    <$type_1>::enter_span_external(
                        &mut self.handlers.$field_1,
                        descriptor,
                        args,
                        parent_span_id,
                        span_id,
                        context,
                    );
                }
                $( if let Some(context) = context.$field_i {
                    <$type_i>::enter_span_external(
                        &mut self.handlers.$field_i,
                        descriptor,
                        args,
                        parent_span_id,
                        span_id,
                        context,
                    );
                } )*
                if let Some(context) = context.$field_n {
                    <$type_n>::enter_span_external(
                        &mut self.handlers.$field_n,
                        descriptor,
                        args,
                        parent_span_id,
                        span_id,
                        context,
                    );
                }
            }

            #[inline]
            fn exit_span_external<Desc>(
                &mut self,
                descriptor: &Desc,
                span_id: SpanId,
                context: Context
            )
            where
                Desc: ExternalDescriptor,
            {
                let was_enabled = [
                    self.disabled_spans[$field_1].remove(&span_id).is_none(),
                    $( self.disabled_spans[$field_i].remove(&span_id).is_none(), )*
                    self.disabled_spans[$field_n].remove(&span_id).is_none(),
                ];

                #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
                let num_enabled = (
                    usize::from(was_enabled[$field_1])
                    $( + usize::from(was_enabled[$field_i]) )*
                    + usize::from(was_enabled[$field_n])
                );

                if num_enabled == 1 {
                    if was_enabled[$field_1] {
                        <$type_1>::exit_span_external(
                            &mut self.handlers.$field_1,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                    $( else if was_enabled[$field_i] {
                        <$type_i>::exit_span_external(
                            &mut self.handlers.$field_i,
                            descriptor,
                            span_id,
                            context
                        );
                    } )*
                    else if was_enabled[$field_n] {
                        <$type_n>::exit_span_external(
                            &mut self.handlers.$field_n,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                } else if num_enabled > 1 {
                    if was_enabled[$field_1] {
                        let context = context.clone();
                        <$type_1>::exit_span_external(
                            &mut self.handlers.$field_1,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                    $( if was_enabled[$field_i] {
                        let context = context.clone();
                        <$type_i>::exit_span_external(
                            &mut self.handlers.$field_i,
                            descriptor,
                            span_id,
                            context
                        );
                    } )*
                    if was_enabled[$field_n] {
                        <$type_n>::exit_span_external(
                            &mut self.handlers.$field_n,
                            descriptor,
                            span_id,
                            context
                        );
                    }
                }
            }

            #[inline]
            fn close_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
            where
                Desc: ExternalDescriptor,
            {
                let was_enabled = [
                    self.disabled_spans[$field_1].remove(&span_id).is_none(),
                    $( self.disabled_spans[$field_i].remove(&span_id).is_none(), )*
                    self.disabled_spans[$field_n].remove(&span_id).is_none(),
                ];

                #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
                let num_enabled = (
                    usize::from(was_enabled[$field_1])
                    $( + usize::from(was_enabled[$field_i]) )*
                    + usize::from(was_enabled[$field_n])
                );

                if num_enabled == 1 {
                    if was_enabled[$field_1] {
                        <$type_1>::close_span_external(&mut self.handlers.$field_1, descriptor, span_id, context);
                    }
                    $( else if was_enabled[$field_i] {
                        <$type_i>::close_span_external(&mut self.handlers.$field_i, descriptor, span_id, context);
                    } )*
                    else if was_enabled[$field_n] {
                        <$type_n>::close_span_external(&mut self.handlers.$field_n, descriptor, span_id, context);
                    }
                } else if num_enabled > 1 {
                    if was_enabled[$field_1] {
                        let context = context.clone();
                        <$type_1>::close_span_external(&mut self.handlers.$field_1, descriptor, span_id, context);
                    }
                    $( if was_enabled[$field_i] {
                        let context = context.clone();
                        <$type_i>::close_span_external(&mut self.handlers.$field_i, descriptor, span_id, context);
                    } )*
                    if was_enabled[$field_n] {
                        <$type_n>::close_span_external(&mut self.handlers.$field_n, descriptor, span_id, context);
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
                let is_enabled = [
                    !self.disabled_spans[$field_1].contains(&span_id),
                    $( !self.disabled_spans[$field_i].contains(&span_id), )*
                    !self.disabled_spans[$field_n].contains(&span_id),
                ];

                #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
                let num_enabled = (
                    usize::from(is_enabled[$field_1])
                    $( + usize::from(is_enabled[$field_i]) )*
                    + usize::from(is_enabled[$field_n])
                );

                if num_enabled == 1 {
                    if is_enabled[$field_1] {
                        <$type_1>::record_span_external(&mut self.handlers.$field_1, descriptor, args, span_id, context);
                    }
                    $( else if is_enabled[$field_i] {
                        <$type_i>::record_span_external(&mut self.handlers.$field_i, descriptor, args, span_id, context);
                    } )*
                    else if is_enabled[$field_n] {
                        <$type_n>::record_span_external(&mut self.handlers.$field_n, descriptor, args, span_id, context);
                    }
                } else if num_enabled > 1 {
                    if is_enabled[$field_1] {
                        let context = context.clone();
                        <$type_1>::record_span_external(&mut self.handlers.$field_1, descriptor, args, span_id, context);
                    }
                    $( if is_enabled[$field_i] {
                        let context = context.clone();
                        <$type_i>::record_span_external(&mut self.handlers.$field_i, descriptor, args, span_id, context);
                    } )*
                    if is_enabled[$field_n] {
                        <$type_n>::record_span_external(&mut self.handlers.$field_n, descriptor, args, span_id, context);
                    }
                }
            }

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
                let is_enabled = [
                    !self.disabled_spans[$field_1].contains(&follower_span_id),
                    $( !self.disabled_spans[$field_i].contains(&follower_span_id), )*
                    !self.disabled_spans[$field_n].contains(&follower_span_id),
                ];

                #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
                let num_enabled = (
                    usize::from(is_enabled[$field_1])
                    $( + usize::from(is_enabled[$field_i]) )*
                    + usize::from(is_enabled[$field_n])
                );

                if num_enabled == 1 {
                    if is_enabled[$field_1] {
                        <$type_1>::link_span_external(&mut self.handlers.$field_1, descriptor, followed_span_id, follower_span_id, context);
                    }
                    $( else if is_enabled[$field_i] {
                        <$type_i>::link_span_external(&mut self.handlers.$field_i, descriptor, followed_span_id, follower_span_id, context);
                    } )*
                    else if is_enabled[$field_n] {
                        <$type_n>::link_span_external(&mut self.handlers.$field_n, descriptor, followed_span_id, follower_span_id, context);
                    }
                } else if num_enabled > 1 {
                    if is_enabled[$field_1] {
                        let context = context.clone();
                        <$type_1>::link_span_external(&mut self.handlers.$field_1, descriptor, followed_span_id, follower_span_id, context);
                    }
                    $( if is_enabled[$field_i] {
                        let context = context.clone();
                        <$type_i>::link_span_external(&mut self.handlers.$field_i, descriptor, followed_span_id, follower_span_id, context);
                    } )*
                    if is_enabled[$field_n] {
                        <$type_n>::link_span_external(&mut self.handlers.$field_n, descriptor, followed_span_id, follower_span_id, context);
                    }
                }
            }
        }
    };
}

macro_rules! count {
    () => {
        0
    };
    ( $tt:tt ) => {
        1
    };
    ( $first:tt $($rest:tt)* ) => {
        1 + count!( $($rest)* )
    };
}

// Generate impls for tuples of 2..=4 elements.
impl_variadics! {
    [] [ (0 T1) (1 T2) (2 T3) (3 T4) ]
}

#[cfg(test)]
mod tests {
    use polog::{
        BaseHandler, BorrowingHandler, Descriptor, Discard, DynHandler, Handler, Level,
        LevelFilter, PollInstrument, SpanKind, log_descriptor_ref,
    };
    use polog_test_helpers::accept_all::AcceptAll;
    use polog_test_helpers::panic::catch_unwind_on_thread;
    use polog_test_helpers::tracker::Tracker;

    use crate::filters::MaxLevel;
    use crate::{
        BorrowingFanout1, BorrowingFanout2, BorrowingFanout3, BorrowingFanout4, Fanout2, Fanout4,
        Filter,
    };

    const BASIC: &[&str] = &[
        "try_enable",
        "log",
        "try_enable",
        // The span id is generated by the other logger.
        "enter_span(nested)",
        "exit_span(nested)",
    ];

    const BASIC_AND_GEN: &[&str] = &[
        "try_enable",
        "log",
        "try_enable",
        "generate_span_id",
        "enter_span(nested)",
        "exit_span(nested)",
    ];

    const STATIC_AND_GEN: &[&str] = &[
        "try_enable",
        "log_static",
        "try_enable",
        "generate_span_id",
        "enter_span_static(nested)",
        "exit_span(nested)",
    ];

    const BORROWED: &[&str] = &[
        "try_enable",
        "log_borrowed",
        "try_enable",
        // The span id is generated by the other logger.
        "enter_span_borrowed(nested)",
        "exit_span(nested)",
    ];

    const BORROWED_AND_GEN: &[&str] = &[
        "try_enable",
        "log_borrowed",
        "try_enable",
        "generate_span_id",
        "enter_span_borrowed(nested)",
        "exit_span(nested)",
    ];

    #[test]
    fn test_borrowing_fanout1() {
        let tracker1 = Tracker::default();

        let mut logger: BorrowingFanout1<Discard> = BorrowingFanout1::default();
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), [""; 0]);

        let mut logger = BorrowingFanout1::new((tracker1.clone(),));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BASIC_AND_GEN);

        let mut logger = Fanout2::new((BorrowingFanout1::new((tracker1.clone(),)), AcceptAll));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), STATIC_AND_GEN);

        let mut logger =
            BorrowingFanout2::new((BorrowingFanout1::new((tracker1.clone(),)), AcceptAll));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BORROWED_AND_GEN);
    }

    #[test]
    fn test_borrowing_fanout2() {
        let tracker1 = Tracker::default();
        let tracker2 = Tracker::default();

        let mut logger: BorrowingFanout2<Discard, Discard> = BorrowingFanout2::default();
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);

        let mut logger = BorrowingFanout2::new((tracker1.clone(), tracker2.clone()));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BORROWED_AND_GEN);
        assert_eq!(tracker2.take_events(), BORROWED);

        let mut logger = BorrowingFanout2::new((tracker1.clone(), Discard));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BASIC_AND_GEN);

        let mut logger = BorrowingFanout2::new((Discard, tracker1.clone()));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BASIC);
    }

    #[test]
    fn test_borrowing_fanout3() {
        let tracker1 = Tracker::default();
        let tracker2 = Tracker::default();
        let tracker3 = Tracker::default();

        let mut logger: BorrowingFanout3<Discard, Discard, Discard> = BorrowingFanout3::default();
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);

        let mut logger =
            BorrowingFanout3::new((tracker1.clone(), tracker2.clone(), tracker3.clone()));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BORROWED_AND_GEN);
        assert_eq!(tracker2.take_events(), BORROWED);
        assert_eq!(tracker3.take_events(), BORROWED);

        let mut logger = BorrowingFanout3::new((tracker1.clone(), Discard, Discard));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BASIC_AND_GEN);

        let mut logger = BorrowingFanout3::new((Discard, tracker1.clone(), Discard));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BASIC);

        let mut logger = BorrowingFanout3::new((Discard, Discard, tracker1.clone()));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BASIC);
    }

    #[test]
    fn test_borrowing_fanout4() {
        let tracker1 = Tracker::default();
        let tracker2 = Tracker::default();
        let tracker3 = Tracker::default();
        let tracker4 = Tracker::default();

        let mut logger: BorrowingFanout4<Discard, Discard, Discard, Discard> =
            BorrowingFanout4::default();
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);

        let mut logger = BorrowingFanout4::new((
            tracker1.clone(),
            tracker2.clone(),
            tracker3.clone(),
            tracker4.clone(),
        ));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BORROWED_AND_GEN);
        assert_eq!(tracker2.take_events(), BORROWED);
        assert_eq!(tracker3.take_events(), BORROWED);
        assert_eq!(tracker4.take_events(), BORROWED);

        let mut logger = BorrowingFanout4::new((tracker1.clone(), Discard, Discard, Discard));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BASIC_AND_GEN);

        let mut logger = BorrowingFanout4::new((Discard, tracker1.clone(), Discard, Discard));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BASIC);

        let mut logger = BorrowingFanout4::new((Discard, Discard, tracker1.clone(), Discard));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BASIC);

        let mut logger = BorrowingFanout4::new((Discard, Discard, Discard, tracker1.clone()));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BASIC);
    }

    #[test]
    fn test_borrowing_fanout2_nested() {
        let tracker1 = Tracker::default();
        let tracker2 = Tracker::default();
        let tracker3 = Tracker::default();
        let tracker4 = Tracker::default();

        let mut logger = BorrowingFanout2::new((
            BorrowingFanout2::new((tracker1.clone(), tracker2.clone())),
            Fanout2::new((tracker3.clone(), tracker4.clone())),
        ));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BORROWED_AND_GEN);
        assert_eq!(tracker2.take_events(), BORROWED);
        assert_eq!(tracker3.take_events(), BORROWED);
        assert_eq!(tracker4.take_events(), BORROWED);

        let mut logger = BorrowingFanout2::new((
            BorrowingFanout2::new((tracker1.clone(), tracker2.clone())),
            BorrowingFanout2::new((tracker3.clone(), tracker4.clone())),
        ));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BORROWED_AND_GEN);
        assert_eq!(tracker2.take_events(), BORROWED);
        assert_eq!(tracker3.take_events(), BORROWED);
        assert_eq!(tracker4.take_events(), BORROWED);

        let mut logger = BorrowingFanout2::new((
            Fanout2::new((tracker1.clone(), Discard)),
            Fanout2::new((tracker2.clone(), Discard)),
        ));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BORROWED_AND_GEN);
        assert_eq!(tracker2.take_events(), BORROWED);

        let mut logger = BorrowingFanout2::new((
            Fanout2::new((Discard, tracker1.clone())),
            Fanout2::new((Discard, tracker2.clone())),
        ));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BORROWED);
        assert_eq!(tracker2.take_events(), BORROWED);
    }

    #[test]
    fn test_borrowing_fanout4_nested() {
        let tracker1 = Tracker::default();
        let tracker2 = Tracker::default();
        let tracker3 = Tracker::default();
        let tracker4 = Tracker::default();

        let mut logger = BorrowingFanout4::new((
            BorrowingFanout4::new((tracker1.clone(), Discard, Discard, Discard)),
            BorrowingFanout4::new((Discard, tracker2.clone(), Discard, Discard)),
            BorrowingFanout4::new((Discard, Discard, tracker3.clone(), Discard)),
            BorrowingFanout4::new((Discard, Discard, Discard, tracker4.clone())),
        ));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BORROWED_AND_GEN);
        assert_eq!(tracker2.take_events(), BORROWED);
        assert_eq!(tracker3.take_events(), BORROWED);
        assert_eq!(tracker4.take_events(), BORROWED);

        let mut logger = BorrowingFanout4::new((
            Fanout4::new((tracker1.clone(), Discard, Discard, Discard)),
            Fanout4::new((Discard, tracker2.clone(), Discard, Discard)),
            Fanout4::new((Discard, Discard, tracker3.clone(), Discard)),
            Fanout4::new((Discard, Discard, Discard, tracker4.clone())),
        ));
        polog::info!(&mut logger, "Message");
        let span = polog::info_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), BORROWED_AND_GEN);
        assert_eq!(tracker2.take_events(), BORROWED);
        assert_eq!(tracker3.take_events(), BORROWED);
        assert_eq!(tracker4.take_events(), BORROWED);
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[tokio::test]
    async fn test_borrowing_fanout_async() {
        const BASIC: &[&str] = &[
            "try_enable",
            // The span id is generated by the other logger.
            "enter_span(detached)",
            "enter_poll_span",
            "try_enable",
            "log",
            "pending_poll_span",
            "enter_poll_span",
            "try_enable",
            "log",
            "ready_poll_span",
            "exit_span(detached)",
        ];

        const BASIC_AND_GEN: &[&str] = &[
            "try_enable",
            "generate_span_id",
            "enter_span(detached)",
            "enter_poll_span",
            "try_enable",
            "log",
            "pending_poll_span",
            "enter_poll_span",
            "try_enable",
            "log",
            "ready_poll_span",
            "exit_span(detached)",
        ];

        const BORROWED: &[&str] = &[
            "try_enable",
            // The span id is generated by the other logger.
            "enter_span_borrowed(detached)",
            "enter_poll_span",
            "try_enable",
            "log_borrowed",
            "pending_poll_span",
            "enter_poll_span",
            "try_enable",
            "log_borrowed",
            "ready_poll_span",
            "exit_span(detached)",
        ];

        const BORROWED_AND_GEN: &[&str] = &[
            "try_enable",
            "generate_span_id",
            "enter_span_borrowed(detached)",
            "enter_poll_span",
            "try_enable",
            "log_borrowed",
            "pending_poll_span",
            "enter_poll_span",
            "try_enable",
            "log_borrowed",
            "ready_poll_span",
            "exit_span(detached)",
        ];

        let tracker1 = Tracker::default();
        let tracker2 = Tracker::default();
        let tracker3 = Tracker::default();
        let tracker4 = Tracker::default();

        let mut logger = BorrowingFanout1::new((tracker1.clone(),));
        let deferred = polog::deferred_info_span!(&mut logger, "Polled");
        let task = tokio::task::spawn(example_async_fn.poll_instrument(deferred));
        task.await.unwrap();

        assert_eq!(tracker1.take_events(), BASIC_AND_GEN);

        let mut logger = BorrowingFanout4::new((
            BorrowingFanout4::new((tracker1.clone(), Discard, Discard, Discard)),
            BorrowingFanout4::new((Discard, tracker2.clone(), Discard, Discard)),
            BorrowingFanout4::new((Discard, Discard, tracker3.clone(), Discard)),
            BorrowingFanout4::new((Discard, Discard, Discard, tracker4.clone())),
        ));
        let deferred = polog::deferred_info_span!(&mut logger, "Polled");
        let task = tokio::task::spawn(example_async_fn.poll_instrument(deferred));
        task.await.unwrap();
        assert_eq!(tracker1.take_events(), BORROWED_AND_GEN);
        assert_eq!(tracker2.take_events(), BORROWED);
        assert_eq!(tracker3.take_events(), BORROWED);
        assert_eq!(tracker4.take_events(), BORROWED);

        let mut logger = BorrowingFanout4::new((
            Fanout4::new((tracker1.clone(), Discard, Discard, Discard)),
            Fanout4::new((Discard, tracker2.clone(), Discard, Discard)),
            Fanout4::new((Discard, Discard, tracker3.clone(), Discard)),
            Fanout4::new((Discard, Discard, Discard, tracker4.clone())),
        ));
        let deferred = polog::deferred_info_span!(&mut logger, "Polled");
        let task = tokio::task::spawn(example_async_fn.poll_instrument(deferred));
        task.await.unwrap();
        assert_eq!(tracker1.take_events(), BORROWED_AND_GEN);
        assert_eq!(tracker2.take_events(), BORROWED);
        assert_eq!(tracker3.take_events(), BORROWED);
        assert_eq!(tracker4.take_events(), BORROWED);

        let mut logger = BorrowingFanout4::new((
            BorrowingFanout4::new((Discard, Discard, Discard, Discard)),
            BorrowingFanout4::new((Discard, Discard, Discard, Discard)),
            BorrowingFanout4::new((Discard, Discard, Discard, Discard)),
            BorrowingFanout4::new((Discard, Discard, Discard, tracker1.clone())),
        ));
        let deferred = polog::deferred_info_span!(&mut logger, "Polled");
        let task = tokio::task::spawn(example_async_fn.poll_instrument(deferred));
        task.await.unwrap();

        assert_eq!(tracker1.take_events(), BASIC);
    }

    async fn example_async_fn<L>(mut logger: L)
    where
        L: polog::Logger,
    {
        polog::info!(&mut logger, "Some warn!"; 3 * 4);
        tokio::task::yield_now().await;
        polog::info!(&mut logger, "Another warn!"; 4 * 5);
    }

    #[test]
    fn test_borrowing_fanout_disabled_span() {
        const BORROWED: &[&str] = &[
            "try_enable",
            "enter_span_borrowed(nested)",
            "exit_span(nested)",
        ];

        let tracker1 = Tracker::default();
        let tracker2 = Tracker::default();
        let tracker3 = Tracker::default();
        let tracker4 = Tracker::default();

        let filtered = |tracker: Tracker, level: LevelFilter| Filter::new(tracker, MaxLevel(level));

        let mut logger = BorrowingFanout1::new((filtered(tracker1.clone(), LevelFilter::Info),));
        let span = polog::debug_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), [""; 0]);

        let mut logger = BorrowingFanout4::new((
            filtered(tracker1.clone(), LevelFilter::Info),
            filtered(tracker2.clone(), LevelFilter::Trace),
            filtered(tracker3.clone(), LevelFilter::Info),
            filtered(tracker4.clone(), LevelFilter::Trace),
        ));
        let span = polog::debug_span!(&mut logger, "Span");
        drop(span);
        assert_eq!(tracker1.take_events(), ["generate_span_id"]);
        assert_eq!(tracker2.take_events(), BORROWED);
        assert_eq!(tracker3.take_events(), [""; 0]);
        assert_eq!(tracker4.take_events(), BORROWED);
    }

    #[test]
    fn test_borrowing_fanout1_low_level() {
        const TRACE_DESC: &Descriptor = log_descriptor_ref!(Level::Trace, "Trace message");
        const DISABLED: (Option<()>,) = (None,);
        const ENABLED: (Option<()>,) = (Some(()),);

        let tracker1 = Tracker::default();

        let filtered = |tracker: Tracker, level: LevelFilter| Filter::new(tracker, MaxLevel(level));

        let mut logger = BorrowingFanout1::new((filtered(tracker1.clone(), LevelFilter::Info),));

        let span_id = logger.generate_span_id();
        assert_eq!(tracker1.take_events(), ["generate_span_id"]);

        // Pass disabled enriched context.

        logger.log(TRACE_DESC, (), None, DISABLED);
        logger.log_static(TRACE_DESC, (), None, DISABLED);
        logger.log_borrowed(TRACE_DESC, &(), None, &DISABLED);
        logger.log(TRACE_DESC, (), None, DISABLED);
        logger.enter_span(TRACE_DESC, (), None, span_id, SpanKind::Nested, DISABLED);
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        logger.enter_span_static(TRACE_DESC, (), None, span_id, SpanKind::Nested, DISABLED);
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        logger.enter_span_borrowed(TRACE_DESC, &(), None, span_id, SpanKind::Nested, &DISABLED);
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        assert_eq!(tracker1.take_events(), [""; 0]);

        // Pass enabled enriched context.

        logger.log(TRACE_DESC, (), None, ENABLED);
        logger.log_static(TRACE_DESC, (), None, ENABLED);
        logger.log_borrowed(TRACE_DESC, &(), None, &ENABLED);
        logger.enter_span(TRACE_DESC, (), None, span_id, SpanKind::Nested, ENABLED);
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        logger.enter_span_static(TRACE_DESC, (), None, span_id, SpanKind::Nested, ENABLED);
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        logger.enter_span_borrowed(TRACE_DESC, &(), None, span_id, SpanKind::Nested, &ENABLED);
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        assert_eq!(
            tracker1.take_events(),
            [
                "log",
                "log_static",
                "log_borrowed",
                "enter_span(nested)",
                "exit_span(nested)",
                "enter_span_static(nested)",
                "exit_span(nested)",
                "enter_span_borrowed(nested)",
                "exit_span(nested)",
            ]
        );

        // Pass disabled enriched context.

        logger.enter_span(TRACE_DESC, (), None, span_id, SpanKind::Nested, DISABLED);
        logger.enter_poll_span(TRACE_DESC, span_id, ());
        logger.pending_poll_span(TRACE_DESC, span_id, ());
        logger.enter_poll_span(TRACE_DESC, span_id, ());
        logger.ready_poll_span(TRACE_DESC, span_id, ());
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        assert_eq!(tracker1.take_events(), [""; 0]);

        // Pass enabled enriched context.

        logger.enter_span(TRACE_DESC, (), None, span_id, SpanKind::Nested, ENABLED);
        logger.enter_poll_span(TRACE_DESC, span_id, ());
        logger.pending_poll_span(TRACE_DESC, span_id, ());
        logger.enter_poll_span(TRACE_DESC, span_id, ());
        logger.ready_poll_span(TRACE_DESC, span_id, ());
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        assert_eq!(
            tracker1.take_events(),
            [
                "enter_span(nested)",
                "enter_poll_span",
                "pending_poll_span",
                "enter_poll_span",
                "ready_poll_span",
                "exit_span(nested)",
            ]
        );
    }

    #[test]
    fn test_borrowing_fanout4_low_level() {
        const TRACE_DESC: &Descriptor = log_descriptor_ref!(Level::Trace, "Trace message");
        const DISABLED: (Option<()>, Option<()>, Option<()>, Option<()>) = (None, None, None, None);
        const ENABLED: (Option<()>, Option<()>, Option<()>, Option<()>) =
            (Some(()), Some(()), Some(()), Some(()));

        let tracker1 = Tracker::default();
        let tracker2 = Tracker::default();
        let tracker3 = Tracker::default();
        let tracker4 = Tracker::default();

        let filtered = |tracker: Tracker, level: LevelFilter| Filter::new(tracker, MaxLevel(level));

        let mut logger = BorrowingFanout4::new((
            filtered(tracker1.clone(), LevelFilter::Info),
            filtered(tracker2.clone(), LevelFilter::Info),
            filtered(tracker3.clone(), LevelFilter::Info),
            filtered(tracker4.clone(), LevelFilter::Info),
        ));

        let span_id = logger.generate_span_id();
        assert_eq!(tracker1.take_events(), ["generate_span_id"]);

        // Pass disabled enriched context.

        logger.log(TRACE_DESC, (), None, DISABLED);
        logger.log_static(TRACE_DESC, (), None, DISABLED);
        logger.log_borrowed(TRACE_DESC, &(), None, &DISABLED);
        logger.log(TRACE_DESC, (), None, DISABLED);
        logger.enter_span(TRACE_DESC, (), None, span_id, SpanKind::Nested, DISABLED);
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        logger.enter_span_static(TRACE_DESC, (), None, span_id, SpanKind::Nested, DISABLED);
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        logger.enter_span_borrowed(TRACE_DESC, &(), None, span_id, SpanKind::Nested, &DISABLED);
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        assert_eq!(tracker1.take_events(), [""; 0]);
        assert_eq!(tracker2.take_events(), [""; 0]);
        assert_eq!(tracker3.take_events(), [""; 0]);
        assert_eq!(tracker4.take_events(), [""; 0]);

        // Pass enabled enriched context.

        logger.log(TRACE_DESC, (), None, ENABLED);
        logger.log_static(TRACE_DESC, (), None, ENABLED);
        logger.log_borrowed(TRACE_DESC, &(), None, &ENABLED);
        logger.enter_span(TRACE_DESC, (), None, span_id, SpanKind::Nested, ENABLED);
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        logger.enter_span_static(TRACE_DESC, (), None, span_id, SpanKind::Nested, ENABLED);
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        logger.enter_span_borrowed(TRACE_DESC, &(), None, span_id, SpanKind::Nested, &ENABLED);
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        for tracker in [&tracker1, &tracker2, &tracker3, &tracker4] {
            assert_eq!(
                tracker.take_events(),
                [
                    "log_borrowed",
                    "log_borrowed",
                    "log_borrowed",
                    "enter_span_borrowed(nested)",
                    "exit_span(nested)",
                    "enter_span_borrowed(nested)",
                    "exit_span(nested)",
                    "enter_span_borrowed(nested)",
                    "exit_span(nested)",
                ]
            );
        }

        // Pass disabled enriched context.

        logger.enter_span(TRACE_DESC, (), None, span_id, SpanKind::Nested, DISABLED);
        logger.enter_poll_span(TRACE_DESC, span_id, ());
        logger.pending_poll_span(TRACE_DESC, span_id, ());
        logger.enter_poll_span(TRACE_DESC, span_id, ());
        logger.ready_poll_span(TRACE_DESC, span_id, ());
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        assert_eq!(tracker1.take_events(), [""; 0]);
        assert_eq!(tracker2.take_events(), [""; 0]);
        assert_eq!(tracker3.take_events(), [""; 0]);
        assert_eq!(tracker4.take_events(), [""; 0]);

        // Pass enabled enriched context.

        logger.enter_span(TRACE_DESC, (), None, span_id, SpanKind::Nested, ENABLED);
        logger.enter_poll_span(TRACE_DESC, span_id, ());
        logger.pending_poll_span(TRACE_DESC, span_id, ());
        logger.enter_poll_span(TRACE_DESC, span_id, ());
        logger.ready_poll_span(TRACE_DESC, span_id, ());
        logger.exit_span(TRACE_DESC, span_id, SpanKind::Nested, ());
        for tracker in [&tracker1, &tracker2, &tracker3, &tracker4] {
            assert_eq!(
                tracker.take_events(),
                [
                    "enter_span_borrowed(nested)",
                    "enter_poll_span",
                    "pending_poll_span",
                    "enter_poll_span",
                    "ready_poll_span",
                    "exit_span(nested)",
                ]
            );
        }
    }

    #[test]
    fn test_borrowing_fanout_flush_and_panic_predicate() {
        const BUG_DESC: &Descriptor = log_descriptor_ref!(Level::Bug, "Bug message");
        const FB_FOP: &[&str] = &["flush_blocking", "flush_on_panic"];
        const FB_FOP_SPOB: &[&str] = &["flush_blocking", "flush_on_panic", "should_panic_on_bug"];
        const FB_FOP_REL_SPOB: &[&str] = if cfg!(debug_assertions) {
            FB_FOP
        } else {
            FB_FOP_SPOB
        };

        let tracker1 = Tracker::default();
        let tracker2 = Tracker::default();
        let tracker3 = Tracker::default();
        let tracker4 = Tracker::default();

        let mut logger = BorrowingFanout1::new((tracker1.clone(),));
        logger.flush_blocking();
        logger.flush_on_panic();
        assert_eq!(
            logger.should_panic_on_bug(BUG_DESC, None),
            cfg!(debug_assertions)
        );
        assert_eq!(tracker1.take_events(), FB_FOP_SPOB);

        let mut logger = BorrowingFanout2::new((tracker1.clone(), tracker2.clone()));
        logger.flush_blocking();
        logger.flush_on_panic();
        assert_eq!(
            logger.should_panic_on_bug(BUG_DESC, None),
            cfg!(debug_assertions)
        );
        assert_eq!(tracker1.take_events(), FB_FOP_SPOB);
        assert_eq!(tracker2.take_events(), FB_FOP_REL_SPOB);

        let mut logger =
            BorrowingFanout3::new((tracker1.clone(), tracker2.clone(), tracker3.clone()));
        logger.flush_blocking();
        logger.flush_on_panic();
        assert_eq!(
            logger.should_panic_on_bug(BUG_DESC, None),
            cfg!(debug_assertions)
        );
        assert_eq!(tracker1.take_events(), FB_FOP_SPOB);
        assert_eq!(tracker2.take_events(), FB_FOP_REL_SPOB);
        assert_eq!(tracker3.take_events(), FB_FOP_REL_SPOB);

        let mut logger = BorrowingFanout4::new((
            tracker1.clone(),
            tracker2.clone(),
            tracker3.clone(),
            tracker4.clone(),
        ));
        logger.flush_blocking();
        logger.flush_on_panic();
        assert_eq!(
            logger.should_panic_on_bug(BUG_DESC, None),
            cfg!(debug_assertions)
        );
        assert_eq!(tracker1.take_events(), FB_FOP_SPOB);
        assert_eq!(tracker2.take_events(), FB_FOP_REL_SPOB);
        assert_eq!(tracker3.take_events(), FB_FOP_REL_SPOB);
        assert_eq!(tracker4.take_events(), FB_FOP_REL_SPOB);

        let mut logger = BorrowingFanout2::new((
            BorrowingFanout2::new((tracker1.clone(), tracker2.clone())),
            Fanout2::new((tracker3.clone(), tracker4.clone())),
        ));
        logger.flush_blocking();
        logger.flush_on_panic();
        assert_eq!(
            logger.should_panic_on_bug(BUG_DESC, None),
            cfg!(debug_assertions)
        );
        assert_eq!(tracker1.take_events(), FB_FOP_SPOB);
        assert_eq!(tracker2.take_events(), FB_FOP_REL_SPOB);
        assert_eq!(tracker3.take_events(), FB_FOP_REL_SPOB);
        assert_eq!(tracker4.take_events(), FB_FOP_REL_SPOB);

        let logger = BorrowingFanout4::new((
            tracker1.clone(),
            tracker2.clone(),
            tracker3.clone(),
            tracker4.clone(),
        ));
        tracker1.set_should_panic_on_bug(Some(false));
        tracker2.set_should_panic_on_bug(Some(false));
        tracker3.set_should_panic_on_bug(Some(false));
        tracker4.set_should_panic_on_bug(Some(false));
        assert!(!logger.should_panic_on_bug(BUG_DESC, None));
        for bits in 1..0b1111 {
            tracker1.set_should_panic_on_bug(Some((bits & 1) == 1));
            tracker2.set_should_panic_on_bug(Some(((bits >> 1) & 1) == 1));
            tracker3.set_should_panic_on_bug(Some(((bits >> 2) & 1) == 1));
            tracker4.set_should_panic_on_bug(Some(((bits >> 3) & 1) == 1));
            assert!(logger.should_panic_on_bug(BUG_DESC, None));
        }
    }

    #[test]
    fn test_borrowing_fanout_panic() {
        const FATAL_DESC: &Descriptor = log_descriptor_ref!(Level::Fatal, "Fatal message");

        let tracker1 = Tracker::default();
        let tracker2 = Tracker::default();
        let tracker3 = Tracker::default();
        let tracker4 = Tracker::default();

        let logger = BorrowingFanout1::new((tracker1.clone(),));
        let _unused = catch_unwind_on_thread(&logger, |mut logger| {
            logger.panic(FATAL_DESC, None);
        })
        .unwrap_err();
        assert_eq!(tracker1.take_events(), ["panic"]);

        let logger = BorrowingFanout2::new((tracker1.clone(), tracker2.clone()));
        let _unused = catch_unwind_on_thread(&logger, |mut logger| {
            logger.panic(FATAL_DESC, None);
        })
        .unwrap_err();
        assert_eq!(tracker1.take_events(), ["panic"]);
        assert_eq!(tracker2.take_events(), [""; 0]);

        let logger = BorrowingFanout4::new((
            tracker1.clone(),
            tracker2.clone(),
            tracker3.clone(),
            tracker4.clone(),
        ));
        let _unused = catch_unwind_on_thread(&logger, |mut logger| {
            logger.panic(FATAL_DESC, None);
        })
        .unwrap_err();
        assert_eq!(tracker1.take_events(), ["panic"]);
        assert_eq!(tracker2.take_events(), [""; 0]);
        assert_eq!(tracker3.take_events(), [""; 0]);
        assert_eq!(tracker4.take_events(), [""; 0]);

        let logger = BorrowingFanout2::new((
            BorrowingFanout2::new((tracker1.clone(), tracker2.clone())),
            Fanout2::new((tracker3.clone(), tracker4.clone())),
        ));
        let _unused = catch_unwind_on_thread(&logger, |mut logger| {
            logger.panic(FATAL_DESC, None);
        })
        .unwrap_err();
        assert_eq!(tracker1.take_events(), ["panic"]);
        assert_eq!(tracker2.take_events(), [""; 0]);
        assert_eq!(tracker3.take_events(), [""; 0]);
        assert_eq!(tracker4.take_events(), [""; 0]);
    }
}
