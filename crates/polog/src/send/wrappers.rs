use core::fmt::{self, Debug, Display, Formatter};

use crate::{__impl_sendable, TemporaryIntoSendable, TemporaryToSendable};

/// A wrapper that marks a value as already `'static + Send`.
///
/// This corresponds to the `:a` send modifier.
/// It is a zero-cost wrapper for values that the logger can send off-thread
/// without conversion.
/// Loggers ignore the wrapper and format the inner value directly.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct AlreadySendable<T: ?Sized>(pub T);

/// A wrapper that marks a value as temporary and converts it into a
/// `'static + Send` representation when required by an off-thread logger.
///
/// This corresponds to the `:t` send modifier.
/// Same-thread loggers ignore the wrapper and format the inner value directly.
/// Off-thread loggers use [`TemporaryIntoSendable`] or [`TemporaryToSendable`]
/// to obtain an owned, sendable value, typically by cloning, collecting, or
/// otherwise converting borrowed data.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Temporary<T: ?Sized>(pub T);

/// A wrapper that marks a value to be stringified before sending to an
/// off-thread logger.
///
/// This corresponds to the `:s` send modifier.
/// Same-thread loggers ignore the wrapper and format the inner value directly.
/// Off-thread loggers convert the value into a `String` using
/// `ToString::to_string`.
/// Formatting modifiers such as `%`, `?`, `#%`, and `#?` control whether the
/// stringification uses [`Display`] or [`Debug`] formatting.
#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Stringifiable<T: ?Sized>(pub T);

/// A wrapper that pairs a value with a conversion function used to produce its
/// `'static + Send` representation for off-thread loggers.
///
/// This is the type returned by [`DecorateSendable::sendable_with`].
/// Same-thread loggers ignore the conversion and format [`value`] directly.
/// Off-thread loggers call [`convert_fn`] once per event and wrap the result in
/// [`AlreadySendable`] before sending it.
///
/// # Conversion-function shape
///
/// `SendableWith<T, F>` implements all four sendable-conversion traits, but the
/// bound on `F` differs between the by-value and by-reference forms:
/// - [`IntoSendable`] and [`TemporaryIntoSendable`] (by value): `F: FnOnce(T)
///   -> U`.
/// - [`ToSendable<'a>`] and [`TemporaryToSendable<'a>`] (by reference): `F:
///   Fn(&'a T) -> U`.
///
/// Which form is used follows from how the `SendableWith` reaches the
/// conversion, not from the macro:
/// - `value.sendable_with(f)` requires `FnOnce(T) -> U`,
/// - `&value.sendable_with(f)` requires `Fn(&T) -> U`.
///
/// [`value`]: Self::value
/// [`IntoSendable`]: crate::IntoSendable
/// [`TemporaryIntoSendable`]: crate::TemporaryIntoSendable
/// [`ToSendable<'a>`]: crate::ToSendable
/// [`TemporaryToSendable<'a>`]: crate::TemporaryToSendable
///
/// [`convert_fn`]: Self::convert_fn

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SendableWith<T, F> {
    /// The original value to be converted.
    pub value: T,

    /// The conversion function applied to [`value`] for off-thread loggers.
    ///
    /// [`value`]: Self::value
    pub convert_fn: F,
}

/// Extension methods for wrapping values with send behavior modifiers.
///
/// The methods correspond to the send modifiers accepted by logging macros:
/// `:a` (`already_sendable`), `:t` (`temporary`), `:s` (`stringifiable`), and
/// custom conversion via `sendable_with`.
pub trait DecorateSendable: Sized {
    /// Marks the value as already `'static + Send`.
    ///
    /// Equivalent to the `:a` send modifier.
    /// Use this when the value can be sent to an off-thread logger without
    /// conversion.
    #[inline]
    fn already_sendable(self) -> AlreadySendable<Self> {
        AlreadySendable(self)
    }

    /// Marks the value as temporary and converts it when required by an
    /// off-thread logger.
    ///
    /// Equivalent to the `:t` send modifier.
    /// The conversion uses [`TemporaryIntoSendable`].
    #[inline]
    fn temporary(self) -> Temporary<Self> {
        Temporary(self)
    }

    /// Marks the value to be stringified when required by an off-thread logger.
    ///
    /// Equivalent to the `:s` send modifier.
    /// The value is converted to a `String` using `ToString::to_string`.
    #[cfg(feature = "alloc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
    #[inline]
    fn stringifiable(self) -> Stringifiable<Self> {
        Stringifiable(self)
    }

    /// Marks the value to be converted with a custom function for off-thread
    /// loggers.
    ///
    /// The function is invoked only by off-thread loggers, and only once per
    /// event.
    /// Same-thread loggers format [`value`] directly and never call `f`.
    ///
    /// The bound required from `F` follows from how the `SendableWith` reaches
    /// the conversion:
    /// - by-value uses `FnOnce(T) -> U`,
    /// - by-reference uses `Fn(&T) -> U`.
    ///
    /// [`value`]: SendableWith::value
    #[inline]
    fn sendable_with<F>(self, f: F) -> SendableWith<Self, F> {
        SendableWith::new(self, f)
    }
}

impl<T> DecorateSendable for T {}

impl<T, F> SendableWith<T, F> {
    /// Creates a new [`SendableWith`] from a value and a conversion function.
    #[inline]
    pub const fn new(value: T, convert_fn: F) -> Self {
        Self { value, convert_fn }
    }
}

macro_rules! impl_fmt_delegate {
    ( [$($arg:tt)*] [$($bounds:tt)*], $ty:ty, $self:tt => $expr:expr ) => {
        impl<$($arg)*> Display for $ty
        where
            $($bounds)* Display
        {
            #[inline]
            fn fmt(&$self, f: &mut Formatter<'_>) -> fmt::Result {
                Display::fmt(&$expr, f)
            }
        }

        impl<$($arg)*> Debug for $ty
        where
            $($bounds)* Debug
        {
            #[inline]
            fn fmt(&$self, f: &mut Formatter<'_>) -> fmt::Result {
                Debug::fmt(&$expr, f)
            }
        }
    };
}

impl_fmt_delegate!([T] [T: ?Sized + ], AlreadySendable<T>, self => self.0);
impl_fmt_delegate!([T] [T: ?Sized + ], Temporary<T>, self => self.0);
#[cfg(feature = "alloc")]
impl_fmt_delegate!([T] [T: ?Sized + ], Stringifiable<T>, self => self.0);
impl_fmt_delegate!([T, F] [T: ], SendableWith<T, F>, self => self.value);

// ==== Implementations for `AlreadySendable<T>` ====

__impl_sendable!(
    for (T) for AlreadySendable<T> where (T: 'static),
    O |self| -> Self { self }
        where ( + Send + Sync ),
    O |&'static self| -> &'static Self { self }
        where ( + ?Sized + Sync ),
    p |self| -> Self { self }
        where ( + Send + Sync ),
    p |&'static self| -> &'static Self { self }
        where ( + ?Sized + Sync ),
);

// ==== Implementations for `Temporary<T>` ====

__impl_sendable!(
    for (T) for Temporary<T> => AlreadySendable<T::Sendable>,
    O |self| { AlreadySendable(self.0.temp_into_sendable()) }
        where ( T: TemporaryIntoSendable ),
    O for<'a> |&'a self| { AlreadySendable(self.0.temp_to_sendable()) }
        where ( T: ?Sized + TemporaryToSendable<'a> ),
    p |self| { AlreadySendable(self.0.temp_into_sendable()) }
        where ( T: TemporaryIntoSendable ),
    p for<'a> |&'a self| { AlreadySendable(self.0.temp_to_sendable()) }
        where ( T: ?Sized + TemporaryToSendable<'a> ),
);

// ==== Implementations for `SendableWith<T, F>` ====

__impl_sendable!(
    for (T, F, U) for SendableWith<T, F> => AlreadySendable<U>,
    O |self| { AlreadySendable((self.convert_fn)(self.value)) }
        where ( F: FnOnce(T) -> U, U: 'static + Send + Sync ),
    O for<'a> |&'a self| { AlreadySendable((self.convert_fn)(&self.value)) }
        where ( T: 'a, F: Fn(&'a T) -> U, U: 'static + Send + Sync ),
    p |self| { AlreadySendable((self.convert_fn)(self.value)) }
        where ( F: FnOnce(T) -> U, U: 'static + Send + Sync ),
    p for<'a> |&'a self| { AlreadySendable((self.convert_fn)(&self.value)) }
        where ( T: 'a, F: Fn(&'a T) -> U, U: 'static + Send + Sync ),
);

// ==== Alloc implementations ====

#[cfg(feature = "alloc")]
mod alloc_impls {
    use alloc::string::{String, ToString};
    use core::fmt::Display;

    use crate::{__impl_sendable, Stringifiable};

    // ==== Implementations for `Stringifiable<T>` ====

    __impl_sendable!(
        for (T) for Stringifiable<T> => String where ( T: Display ),
        O |self| { ToString::to_string(&self.0) },
        O for<'a> |&'a self| { ToString::to_string(&self.0) },
        p |self| { ToString::to_string(&self.0) },
        p for<'a> |&'a self| { ToString::to_string(&self.0) },
    );
}

#[cfg(test)]
mod tests {

    use alloc::format;
    use core::fmt;

    use crate::{
        AlreadySendable, DecorateSendable, IntoSendable, Stringifiable, Temporary,
        TemporaryIntoSendable, TemporaryToSendable, ToSendable,
    };

    #[cfg(not(miri))]
    fn leak<T>(value: T) -> &'static T {
        use alloc::boxed::Box;
        Box::leak(Box::new(value))
    }

    #[test]
    fn test_already_sendable() {
        #[derive(Eq, PartialEq, Debug)]
        struct Foo(u32, u32);

        let value = || Foo(123, 234).already_sendable();
        assert_eq!(value().into_sendable(), AlreadySendable(Foo(123, 234)));
        #[cfg(not(miri))]
        assert_eq!(leak(value()).to_sendable(), &AlreadySendable(Foo(123, 234)));
        assert_eq!(value().temp_into_sendable(), AlreadySendable(Foo(123, 234)));
        #[cfg(not(miri))]
        assert_eq!(
            leak(value()).temp_to_sendable(),
            &AlreadySendable(Foo(123, 234))
        );

        let value = || "abc".already_sendable();
        let _unused: AlreadySendable<&'static str> = value().temp_into_sendable();
        assert_eq!(value().temp_into_sendable(), AlreadySendable("abc"));

        assert_eq!(format!("{}", AlreadySendable("abc")), "abc");
        assert_eq!(format!("{:?}", AlreadySendable("abc")), "\"abc\"");
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn test_temporary() {
        use alloc::borrow::ToOwned;
        use alloc::string::String;

        let _unused: &'static str = "abc".into_sendable();
        assert_eq!("abc".into_sendable(), "abc");

        let value = || "abc".temporary();
        let _unused: AlreadySendable<String> = value().into_sendable();
        let _unused: AlreadySendable<String> = value().to_sendable();
        let _unused: AlreadySendable<String> = value().temp_into_sendable();
        let _unused: AlreadySendable<String> = value().temp_to_sendable();

        assert_eq!(value().into_sendable(), AlreadySendable("abc".to_owned()));
        assert_eq!(value().to_sendable(), AlreadySendable("abc".to_owned()));
        assert_eq!(
            value().temp_into_sendable(),
            AlreadySendable("abc".to_owned())
        );
        assert_eq!(
            value().temp_to_sendable(),
            AlreadySendable("abc".to_owned())
        );

        assert_eq!(format!("{}", Temporary("abc")), "abc");
        assert_eq!(format!("{:?}", Temporary("abc")), "\"abc\"");
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn test_stringifiable() {
        use alloc::string::String;

        let _unused: i32 = 123.into_sendable();
        assert_eq!(123.into_sendable(), 123);

        let _unused: String = 123.stringifiable().into_sendable();
        let _unused: String = 123.stringifiable().to_sendable();
        let _unused: String = 123.stringifiable().temp_into_sendable();
        let _unused: String = 123.stringifiable().temp_to_sendable();
        assert_eq!(123.stringifiable().into_sendable(), "123");
        assert_eq!(123.stringifiable().to_sendable(), "123");
        assert_eq!(123.stringifiable().temp_into_sendable(), "123");
        assert_eq!(123.stringifiable().temp_to_sendable(), "123");

        assert_eq!(format!("{}", Stringifiable("abc")), "abc");
        assert_eq!(format!("{:?}", Stringifiable("abc")), "\"abc\"");
    }

    #[test]
    fn test_sendable_with() {
        struct Foo(&'static str);

        impl fmt::Display for Foo {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "dummy display impl")
            }
        }

        impl fmt::Debug for Foo {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "dummy debug impl")
            }
        }

        let value = || Foo("abc").sendable_with(|Foo(value)| value);
        let _unused: AlreadySendable<&'static str> = value().into_sendable();
        let _unused: AlreadySendable<&'static str> = value().temp_into_sendable();
        #[cfg(not(miri))]
        {
            let value = || Foo("abc").sendable_with(|&Foo(value)| value);
            let _unused: AlreadySendable<&'static str> = leak(value()).to_sendable();
            let _unused: AlreadySendable<&'static str> = leak(value()).temp_to_sendable();
        }

        let value = || Foo("abc").sendable_with(|Foo(value)| value);
        assert_eq!(value().into_sendable(), AlreadySendable("abc"));
        assert_eq!(value().temp_into_sendable(), AlreadySendable("abc"));
        #[cfg(not(miri))]
        {
            let value = || Foo("abc").sendable_with(|&Foo(value)| value);
            assert_eq!(leak(value()).to_sendable(), AlreadySendable("abc"));
            assert_eq!(leak(value()).temp_to_sendable(), AlreadySendable("abc"));
        }

        let value = value();
        assert_eq!(format!("{value}"), "dummy display impl");
        assert_eq!(format!("{value:?}"), "dummy debug impl");
    }
}
