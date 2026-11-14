use core::cell::{Cell, LazyCell, OnceCell, Ref, RefCell, RefMut};
use core::ffi::CStr;
use core::marker::{PhantomData, PhantomPinned};
use core::mem::{ManuallyDrop, MaybeUninit};
use core::num::{NonZero, Saturating, Wrapping};
use core::panic::AssertUnwindSafe;
use core::pin::Pin;
use core::ptr::NonNull;
use core::sync::atomic::{
    AtomicBool, AtomicI8, AtomicI16, AtomicI32, AtomicIsize, AtomicPtr, AtomicU8, AtomicU16,
    AtomicU32, AtomicUsize,
};
#[cfg(target_has_atomic = "64")]
use core::sync::atomic::{AtomicI64, AtomicU64};

use crate::{Arg, UnsafeSendablePtr, ViaAltDebug, ViaAltDisplay, ViaDebug, ViaDisplay};

/// Converts a value into a form that is `'static + Send` using an
/// **optimistic** conversion strategy.
///
/// The implementations are zero-cost where possible and use optimistic generic
/// bounds.
/// These bounds may often not be satisfied.
/// This is by design as it avoids unnecessary allocations or clones for values
/// that can be sent to an offloaded logger as-is.
///
/// For a **pessimistic** conversion strategy that may allocate or clone, wrap
/// the value in `Temporary` or use the `:t` syntax in the `log!` macro.
///
/// # Examples
///
/// This example shows `IntoSendable` as a bound in a logging helper and how the
/// `polog::info!` macro selects the conversion strategy using the `:t`, `:a`,
/// and `:s` modifiers:
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// use std::fmt::Display;
///
/// use polog::{IntoSendable, TemporaryIntoSendable, ToSendable, TemporaryToSendable};
///
/// # log_smth(
/// #     &mut AcceptAll,
/// #     123, 123, &123, &123, 123, &123
/// # );
/// #
/// fn log_smth<'a>(
///     logger: &mut impl polog::Logger,
///     arg1: impl Display + IntoSendable<Sendable: Display>,
///     arg2: impl Display + TemporaryIntoSendable<Sendable: Display>,
///     arg3: &'a (impl Display + ToSendable<'a, Sendable: Display>),
///     arg4: &'a (impl Display + TemporaryToSendable<'a, Sendable: Display>),
///     arg5: impl 'static + Send + Sync + Display,
///     arg6: &impl Display,
/// ) {
///     polog::info!(
///         logger, "MESSAGE";
///         arg1, :t arg2, arg3, :t arg4, :a arg5, :s arg6,
///     );
/// }
/// ```
///
/// This example checks the `Sendable` type produced by `IntoSendable` and
/// `TemporaryIntoSendable` for a static string, a temporary string, and a
/// `Temporary`-wrapped temporary string:
/// ```rust
/// # #[cfg(feature = "alloc")] {
/// use core::any::TypeId;
/// use core::marker::PhantomData;
///
/// use polog::{AlreadySendable, IntoSendable, Temporary, TemporaryIntoSendable};
///
/// let STATIC_STR: &'static str = "Hello world!";
/// let sendable = IntoSendable::into_sendable(STATIC_STR);
/// assert_type_eq(sendable, PhantomData::<&'static str>);
///
/// let temp_string = String::from("Hello world!");
/// let temp_str: &str = temp_string.as_str();
/// let sendable = TemporaryIntoSendable::temp_into_sendable(temp_str);
/// assert_type_eq(sendable, PhantomData::<String>);
/// let sendable = IntoSendable::into_sendable(Temporary(temp_str));
/// assert_type_eq(sendable, PhantomData::<AlreadySendable<String>>);
///
/// fn assert_type_eq<T, U>(_: T, _: PhantomData<U>)
/// where
///     T: 'static,
///     U: 'static,
/// {
///     assert_eq!(TypeId::of::<T>(), TypeId::of::<U>());
/// }
/// # }
/// ```
///
/// This example fails to compile because `IntoSendable` requires the resulting
/// `Sendable` type to be `'static`, but `temp_str` borrows from a local
/// `String`:
/// ```rust,compile_fail
/// let temp_string = String::from("Hello world!");
/// let temp_str = temp_string.as_str();
/// let sendable = IntoSendable::into_sendable(temp_str);
/// ```
pub trait IntoSendable {
    /// The resulting `'static + Send` value.
    type Sendable: 'static + Send + Sync;

    /// Converts `self` into [`Self::Sendable`].
    fn into_sendable(self) -> Self::Sendable;
}

/// Converts a value into a form that is `'static + Send` using a
/// **pessimistic** conversion strategy.
///
/// Implementations eagerly convert the value into an owned, `Send`-compatible
/// form, allocating or cloning if needed.
///
/// This trait is designed for values that are explicitly marked as temporary
/// via the `Temporary` wrapper or the `:t` syntax in the `log!` macro.
///
/// # Examples
///
/// This example shows `IntoSendable` as a bound in a logging helper and how the
/// `polog::info!` macro selects the conversion strategy using the `:t`, `:a`,
/// and `:s` modifiers:
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// use std::fmt::Display;
///
/// use polog::{IntoSendable, TemporaryIntoSendable, ToSendable, TemporaryToSendable};
///
/// # log_smth(
/// #     &mut AcceptAll,
/// #     123, 123, &123, &123, 123, &123
/// # );
/// #
/// fn log_smth<'a>(
///     logger: &mut impl polog::Logger,
///     arg1: impl Display + IntoSendable<Sendable: Display>,
///     arg2: impl Display + TemporaryIntoSendable<Sendable: Display>,
///     arg3: &'a (impl Display + ToSendable<'a, Sendable: Display>),
///     arg4: &'a (impl Display + TemporaryToSendable<'a, Sendable: Display>),
///     arg5: impl 'static + Send + Sync + Display,
///     arg6: &impl Display,
/// ) {
///     polog::info!(
///         logger, "MESSAGE";
///         arg1, :t arg2, arg3, :t arg4, :a arg5, :s arg6,
///     );
/// }
/// ```
///
/// This example checks the `Sendable` type produced by `IntoSendable` and
/// `TemporaryIntoSendable` for a static string, a temporary string, and a
/// `Temporary`-wrapped temporary string:
/// ```rust
/// # #[cfg(feature = "alloc")] {
/// use core::any::TypeId;
/// use core::marker::PhantomData;
///
/// use polog::{AlreadySendable, IntoSendable, Temporary, TemporaryIntoSendable};
///
/// let STATIC_STR: &'static str = "Hello world!";
/// let sendable = IntoSendable::into_sendable(STATIC_STR);
/// assert_type_eq(sendable, PhantomData::<&'static str>);
///
/// let temp_string = String::from("Hello world!");
/// let temp_str: &str = temp_string.as_str();
/// let sendable = TemporaryIntoSendable::temp_into_sendable(temp_str);
/// assert_type_eq(sendable, PhantomData::<String>);
/// let sendable = IntoSendable::into_sendable(Temporary(temp_str));
/// assert_type_eq(sendable, PhantomData::<AlreadySendable<String>>);
///
/// fn assert_type_eq<T, U>(_: T, _: PhantomData<U>)
/// where
///     T: 'static,
///     U: 'static,
/// {
///     assert_eq!(TypeId::of::<T>(), TypeId::of::<U>());
/// }
/// # }
/// ```
pub trait TemporaryIntoSendable {
    /// The resulting `'static + Send` value.
    type Sendable: 'static + Send + Sync;

    /// Converts `self` into [`Self::Sendable`], allocating or cloning if
    /// needed.
    fn temp_into_sendable(self) -> Self::Sendable;
}

/// Converts a borrowed value into a form that is `'static + Send` using
/// an **optimistic** conversion strategy.
///
/// This is the borrowed counterpart to [`IntoSendable`].
/// The lifetime `'a` is the lifetime of the borrow.
///
/// Implementations use optimistic generic bounds and avoid allocations or
/// clones where possible.
/// These bounds may often not be satisfied.
/// This is by design as it avoids unnecessary allocations or clones for values
/// that can be sent to an offloaded logger as-is.
///
/// For a **pessimistic** conversion strategy that may allocate or clone, wrap
/// the value in `Temporary` or use the `:t` syntax in the `log!` macro.
///
/// # Examples
///
/// This example shows `IntoSendable` as a bound in a logging helper and how the
/// `polog::info!` macro selects the conversion strategy using the `:t`, `:a`,
/// and `:s` modifiers:
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// use std::fmt::Display;
///
/// use polog::{IntoSendable, TemporaryIntoSendable, ToSendable, TemporaryToSendable};
///
/// # log_smth(
/// #     &mut AcceptAll,
/// #     123, 123, &123, &123, 123, &123
/// # );
/// #
/// fn log_smth<'a>(
///     logger: &mut impl polog::Logger,
///     arg1: impl Display + IntoSendable<Sendable: Display>,
///     arg2: impl Display + TemporaryIntoSendable<Sendable: Display>,
///     arg3: &'a (impl Display + ToSendable<'a, Sendable: Display>),
///     arg4: &'a (impl Display + TemporaryToSendable<'a, Sendable: Display>),
///     arg5: impl 'static + Send + Sync + Display,
///     arg6: &impl Display,
/// ) {
///     polog::info!(
///         logger, "MESSAGE";
///         arg1, :t arg2, arg3, :t arg4, :a arg5, :s arg6,
///     );
/// }
/// ```
///
/// This example checks the `Sendable` type produced by `IntoSendable` and
/// `TemporaryIntoSendable` for a static string, a temporary string, and a
/// `Temporary`-wrapped temporary string:
/// ```rust
/// # #[cfg(feature = "alloc")] {
/// use core::any::TypeId;
/// use core::marker::PhantomData;
///
/// use polog::{AlreadySendable, IntoSendable, Temporary, TemporaryIntoSendable};
///
/// let STATIC_STR: &'static str = "Hello world!";
/// let sendable = IntoSendable::into_sendable(STATIC_STR);
/// assert_type_eq(sendable, PhantomData::<&'static str>);
///
/// let temp_string = String::from("Hello world!");
/// let temp_str: &str = temp_string.as_str();
/// let sendable = TemporaryIntoSendable::temp_into_sendable(temp_str);
/// assert_type_eq(sendable, PhantomData::<String>);
/// let sendable = IntoSendable::into_sendable(Temporary(temp_str));
/// assert_type_eq(sendable, PhantomData::<AlreadySendable<String>>);
///
/// fn assert_type_eq<T, U>(_: T, _: PhantomData<U>)
/// where
///     T: 'static,
///     U: 'static,
/// {
///     assert_eq!(TypeId::of::<T>(), TypeId::of::<U>());
/// }
/// # }
/// ```
///
/// This example fails to compile because `ToSendable` requires the resulting
/// `Sendable` type to be `'static`, but `temp_str` borrows from a local
/// `String`:
/// ```rust,compile_fail
/// let temp_string = String::from("Hello world!");
/// let temp_str = temp_string.as_str();
/// let sendable = IntoSendable::into_sendable(temp_str);
/// ```
pub trait ToSendable<'a> {
    /// The resulting `'static + Send` value.
    type Sendable: 'static + Send + Sync;

    /// Converts `&'a self` into [`Self::Sendable`].
    fn to_sendable(&'a self) -> Self::Sendable;
}

/// Converts a borrowed value into a form that is `'static + Send` using a
/// **pessimistic** conversion strategy.
///
/// This is the borrowed counterpart to [`TemporaryIntoSendable`].
/// The lifetime `'a` is the lifetime of the borrow.
///
/// Implementations eagerly convert the value into an owned, `Send`-compatible
/// form, allocating or cloning if needed.
///
/// This trait is designed for values that are explicitly marked as temporary
/// via the `Temporary` wrapper or the `:t` syntax in the `log!` macro.
///
/// # Examples
///
/// This example shows `IntoSendable` as a bound in a logging helper and how the
/// `polog::info!` macro selects the conversion strategy using the `:t`, `:a`,
/// and `:s` modifiers:
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// use std::fmt::Display;
///
/// use polog::{IntoSendable, TemporaryIntoSendable, ToSendable, TemporaryToSendable};
///
/// # log_smth(
/// #     &mut AcceptAll,
/// #     123, 123, &123, &123, 123, &123
/// # );
/// #
/// fn log_smth<'a>(
///     logger: &mut impl polog::Logger,
///     arg1: impl Display + IntoSendable<Sendable: Display>,
///     arg2: impl Display + TemporaryIntoSendable<Sendable: Display>,
///     arg3: &'a (impl Display + ToSendable<'a, Sendable: Display>),
///     arg4: &'a (impl Display + TemporaryToSendable<'a, Sendable: Display>),
///     arg5: impl 'static + Send + Sync + Display,
///     arg6: &impl Display,
/// ) {
///     polog::info!(
///         logger, "MESSAGE";
///         arg1, :t arg2, arg3, :t arg4, :a arg5, :s arg6,
///     );
/// }
/// ```
///
/// This example checks the `Sendable` type produced by `IntoSendable` and
/// `TemporaryIntoSendable` for a static string, a temporary string, and a
/// `Temporary`-wrapped temporary string:
/// ```rust
/// # #[cfg(feature = "alloc")] {
/// use core::any::TypeId;
/// use core::marker::PhantomData;
///
/// use polog::{AlreadySendable, IntoSendable, Temporary, TemporaryIntoSendable};
///
/// let STATIC_STR: &'static str = "Hello world!";
/// let sendable = IntoSendable::into_sendable(STATIC_STR);
/// assert_type_eq(sendable, PhantomData::<&'static str>);
///
/// let temp_string = String::from("Hello world!");
/// let temp_str: &str = temp_string.as_str();
/// let sendable = TemporaryIntoSendable::temp_into_sendable(temp_str);
/// assert_type_eq(sendable, PhantomData::<String>);
/// let sendable = IntoSendable::into_sendable(Temporary(temp_str));
/// assert_type_eq(sendable, PhantomData::<AlreadySendable<String>>);
///
/// fn assert_type_eq<T, U>(_: T, _: PhantomData<U>)
/// where
///     T: 'static,
///     U: 'static,
/// {
///     assert_eq!(TypeId::of::<T>(), TypeId::of::<U>());
/// }
/// # }
/// ```
///
/// This example fails to compile because `IntoSendable` requires the resulting
/// `Sendable` type to be `'static`, but `temp_str` borrows from a local
/// `String`:
/// ```rust,compile_fail
/// let temp_string = String::from("Hello world!");
/// let temp_str = temp_string.as_str();
/// let sendable = IntoSendable::into_sendable(temp_str);
/// ```
pub trait TemporaryToSendable<'a> {
    /// The resulting `'static + Send` value.
    type Sendable: 'static + Send + Sync;

    /// Converts `&'a self` into [`Self::Sendable`], allocating or cloning if
    /// needed.
    fn temp_to_sendable(&'a self) -> Self::Sendable;
}

/// Emits up to four sendable-conversion impls for one input type from a single
/// uniform shape.
///
/// The macro is internal (exported only so sibling modules and downstream
/// macros can reach it) and implements a subset of [`IntoSendable`],
/// [`ToSendable`], [`TemporaryIntoSendable`], and [`TemporaryToSendable`].
///
/// # Syntax
///
/// ```text
/// __impl_sendable!(
///     for ($($params)*) for $in $( => $out )? $( where ( $($bounds)* ) )?,
///     O |self|                     $block1 $( where ( $($b1)* ) )?,
///     O $( for <$lt> )? |&'a self| $block2 $( where ( $($b2)* ) )?,
///     p |self|                     $block3 $( where ( $($b3)* ) )?,
///     p $( for <$lt> )? |&'a self| $block4 $( where ( $($b4)* ) )?,
/// );
/// ```
/// or
/// ```text
/// __impl_sendable!(
///     for ($($params)*) for $in $( where ( $($bounds)* ) )?,
///     O |self|                     -> $out1 $block1 $( where ( $($b1)* ) )?,
///     O $( for <$lt> )? |&'a self| -> $out2 $block2 $( where ( $($b2)* ) )?,
///     p |self|                     -> $out3 $block3 $( where ( $($b3)* ) )?,
///     p $( for <$lt> )? |&'a self| -> $out4 $block4 $( where ( $($b4)* ) )?,
/// );
/// ```
///
/// where:
/// - `O` (optimistic): emits [`IntoSendable`] (`|self|`) or [`ToSendable`]
///   (`|&'a self|`). Optimistic conversions are zero-cost where possible and
///   use optimistic generic bounds.
/// - `p` (pessimistic): emits [`TemporaryIntoSendable`] (`|self|`) or
///   [`TemporaryToSendable`] (`|&'a self|`). Pessimistic conversions may
///   allocate or clone to produce an owned, sendable form.
/// - `|self|` vs `|&'a self|` is the receiver. The extra `for<'a>` introduces a
///   fresh lifetime when the impl needs one to be independent from the input
///   type's lifetimes.
/// - `$( => $out )?` specifies the shared associated `Sendable` type.
/// - `$( -> $outX )?` specifies the associated `Sendable` type per slot.
///
/// # Where clauses
///
/// The header `where` is shared by every emitted impl.
/// Each slot's `where ( + … )` is a **continuation** of the header clause: the
/// leading `+` appends to the shared bounds, which is why a type such as
/// `PhantomData<T>` can be declared with `where ( T: 'static + Send )` once and
/// still have per-slot bounds without repeating them.
///
/// # Skipping
///
/// A slot may be omitted (leaving the comma in place):
///
/// ```text
/// __impl_sendable!(
///     for (T) for RefCell<T> => RefCell<T::Sendable>,
///     O |self| { ... } where ( T: IntoSendable ),
///     /* skip optimistic by-reference */,
///     p |self| { ... } where ( T: TemporaryIntoSendable ),
///     /* skip pessimistic by-reference */,
/// );
/// ```
///
/// Passing only the header is a no-op and used internally to simplify the
/// macro implementation.
///
/// See [`IntoSendable`], [`ToSendable`], [`TemporaryIntoSendable`], and
/// [`TemporaryToSendable`] for the contract each trait carries.
#[macro_export]
#[doc(hidden)]
macro_rules! __impl_sendable {
    (
        for ($($params1:tt)*) for $in:ty $( => $out1:ty )?
        $( where ( $($bounds1:tt)* ) )? $(,)?
    ) => {
        /* ignore as intentionally skipped */
    };
    (
        for ($($params1:tt)*) for $in:ty $( => $out1:ty )?
        $( where ( $($bounds1:tt)* ) )?,
        O |$self:ident| $( -> $out2:ty )? $block:block $(,)?
        $( where ( $($bounds2:tt)* ) )? $(,)?
    ) => {
        impl<$($params1)*> $crate::IntoSendable for $in
        where $( $($bounds1)* )? $( $($bounds2)* )?
        {
            type Sendable = $($out1)? $($out2)?;

            #[inline]
            fn into_sendable($self) -> Self::Sendable $block
        }
    };
    (
        for ($($params1:tt)*) for $in:ty $( => $out1:ty )?
        $( where ( $($bounds1:tt)* ) )?,
        O $( for <$lt1:lifetime> )?
        |&$lt2:lifetime $self:ident| $( -> $out2:ty )? $block:block $(,)?
        $( where ( $($bounds2:tt)* ) )? $(,)?
    ) => {
        impl< $( $lt1, )? $($params1)*> $crate::ToSendable<$lt2> for $in
        where $( $($bounds1)* )? $( $($bounds2)* )?
        {
            type Sendable = $($out1)? $($out2)?;

            #[inline]
            fn to_sendable(&$lt2 $self) -> Self::Sendable $block
        }
    };
    (
        for ($($params1:tt)*) for $in:ty $( => $out1:ty )?
        $( where ( $($bounds1:tt)* ) )?,
        p |$self:ident| $( -> $out2:ty )? $block:block $(,)?
        $( where ( $($bounds2:tt)* ) )? $(,)?
    ) => {
        impl<$($params1)*> $crate::TemporaryIntoSendable for $in
        where $( $($bounds1)* )? $( $($bounds2)* )?
        {
            type Sendable = $($out1)? $($out2)?;

            #[inline]
            fn temp_into_sendable($self) -> Self::Sendable $block
        }
    };
    (
        for ($($params1:tt)*) for $in:ty $( => $out1:ty )?
        $( where ( $($bounds1:tt)* ) )?,
        p $( for <$lt1:lifetime> )?
        |&$lt2:lifetime $self:ident| $( -> $out2:ty )? $block:block $(,)?
        $( where ( $($bounds2:tt)* ) )? $(,)?
    ) => {
        impl< $( $lt1, )? $($params1)*> $crate::TemporaryToSendable<$lt2> for $in
        where $( $($bounds1)* )? $( $($bounds2)* )?
        {
            type Sendable = $($out1)? $($out2)?;

            #[inline]
            fn temp_to_sendable(&$lt2 $self) -> Self::Sendable $block
        }
    };
    (
        for ($($params:tt)*) for $in:ty $( => $out:ty )?
        $( where ( $($bounds:tt)* ) )?,
        $(
            O |$self1:ident| $( -> $out1:ty )? $block1:block
            $( where ( $($bounds1:tt)* ) )?
        )?,
        $(
            O $( for <$lt21:lifetime> )?
            |&$lt22:lifetime $self2:ident| $( -> $out2:ty )? $block2:block
            $( where ( $($bounds2:tt)* ) )?
        )?,
        $(
            p |$self3:ident| $( -> $out3:ty )? $block3:block
            $( where ( $($bounds3:tt)* ) )?
        )?,
        $(
            p $( for <$lt41:lifetime> )?
            |&$lt42:lifetime $self4:ident| $( -> $out4:ty )? $block4:block
            $( where ( $($bounds4:tt)* ) )?
        )? $(,)?
    ) => {
        $crate::__impl_sendable!(
            for ( $($params)* ) for $in $( => $out )?
            $( where ( $($bounds)* ) )?,
            $(
                O |$self1| $( -> $out1 )? $block1
                $( where ( $($bounds1)* ) )?
            )?
        );
        $crate::__impl_sendable!(
            for ( $($params)* ) for $in $( => $out )?
            $( where ( $($bounds)* ) )?,
            $(
                O $( for <$lt21> )? |&$lt22 $self2| $( -> $out2 )? $block2
                $( where ( $($bounds2)* ) )?
            )?
        );
        $crate::__impl_sendable!(
            for ( $($params)* ) for $in $( => $out )?
            $( where ( $($bounds)* ) )?,
            $(
                p |$self3| $( -> $out3 )? $block3
                $( where ( $($bounds3)* ) )?
            )?
        );
        $crate::__impl_sendable!(
            for ( $($params)* ) for $in $( => $out )?
            $( where ( $($bounds)* ) )?,
            $(
                p $( for <$lt41> )? |&$lt42 $self4| $( -> $out4 )? $block4
                $( where ( $($bounds4)* ) )?
            )?
        );
    };
}

// ==== Implementations for references ====

__impl_sendable!(
    for ('a, T) for &'a T where ( T: ?Sized ),
    O |self| -> T::Sendable { T::to_sendable(self) } where ( + ToSendable<'a> ),
    O |&'a self| -> T::Sendable { T::to_sendable(self) } where ( + ToSendable<'a> ),
    p |self| -> T::Sendable { T::temp_to_sendable(self) } where ( + TemporaryToSendable<'a> ),
    p |&'a self| -> T::Sendable { T::temp_to_sendable(self) } where ( + TemporaryToSendable<'a> ),
);

__impl_sendable!(
    for ('a, T) for &'a mut T where ( T: ?Sized ),
    O |self| -> T::Sendable { T::to_sendable(self) } where ( + ToSendable<'a> ),
    O |&'a self| -> T::Sendable { T::to_sendable(self) } where ( + ToSendable<'a> ),
    p |self| -> T::Sendable { T::temp_to_sendable(self) } where ( + TemporaryToSendable<'a> ),
    p |&'a self| -> T::Sendable { T::temp_to_sendable(self) } where ( + TemporaryToSendable<'a> ),
);

// ==== Implementations for copiable types ====

macro_rules! impl_copiable {
    ( $($ty:ty),* $(,)? ) => {
        $(
            __impl_sendable!(
                for () for $ty,
                O |self| -> Self { self },
                O for<'a> |&'a self| -> Self { *self },
                p |self| -> Self { self },
                p for<'a> |&'a self| -> Self { *self },
            );
        )*
    }
}

impl_copiable! {
    !, PhantomPinned, bool, char, NonZero<char>,
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize,
    f32, f64,
    NonZero<u8>, NonZero<u16>, NonZero<u32>, NonZero<u64>, NonZero<u128>, NonZero<usize>,
    NonZero<i8>, NonZero<i16>, NonZero<i32>, NonZero<i64>, NonZero<i128>, NonZero<isize>,
    core::any::TypeId, core::cmp::Ordering,
    core::ops::RangeFull,
    core::time::Duration,
    core::fmt::Error,
    core::net::IpAddr, core::net::Ipv4Addr, core::net::Ipv6Addr,
    core::net::SocketAddr, core::net::SocketAddrV4, core::net::SocketAddrV6,
}

__impl_sendable!(
    for (T) for PhantomData<T> => Self where ( T: 'static + Send + Sync ),
    O |self| { PhantomData },
    O for<'a> |&'a self| { PhantomData },
    p |self| { PhantomData },
    p for<'a> |&'a self| { PhantomData },
);

// ==== Macro for cloneable types ====

macro_rules! impl_cloneable {
    ( $( for( $($param:ident),* $(,)? ) $ty:ty ),* $(,)? ) => {
        $(
            __impl_sendable!(
                for ($($param),*) for $ty,
                O |self| -> Self { self }
                    where ( $( $param: 'static + Send + Sync, )* ),
                O |&'static self| -> &'static Self { self }
                    where ( $( $param: 'static + Sync, )* ),
                p |self| -> Self { self }
                    where ( $( $param: 'static + Send + Sync, )* ),
                p for<'a> |&'a self| -> Self { self.clone() }
                    where ( $( $param: 'static + Send + Sync + Clone, )* ),
            );
        )*
    };
    ( $($ty:ty),* $(,)? ) => {
        $(
            __impl_sendable!(
                for () for $ty,
                O |self| -> Self { self },
                O |&'static self| -> &'static Self { self },
                p |self| -> Self { self },
                p for<'a> |&'a self| -> Self { self.clone() },
            );
        )*
    };
}

impl_cloneable! {
    for(T) core::ops::Range<T>,
    for(T) core::ops::RangeInclusive<T>,
    for(T) core::ops::RangeFrom<T>,
    for(T) core::ops::RangeTo<T>,
    for(T) core::ops::RangeToInclusive<T>,
    for(T) core::range::Range<T>,
    for(T) core::range::RangeInclusive<T>,
    for(T) core::range::RangeFrom<T>,
    for(T) core::range::RangeToInclusive<T>,
    for(T) core::ops::Bound<T>,
    for(T) core::cmp::Reverse<T>,
    for(B, C) core::ops::ControlFlow<B, C>,
    for(T) core::mem::Discriminant<T>,
}

// Some types in `core::range` are just aliases for types in `core::ops`:
// - `core::ops::RangeFull` == `core::range::RangeFull`
// - `core::ops::RangeTo<u32>` == `core::range::RangeTo<u32>`
// - `core::ops::RangeTo<u32>` == `core::range::RangeTo<u32>`

// ==== Implementations for non-cloneable types ====

macro_rules! impl_non_cloneable {
    ( $( for( $($param:ident),* $(,)? ) $ty:ty ),* $(,)? ) => {
        $(
            __impl_sendable!(
                for ($($param),*) for $ty,
                O |self| -> Self { self }
                    where ( $( $param: 'static + Send + Sync, )* ),
                O |&'static self| -> &'static Self { self }
                    where ( $( $param: 'static + Sync, )* ),
                p |self| -> Self { self }
                    where ( $( $param: 'static + Send + Sync, )* ),
                p |&'static self| -> &'static Self { self }
                    where ( $( $param: 'static + Sync, )* ),
            );
        )*
    };
    ( $($ty:ty),* $(,)? ) => {
        $(
            __impl_sendable!(
                for () for $ty,
                O |self| -> Self { self },
                O |&'static self| -> &'static Self { self },
                p |self| -> Self { self },
                p |&'static self| -> &'static Self { self },
            );
        )*
    }
}

impl_non_cloneable! {
    AtomicBool,
    AtomicU8, AtomicU16, AtomicU32,  AtomicUsize,
    AtomicI8, AtomicI16, AtomicI32,  AtomicIsize,
}
#[cfg(target_has_atomic = "64")]
impl_non_cloneable! { AtomicU64, AtomicI64 }

impl_non_cloneable! {
    for(Ptr) AtomicPtr<Ptr>,
    for(Ptr) Pin<Ptr>,
    for(T) ManuallyDrop<T>,
    for(T) MaybeUninit<T>,
    for(T) AssertUnwindSafe<T>,
}

// ==== Implementations for simple wrapper types ====

macro_rules! impl_simple_wrapper {
    ( $ty:ident ) => {

        __impl_sendable!(
            for (T) for $ty<T> => $ty<T::Sendable>,
            O |self| { $ty(T::into_sendable(self.0)) }
                where ( T: IntoSendable ),
            O for<'a> |&'a self| { $ty(T::to_sendable(&self.0)) }
                where ( T: ToSendable<'a> ),
            p |self| { $ty(T::temp_into_sendable(self.0)) }
                where ( T: TemporaryIntoSendable ),
            p for<'a> |&'a self| { $ty(T::temp_to_sendable(&self.0)) }
                where ( T: TemporaryToSendable<'a> ),
        );
    }
}

impl_simple_wrapper!(Saturating);
impl_simple_wrapper!(Wrapping);
impl_simple_wrapper!(ViaDisplay);
impl_simple_wrapper!(ViaDebug);
impl_simple_wrapper!(ViaAltDisplay);
impl_simple_wrapper!(ViaAltDebug);
impl_simple_wrapper!(Arg);

// ==== Implementations for delegating types ====

__impl_sendable!(
    for (T) for Option<T> => Option<T::Sendable>,
    O |self| { self.map(T::into_sendable) }
        where ( T: IntoSendable ),
    O for<'a> |&'a self| { self.as_ref().map(T::to_sendable) }
        where ( T: ToSendable<'a> ),
    p |self| { self.map(T::temp_into_sendable) }
        where ( T: TemporaryIntoSendable ),
    p for<'a> |&'a self| { self.as_ref().map(T::temp_to_sendable) }
        where ( T: TemporaryToSendable<'a> ),
);

__impl_sendable!(
    for (T, E) for Result<T, E> => Result<T::Sendable, E::Sendable>,
    O |self| { self.map(T::into_sendable).map_err(E::into_sendable) }
        where ( T: IntoSendable, E: IntoSendable ),
    O for<'a> |&'a self| { self.as_ref().map(T::to_sendable).map_err(E::to_sendable) }
        where ( T: ToSendable<'a>, E: ToSendable<'a> ),
    p |self| { self.map(T::temp_into_sendable).map_err(E::temp_into_sendable) }
        where ( T: TemporaryIntoSendable, E: TemporaryIntoSendable ),
    p for<'a> |&'a self| { self.as_ref().map(T::temp_to_sendable).map_err(E::temp_to_sendable) }
        where ( T: TemporaryToSendable<'a>, E: TemporaryToSendable<'a> ),
);

__impl_sendable!(
    for (T, const N: usize) for [T; N] => [T::Sendable; N],
    O |self| { self.map(T::into_sendable) }
        where ( T: IntoSendable ),
    O for<'a> |&'a self| { self.each_ref().map(T::to_sendable) }
        where ( T: ToSendable<'a> ),
    p |self| { self.map(T::temp_into_sendable) }
        where ( T: TemporaryIntoSendable ),
    p for<'a> |&'a self| { self.each_ref().map(T::temp_to_sendable) }
        where ( T: TemporaryToSendable<'a> ),
);

__impl_sendable!(
    for (T, U) for Ref<'_, T> => U where ( U: 'static + Send + Sync ),
    O |self| { T::to_sendable(&*self) }
        where ( , T: for<'t> ToSendable<'t, Sendable = U> ),
    O for<'a> |&'a self| { T::to_sendable(&**self) }
        where ( , T: for<'t> ToSendable<'t, Sendable = U> ),
    p |self| { T::temp_to_sendable(&*self) }
        where ( , T: for<'t> TemporaryToSendable<'t, Sendable = U>),
    p for<'a> |&'a self| { T::temp_to_sendable(&**self) }
        where ( , T: for<'t> TemporaryToSendable<'t, Sendable = U> ),
);

__impl_sendable!(
    for (T, U) for RefMut<'_, T> => U where ( U: 'static + Send + Sync ),
    O |self| { T::to_sendable(&*self) }
        where ( , T: for<'t> ToSendable<'t, Sendable = U> ),
    O for<'a> |&'a self| { T::to_sendable(&**self) }
        where ( , T: for<'t> ToSendable<'t, Sendable = U> ),
    p |self| { T::temp_to_sendable(&*self) }
        where ( , T: for<'t> TemporaryToSendable<'t, Sendable = U>),
    p for<'a> |&'a self| { T::temp_to_sendable(&**self) }
        where ( , T: for<'t> TemporaryToSendable<'t, Sendable = U> ),
);

__impl_sendable!(
    for (T) for Cell<T> => T::Sendable,
    O |self| { T::into_sendable(self.into_inner()) }
        where ( T: IntoSendable ),
    O for<'a> |&'a self| { T::into_sendable(self.get()) }
        where ( T: Copy + IntoSendable ),
    p |self| { T::temp_into_sendable(self.into_inner()) }
        where ( T: TemporaryIntoSendable),
    p for<'a> |&'a self| { T::temp_into_sendable(self.get()) }
        where ( T: Copy + TemporaryIntoSendable ),
);

// `RefCell<T>: !Sync`, so `&RefCell<T>` cannot be turned into a `Sync`
// Sendable without taking the borrow at an unpredictable time.
// `RefCell::borrow()` also panics if the cell is already mutably
// borrowed, which is not a sound foundation for a by-reference
// conversion.
// By-value conversion moves the value out and avoids both problems.
__impl_sendable!(
    for (T) for RefCell<T> => T::Sendable,
    O |self| { T::into_sendable(self.into_inner()) }
        where ( T: IntoSendable ),
    /* skip optimistic by-reference */ ,
    p |self| { T::temp_into_sendable(self.into_inner()) }
        where ( T: TemporaryIntoSendable),
    /* skip pessimistic by-reference */ ,
);

__impl_sendable!(
    for (T) for OnceCell<T> => Option<T::Sendable>,
    O |self| { self.into_inner().map(T::into_sendable) }
        where ( T: IntoSendable ),
    O for<'a> |&'a self| { self.get().map(T::to_sendable) }
        where ( T: ToSendable<'a> ),
    p |self| { self.into_inner().map(T::temp_into_sendable) }
        where ( T: TemporaryIntoSendable),
    p for<'a> |&'a self| { self.get().map(T::temp_to_sendable) }
        where ( T: TemporaryToSendable<'a> ),
);

__impl_sendable!(
    for (T, F, U) for LazyCell<T, F> => U where ( F: FnOnce() -> T, U: 'static + Send + Sync ),
    O |self| { T::to_sendable(&*self) }
        where ( , T: for<'t> ToSendable<'t, Sendable = U> ),
    O for<'a> |&'a self| { T::to_sendable(&**self) }
        where ( , T: ToSendable<'a, Sendable = U> ),
    p |self| { T::temp_to_sendable(&*self) }
        where ( , T: for<'t> TemporaryToSendable<'t, Sendable = U>),
    p for<'a> |&'a self| { T::temp_to_sendable(&**self) }
        where ( , T: TemporaryToSendable<'a, Sendable = U> ),
);

// ==== Implementations for unsized types ====

__impl_sendable!(
    for () for str => &'static Self, O |&'static self| { self }
);

__impl_sendable!(
    for () for CStr => &'static Self, O |&'static self| { self }
);

__impl_sendable!(
    for (T) for [T] => &'static Self where ( T: 'static + Sync ),
    O |&'static self| { self }
);

// ==== Implementations for pointers ====

__impl_sendable!(
    for (T) for *const T => UnsafeSendablePtr<Self> where ( T: 'static + ?Sized ),
    O |self| { UnsafeSendablePtr::new(self) },
    O for<'a> |&'a self| { UnsafeSendablePtr::new(*self) },
    p |self| { UnsafeSendablePtr::new(self) },
    p for<'a> |&'a self| { UnsafeSendablePtr::new(*self) },
);

__impl_sendable!(
    for (T) for *mut T => UnsafeSendablePtr<Self> where ( T: 'static + ?Sized ),
    O |self| { UnsafeSendablePtr::new(self) },
    O for<'a> |&'a self| { UnsafeSendablePtr::new(*self) },
    p |self| { UnsafeSendablePtr::new(self) },
    p for<'a> |&'a self| { UnsafeSendablePtr::new(*self) },
);

__impl_sendable!(
    for (T) for NonNull<T> => UnsafeSendablePtr<Self> where ( T: 'static + ?Sized ),
    O |self| { UnsafeSendablePtr::new(self) },
    O for<'a> |&'a self| { UnsafeSendablePtr::new(*self) },
    p |self| { UnsafeSendablePtr::new(self) },
    p for<'a> |&'a self| { UnsafeSendablePtr::new(*self) },
);

// ==== Implementations for tuples ====

impl IntoSendable for () {
    type Sendable = ();
    #[inline]
    fn into_sendable(self) -> Self::Sendable {}
}

impl<'a> ToSendable<'a> for () {
    type Sendable = ();
    #[inline]
    fn to_sendable(&'a self) -> Self::Sendable {}
}

impl TemporaryIntoSendable for () {
    type Sendable = ();
    #[inline]
    fn temp_into_sendable(self) -> Self::Sendable {}
}

impl<'a> TemporaryToSendable<'a> for () {
    type Sendable = ();
    #[inline]
    fn temp_to_sendable(&'a self) -> Self::Sendable {}
}

macro_rules! impl_variadics {
    ( [ $( $args:tt )* ] [] ) => {};
    ( [ $( $args:tt )* ] [ $next:tt $( $rest:tt )* ] ) => {
        impl_variadic!( $( $args )* $next );
        impl_variadics!( [ $( $args )* $next ] [ $( $rest )* ] );
    };
}

macro_rules! impl_variadic {
    ( $( ( $field:tt $type:ident ) )* ) => {
        impl<$( $type ),*> IntoSendable for ( $( $type ),*, )
        where
            $( $type: IntoSendable, )*
        {
            type Sendable = ( $( $type::Sendable, )* );

            #[inline]
            fn into_sendable(self) -> Self::Sendable {
                (
                    $( self.$field.into_sendable(), )*
                )
            }
        }

        impl<'a, $( $type ),*> ToSendable<'a> for ( $( $type ),*, )
        where
            $( $type: ToSendable<'a>, )*
        {
            type Sendable = ( $( $type::Sendable, )* );

            #[inline]
            fn to_sendable(&'a self) -> Self::Sendable {
                (
                    $( self.$field.to_sendable(), )*
                )
            }
        }

        impl<$( $type ),*> TemporaryIntoSendable for ( $( $type ),*, )
        where
            $( $type: TemporaryIntoSendable, )*
        {
            type Sendable = ( $( $type::Sendable, )* );

            #[inline]
            fn temp_into_sendable(self) -> Self::Sendable {
                (
                    $( self.$field.temp_into_sendable(), )*
                )
            }
        }

        impl<'a, $( $type ),*> TemporaryToSendable<'a> for ( $( $type ),*, )
        where
            $( $type: TemporaryToSendable<'a>, )*
        {
            type Sendable = ( $( $type::Sendable, )* );

            #[inline]
            fn temp_to_sendable(&'a self) -> Self::Sendable {
                (
                    $( self.$field.temp_to_sendable(), )*
                )
            }
        }
    };
}

impl_variadics! {
    [] [
        (0 T1) (1 T2) (2 T3) (3 T4)
        (4 T5) (5 T6) (6 T7) (7 T8)
        (8 T9) (9 T10) (10 T11) (11 T12)
    ]
}

// ==== Helper functions ====

#[cfg(feature = "alloc")]
mod alloc_impls {
    use alloc::borrow::{Cow, ToOwned};
    use alloc::boxed::Box;
    use alloc::collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque};
    use alloc::ffi::CString;
    use alloc::rc::Rc;
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec::Vec;
    use core::alloc::Allocator;
    use core::ffi::CStr;

    use crate::{TemporaryIntoSendable, TemporaryToSendable, ToSendable};

    // ==== Implementations for unsized types ====

    __impl_sendable!(
        for () for str => String, p for<'a> |&'a self| { self.to_owned() }
    );

    __impl_sendable!(
        for () for CStr => CString, p for<'a> |&'a self| { self.to_owned() }
    );

    __impl_sendable!(
        for ('a, T) for [T] => Vec<T::Sendable> where ( T: TemporaryToSendable<'a> ),
        p |&'a self| { self.iter().map(T::temp_to_sendable).collect() }
    );

    // ==== Implementations for allocating types ====

    impl_cloneable! { String, CString }

    __impl_sendable!(
        for (T, A: Allocator) for Box<T, A>,
        O |self| -> Self { self }
            where ( T: 'static + ?Sized + Send + Sync, A: 'static + Send + Sync ),
        O |&'static self| -> &'static Self { self }
            where ( T: 'static + ?Sized + Sync, A: 'static + Sync ),
        p |self| -> T::Sendable { T::temp_into_sendable(*self) }
            where ( T: TemporaryIntoSendable ),
        p for<'a> |&'a self| -> T::Sendable { T::temp_to_sendable(&**self) }
            where ( T: TemporaryToSendable<'a> ),
    );

    // The Rc clone is unavoidable as it doesn't implement no `Sync`.
    __impl_sendable!(
        for (T, U) for Rc<T>
            where ( T: for<'t> ToSendable<'t, Sendable = U>, U: 'static + Send + Sync ),
        O |self| -> U { T::to_sendable(&*self) }
    );
    __impl_sendable!(
        for (T) for Rc<T> where ( T: ToSendable<'a> ),
        O for<'a> |&'a self| -> T::Sendable { T::to_sendable(&**self) }
    );
    __impl_sendable!(
        for (T, U) for Rc<T>
            where ( T: for<'t> TemporaryToSendable<'t, Sendable = U>, U: 'static + Send + Sync ),
        p |self| -> U { T::temp_to_sendable(&*self) }
    );
    __impl_sendable!(
        for (T) for Rc<T> where ( T: TemporaryToSendable<'a> ),
        p for<'a> |&'a self| -> T::Sendable { T::temp_to_sendable(&**self) }
    );

    __impl_sendable!(
        for (T, U) for alloc::rc::Weak<T>
            where ( T: for<'t> ToSendable<'t, Sendable = U>, U: 'static + Send + Sync ),
        O |self| -> Option<U> { Some(T::to_sendable(&*self.upgrade()?)) }
    );
    __impl_sendable!(
        for (T, U) for alloc::rc::Weak<T>
            where ( T: for<'t> ToSendable<'t, Sendable = U>, U: 'static + Send + Sync ),
        O for<'a> |&'a self| -> Option<U> { Some(T::to_sendable(&*self.upgrade()?)) }
    );
    __impl_sendable!(
        for (T, U) for alloc::rc::Weak<T>
            where ( T: for<'t> TemporaryToSendable<'t, Sendable = U>, U: 'static + Send + Sync ),
        p |self| -> Option<U> { Some(T::temp_to_sendable(&*self.upgrade()?)) }
    );
    __impl_sendable!(
        for (T, U) for alloc::rc::Weak<T>
            where ( T: for<'t> TemporaryToSendable<'t, Sendable = U>, U: 'static + Send + Sync ),
        p for<'a> |&'a self| -> Option<U> { Some(T::temp_to_sendable(&*self.upgrade()?)) }
    );

    __impl_sendable!(
        for (T) for Arc<T> where ( T: 'static + ?Sized + Send + Sync ),
        O |self| -> Self { self }
    );
    __impl_sendable!(
        for (T) for Arc<T> where ( T: 'static + ?Sized + Sync ),
        O |&'static self| -> &'static T { self }
    );
    __impl_sendable!(
        for (T, U) for Arc<T>
            where ( T: for<'t> TemporaryToSendable<'t, Sendable = U>, U: 'static + Send + Sync ),
        p |self| -> U { T::temp_to_sendable(&*self) }
    );
    __impl_sendable!(
        for (T) for Arc<T>
            where ( T: TemporaryToSendable<'a> ),
        p for<'a> |&'a self| -> T::Sendable { T::temp_to_sendable(&**self) }
    );

    __impl_sendable!(
        for (T) for alloc::sync::Weak<T> where (T: 'static + Send + Sync),
        O |self| -> Self { self },
        O for<'a> |&'a self| -> Self { alloc::sync::Weak::<T>::clone(self) },
        p |self| -> Self { self },
        p for<'a> |&'a self| -> Self { alloc::sync::Weak::<T>::clone(self) },
    );

    __impl_sendable!(
        for (T) for Cow<'static, T>
            where ( T: 'static + ?Sized + Sync + ToOwned, T::Owned: Send + Sync ),
        O |self| -> Self { self }
    );
    __impl_sendable!(
        for (T) for Cow<'static, T>
            where ( T: 'static + ?Sized + Sync + ToOwned, T::Owned: Sync ),
        O |&'static self| -> &'static Self { self }
    );
    __impl_sendable!(
        for (T, U) for Cow<'_, T> => U
            where (
                T: ?Sized + ToOwned,
                T::Owned: TemporaryIntoSendable<Sendable = U>,
                U: 'static + Send + Sync,
            ),
        p |self| { T::Owned::temp_into_sendable(self.into_owned()) }
    );
    __impl_sendable!(
        for (T, U) for Cow<'_, T> => U
            where (
                T: 'static + ?Sized + Sync + ToOwned + TemporaryToSendable<'a, Sendable = U>,
                U: 'static + Send + Sync,
            ),
        p for<'a> |&'a self| { T::temp_to_sendable(&**self) }
    );

    // ==== Implementations for collections ====

    __impl_sendable!(
        for (T, A: Allocator) for Vec<T, A>,
        O |self| -> Self { self }
            where ( T: 'static + Send + Sync, A: 'static + Send + Sync ),
        O |&'static self| -> &'static Self { self }
            where ( T: 'static + Sync, A: 'static + Sync ),,,
    );

    // `FromIterator<T>` is currently implemented only for `Vec<T, Global>`.
    // Prefer optimized stdlib `FromIterator` implementation with
    // specializations over the custom generic implementation for any allocator.
    __impl_sendable!(
        for (T) for Vec<T>,,,
        p |self| -> Vec<T::Sendable> {
            self.into_iter().map(TemporaryIntoSendable::temp_into_sendable).collect()
        } where ( T: TemporaryIntoSendable ),
        p for<'a> |&'a self| -> Vec<T::Sendable> {
            self.iter().map(TemporaryToSendable::temp_to_sendable).collect()
        } where ( T: TemporaryToSendable<'a> ),
    );

    macro_rules! impl_simple_collection {
        ( for($($param:tt)*) $ty:ident<$($arg:ident),* $(,)?> $(, where $($bounds:tt)* )? ) => {
            __impl_sendable!(
                for ($($param),*) for $ty<$($arg),*>,
                O |self| -> Self { self }
                    where ( $( $arg: 'static + Send + Sync, )* ),
                O |&'static self| -> &'static Self { self }
                    where ( $( $arg: 'static + Sync, )* ),
                p |self| -> $ty<$($arg::Sendable),*> {
                    self.into_iter().map(TemporaryIntoSendable::temp_into_sendable).collect()
                } where ( $( $arg: TemporaryIntoSendable, )* $($($bounds)*)? ),
                p for<'a> |&'a self| -> $ty<$($arg::Sendable),*> {
                    self.into_iter().map(TemporaryToSendable::temp_to_sendable).collect()
                } where ( $( $arg: TemporaryToSendable<'a>, )* $($($bounds)*)? ),
            );
        };
    }

    impl_simple_collection!(for(T) VecDeque<T>);
    impl_simple_collection!(for(T) LinkedList<T>);
    impl_simple_collection!(for(T) BinaryHeap<T>, where T::Sendable: Ord );
    impl_simple_collection!(for(T) BTreeSet<T>, where T::Sendable: Ord );

    __impl_sendable!(
        for (K, V) for BTreeMap<K, V>,
        O |self| -> Self { self }
            where ( K: 'static + Send + Sync, V: 'static + Send + Sync ),
        O |&'static self| -> &'static Self { self }
            where ( K: 'static + Sync, V: 'static + Sync ),
        p |self| -> BTreeMap<K::Sendable, V::Sendable> {
            self.into_iter().map(TemporaryIntoSendable::temp_into_sendable).collect()
        }
            where ( K: TemporaryIntoSendable, V: TemporaryIntoSendable, K::Sendable: Ord ),
        p for<'a> |&'a self| -> BTreeMap<K::Sendable, V::Sendable> {
            self.iter()
                .map(|(k, v)| (k.temp_to_sendable(), v.temp_to_sendable()))
                .collect()
        }
            where ( K: TemporaryToSendable<'a>, V: TemporaryToSendable<'a>, K::Sendable: Ord ),
    );
}

#[cfg(feature = "std")]
mod std_impls {
    use alloc::vec::Vec;
    use core::hash::{BuildHasher, Hash};
    // Hack to prevent `clippy::std_instead_of_core` failure.
    // `std::io::ErrorKind` moved to `core` in nightly.
    // `clippy::std_instead_of_core` works only if disabled on crate level.
    // See: https://github.com/rust-lang/rust/issues/154046.
    use std as std_or_core;
    use std::borrow::ToOwned;
    use std::collections::{HashMap, HashSet};
    use std::ffi::{OsStr, OsString};
    use std::path::{Path, PathBuf};
    use std::sync::{LazyLock, MutexGuard, OnceLock, RwLockReadGuard, RwLockWriteGuard};

    use crate::{
        CloneableStdIoError, IntoSendable, TemporaryIntoSendable, TemporaryToSendable, ToSendable,
    };

    // ==== Implementations for non-generic types ====

    impl_copiable! {
        std_or_core::io::ErrorKind,
        std::time::Instant, std::time::SystemTime,
        std::thread::ThreadId,
        std::io::SeekFrom,
        std::process::ExitStatus, std::process::ExitCode,
    }

    impl_cloneable! {
        for(T) std::io::Cursor<T>,
    }

    impl_non_cloneable! {
        std::backtrace::Backtrace,
    }

    __impl_sendable!(
        for () for std::io::IoSlice<'static>,
        O |self| -> Self { self },
        O for<'b> |&'b self| -> Self { *self },,,
    );
    __impl_sendable!(
        for () for std::io::IoSlice<'_>,,,
        p |self| -> Vec<u8> { self.to_vec() },
        p for<'b> |&'b self| -> Vec<u8> { self.to_vec() },
    );

    __impl_sendable!(
        for () for std::io::IoSliceMut<'static>,
        O |self| -> Self { self },,,,
    );
    __impl_sendable!(
        for () for std::io::IoSliceMut<'_>,,
        O for<'b> |&'b self| -> Vec<u8> { self.to_vec() },
        p |self| -> Vec<u8> { self.to_vec() },
        p for<'b> |&'b self| -> Vec<u8> { self.to_vec() },
    );

    __impl_sendable!(
        for () for std::io::Error,
        O |self| -> Self { self },
        O |&'static self| -> &'static Self { self },
        p |self| -> Self { self },
        p for<'a> |&'a self| -> CloneableStdIoError { CloneableStdIoError::from_ref(self) },
    );

    // ==== Implementations for delegating types ====

    __impl_sendable!(
        for (T) for OnceLock<T> => Option<T::Sendable>,
        O |self| { self.into_inner().map(T::into_sendable) }
            where ( T: IntoSendable ),
        O for<'a> |&'a self| { self.get().map(T::to_sendable) }
            where ( T: ToSendable<'a> ),
        p |self| { self.into_inner().map(T::temp_into_sendable) }
            where ( T: TemporaryIntoSendable ),
        p for<'a> |&'a self| { self.get().map(T::temp_to_sendable) }
            where ( T: TemporaryToSendable<'a> ),
    );

    __impl_sendable!(
        for (T, F, U) for LazyLock<T, F> => U where ( F: FnOnce() -> T, U: 'static + Send + Sync ),
        O |self| { T::to_sendable(&*self) }
            where ( , T: for<'t> ToSendable<'t, Sendable = U>, ),
        O for<'a> |&'a self| { T::to_sendable(&**self) }
            where ( , T: ToSendable<'a, Sendable = U>, ),
        p |self| { T::temp_to_sendable(&*self) }
            where ( , T: for<'t> TemporaryToSendable<'t, Sendable = U>,
            ),
        p for<'a> |&'a self| { T::temp_to_sendable(&**self) }
            where ( , T: TemporaryToSendable<'a, Sendable = U>, ),
    );

    // ==== Implementations for unsized types ====

    __impl_sendable!(
        for () for OsStr,
        /* optimistic by-value is implemented for non-alloc */,
        O |&'static self| -> &'static OsStr { self },
        /* optimistic by-reference is implemented for non-alloc */,
        p for<'a> |&'a self| -> OsString { self.to_owned() },
    );

    __impl_sendable!(
        for () for Path,
        /* optimistic by-value is implemented for non-alloc */,
        O |&'static self| -> &'static Path { self },
        /* optimistic by-reference is implemented for non-alloc */,
        p for<'a> |&'a self| -> PathBuf { self.to_owned() },
    );

    // ==== Implementations for clonable types ====

    impl_cloneable! { OsString, PathBuf }

    // ==== Implementations for collections ====

    __impl_sendable!(
        for (T, S) for HashSet<T, S> => Self
            where ( T: 'static + Send + Sync, S: 'static + Send + Sync ),
        O |self| { self }
    );
    __impl_sendable!(
        for (T, S) for HashSet<T, S> => &'static Self
            where ( T: 'static + Sync, S: 'static + Sync ),
        O |&'static self| { self }
    );
    __impl_sendable!(
        for (T, S) for HashSet<T, S> => HashSet<T::Sendable, S>
            where (
                T: TemporaryIntoSendable,
                T::Sendable: Eq + Hash,
                S: 'static + Send + Sync + Default + BuildHasher,
            ),
        p |self| { self.into_iter().map(TemporaryIntoSendable::temp_into_sendable).collect() }
    );
    __impl_sendable!(
        for ('a, T, S) for HashSet<T, S> => HashSet<T::Sendable, S>
            where (
                T: TemporaryToSendable<'a>,
                T::Sendable: Eq + Hash,
                S: 'static + Send + Sync + Default + BuildHasher,
            ),
        p |&'a self| {
            self.iter().map(TemporaryToSendable::temp_to_sendable).collect()
        }
    );

    __impl_sendable!(
        for (K, V, S) for HashMap<K, V, S> => Self
            where ( K: 'static + Send + Sync, V: 'static + Send + Sync, S: 'static + Send + Sync ),
        O |self| { self }
    );
    __impl_sendable!(
        for (K, V, S) for HashMap<K, V, S> => &'static Self
            where ( K: 'static + Sync, V: 'static + Sync, S: 'static + Sync ),
        O |&'static self| { self }
    );
    __impl_sendable!(
        for (K, V, S) for HashMap<K, V, S> => HashMap<K::Sendable, V::Sendable, S>
            where (
                K: TemporaryIntoSendable,
                V: TemporaryIntoSendable,
                K::Sendable: Eq + Hash,
                S: 'static + Send + Sync + Default + BuildHasher,
            ),
        p |self| { self.into_iter().map(TemporaryIntoSendable::temp_into_sendable).collect() }
    );
    __impl_sendable!(
        for ('a, K, V, S) for HashMap<K, V, S> => HashMap<K::Sendable, V::Sendable, S>
            where (
                K: TemporaryToSendable<'a>,
                V: TemporaryToSendable<'a>,
                K::Sendable: Eq + Hash,
                S: 'static + Send + Sync + Default + BuildHasher,
            ),
        p |&'a self| {
            self.iter()
                .map(|(k, v)| (k.temp_to_sendable(), v.temp_to_sendable()))
                .collect()
        }
    );

    // ==== Implementations for lock references ====

    __impl_sendable!(
        for (T, U) for MutexGuard<'_, T>,
        O |self| -> U { T::to_sendable(&*self) }
            where (
                T: for<'b> ToSendable<'b, Sendable = U>,
                U: 'static + Send + Sync
            ),,
        p |self| -> U { T::temp_to_sendable(&*self) }
            where (
                T: for<'b> TemporaryToSendable<'b, Sendable = U>,
                U: 'static + Send + Sync
            ),,
    );
    __impl_sendable!(
        for (T) for MutexGuard<'_, T>,,
        O for<'b> |&'b self| -> T::Sendable { T::to_sendable(self) }
            where ( T: ToSendable<'b> ),,
        p for<'b> |&'b self| -> T::Sendable { T::temp_to_sendable(self) }
            where ( T: TemporaryToSendable<'b> )
    );

    __impl_sendable!(
        for (T, U) for RwLockReadGuard<'_, T>,
        O |self| -> U { T::to_sendable(&*self) }
            where (
                T: for<'b> ToSendable<'b, Sendable = U>,
                U: 'static + Send + Sync
            ),,
        p |self| -> U { T::temp_to_sendable(&*self) }
            where (
                T: for<'b> TemporaryToSendable<'b, Sendable = U>,
                U: 'static + Send + Sync
            ),,
    );
    __impl_sendable!(
        for (T) for RwLockReadGuard<'_, T>,,
        O for<'b> |&'b self| -> T::Sendable { T::to_sendable(self) }
            where ( T: ToSendable<'b> ),,
        p for<'b> |&'b self| -> T::Sendable { T::temp_to_sendable(self) }
            where ( T: TemporaryToSendable<'b> )
    );

    __impl_sendable!(
        for (T, U) for RwLockWriteGuard<'_, T>,
        O |self| -> U { T::to_sendable(&*self) }
            where (
                T: for<'b> ToSendable<'b, Sendable = U>,
                U: 'static + Send + Sync
            ),,
        p |self| -> U { T::temp_to_sendable(&*self) }
            where (
                T: for<'b> TemporaryToSendable<'b, Sendable = U>,
                U: 'static + Send + Sync
            ),,
    );
    __impl_sendable!(
        for (T) for RwLockWriteGuard<'_, T>,,
        O for<'b> |&'b self| -> T::Sendable { T::to_sendable(self) }
            where ( T: ToSendable<'b> ),,
        p for<'b> |&'b self| -> T::Sendable { T::temp_to_sendable(self) }
            where ( T: TemporaryToSendable<'b> )
    );
}

#[cfg(test)]
mod tests {
    use core::any::TypeId;
    use core::cell::{Cell, LazyCell, OnceCell, RefCell};
    use core::cmp::{self, Ordering};
    use core::fmt::Debug;
    use core::marker::{PhantomData, PhantomPinned};
    use core::mem::{ManuallyDrop, MaybeUninit};
    use core::num::{NonZero, Saturating, Wrapping};
    use core::ops::Range;
    use core::panic::AssertUnwindSafe;
    use core::pin::Pin;
    use core::ptr::NonNull;
    use core::sync::atomic::Ordering::Relaxed;
    use core::sync::atomic::{
        AtomicBool, AtomicI8, AtomicI16, AtomicI32, AtomicIsize, AtomicPtr, AtomicU8, AtomicU16,
        AtomicU32, AtomicUsize,
    };
    #[cfg(target_has_atomic = "64")]
    use core::sync::atomic::{AtomicI64, AtomicU64};
    use core::time::Duration;
    use core::{fmt, mem, net, ops, range};

    use crate::{
        Arg, IntoSendable, TemporaryIntoSendable, TemporaryToSendable, ToSendable,
        UnsafeSendablePtr, ViaAltDebug, ViaAltDisplay, ViaDebug, ViaDisplay,
    };

    macro_rules! non_zero {
        ($value:expr) => {
            const { NonZero::new($value).unwrap() }
        };
    }

    #[track_caller]
    fn check_copiable_eq<T>(value: T)
    where
        T: Copy
            + PartialEq
            + Debug
            + IntoSendable<Sendable = T>
            + for<'a> ToSendable<'a, Sendable = T>
            + TemporaryIntoSendable<Sendable = T>
            + for<'a> TemporaryToSendable<'a, Sendable = T>,
    {
        assert_eq!(value.into_sendable(), value);
        assert_eq!(value.to_sendable(), value);
        assert_eq!(value.temp_into_sendable(), value);
        assert_eq!(value.temp_to_sendable(), value);
    }

    #[track_caller]
    fn check_lazy_cloneable_eq<T>(value: &T)
    where
        T: 'static
            + Clone
            + PartialEq
            + Debug
            + IntoSendable<Sendable = T>
            + ToSendable<'static, Sendable = &'static T>
            + TemporaryIntoSendable<Sendable = T>
            + for<'a> TemporaryToSendable<'a, Sendable = T>,
    {
        let static_ref: &'static Option<T> = &None;

        assert_eq!(value.clone().into_sendable(), value.clone());
        assert_eq!(static_ref.to_sendable(), None);
        assert_eq!(value.clone().temp_into_sendable(), value.clone());
        assert_eq!(&value.temp_to_sendable(), value);
    }

    #[track_caller]
    fn check_non_cloneable<T, F1, F2>(mut func: F1, mut check: F2)
    where
        T: 'static
            + IntoSendable<Sendable = T>
            + ToSendable<'static, Sendable = &'static T>
            + TemporaryIntoSendable<Sendable = T>
            + TemporaryToSendable<'static, Sendable = &'static T>,
        F1: FnMut() -> T,
        F2: FnMut(&T) -> bool,
    {
        assert!(check(&func().into_sendable()));
        assert!(check(&func().temp_into_sendable()));
    }

    #[cfg(not(miri))]
    #[cfg(feature = "alloc")]
    fn leak<T>(value: T) -> &'static T {
        use alloc::boxed::Box;
        Box::leak(Box::new(value))
    }

    #[expect(
        clippy::mut_mut,
        clippy::unnecessary_mut_passed,
        reason = "explicitly tested"
    )]
    #[test]
    fn test_into_sendable_core_references() {
        assert_eq!((&&&&123).into_sendable(), 123);
        assert_eq!((&&&&123).to_sendable(), 123);
        assert_eq!((&&&&123).temp_into_sendable(), 123);
        assert_eq!((&&&&123).temp_to_sendable(), 123);

        assert_eq!((&mut &mut &mut &mut 123).into_sendable(), 123);
        assert_eq!((&mut &mut &mut &mut 123).to_sendable(), 123);
        assert_eq!((&mut &mut &mut &mut 123).temp_into_sendable(), 123);
        assert_eq!((&mut &mut &mut &mut 123).temp_to_sendable(), 123);

        assert_eq!((&mut &&mut &123).into_sendable(), 123);
        assert_eq!((&mut &&mut &123).to_sendable(), 123);
        assert_eq!((&mut &&mut &123).temp_into_sendable(), 123);
        assert_eq!((&mut &&mut &123).temp_to_sendable(), 123);
    }

    #[test]
    fn test_into_sendable_core_copiable() {
        check_copiable_eq(Option::<!>::None);
        check_copiable_eq(PhantomPinned);
        check_copiable_eq(true);
        check_copiable_eq('a');
        check_copiable_eq(non_zero!('a'));

        check_copiable_eq(123_u8);
        check_copiable_eq(123_u16);
        check_copiable_eq(123_u32);
        check_copiable_eq(123_u64);
        check_copiable_eq(123_u128);
        check_copiable_eq(123_usize);
        check_copiable_eq(123_i8);
        check_copiable_eq(123_i16);
        check_copiable_eq(123_i32);
        check_copiable_eq(123_i64);
        check_copiable_eq(123_i128);
        check_copiable_eq(123_isize);

        check_copiable_eq(non_zero!(123_u8));
        check_copiable_eq(non_zero!(123_u16));
        check_copiable_eq(non_zero!(123_u32));
        check_copiable_eq(non_zero!(123_u64));
        check_copiable_eq(non_zero!(123_u128));
        check_copiable_eq(non_zero!(123_usize));
        check_copiable_eq(non_zero!(123_i8));
        check_copiable_eq(non_zero!(123_i16));
        check_copiable_eq(non_zero!(123_i32));
        check_copiable_eq(non_zero!(123_i64));
        check_copiable_eq(non_zero!(123_i128));
        check_copiable_eq(non_zero!(123_isize));

        check_copiable_eq(123.234_f32);
        check_copiable_eq(123.234_f64);
        check_copiable_eq(TypeId::of::<u32>());
        check_copiable_eq(Ordering::Equal);
        check_copiable_eq(..);
        check_copiable_eq(Duration::from_secs(1));
        check_copiable_eq(fmt::Error);

        let ipv4 = net::Ipv4Addr::LOCALHOST;
        let ipv6 = net::Ipv6Addr::LOCALHOST;
        check_copiable_eq(net::IpAddr::V4(ipv4));
        check_copiable_eq(ipv4);
        check_copiable_eq(ipv6);
        check_copiable_eq(net::SocketAddr::new(net::IpAddr::V4(ipv4), 123));
        check_copiable_eq(net::SocketAddrV4::new(ipv4, 123));
        check_copiable_eq(net::SocketAddrV6::new(ipv6, 123, 0, 0));

        check_copiable_eq(PhantomData::<u32>);
    }

    #[test]
    fn test_into_sendable_core_cloneable() {
        static RANGE: Range<i32> = 10..20;

        #[derive(Copy, Clone, Debug)]
        enum Enum {
            A(#[expect(dead_code, reason = "used for discriminant")] i32),
            #[expect(dead_code, reason = "used for discriminant")]
            B(u32),
        }

        assert_eq!(RANGE.clone().into_sendable(), RANGE.clone());
        assert_eq!(RANGE.to_sendable(), &RANGE);
        assert_eq!(RANGE.clone().temp_into_sendable(), RANGE.clone());
        assert_eq!(RANGE.temp_to_sendable(), RANGE);

        check_lazy_cloneable_eq(&(10..20));
        check_lazy_cloneable_eq(&(10..=20));
        check_lazy_cloneable_eq(&(10..));
        check_lazy_cloneable_eq(&(..20));
        check_lazy_cloneable_eq(&(..=20));
        check_lazy_cloneable_eq(&range::Range::from(10..20));
        check_lazy_cloneable_eq(&range::RangeInclusive::from(10..=20));
        check_lazy_cloneable_eq(&range::RangeFrom::from(10..));
        // `range::RangeTo` and `ops::RangeTo` are the same types.
        check_lazy_cloneable_eq(&range::RangeToInclusive::from(..=20));
        check_lazy_cloneable_eq(&ops::Bound::Included(123));
        check_lazy_cloneable_eq(&cmp::Reverse(123));
        check_lazy_cloneable_eq(&ops::ControlFlow::<i32, ()>::Break(123));

        let discriminant = mem::discriminant(&Enum::A(123));
        check_lazy_cloneable_eq(&discriminant);
    }

    #[test]
    fn test_into_sendable_core_non_cloneable() {
        static ATOMIC: AtomicU32 = AtomicU32::new(123);
        static UNINIT: MaybeUninit<i32> = MaybeUninit::uninit();

        let mut value = 123;
        let value_mut_ptr = &raw mut value;

        check_non_cloneable(|| AtomicBool::new(true), |v| v.load(Relaxed));
        check_non_cloneable(|| AtomicU8::new(123), |v| v.load(Relaxed) == 123);
        check_non_cloneable(|| AtomicU16::new(123), |v| v.load(Relaxed) == 123);
        check_non_cloneable(|| AtomicU32::new(123), |v| v.load(Relaxed) == 123);
        #[cfg(target_has_atomic = "64")]
        check_non_cloneable(|| AtomicU64::new(123), |v| v.load(Relaxed) == 123);
        check_non_cloneable(|| AtomicUsize::new(123), |v| v.load(Relaxed) == 123);
        check_non_cloneable(|| AtomicI8::new(123), |v| v.load(Relaxed) == 123);
        check_non_cloneable(|| AtomicI16::new(123), |v| v.load(Relaxed) == 123);
        check_non_cloneable(|| AtomicI32::new(123), |v| v.load(Relaxed) == 123);
        #[cfg(target_has_atomic = "64")]
        check_non_cloneable(|| AtomicI64::new(123), |v| v.load(Relaxed) == 123);
        check_non_cloneable(|| AtomicIsize::new(123), |v| v.load(Relaxed) == 123);

        check_non_cloneable(
            || AtomicPtr::new(value_mut_ptr),
            |v| v.load(Relaxed) == value_mut_ptr,
        );
        check_non_cloneable(|| Pin::new(&123), |v| **v == 123);
        check_non_cloneable(|| ManuallyDrop::new(123), |_| true);
        check_non_cloneable(MaybeUninit::<i32>::uninit, |_| true);
        check_non_cloneable(|| AssertUnwindSafe(123), |v| v.0 == 123);

        let _unused: &AtomicU32 = ATOMIC.to_sendable();
        let _unused: &AtomicU32 = ATOMIC.temp_to_sendable();

        let _unused: MaybeUninit<i32> = UNINIT.into_sendable();
        let _unused: &MaybeUninit<i32> = UNINIT.to_sendable();
        let _unused: MaybeUninit<i32> = UNINIT.temp_into_sendable();
        let _unused: &MaybeUninit<i32> = UNINIT.temp_to_sendable();
    }

    #[test]
    fn test_into_sendable_core_wrappers() {
        // Newtype wrappers of copiable types.
        check_copiable_eq(Saturating(123));
        check_copiable_eq(Wrapping(123));
        check_copiable_eq(ViaDisplay(123));
        check_copiable_eq(ViaDebug(123));
        check_copiable_eq(ViaAltDisplay(123));
        check_copiable_eq(ViaAltDebug(123));
        check_copiable_eq(Arg(123));
        check_copiable_eq(Some(123));
        check_copiable_eq(Result::<i32, i32>::Ok(123));
        check_copiable_eq(Result::<i32, i32>::Err(123));
        check_copiable_eq([1, 2, 3, 4, 5, 6, 7, 8]);

        // Mix interior mutability types and newtype wrappers.
        let cell = || Cell::new(123);
        assert_eq!(Arg(cell()).into_sendable(), Arg(123));
        assert_eq!(Arg(cell()).to_sendable(), Arg(123));
        assert_eq!(Arg(cell()).temp_into_sendable(), Arg(123));
        assert_eq!(Arg(cell()).temp_to_sendable(), Arg(123));

        // Stdlib interior mutability types.
        let ref_cell = || RefCell::new(123);
        assert_eq!(ref_cell().into_sendable(), 123);
        assert_eq!(ref_cell().temp_into_sendable(), 123);
        assert_eq!(ref_cell().borrow().into_sendable(), 123);
        assert_eq!(ref_cell().borrow().to_sendable(), 123);
        assert_eq!(ref_cell().borrow().temp_into_sendable(), 123);
        assert_eq!(ref_cell().borrow().temp_to_sendable(), 123);
        assert_eq!(ref_cell().borrow_mut().into_sendable(), 123);
        assert_eq!(ref_cell().borrow_mut().to_sendable(), 123);
        assert_eq!(ref_cell().borrow_mut().temp_into_sendable(), 123);
        assert_eq!(ref_cell().borrow_mut().temp_to_sendable(), 123);

        let cell = || Cell::new(123);
        assert_eq!(cell().into_sendable(), 123);
        assert_eq!(cell().to_sendable(), 123);
        assert_eq!(cell().temp_into_sendable(), 123);
        assert_eq!(cell().temp_to_sendable(), 123);

        let once_cell = || OnceCell::<i32>::new();
        assert_eq!(once_cell().into_sendable(), None);
        assert_eq!(once_cell().to_sendable(), None);
        assert_eq!(once_cell().temp_into_sendable(), None);
        assert_eq!(once_cell().temp_to_sendable(), None);

        let once_cell = || {
            let once_cell = OnceCell::new();
            assert!(once_cell.set(123).is_ok());
            once_cell
        };
        assert_eq!(once_cell().into_sendable(), Some(123));
        assert_eq!(once_cell().to_sendable(), Some(123));
        assert_eq!(once_cell().temp_into_sendable(), Some(123));
        assert_eq!(once_cell().temp_to_sendable(), Some(123));

        let lazy_cell = || LazyCell::new(|| 123);
        assert_eq!(lazy_cell().into_sendable(), 123);
        assert_eq!(lazy_cell().to_sendable(), 123);
        assert_eq!(lazy_cell().temp_into_sendable(), 123);
        assert_eq!(lazy_cell().temp_to_sendable(), 123);
    }

    #[test]
    fn test_into_sendable_core_unsized() {
        assert_eq!("abc".into_sendable(), "abc");
        assert_eq!("abc".to_sendable(), "abc");

        assert_eq!(c"abc".into_sendable(), c"abc");
        assert_eq!(c"abc".to_sendable(), c"abc");

        assert_eq!([1, 2, 3][..].into_sendable(), &[1, 2, 3][..]);
        assert_eq!([1, 2, 3][..].to_sendable(), &[1, 2, 3][..]);
    }

    #[expect(clippy::unsafe_removed_from_name, reason = "okay in tests")]
    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_into_sendable_core_pointers() {
        use UnsafeSendablePtr as Ptr;

        let value = 123;
        let value_ptr = &raw const value;
        assert_eq!(value_ptr.into_sendable(), Ptr::new(value_ptr));
        assert_eq!(value_ptr.to_sendable(), Ptr::new(value_ptr));
        assert_eq!(value_ptr.temp_into_sendable(), Ptr::new(value_ptr));
        assert_eq!(value_ptr.temp_to_sendable(), Ptr::new(value_ptr));

        let mut value = 123;
        let value_ptr = &raw mut value;
        assert_eq!(value_ptr.into_sendable(), Ptr::new(value_ptr));
        assert_eq!(value_ptr.to_sendable(), Ptr::new(value_ptr));
        assert_eq!(value_ptr.temp_into_sendable(), Ptr::new(value_ptr));
        assert_eq!(value_ptr.temp_to_sendable(), Ptr::new(value_ptr));

        let mut value = 123;
        let value_ptr = NonNull::new(&raw mut value).unwrap();
        assert_eq!(value_ptr.into_sendable(), Ptr::new(value_ptr));
        assert_eq!(value_ptr.to_sendable(), Ptr::new(value_ptr));
        assert_eq!(value_ptr.temp_into_sendable(), Ptr::new(value_ptr));
        assert_eq!(value_ptr.temp_to_sendable(), Ptr::new(value_ptr));
    }

    #[test]
    fn test_into_sendable_core_tuples() {
        check_copiable_eq(());
        check_copiable_eq((1,));
        check_copiable_eq((1, 2));
        check_copiable_eq((1, 2, 3));
        check_copiable_eq((1, 2, 3, 4));
        check_copiable_eq((1, 2, 3, 4, 5));
        check_copiable_eq((1, 2, 3, 4, 5, 6));
        check_copiable_eq((1, 2, 3, 4, 5, 6, 7));
        check_copiable_eq((1, 2, 3, 4, 5, 6, 7, 8));
        check_copiable_eq((1, 2, 3, 4, 5, 6, 7, 8, 9));
        check_copiable_eq((1, 2, 3, 4, 5, 6, 7, 8, 9, 10));
        check_copiable_eq((1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11));
        check_copiable_eq((1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12));

        assert_eq!((1..2, 3..=4).into_sendable(), (1..2, 3..=4));
        assert_eq!((1..2, 3..=4).to_sendable(), (&(1..2), &(3..=4)));
        assert_eq!((1..2, 3..=4).temp_into_sendable(), (1..2, 3..=4));
        assert_eq!((1..2, 3..=4).temp_to_sendable(), (1..2, 3..=4));
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn test_into_sendable_alloc_basic() {
        use alloc::borrow::Cow;
        use alloc::boxed::Box;
        use alloc::ffi::CString;
        use alloc::rc::Rc;
        use alloc::string::String;
        use alloc::sync::{self, Arc};
        use alloc::vec::Vec;

        let temp_string = String::from("abc");
        let temp_str = temp_string.as_str();
        assert_eq!(temp_str.temp_into_sendable(), "abc");
        assert_eq!(temp_str.temp_to_sendable(), "abc");

        let temp_c_string = CString::from(c"abc");
        let temp_c_str = temp_c_string.as_c_str();
        assert_eq!(temp_c_str.temp_into_sendable(), c"abc");
        assert_eq!(temp_c_str.temp_to_sendable(), c"abc");

        let temp_vec = Vec::from([1, 2, 3]);
        let temp_slice = temp_vec.as_slice();
        assert_eq!(temp_slice.temp_into_sendable(), [1, 2, 3]);
        assert_eq!(temp_slice.temp_to_sendable(), [1, 2, 3]);

        assert_eq!(String::from("abc").into_sendable(), "abc");
        #[cfg(not(miri))]
        assert_eq!(leak(String::from("abc")).to_sendable(), "abc");
        assert_eq!(String::from("abc").temp_into_sendable(), "abc");
        assert_eq!(String::from("abc").temp_to_sendable(), "abc");

        assert_eq!(CString::from(c"abc").into_sendable(), c"abc");
        #[cfg(not(miri))]
        assert_eq!(leak(CString::from(c"abc")).to_sendable(), c"abc");
        assert_eq!(CString::from(c"abc").temp_into_sendable(), c"abc");
        assert_eq!(CString::from(c"abc").temp_to_sendable(), c"abc");

        assert_eq!(Vec::from([1, 2, 3]).into_sendable(), [1, 2, 3]);
        #[cfg(not(miri))]
        assert_eq!(leak(Vec::from([1, 2, 3])).to_sendable(), &[1, 2, 3]);
        assert_eq!(Vec::from([1, 2, 3]).temp_into_sendable(), [1, 2, 3]);
        assert_eq!(Vec::from([1, 2, 3]).temp_to_sendable(), [1, 2, 3]);

        assert_eq!(Box::new(123).into_sendable(), Box::new(123));
        #[cfg(not(miri))]
        assert_eq!(**leak(Box::new(123)).to_sendable(), 123);
        assert_eq!(Box::new(123).temp_into_sendable(), 123);
        assert_eq!(Box::new(123).temp_to_sendable(), 123);

        let rc = || Rc::new(123);
        assert_eq!(rc().into_sendable(), 123);
        #[cfg(not(miri))]
        assert_eq!(leak(rc()).to_sendable(), 123);
        assert_eq!(rc().temp_into_sendable(), 123);
        assert_eq!(rc().temp_to_sendable(), 123);

        let weak = || Rc::downgrade(&Rc::new(123));
        let _unused: Option<i32> = weak().into_sendable();
        let _unused: Option<i32> = weak().to_sendable();
        let _unused: Option<i32> = weak().temp_into_sendable();
        let _unused: Option<i32> = weak().temp_to_sendable();

        let arc = || Arc::new(123);
        assert_eq!(arc().into_sendable(), arc());
        #[cfg(not(miri))]
        assert_eq!(leak(arc()).to_sendable(), &123);
        assert_eq!(arc().temp_into_sendable(), 123);
        assert_eq!(arc().temp_to_sendable(), 123);

        let weak = || Arc::downgrade(&Arc::new(123));
        let _unused: sync::Weak<i32> = weak().into_sendable();
        let _unused: sync::Weak<i32> = weak().to_sendable();
        let _unused: sync::Weak<i32> = weak().temp_into_sendable();
        let _unused: sync::Weak<i32> = weak().temp_to_sendable();

        let cow = || Cow::Borrowed("abc");
        assert_eq!(cow().into_sendable(), cow());
        #[cfg(not(miri))]
        assert_eq!(leak(cow()).into_sendable(), &cow());
        assert_eq!(cow().temp_into_sendable(), cow());
        assert_eq!(cow().temp_to_sendable(), cow());
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn test_into_sendable_alloc_collections() {
        use alloc::collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque};

        fn build<T>() -> T
        where
            T: FromIterator<(i32, i32)>,
        {
            [(1, 2), (3, 4), (5, 6)].into_iter().collect()
        }

        // `None` is the second position is used to crate a `'static` reference
        // to `Option<T>` for optimistic `ToSendable` implementation.

        let vec_deque = || build::<VecDeque<_>>();
        assert_eq!(vec_deque().into_sendable(), vec_deque());
        #[cfg(not(miri))]
        assert_eq!(leak(vec_deque()).to_sendable(), &vec_deque());
        assert_eq!(vec_deque().temp_into_sendable(), vec_deque());
        assert_eq!(vec_deque().temp_to_sendable(), vec_deque());

        let linked_list = || build::<LinkedList<_>>();
        assert_eq!(linked_list().into_sendable(), linked_list());
        #[cfg(not(miri))]
        assert_eq!(leak(linked_list()).to_sendable(), &linked_list());
        assert_eq!(linked_list().temp_into_sendable(), linked_list());
        assert_eq!(linked_list().temp_to_sendable(), linked_list());

        let binary_heap = || build::<BinaryHeap<_>>();
        assert_eq!(binary_heap().into_sendable().len(), binary_heap().len());
        #[cfg(not(miri))]
        assert_eq!(leak(binary_heap()).to_sendable().len(), binary_heap().len());
        assert_eq!(
            binary_heap().temp_into_sendable().len(),
            binary_heap().len(),
        );
        assert_eq!(binary_heap().temp_to_sendable().len(), binary_heap().len(),);

        let btree_set = || build::<BTreeSet<_>>();
        assert_eq!(btree_set().into_sendable(), btree_set());
        #[cfg(not(miri))]
        assert_eq!(leak(btree_set()).to_sendable(), &btree_set());
        assert_eq!(btree_set().temp_into_sendable(), btree_set());
        assert_eq!(btree_set().temp_to_sendable(), btree_set());

        let btree_map = || build::<BTreeMap<_, _>>();
        assert_eq!(btree_map().into_sendable(), btree_map());
        #[cfg(not(miri))]
        assert_eq!(leak(btree_map()).to_sendable(), &btree_map());
        assert_eq!(btree_map().temp_into_sendable(), btree_map());
        assert_eq!(btree_map().temp_to_sendable(), btree_map());
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_into_sendable_std_basic() {
        use alloc::vec::Vec;
        use std::{backtrace, io, process, thread};

        use crate::CloneableStdIoError;

        let instant = std::time::Instant::now();
        check_copiable_eq(instant);

        #[cfg(not(miri))]
        {
            let now = std::time::SystemTime::now();
            check_copiable_eq(now);
        }

        check_copiable_eq(io::ErrorKind::NotFound);
        check_copiable_eq(thread::current().id());
        check_copiable_eq(io::SeekFrom::Start(123));
        check_copiable_eq(process::ExitCode::SUCCESS);

        check_lazy_cloneable_eq(&io::Cursor::new("abc"));
        check_non_cloneable(backtrace::Backtrace::capture, |_: &backtrace::Backtrace| {
            true
        });

        let io_slice = io::IoSlice::new(&b"abc"[..]);
        let _unused: io::IoSlice<'static> = io_slice.into_sendable();
        let _unused: io::IoSlice<'static> = io_slice.to_sendable();
        let _unused: Vec<u8> = io_slice.temp_into_sendable();
        let _unused: Vec<u8> = io_slice.temp_to_sendable();
        assert_eq!(io_slice.temp_into_sendable(), b"abc");
        assert_eq!(io_slice.temp_to_sendable(), b"abc");

        let mut data = *b"abc";
        let io_slice = io::IoSliceMut::new(&mut data[..]);
        let _unused: io::IoSliceMut<'static> = io::IoSliceMut::new(&mut []).into_sendable();
        let _unused: Vec<u8> = io_slice.to_sendable();
        let _unused: Vec<u8> = io_slice.temp_into_sendable();
        let io_slice = io::IoSliceMut::new(&mut data[..]);
        let _unused: Vec<u8> = io_slice.temp_to_sendable();
        assert_eq!(io_slice.to_sendable(), b"abc");
        assert_eq!(io_slice.temp_into_sendable(), b"abc");
        let io_slice = io::IoSliceMut::new(&mut data[..]);
        assert_eq!(io_slice.temp_to_sendable(), b"abc");

        let err = || io::Error::from(io::ErrorKind::NotFound);
        let _unused: io::Error = err().into_sendable();
        #[cfg(not(miri))]
        let _unused: &'static io::Error = leak(err()).to_sendable();
        let _unused: io::Error = err().temp_into_sendable();
        let _unused: CloneableStdIoError = err().temp_to_sendable();
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_into_sendable_std_wrappers() {
        use std::sync::{LazyLock, OnceLock};

        let once_cell = || OnceLock::<i32>::new();
        assert_eq!(once_cell().into_sendable(), None);
        assert_eq!(once_cell().to_sendable(), None);
        assert_eq!(once_cell().temp_into_sendable(), None);
        assert_eq!(once_cell().temp_to_sendable(), None);

        let once_cell = || {
            let once_cell = OnceLock::new();
            assert!(once_cell.set(123).is_ok());
            once_cell
        };
        assert_eq!(once_cell().into_sendable(), Some(123));
        assert_eq!(once_cell().to_sendable(), Some(123));
        assert_eq!(once_cell().temp_into_sendable(), Some(123));
        assert_eq!(once_cell().temp_to_sendable(), Some(123));

        let lazy_cell = || LazyLock::new(|| 123);
        assert_eq!(lazy_cell().into_sendable(), 123);
        assert_eq!(lazy_cell().to_sendable(), 123);
        assert_eq!(lazy_cell().temp_into_sendable(), 123);
        assert_eq!(lazy_cell().temp_to_sendable(), 123);
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_into_sendable_std_unsized() {
        use std::ffi::{OsStr, OsString};
        use std::path::{Path, PathBuf};

        let os_str = OsStr::new("abc");
        let _unused: &OsStr = os_str.into_sendable();
        let _unused: &OsStr = os_str.to_sendable();
        let _unused: OsString = os_str.temp_into_sendable();
        let _unused: OsString = os_str.temp_to_sendable();
        assert_eq!(os_str.into_sendable(), "abc");
        assert_eq!(os_str.to_sendable(), "abc");
        assert_eq!(os_str.temp_into_sendable(), "abc");
        assert_eq!(os_str.temp_to_sendable(), "abc");

        let path = Path::new("abc");
        let _unused: &Path = path.into_sendable();
        let _unused: &Path = path.to_sendable();
        let _unused: PathBuf = path.temp_into_sendable();
        let _unused: PathBuf = path.temp_to_sendable();
        assert_eq!(path.into_sendable(), "abc");
        assert_eq!(path.to_sendable(), "abc");
        assert_eq!(path.temp_into_sendable(), Path::new("abc"));
        assert_eq!(path.temp_to_sendable(), Path::new("abc"));
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_into_sendable_std_collections() {
        use std::collections::{HashMap, HashSet};

        fn build<T>() -> T
        where
            T: FromIterator<(i32, i32)>,
        {
            [(1, 2), (3, 4), (5, 6)].into_iter().collect()
        }

        // `None` is the second position is used to crate a `'static` reference
        // to `Option<T>` for optimistic `ToSendable` implementation.

        let hash_set = || build::<HashSet<_>>();
        assert_eq!(hash_set().into_sendable(), hash_set());
        #[cfg(not(miri))]
        assert_eq!(leak(hash_set()).to_sendable(), &hash_set());
        assert_eq!(hash_set().temp_into_sendable(), hash_set());
        assert_eq!(hash_set().temp_to_sendable(), hash_set());

        let hash_map = || build::<HashMap<_, _>>();
        assert_eq!(hash_map().into_sendable(), hash_map());
        #[cfg(not(miri))]
        assert_eq!(leak(hash_map()).to_sendable(), &hash_map());
        assert_eq!(hash_map().temp_into_sendable(), hash_map());
        assert_eq!(hash_map().temp_to_sendable(), hash_map());
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[cfg(feature = "std")]
    #[test]
    fn test_into_sendable_std_guards() {
        use std::sync::{Mutex, RwLock};

        let mutex = Mutex::new(123);
        let guard = || mutex.lock().unwrap();
        assert_eq!(guard().into_sendable(), 123);
        assert_eq!(guard().to_sendable(), 123);
        assert_eq!(guard().temp_into_sendable(), 123);
        assert_eq!(guard().temp_to_sendable(), 123);

        let mutex = RwLock::new(123);
        let guard = || mutex.read().unwrap();
        assert_eq!(guard().into_sendable(), 123);
        assert_eq!(guard().to_sendable(), 123);
        assert_eq!(guard().temp_into_sendable(), 123);
        assert_eq!(guard().temp_to_sendable(), 123);

        let mutex = RwLock::new(123);
        let guard = || mutex.write().unwrap();
        assert_eq!(guard().into_sendable(), 123);
        assert_eq!(guard().to_sendable(), 123);
        assert_eq!(guard().temp_into_sendable(), 123);
        assert_eq!(guard().temp_to_sendable(), 123);
    }
}
