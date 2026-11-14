use core::fmt::{self, Debug, Display, Formatter};

/// A formatting wrapper that renders `T` using its [`Display`] implementation.
///
/// Used by the `%` format modifier.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default)]
pub struct ViaDisplay<T: ?Sized>(
    /// The wrapped value.
    pub T,
);

/// A formatting wrapper that renders `T` using its [`Debug`] implementation.
///
/// Used by the `?` format modifier.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default)]
pub struct ViaDebug<T: ?Sized>(
    /// The wrapped value.
    pub T,
);

/// A formatting wrapper that renders `T` using alternative [`Display`]
/// formatting (`{:#}`).
///
/// Used by the `#%` format modifier.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default)]
pub struct ViaAltDisplay<T: ?Sized>(
    /// The wrapped value.
    pub T,
);

/// A formatting wrapper that renders `T` using alternative [`Debug`] formatting
/// (`{:#?}`).
///
/// Used by the `#?` format modifier.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default)]
pub struct ViaAltDebug<T: ?Sized>(
    /// The wrapped value.
    pub T,
);

macro_rules! impl_fmt {
    (
        ( $($params:tt)* ) where ( $($bounds:tt)* ), $ty:ty,
        (&$self:tt, $fmt:tt) => $expr:expr $(,)?
    ) => {
        impl<$($params)*> Display for $ty
        where
            $($bounds)*
        {
            #[inline]
            fn fmt(&$self, $fmt: &mut Formatter<'_>) -> fmt::Result {
                $expr
            }
        }

        impl<$($params)*> Debug for $ty
        where
            $($bounds)*
        {
            #[inline]
            fn fmt(&$self, $fmt: &mut Formatter<'_>) -> fmt::Result {
                $expr
            }
        }
    }
}

impl_fmt!(
    (T) where ( T: ?Sized + Display ),
    ViaDisplay<T>, (&self, f) => Display::fmt(&self.0, f)
);

impl_fmt!(
    (T) where ( T: ?Sized + Debug ),
    ViaDebug<T>, (&self, f) => Debug::fmt(&self.0, f)
);

impl_fmt!(
    (T) where ( T: ?Sized + Display ),
    ViaAltDisplay<T>, (&self, f) => write!(f, "{:#}", &self.0)
);

impl_fmt!(
    (T) where ( T: ?Sized + Debug ),
    ViaAltDebug<T>, (&self, f) => write!(f, "{:#?}", &self.0)
);

#[cfg(test)]
mod tests {
    use alloc::format;
    use core::fmt;

    use crate::{ViaAltDebug, ViaAltDisplay, ViaDebug, ViaDisplay};

    #[test]
    fn test_fmt_wrappers() {
        struct Displayable;

        impl fmt::Display for Displayable {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let kind = if f.alternate() {
                    "alternative"
                } else {
                    "default"
                };
                write!(f, "{kind}")
            }
        }

        assert_eq!(format!("{}", ViaDisplay("abc")), "abc");
        assert_eq!(format!("{:?}", ViaDisplay("abc")), "abc");
        assert_eq!(format!("{:#}", ViaDisplay("abc")), "abc");
        assert_eq!(format!("{:#?}", ViaDisplay("abc")), "abc");
        assert_eq!(format!("{}", ViaDisplay(Displayable)), "default");
        assert_eq!(format!("{:?}", ViaDisplay(Displayable)), "default");
        assert_eq!(format!("{:#}", ViaDisplay(Displayable)), "alternative");
        assert_eq!(format!("{:#?}", ViaDisplay(Displayable)), "alternative");

        assert_eq!(format!("{}", ViaDebug("abc")), "\"abc\"");
        assert_eq!(format!("{:?}", ViaDebug("abc")), "\"abc\"");
        assert_eq!(format!("{:#}", ViaDebug("abc")), "\"abc\"");
        assert_eq!(format!("{:#?}", ViaDebug("abc")), "\"abc\"");
        assert_eq!(format!("{}", ViaDebug((1,))), "(1,)");
        assert_eq!(format!("{:?}", ViaDebug((1,))), "(1,)");
        assert_eq!(format!("{:#}", ViaDebug((1,))), "(\n    1,\n)");
        assert_eq!(format!("{:#?}", ViaDebug((1,))), "(\n    1,\n)");

        assert_eq!(format!("{}", ViaAltDisplay("abc")), "abc");
        assert_eq!(format!("{:?}", ViaAltDisplay("abc")), "abc");
        assert_eq!(format!("{:#}", ViaAltDisplay("abc")), "abc");
        assert_eq!(format!("{:#?}", ViaAltDisplay("abc")), "abc");
        assert_eq!(format!("{}", ViaAltDisplay(Displayable)), "alternative");
        assert_eq!(format!("{:?}", ViaAltDisplay(Displayable)), "alternative");
        assert_eq!(format!("{:#}", ViaAltDisplay(Displayable)), "alternative");
        assert_eq!(format!("{:#?}", ViaAltDisplay(Displayable)), "alternative");

        assert_eq!(format!("{}", ViaAltDebug("abc")), "\"abc\"");
        assert_eq!(format!("{:?}", ViaAltDebug("abc")), "\"abc\"");
        assert_eq!(format!("{:#}", ViaAltDebug("abc")), "\"abc\"");
        assert_eq!(format!("{:#?}", ViaAltDebug("abc")), "\"abc\"");
        assert_eq!(format!("{}", ViaAltDebug((1,))), "(\n    1,\n)");
        assert_eq!(format!("{:?}", ViaAltDebug((1,))), "(\n    1,\n)");
        assert_eq!(format!("{:#}", ViaAltDebug((1,))), "(\n    1,\n)");
        assert_eq!(format!("{:#?}", ViaAltDebug((1,))), "(\n    1,\n)");
    }
}
