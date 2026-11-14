use core::error::Error;
use core::fmt::{self, Display};
// Hack to prevent `clippy::std_instead_of_core` failure.
// `std::io::ErrorKind` moved to `core` in nightly.
// `clippy::std_instead_of_core` works only if disabled on crate level.
// See: https://github.com/rust-lang/rust/issues/154046.
use std as core_or_std;
use std::string::ToString;

/// A cloneable representation of [`std::io::Error`].
///
/// This type captures enough information to reconstruct an equivalent
/// [`std::io::Error`] while implementing [`Clone`].
/// It is useful when an error must be stored or shared in contexts that require
/// `Clone`.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum CloneableStdIoError {
    /// An OS error with the given raw OS error code.
    ///
    /// Reconstructed with [`std::io::Error::from_raw_os_error`].
    ///
    /// [`std::io::Error::from_raw_os_error`]: https://doc.rust-lang.org/nightly/std/io/struct.Error.html#method.from_raw_os_error
    Os(i32),

    /// A simple I/O error identified only by its [`std::io::ErrorKind`].
    ///
    /// Reconstructed as a simple [`std::io::Error`].
    Simple(core_or_std::io::ErrorKind),

    /// An I/O error with a custom payload.
    ///
    /// Stores the [`std::io::ErrorKind`] and the original error's string
    /// representation.
    /// Reconstructed with [`std::io::Error::new`].
    ///
    /// [`std::io::Error::new`]: https://doc.rust-lang.org/nightly/std/io/struct.Error.html#method.new
    Stringified(core_or_std::io::ErrorKind, alloc::string::String),
}

impl CloneableStdIoError {
    /// Creates a cloneable representation from a borrowed [`std::io::Error`].
    ///
    /// If the error has a raw OS error code, stores that code.
    /// Otherwise, if it has no inner error, stores only its kind.
    /// Otherwise, stores its kind and its string representation.
    #[expect(clippy::option_if_let_else, reason = "clearer and more readable")]
    #[inline]
    #[must_use]
    pub fn from_ref(error: &std::io::Error) -> Self {
        if let Some(code) = error.raw_os_error() {
            Self::Os(code)
        } else if error.get_ref().is_none() {
            Self::Simple(error.kind())
        } else {
            Self::Stringified(error.kind(), error.to_string())
        }
    }

    /// Returns the [`std::io::ErrorKind`] of the original error.
    ///
    /// For OS errors, the kind is obtained by reconstructing a temporary
    /// [`std::io::Error`].
    #[inline]
    #[must_use]
    pub fn kind(&self) -> core_or_std::io::ErrorKind {
        match self {
            Self::Os(code) => std::io::Error::from_raw_os_error(*code).kind(),
            Self::Simple(kind) | Self::Stringified(kind, _) => *kind,
        }
    }

    /// Returns the stored string representation, if this error was stringified.
    ///
    /// Returns [`None`] for [`Self::Os`] and [`Self::Simple`].
    #[inline]
    #[must_use]
    pub fn try_as_str(&self) -> Option<&str> {
        match self {
            Self::Os(_) | Self::Simple(_) => None,
            Self::Stringified(_, display) => Some(display),
        }
    }

    /// Reconstructs a [`std::io::Error`] from this value.
    ///
    /// Any stored string payload is cloned.
    #[inline]
    #[must_use]
    pub fn to_std_io_error(&self) -> std::io::Error {
        match self {
            Self::Os(code) => std::io::Error::from_raw_os_error(*code),
            Self::Simple(error_kind) => std::io::Error::from(*error_kind),
            Self::Stringified(error_kind, display) => {
                std::io::Error::new(*error_kind, display.clone())
            }
        }
    }

    /// Converts this value into a [`std::io::Error`].
    ///
    /// Any stored string payload is consumed.
    #[inline]
    #[must_use]
    pub fn into_std_io_error(self) -> std::io::Error {
        match self {
            Self::Os(code) => std::io::Error::from_raw_os_error(code),
            Self::Simple(error_kind) => std::io::Error::from(error_kind),
            Self::Stringified(error_kind, display) => std::io::Error::new(error_kind, display),
        }
    }
}

impl Display for CloneableStdIoError {
    #[expect(clippy::missing_inline_in_public_items, reason = "bloats binary size")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Os(_) | Self::Simple(_) => write!(f, "{}", self.to_std_io_error()),
            Self::Stringified(_, display) => display.fmt(f),
        }
    }
}

impl Error for CloneableStdIoError {}

impl From<!> for CloneableStdIoError {
    #[inline]
    fn from(value: !) -> Self {
        match value {}
    }
}

impl From<std::io::Error> for CloneableStdIoError {
    #[inline]
    fn from(error: std::io::Error) -> Self {
        Self::from_ref(&error)
    }
}

impl From<&std::io::Error> for CloneableStdIoError {
    #[inline]
    fn from(error: &std::io::Error) -> Self {
        Self::from_ref(error)
    }
}

impl From<CloneableStdIoError> for std::io::Error {
    #[inline]
    fn from(error: CloneableStdIoError) -> Self {
        error.into_std_io_error()
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;
    use std::io;

    use crate::CloneableStdIoError;

    #[test]
    fn test_cloneable_os_error() {
        let error1 = io::Error::from_raw_os_error(110);
        let error2 = CloneableStdIoError::from_ref(&error1);
        let error3 = CloneableStdIoError::from(&error1);
        let error4 = CloneableStdIoError::from(error1);
        assert_eq!(error2, CloneableStdIoError::Os(110));
        assert_eq!(error3, error2);
        assert_eq!(error4, error4);
        assert_eq!(error2.clone(), error2);
        assert_eq!(error3.clone(), error3);
        assert_eq!(error4.clone(), error4);

        #[cfg(unix)]
        {
            #[cfg(not(miri))]
            let expected = "Connection timed out (os error 110)";
            #[cfg(miri)]
            let expected = "Connection timed out (os error 110) (os error 110)";

            assert_eq!(error2.kind(), io::ErrorKind::TimedOut);
            assert_eq!(error2.try_as_str(), None);
            assert_eq!(format!("{error2}"), expected);

            let error5 = error2.to_std_io_error();
            assert_eq!(error5.kind(), io::ErrorKind::TimedOut);
            assert_eq!(format!("{error5}"), expected);

            let error6 = error2.clone().into_std_io_error();
            assert_eq!(error6.kind(), io::ErrorKind::TimedOut);
            assert_eq!(format!("{error6}"), expected);

            let error7 = io::Error::from(error2);
            assert_eq!(error7.kind(), io::ErrorKind::TimedOut);
            assert_eq!(format!("{error7}"), expected);
        }
    }

    #[test]
    fn test_cloneable_simple_error() {
        let error1 = io::Error::from(io::ErrorKind::AddrInUse);
        let error2 = CloneableStdIoError::from_ref(&error1);
        let error3 = CloneableStdIoError::from(&error1);
        let error4 = CloneableStdIoError::from(error1);
        assert_eq!(
            error2,
            CloneableStdIoError::Simple(io::ErrorKind::AddrInUse)
        );
        assert_eq!(error3, error2);
        assert_eq!(error4, error4);
        assert_eq!(error2.clone(), error2);
        assert_eq!(error3.clone(), error3);
        assert_eq!(error4.clone(), error4);

        assert_eq!(error2.kind(), io::ErrorKind::AddrInUse);
        assert_eq!(error2.try_as_str(), None);
        assert_eq!(format!("{error2}"), "address in use");

        let error5 = error2.to_std_io_error();
        assert_eq!(error5.kind(), io::ErrorKind::AddrInUse);
        assert_eq!(format!("{error5}"), "address in use");

        let error6 = error2.clone().into_std_io_error();
        assert_eq!(error6.kind(), io::ErrorKind::AddrInUse);
        assert_eq!(format!("{error6}"), "address in use");

        let error7 = io::Error::from(error2);
        assert_eq!(error7.kind(), io::ErrorKind::AddrInUse);
        assert_eq!(format!("{error7}"), "address in use");
    }

    #[test]
    fn test_cloneable_stringified_error() {
        let error1 = io::Error::other("Other error message");
        let error2 = CloneableStdIoError::from_ref(&error1);
        let error3 = CloneableStdIoError::from(&error1);
        let error4 = CloneableStdIoError::from(error1);
        assert_eq!(error3, error2);
        assert_eq!(error4, error4);
        assert_eq!(error2.clone(), error2);
        assert_eq!(error3.clone(), error3);
        assert_eq!(error4.clone(), error4);

        assert_eq!(error2.kind(), io::ErrorKind::Other);
        assert_eq!(error2.try_as_str(), Some("Other error message"));
        assert_eq!(format!("{error2}"), "Other error message");

        let error5 = error2.to_std_io_error();
        assert_eq!(error5.kind(), io::ErrorKind::Other);
        assert_eq!(format!("{error5}"), "Other error message");

        let error6 = error2.clone().into_std_io_error();
        assert_eq!(error6.kind(), io::ErrorKind::Other);
        assert_eq!(format!("{error6}"), "Other error message");

        let error7 = io::Error::from(error2);
        assert_eq!(error7.kind(), io::ErrorKind::Other);
        assert_eq!(format!("{error7}"), "Other error message");
    }
}
