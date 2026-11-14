//! Type-erased argument wrappers.
//!
//! Provides [`TypeErasedArgs`] and its `Arc`-shared aliases, which erase the
//! concrete heterogeneous argument tuple while preserving the argument access
//! and formatting traits required by `polog`.

use core::fmt::{self, Debug, Display};

use polog::{
    Arg, DebuggableMixedArgs, DebuggableMixedArgsBuilder, DisplayableMixedArgs,
    DisplayableMixedArgsBuilder, DynArg, DynDebuggableMixedArgs, DynDisplayableMixedArgs,
    DynMixedArgs, DynMixedArgsBuilder, IntoSendable, MixedArgs, MixedArgsBuilder,
    StaticTypeMetadata, TemporaryIntoSendable, TemporaryToSendable, ToSendable, TypeMetadata,
};

#[cfg(feature = "alloc")]
use crate::args::AnyDebuggableMixedArgs;

/// A transparent wrapper that erases the concrete argument tuple type while
/// preserving its [`MixedArgs`] behavior.
///
/// `TypeErasedArgs` forwards all argument access and formatting operations to
/// the wrapped value.
/// It is useful for storing or passing heterogeneous argument tuples behind a
/// single type, such as behind `Box` or `Arc` in `dyn` form for deferred
/// logging or middleware context.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct TypeErasedArgs<T>(pub T)
where
    T: ?Sized;

/// A reference-counted, type-erased, `'static + Send + Sync` log argument
/// tuple.
///
/// The wrapped `Arc` implements [`DisplayableMixedArgs`], so the tuple can be
/// formatted after being sent to another thread.
#[cfg(feature = "alloc")]
pub type TypeErasedArcLogArgs =
    TypeErasedArgs<alloc::sync::Arc<dyn 'static + Send + Sync + DisplayableMixedArgs<'static>>>;

/// A reference-counted, type-erased, `'static + Send + Sync` context argument
/// tuple.
///
/// The wrapped `Arc` implements [`AnyDebuggableMixedArgs`], so the tuple can
/// be rendered and downcast after being sent to another thread.
#[cfg(feature = "alloc")]
pub type TypeErasedArcContextArgs =
    TypeErasedArgs<alloc::sync::Arc<dyn 'static + Send + Sync + AnyDebuggableMixedArgs<'static>>>;

impl<T> TypeErasedArgs<T> {
    /// Creates a new [`TypeErasedArgs`] wrapper around `args`.
    #[inline]
    #[must_use]
    pub const fn new(args: T) -> Self {
        Self(args)
    }
}

impl<T> MixedArgs for TypeErasedArgs<T>
where
    T: ?Sized + MixedArgs,
{
    #[inline]
    fn num_args(&self) -> usize {
        self.0.num_args()
    }

    #[inline]
    fn nth_arg_type_metadata(&self, index: usize) -> Option<TypeMetadata> {
        self.0.nth_arg_type_metadata(index)
    }

    #[inline]
    fn nth_static_arg_type_metadata(&self, index: usize) -> Option<StaticTypeMetadata>
    where
        Self: 'static,
    {
        self.0.nth_static_arg_type_metadata(index)
    }

    #[inline]
    fn nth_arg_as_dyn_any(&self, index: usize) -> Option<&dyn core::any::Any>
    where
        Self: 'static,
    {
        self.0.nth_arg_as_dyn_any(index)
    }
}

impl<T> MixedArgsBuilder for TypeErasedArgs<T>
where
    T: MixedArgs,
{
    type WithArg<U> = (Self, Arg<U>);

    #[inline]
    fn with_arg<U>(self, arg: U) -> Self::WithArg<U> {
        (self, Arg(arg))
    }
}

impl<T, U> DynMixedArgs<U> for TypeErasedArgs<T>
where
    T: ?Sized + DynMixedArgs<U>,
    U: ?Sized,
{
    #[inline]
    fn nth_dyn(&self, index: usize) -> Option<&U> {
        self.0.nth_dyn(index)
    }
}

impl<T, U> DynMixedArgsBuilder<U> for TypeErasedArgs<T>
where
    T: DynMixedArgs<U>,
    U: ?Sized,
{
    type WithDynArg<V>
        = (Self, Arg<V>)
    where
        V: DynArg<U>;

    #[inline]
    fn with_dyn_arg<V>(self, arg: V) -> Self::WithDynArg<V>
    where
        V: DynArg<U>,
    {
        (self, Arg(arg))
    }
}

impl<'a, T> DynDisplayableMixedArgs<'a> for TypeErasedArgs<T>
where
    T: ?Sized + DynDisplayableMixedArgs<'a>,
{
    #[inline]
    fn dyn_display_fmt_nth(
        &self,
        writer: &mut dyn fmt::Write,
        index: usize,
    ) -> Option<Result<(), fmt::Error>> {
        self.0.dyn_display_fmt_nth(writer, index)
    }
}

impl<'a, T> DisplayableMixedArgs<'a> for TypeErasedArgs<T>
where
    T: ?Sized + DynDisplayableMixedArgs<'a>,
{
    #[inline]
    fn display_fmt_nth<W>(&self, writer: &mut W, index: usize) -> Option<Result<(), fmt::Error>>
    where
        W: fmt::Write,
    {
        self.0.dyn_display_fmt_nth(writer, index)
    }
}

impl<'a, T> DisplayableMixedArgsBuilder<'a> for TypeErasedArgs<T>
where
    T: DynDisplayableMixedArgs<'a>,
{
    type WithDisplayableArg<U>
        = (Self, Arg<U>)
    where
        U: 'a + Display;

    #[inline]
    fn with_displayable_arg<U>(self, arg: U) -> Self::WithDisplayableArg<U>
    where
        U: 'a + Display,
    {
        (self, Arg(arg))
    }
}

impl<'a, T> DynDebuggableMixedArgs<'a> for TypeErasedArgs<T>
where
    T: ?Sized + DynDebuggableMixedArgs<'a>,
{
    #[inline]
    fn dyn_debug_fmt_nth(
        &self,
        writer: &mut dyn fmt::Write,
        index: usize,
    ) -> Option<Result<(), fmt::Error>> {
        self.0.dyn_debug_fmt_nth(writer, index)
    }
}

impl<'a, T> DebuggableMixedArgs<'a> for TypeErasedArgs<T>
where
    T: ?Sized + DynDebuggableMixedArgs<'a>,
{
    #[inline]
    fn debug_fmt_nth<W>(&self, writer: &mut W, index: usize) -> Option<Result<(), fmt::Error>>
    where
        W: fmt::Write,
    {
        self.0.dyn_debug_fmt_nth(writer, index)
    }
}

impl<'a, T> DebuggableMixedArgsBuilder<'a> for TypeErasedArgs<T>
where
    T: DynDebuggableMixedArgs<'a>,
{
    type WithDebuggableArg<U>
        = (Self, Arg<U>)
    where
        U: 'a + Debug;

    #[inline]
    fn with_debuggable_arg<U>(self, arg: U) -> Self::WithDebuggableArg<U>
    where
        U: 'a + Debug,
    {
        (self, Arg(arg))
    }
}

impl<T> IntoSendable for TypeErasedArgs<T>
where
    T: 'static + Send + Sync,
{
    type Sendable = Self;

    #[inline]
    fn into_sendable(self) -> Self::Sendable {
        self
    }
}

impl<T> ToSendable<'static> for TypeErasedArgs<T>
where
    T: 'static + Sync,
{
    type Sendable = &'static Self;

    #[inline]
    fn to_sendable(&'static self) -> Self::Sendable {
        self
    }
}

impl<T> TemporaryIntoSendable for TypeErasedArgs<T>
where
    T: 'static + Send + Sync,
{
    type Sendable = Self;

    #[inline]
    fn temp_into_sendable(self) -> Self::Sendable {
        self
    }
}

impl<T> TemporaryToSendable<'static> for TypeErasedArgs<T>
where
    T: 'static + Sync,
{
    type Sendable = &'static Self;

    #[inline]
    fn temp_to_sendable(&'static self) -> Self::Sendable {
        self
    }
}

#[cfg(feature = "alloc")]
#[cfg(test)]
mod tests {

    use alloc::boxed::Box;
    use alloc::format;
    use alloc::string::String;
    use alloc::sync::Arc;
    use core::any::TypeId;
    use core::fmt;

    use polog::{
        Arg, ContextArgs, DebuggableMixedArgs, DebuggableMixedArgsBuilder, DisplayableMixedArgs,
        DisplayableMixedArgsBuilder, DynDebuggableMixedArgs, DynDisplayableMixedArgs, DynMixedArgs,
        DynMixedArgsBuilder, IntoSendable, LogArgs, MixedArgs, MixedArgsBuilder,
        StaticTypeMetadata, TemporaryIntoSendable, TypeMetadata,
    };
    #[cfg(not(miri))]
    use polog::{TemporaryToSendable, ToSendable};

    use crate::args::{TypeErasedArcContextArgs, TypeErasedArcLogArgs, TypeErasedArgs};

    #[test]
    fn test_log_args_dyn_compatible() {
        handle_static_log_args((), 16);
        handle_static_log_args(Arg(123), 17);
        handle_static_log_args(Arg("foo"), 18);
        handle_static_log_args((Arg(123), Arg("foo")), 19);
    }

    fn handle_static_log_args<'a, Args>(args: Args, steps: usize)
    where
        Args: LogArgs<'a>,
    {
        if let Some(steps) = steps.checked_sub(1) {
            handle_dyn_log_args(TypeErasedArgs::new(Arc::new(args.into_sendable())), steps);
        }
    }

    fn handle_dyn_log_args(args: TypeErasedArcLogArgs, steps: usize) {
        if let Some(steps) = steps.checked_sub(1) {
            handle_static_log_args(args, steps);
        }
    }

    #[test]
    fn test_context_args_dyn_compatible() {
        handle_static_context_args((), 16);
        handle_static_context_args(Arg(123), 17);
        handle_static_context_args(Arg("foo"), 18);
        handle_static_context_args((Arg(123), Arg("foo")), 19);
    }

    fn handle_static_context_args<Context>(context: Context, steps: usize)
    where
        Context: Sync + ContextArgs,
    {
        if let Some(steps) = steps.checked_sub(1) {
            handle_dyn_context_args(TypeErasedArgs::new(Arc::new(context)), steps);
        }
    }

    fn handle_dyn_context_args(context: TypeErasedArcContextArgs, steps: usize) {
        if let Some(steps) = steps.checked_sub(1) {
            handle_static_context_args(context, steps);
        }
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_type_erased_args_nested_tuple() {
        let args = (
            Arg(true),
            (
                (Arc::new(Box::new(Arg(123))), (&&Arg('b'),)),
                Arg("abc"),
                (),
            ),
            (),
            Arc::new(Arg(12345)),
        );
        let args: TypeErasedArcLogArgs = TypeErasedArgs::new(Arc::new(args));

        assert_eq!(args.num_args(), 5);

        assert_eq!(
            args.nth_arg_type_metadata(0),
            Some(TypeMetadata {
                name: "bool",
                size: 1,
                align: 1,
            })
        );
        assert!(matches!(
            args.nth_arg_type_metadata(1),
            Some(TypeMetadata {
                size: 4,
                align: 4,
                ..
            })
        ));
        assert_eq!(
            args.nth_arg_type_metadata(2),
            Some(TypeMetadata {
                name: "char",
                size: 4,
                align: 4,
            })
        );
        assert!(matches!(
            args.nth_arg_type_metadata(4),
            Some(TypeMetadata {
                name: _,
                size: 4,
                align: 4
            })
        ));
        assert_eq!(args.nth_arg_type_metadata(5), None);

        assert_eq!(
            args.nth_static_arg_type_metadata(0),
            Some(StaticTypeMetadata {
                id: TypeId::of::<bool>(),
                name: "bool",
                size: 1,
                align: 1,
            })
        );
        assert_eq!(
            args.nth_static_arg_type_metadata(1),
            Some(StaticTypeMetadata {
                id: TypeId::of::<i32>(),
                name: "i32",
                size: 4,
                align: 4,
            })
        );
        assert_eq!(
            args.nth_static_arg_type_metadata(2),
            Some(StaticTypeMetadata {
                id: TypeId::of::<char>(),
                name: "char",
                size: 4,
                align: 4,
            })
        );
        assert_eq!(args.nth_static_arg_type_metadata(5), None);

        assert_eq!(
            args.nth_arg_as_dyn_any(0).unwrap().downcast_ref(),
            Option::<&bool>::Some(&true)
        );
        assert_eq!(
            args.nth_arg_as_dyn_any(0).unwrap().downcast_ref(),
            Option::<&i32>::None
        );

        assert_eq!(
            args.nth_arg_as_dyn_any(1).unwrap().downcast_ref(),
            Option::<&i32>::Some(&123)
        );
        assert_eq!(
            args.nth_arg_as_dyn_any(1).unwrap().downcast_ref(),
            Option::<&bool>::None
        );

        assert_eq!(
            args.nth_arg_as_dyn_any(4).unwrap().downcast_ref(),
            Option::<&i32>::Some(&12345)
        );
        assert_eq!(
            args.nth_arg_as_dyn_any(4).unwrap().downcast_ref(),
            Option::<&(i32, i32)>::None
        );

        assert!(args.nth_arg_as_dyn_any(5).is_none());
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_type_erased_args_nested_tuple_fmt() {
        let context_args = (
            Arg(true),
            ((Arg(123), (&&Arg('b'),)), Arg("abc"), ()),
            (),
            Arc::new(Arg((123, 234))),
        );
        let context_args: TypeErasedArcContextArgs = TypeErasedArgs::new(Arc::new(context_args));

        let log_args = (
            Arg(true),
            ((Arg(123), (&&Arg('b'),)), Arg("abc"), ()),
            (),
            Arc::new(Arg(12345)),
        );
        let log_args: TypeErasedArcLogArgs = TypeErasedArgs::new(Arc::new(log_args));

        let fmt = DynMixedArgs::<dyn fmt::Debug>::nth_dyn(&context_args, 0).unwrap();
        assert_eq!(format!("{fmt:?}"), "true");
        let fmt = DynMixedArgs::<dyn fmt::Debug>::nth_dyn(&context_args, 1).unwrap();
        assert_eq!(format!("{fmt:?}"), "123");

        let fmt = DynMixedArgs::<dyn fmt::Display>::nth_dyn(&log_args, 0).unwrap();
        assert_eq!(format!("{fmt}"), "true");
        let fmt = DynMixedArgs::<dyn fmt::Display>::nth_dyn(&log_args, 1).unwrap();
        assert_eq!(format!("{fmt}"), "123");

        let mut debug = String::new();
        let mut display = String::new();
        for i in 0..5 {
            context_args.debug_fmt_nth(&mut debug, i).unwrap().unwrap();
            log_args.display_fmt_nth(&mut display, i).unwrap().unwrap();
        }
        assert!(context_args.debug_fmt_nth(&mut debug, 5).is_none());
        assert!(log_args.display_fmt_nth(&mut display, 5).is_none());

        assert_eq!(debug, "true123'b'\"abc\"(123, 234)");
        assert_eq!(display, "true123babc12345");

        let mut debug = String::new();
        let mut display = String::new();
        for i in 0..5 {
            context_args
                .dyn_debug_fmt_nth(&mut debug, i)
                .unwrap()
                .unwrap();
            log_args
                .dyn_display_fmt_nth(&mut display, i)
                .unwrap()
                .unwrap();
        }
        assert!(context_args.dyn_debug_fmt_nth(&mut debug, 5).is_none());
        assert!(log_args.dyn_display_fmt_nth(&mut display, 5).is_none());

        assert_eq!(debug, "true123'b'\"abc\"(123, 234)");
        assert_eq!(display, "true123babc12345");
    }

    #[test]
    fn test_type_erased_args_with_arg() {
        let args0 = ();
        let args0c = || -> TypeErasedArcContextArgs { TypeErasedArgs::new(Arc::new(args0)) };
        let args0l = || -> TypeErasedArcLogArgs { TypeErasedArgs::new(Arc::new(args0)) };

        let args4 = (Arg(1), Arg(2), Arg(3), Arg(4));
        let args4c = || -> TypeErasedArcContextArgs { TypeErasedArgs::new(Arc::new(args4)) };
        let args4l = || -> TypeErasedArcLogArgs { TypeErasedArgs::new(Arc::new(args4)) };

        let _unused: (TypeErasedArcContextArgs, Arg<i32>) = args0c().with_arg(123);
        let _unused: (TypeErasedArcContextArgs, Arg<i32>) = args4c().with_arg(123);

        let _unused: (TypeErasedArcContextArgs, Arg<i32>) = args0c().with_dyn_arg(123);
        let _unused: (TypeErasedArcContextArgs, Arg<i32>) = args4c().with_dyn_arg(123);

        let _unused: (TypeErasedArcContextArgs, Arg<i32>) = args0c().with_debuggable_arg(123);
        let _unused: (TypeErasedArcContextArgs, Arg<i32>) = args4c().with_debuggable_arg(123);

        let _unused: (TypeErasedArcLogArgs, Arg<i32>) = args0l().with_displayable_arg(123);
        let _unused: (TypeErasedArcLogArgs, Arg<i32>) = args4l().with_displayable_arg(123);
    }

    #[test]
    fn test_type_erased_args_into_sendable() {
        let args = (Arg(1), Arg(2), Arg(3), Arg(4));
        let args = || -> TypeErasedArcLogArgs { TypeErasedArgs::new(Arc::new(args)) };
        let _unused: TypeErasedArcLogArgs = args().into_sendable();
        let _unused: TypeErasedArcLogArgs = args().temp_into_sendable();
        #[cfg(not(miri))]
        {
            let args: &'static TypeErasedArcLogArgs = Box::leak(Box::new(args()));
            let _unused = args.to_sendable();
            let _unused = args.temp_to_sendable();
        }
    }
}
