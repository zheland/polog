//! Utilities module.

use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};

/// Blanket alias for `Display + Debug`.
///
/// Used by [`ExternalArgValue::Display`] to name the bound for values that
/// implement both traits.
/// Implementing it is automatic: this trait has no methods and no required
/// impls.
///
/// [`ExternalArgValue::Display`]: crate::ExternalArgValue::Display
pub trait DisplayAndDebug: fmt::Display + fmt::Debug {}
impl<T> DisplayAndDebug for T where T: ?Sized + fmt::Display + fmt::Debug {}

/// A `&str` that may be `'static` or may be tied to a shorter lifetime.
#[derive(Copy, Clone, Debug)]
pub enum MaybeStaticStr<'a> {
    /// A `'static` string.
    Static(&'static str),

    /// A string borrowed for the lifetime `'a`.
    Borrowed(&'a str),
}

impl<'a> MaybeStaticStr<'a> {
    /// Returns the string, regardless of whether it is `'static`.
    ///
    /// The returned reference carries the `'a` lifetime of the wrapper, not
    /// the `'static` lifetime of the [`Static`](Self::Static) variant.
    /// Use [`try_as_static_str`](Self::try_as_static_str) to preserve `'static`
    /// when the value is known to be static.
    #[must_use]
    #[inline]
    pub const fn as_str(&self) -> &'a str {
        #[expect(clippy::match_same_arms, reason = "aliasing")]
        match *self {
            Self::Static(value) => value,
            Self::Borrowed(value) => value,
        }
    }

    /// Returns the string as `&'static str` if the variant is
    /// [`Static`](Self::Static), or `None` if it is
    /// [`Borrowed`](Self::Borrowed).
    ///
    /// Used by owning conversions to avoid copying the contents of a static
    /// string when storing an owned form.
    #[must_use]
    #[inline]
    pub const fn try_as_static_str(&self) -> Option<&'static str> {
        match *self {
            Self::Static(value) => Some(value),
            Self::Borrowed(_) => None,
        }
    }
}

impl Eq for MaybeStaticStr<'_> {}

impl PartialEq for MaybeStaticStr<'_> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Ord for MaybeStaticStr<'_> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl PartialOrd for MaybeStaticStr<'_> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for MaybeStaticStr<'_> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl fmt::Display for MaybeStaticStr<'_> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Write::write_str(f, self.as_str())
    }
}
