use core::cmp::Ordering;
use core::error::Error;
use core::fmt::{self, Display};
use core::str::FromStr;

/// Severity level for a log entry.
///
/// Severity is ordered from most severe (`Fatal`) to least severe (`Trace`).
/// The derived `Ord` is intentionally reversed:
/// `Fatal < Bug < Error < Warn < Info < Debug < Trace`, so `level <= filter`
/// selects levels at or above the filter's severity.
///
/// # Special panic-integration levels
///
/// Two levels unify logging with the `panic!` / `assert!` machinery:
/// - **`Fatal`**: an unrecoverable error. Logging at this level always panics
///   after the event is emitted. The panic path calls
///   [`BaseHandler::flush_on_panic`] first, then [`BaseHandler::panic`], so
///   buffered events — including the one that triggered the panic — are given a
///   best-effort chance to reach the sink. Use `Fatal` for conditions that must
///   never be silently swallowed.
/// - **`Bug`**: a recoverable programming error (invariant violation). Logging
///   at this level *may* panic, depending on whether
///   [`BaseHandler::should_panic_on_bug`] returns `true`. The default
///   implementation returns `cfg!(debug_assertions)`, so `bug!` panics in dev
///   builds and logs without panicking in release builds when the default
///   profile is used.
///
/// All other levels are purely informational.
/// Logging at `Error`, `Warn`, `Info`, `Debug`, or `Trace` will not cause a
/// panic on its own.
///
/// [`BaseHandler::flush_on_panic`]: crate::BaseHandler::flush_on_panic
/// [`BaseHandler::panic`]: crate::BaseHandler::panic
/// [`BaseHandler::should_panic_on_bug`]: crate::BaseHandler::should_panic_on_bug
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[repr(usize)]
pub enum Level {
    /// An unrecoverable error that halts the program immediately.
    ///
    /// `Fatal` indicates a condition so severe that continued execution is
    /// impossible or would be unsafe (e.g., corrupted state, critical resource
    /// exhaustion). After the log event is emitted the logger will
    /// unconditionally call [`panic!`], terminating the current thread or
    /// process.
    ///
    /// **Always panics.** This is the logging equivalent of a hard assertion
    /// that must never be bypassed.
    Fatal = 1,

    /// A programming error, a bug that *should* never occur.
    ///
    /// `Bug` represents an invariant violation, unexpected state, or
    /// condition that indicates incorrect code logic.
    /// It is the intended replacement for debug/release assertions that you
    /// might previously have written with `debug_assert!` or a custom
    /// `soft_assert!` macro.
    ///
    /// Whether a `Bug` event triggers a panic is controlled by the
    /// `Logger::panic_on_bug` returned value.
    /// When panicking is disabled, the event is still logged at full fidelity,
    /// so you never silently lose the diagnostic information.
    ///
    /// **May panic, depending on configuration.**
    /// Use `Bug` for "soft" assertions that you want to optionally escalate.
    Bug = 2,

    /// A serious, but non-fatal error.
    ///
    /// The current operation or request cannot be completed successfully, but
    /// the program as a whole remains in a valid state and will continue
    /// running.
    ///
    /// **Never panics.** Logging at `Error` will not terminate the program.
    Error = 3,

    /// A potentially problematic situation that is not yet an error.
    ///
    /// Use `Warn` to flag conditions that are unexpected or undesirable but the
    /// program can safely continue.
    /// These messages often warrant investigation but do not require immediate
    /// action.
    Warn = 4,

    /// Informational messages about normal, noteworthy program operation.
    ///
    /// `Info` is the default verbosity for production monitoring.
    /// It records high-level events that show the application's progress or
    /// state changes.
    /// These messages are typically always enabled.
    Info = 5,

    /// Diagnostic information useful for debugging the application.
    ///
    /// `Debug` messages reveal the program's internal flow and state
    /// to help developers understand behavior.
    /// They are usually disabled in release builds to reduce noise and
    /// performance overhead.
    Debug = 6,

    /// Extremely verbose, low-level tracing information.
    ///
    /// `Trace` is the finest-grained logging level, used for line‑by‑line
    /// execution tracing, lock acquisition/release, raw byte dumps, or
    /// intricate state‑machine transitions.
    /// Enable this only when you need to follow the exact execution path,
    /// typically during deep debugging sessions.
    Trace = 7,
}

/// Severity level for a span.
///
/// Span levels are a subset of [`Level`] excluding the panic-integrated
/// [`Level::Fatal`] and [`Level::Bug`] variants.
/// They are ordered from most severe (`Error`) to least severe (`Trace`).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[repr(usize)]
pub enum SpanLevel {
    /// A serious, but non-fatal error.
    ///
    /// The current operation or request cannot be completed successfully, but
    /// the program as a whole remains in a valid state and will continue
    /// running.
    Error = 3,

    /// A potentially problematic situation that is not yet an error.
    ///
    /// Use `Warn` to flag conditions that are unexpected or undesirable but the
    /// program can safely continue.
    Warn = 4,

    /// Informational messages about normal, noteworthy program operation.
    Info = 5,

    /// Diagnostic information useful for debugging the application.
    Debug = 6,

    /// Extremely verbose, low-level tracing information.
    Trace = 7,
}

/// A maximum verbosity filter for log events.
///
/// `LevelFilter` adds [`Off`] to the [`Level`] values.
/// Larger discriminants enable more verbose events: `Off` disables all logging,
/// `Fatal` enables only fatal events, and `Trace` enables all events.
///
/// [`Off`]: Self::Off
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[repr(usize)]
pub enum LevelFilter {
    /// Disables all logging.
    Off = 0,

    /// Enables only [`Level::Fatal`] events.
    Fatal = 1,

    /// Enables [`Level::Fatal`] and [`Level::Bug`] events.
    Bug = 2,

    /// Enables fatal, bug, and error events.
    Error = 3,

    /// Enables fatal, bug, error, and warn events.
    Warn = 4,

    /// Enables fatal, bug, error, warn, and info events.
    Info = 5,

    /// Enables all events except [`Level::Trace`].
    Debug = 6,

    /// Enables all log events.
    Trace = 7,
}

impl Level {
    /// Converts a raw `u8` into a [`Level`], returning `None` for invalid
    /// values.
    #[inline]
    #[must_use]
    pub const fn try_from_u8(level: u8) -> Option<Self> {
        match level {
            1 => Some(Self::Fatal),
            2 => Some(Self::Bug),
            3 => Some(Self::Error),
            4 => Some(Self::Warn),
            5 => Some(Self::Info),
            6 => Some(Self::Debug),
            7 => Some(Self::Trace),
            _ => None,
        }
    }

    /// Returns the raw `u8` representation of this level.
    #[inline]
    #[must_use]
    pub const fn into_u8(self) -> u8 {
        match self {
            Self::Fatal => 1,
            Self::Bug => 2,
            Self::Error => 3,
            Self::Warn => 4,
            Self::Info => 5,
            Self::Debug => 6,
            Self::Trace => 7,
        }
    }

    /// Converts a [`SpanLevel`] into a [`Level`].
    #[inline]
    #[must_use]
    pub const fn from_span_level(span_level: SpanLevel) -> Self {
        match span_level {
            SpanLevel::Error => Self::Error,
            SpanLevel::Warn => Self::Warn,
            SpanLevel::Info => Self::Info,
            SpanLevel::Debug => Self::Debug,
            SpanLevel::Trace => Self::Trace,
        }
    }

    /// Converts a [`LevelFilter`] into a [`Level`], returning `None` for `Off`.
    #[inline]
    #[must_use]
    pub const fn try_from_level_filter(filter: LevelFilter) -> Option<Self> {
        match filter {
            LevelFilter::Off => None,
            LevelFilter::Fatal => Some(Self::Fatal),
            LevelFilter::Bug => Some(Self::Bug),
            LevelFilter::Error => Some(Self::Error),
            LevelFilter::Warn => Some(Self::Warn),
            LevelFilter::Info => Some(Self::Info),
            LevelFilter::Debug => Some(Self::Debug),
            LevelFilter::Trace => Some(Self::Trace),
        }
    }

    /// Converts this level into the equivalent [`LevelFilter`].
    #[inline]
    #[must_use]
    pub const fn into_level_filter(self) -> LevelFilter {
        LevelFilter::from_level(self)
    }

    /// Returns the lowercase string representation of this level.
    #[inline]
    #[must_use]
    pub const fn as_lowercase_str(self) -> &'static str {
        self.into_level_filter().as_lowercase_str()
    }

    /// Returns the uppercase string representation of this level.
    #[inline]
    #[must_use]
    pub const fn as_uppercase_str(self) -> &'static str {
        self.into_level_filter().as_uppercase_str()
    }

    /// # Errors
    ///
    /// Returns [`ParseLevelError::UnknownLevel`] if an unknown level name is
    /// encountered.
    #[inline]
    pub const fn from_str_const(value: &str) -> Result<Self, ParseLevelError> {
        if value.eq_ignore_ascii_case("fatal") {
            Ok(Self::Fatal)
        } else if value.eq_ignore_ascii_case("bug") {
            Ok(Self::Bug)
        } else if value.eq_ignore_ascii_case("error") {
            Ok(Self::Error)
        } else if value.eq_ignore_ascii_case("warn") {
            Ok(Self::Warn)
        } else if value.eq_ignore_ascii_case("info") {
            Ok(Self::Info)
        } else if value.eq_ignore_ascii_case("debug") {
            Ok(Self::Debug)
        } else if value.eq_ignore_ascii_case("trace") {
            Ok(Self::Trace)
        } else {
            Err(ParseLevelError::UnknownLevel)
        }
    }
}

impl SpanLevel {
    /// Converts a raw `u8` into a [`SpanLevel`], returning `None` for invalid
    /// values.
    #[inline]
    #[must_use]
    pub const fn try_from_u8(level: u8) -> Option<Self> {
        match level {
            3 => Some(Self::Error),
            4 => Some(Self::Warn),
            5 => Some(Self::Info),
            6 => Some(Self::Debug),
            7 => Some(Self::Trace),
            _ => None,
        }
    }

    /// Returns the raw `u8` representation of this span level.
    #[inline]
    #[must_use]
    pub const fn into_u8(self) -> u8 {
        match self {
            Self::Error => 3,
            Self::Warn => 4,
            Self::Info => 5,
            Self::Debug => 6,
            Self::Trace => 7,
        }
    }

    /// Converts a [`Level`] into a [`SpanLevel`], returning `None` for `Fatal`
    /// and `Bug`.
    #[inline]
    #[must_use]
    pub const fn try_from_level(level: Level) -> Option<Self> {
        match level {
            Level::Fatal | Level::Bug => None,
            Level::Error => Some(Self::Error),
            Level::Warn => Some(Self::Warn),
            Level::Info => Some(Self::Info),
            Level::Debug => Some(Self::Debug),
            Level::Trace => Some(Self::Trace),
        }
    }

    /// Converts a [`LevelFilter`] into a [`Level`], returning `None` for `Off`,
    /// `Fatal` and `Bug`.
    #[inline]
    #[must_use]
    pub const fn try_from_level_filter(filter: LevelFilter) -> Option<Self> {
        match filter {
            LevelFilter::Off | LevelFilter::Fatal | LevelFilter::Bug => None,
            LevelFilter::Error => Some(Self::Error),
            LevelFilter::Warn => Some(Self::Warn),
            LevelFilter::Info => Some(Self::Info),
            LevelFilter::Debug => Some(Self::Debug),
            LevelFilter::Trace => Some(Self::Trace),
        }
    }

    /// Converts this level into the equivalent [`Level`].
    #[inline]
    #[must_use]
    pub const fn into_level(self) -> Level {
        Level::from_span_level(self)
    }

    /// Converts this level into the equivalent [`LevelFilter`]
    #[inline]
    #[must_use]
    pub const fn into_level_filter(self) -> LevelFilter {
        LevelFilter::from_span_level(self)
    }

    /// Returns the lowercase string representation of this level.
    #[inline]
    #[must_use]
    pub const fn as_lowercase_str(self) -> &'static str {
        self.into_level_filter().as_lowercase_str()
    }

    /// Returns the uppercase string representation of this level.
    #[inline]
    #[must_use]
    pub const fn as_uppercase_str(self) -> &'static str {
        self.into_level_filter().as_uppercase_str()
    }

    /// # Errors
    ///
    /// Returns [`ParseLevelError::UnknownLevel`] if an unknown level name is
    /// encountered.
    #[inline]
    pub const fn from_str_const(value: &str) -> Result<Self, ParseLevelError> {
        if value.eq_ignore_ascii_case("error") {
            Ok(Self::Error)
        } else if value.eq_ignore_ascii_case("warn") {
            Ok(Self::Warn)
        } else if value.eq_ignore_ascii_case("info") {
            Ok(Self::Info)
        } else if value.eq_ignore_ascii_case("debug") {
            Ok(Self::Debug)
        } else if value.eq_ignore_ascii_case("trace") {
            Ok(Self::Trace)
        } else {
            Err(ParseLevelError::UnknownLevel)
        }
    }
}

impl LevelFilter {
    /// Disables all logging.
    pub const NONE: Self = Self::Off;

    /// Enables all log events.
    pub const ALL: Self = Self::Trace;

    /// Converts a raw `u8` into a [`LevelFilter`], returning `None` for invalid
    /// values.
    #[inline]
    #[must_use]
    pub const fn try_from_u8(filter: u8) -> Option<Self> {
        match filter {
            0 => Some(Self::Off),
            1 => Some(Self::Fatal),
            2 => Some(Self::Bug),
            3 => Some(Self::Error),
            4 => Some(Self::Warn),
            5 => Some(Self::Info),
            6 => Some(Self::Debug),
            7 => Some(Self::Trace),
            _ => None,
        }
    }

    /// Returns the raw `u8` representation of this level filter.
    #[inline]
    #[must_use]
    pub const fn into_u8(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Fatal => 1,
            Self::Bug => 2,
            Self::Error => 3,
            Self::Warn => 4,
            Self::Info => 5,
            Self::Debug => 6,
            Self::Trace => 7,
        }
    }

    /// Converts a [`Level`] into a [`LevelFilter`].
    #[inline]
    #[must_use]
    pub const fn from_level(level: Level) -> Self {
        match level {
            Level::Fatal => Self::Fatal,
            Level::Bug => Self::Bug,
            Level::Error => Self::Error,
            Level::Warn => Self::Warn,
            Level::Info => Self::Info,
            Level::Debug => Self::Debug,
            Level::Trace => Self::Trace,
        }
    }

    /// Converts a [`SpanLevel`] into a [`LevelFilter`].
    #[inline]
    #[must_use]
    pub const fn from_span_level(span_level: SpanLevel) -> Self {
        match span_level {
            SpanLevel::Error => Self::Error,
            SpanLevel::Warn => Self::Warn,
            SpanLevel::Info => Self::Info,
            SpanLevel::Debug => Self::Debug,
            SpanLevel::Trace => Self::Trace,
        }
    }

    /// Returns the lowercase string representation of this level.
    #[inline]
    #[must_use]
    pub const fn as_lowercase_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Fatal => "fatal",
            Self::Bug => "bug",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }

    /// Returns the uppercase string representation of this level.
    #[inline]
    #[must_use]
    pub const fn as_uppercase_str(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Fatal => "FATAL",
            Self::Bug => "BUG",
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Trace => "TRACE",
        }
    }

    /// # Errors
    ///
    /// Returns [`ParseLevelError::UnknownLevel`] if an unknown level name is
    /// encountered.
    #[inline]
    pub const fn from_str_const(value: &str) -> Result<Self, ParseLevelError> {
        if value.eq_ignore_ascii_case("off") {
            Ok(Self::Off)
        } else if value.eq_ignore_ascii_case("fatal") {
            Ok(Self::Fatal)
        } else if value.eq_ignore_ascii_case("bug") {
            Ok(Self::Bug)
        } else if value.eq_ignore_ascii_case("error") {
            Ok(Self::Error)
        } else if value.eq_ignore_ascii_case("warn") {
            Ok(Self::Warn)
        } else if value.eq_ignore_ascii_case("info") {
            Ok(Self::Info)
        } else if value.eq_ignore_ascii_case("debug") {
            Ok(Self::Debug)
        } else if value.eq_ignore_ascii_case("trace") {
            Ok(Self::Trace)
        } else {
            Err(ParseLevelError::UnknownLevel)
        }
    }
}

impl From<Level> for LevelFilter {
    #[inline]
    fn from(level: Level) -> Self {
        level.into_level_filter()
    }
}

impl From<SpanLevel> for Level {
    #[inline]
    fn from(level: SpanLevel) -> Self {
        level.into_level()
    }
}

impl From<SpanLevel> for LevelFilter {
    #[inline]
    fn from(level: SpanLevel) -> Self {
        level.into_level_filter()
    }
}

impl From<Level> for u8 {
    #[inline]
    fn from(level: Level) -> Self {
        level.into_u8()
    }
}

impl From<SpanLevel> for u8 {
    #[inline]
    fn from(level: SpanLevel) -> Self {
        level.into_u8()
    }
}

impl From<LevelFilter> for u8 {
    #[inline]
    fn from(filter: LevelFilter) -> Self {
        filter.into_u8()
    }
}

macro_rules! impl_mutual_ops {
    ($lhs:ty, $rhs:ty) => {
        impl PartialEq<$lhs> for $rhs {
            #[inline]
            fn eq(&self, other: &$lhs) -> bool {
                LevelFilter::from(*self).eq(&LevelFilter::from(*other))
            }
        }

        impl PartialOrd<$lhs> for $rhs {
            #[inline]
            fn partial_cmp(&self, other: &$lhs) -> Option<Ordering> {
                LevelFilter::from(*self).partial_cmp(&LevelFilter::from(*other))
            }
        }
    };
}

impl_mutual_ops!(Level, SpanLevel);
impl_mutual_ops!(Level, LevelFilter);
impl_mutual_ops!(SpanLevel, Level);
impl_mutual_ops!(SpanLevel, LevelFilter);
impl_mutual_ops!(LevelFilter, Level);
impl_mutual_ops!(LevelFilter, SpanLevel);

impl Display for Level {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_uppercase_str())
    }
}

impl Display for SpanLevel {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_uppercase_str())
    }
}

impl Display for LevelFilter {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_uppercase_str())
    }
}

impl FromStr for Level {
    type Err = ParseLevelError;

    #[inline]
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_str_const(value)
    }
}

impl FromStr for SpanLevel {
    type Err = ParseLevelError;

    #[inline]
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_str_const(value)
    }
}

impl FromStr for LevelFilter {
    type Err = ParseLevelError;

    #[inline]
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_str_const(value)
    }
}

/// An error returned when a string does not name a valid [`Level`],
/// [`SpanLevel`], or [`LevelFilter`].
///
/// Each parser accepts a different subset of the same case-insensitive names:
/// - [`LevelFilter`]: `off`, `fatal`, `bug`, `error`, `warn`, `info`, `debug`,
///   `trace`.
/// - [`Level`]: `fatal`, `bug`, `error`, `warn`, `info`, `debug`, `trace`.
/// - [`SpanLevel`]: `error`, `warn`, `info`, `debug`, `trace`.
///
/// The error type is shared by all three parsers because each accepts a
/// subset of the same names.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum ParseLevelError {
    /// The input does not name a level accepted by the calling parser.
    ///
    /// See [`ParseLevelError`] for the exact set of names accepted by each
    /// parser.
    UnknownLevel,
}

impl Display for ParseLevelError {
    #[expect(clippy::missing_inline_in_public_items, reason = "cold path")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownLevel => f.write_str("unknown level name"),
        }
    }
}

impl Error for ParseLevelError {}

impl From<!> for ParseLevelError {
    #[inline]
    fn from(value: !) -> Self {
        match value {}
    }
}

#[cfg(test)]
mod tests {
    use alloc::borrow::ToOwned;
    use alloc::format;
    use core::str::FromStr;

    use crate::{Level, LevelFilter, ParseLevelError, SpanLevel};

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_level_filter() {
        assert!(LevelFilter::try_from_u8(8).is_none());

        assert_eq!(LevelFilter::from_str_const("OfF"), Ok(LevelFilter::Off));
        assert_eq!(LevelFilter::from_str_const("wArN"), Ok(LevelFilter::Warn));
        assert_eq!(
            LevelFilter::from_str_const("warning"),
            Err(ParseLevelError::UnknownLevel)
        );
        assert_eq!(
            LevelFilter::from_str_const("warning").map_err(|err| format!("{err}")),
            Err("unknown level name".to_owned())
        );

        for i in 0..=7 {
            let level_filter = LevelFilter::try_from_u8(i).unwrap();

            assert_eq!(level_filter.into_u8(), i);
            assert_eq!(u8::from(level_filter), i);

            assert_eq!(
                LevelFilter::from_str_const(level_filter.as_lowercase_str()),
                Ok(level_filter)
            );
            assert_eq!(
                LevelFilter::from_str_const(level_filter.as_uppercase_str()),
                Ok(level_filter)
            );
            assert_eq!(
                LevelFilter::from_str(level_filter.as_lowercase_str()),
                Ok(level_filter)
            );
            assert_eq!(
                LevelFilter::from_str(level_filter.as_uppercase_str()),
                Ok(level_filter)
            );

            assert_eq!(format!("{level_filter}"), level_filter.as_uppercase_str());
        }
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_level() {
        assert!(Level::try_from_u8(0).is_none());
        assert!(Level::try_from_u8(8).is_none());

        assert!(Level::try_from_level_filter(LevelFilter::Off).is_none());

        assert_eq!(
            Level::from_str_const("OfF"),
            Err(ParseLevelError::UnknownLevel)
        );
        assert_eq!(Level::from_str_const("wArN"), Ok(Level::Warn));
        assert_eq!(
            Level::from_str_const("warning"),
            Err(ParseLevelError::UnknownLevel)
        );

        for i in 1..=7 {
            let level = Level::try_from_u8(i).unwrap();
            let level_filter = LevelFilter::from_level(level);

            assert_eq!(level.into_u8(), i);
            assert_eq!(u8::from(level), i);
            assert_eq!(level, level_filter);

            assert_eq!(LevelFilter::from(level), level_filter);
            assert_eq!(Level::try_from_level_filter(level_filter).unwrap(), level);

            assert_eq!(format!("{level}"), level.as_uppercase_str());
            assert_eq!(level.as_lowercase_str(), level_filter.as_lowercase_str());
            assert_eq!(level.as_uppercase_str(), level_filter.as_uppercase_str());

            assert_eq!(Level::from_str_const(level.as_lowercase_str()), Ok(level));
            assert_eq!(Level::from_str_const(level.as_uppercase_str()), Ok(level));
            assert_eq!(Level::from_str(level.as_lowercase_str()), Ok(level));
            assert_eq!(Level::from_str(level.as_uppercase_str()), Ok(level));
        }

        for i1 in 0..=7 {
            let level_filter = LevelFilter::try_from_u8(i1).unwrap();
            for i2 in 1..=7 {
                let level = Level::try_from_u8(i2).unwrap();
                assert_eq!(level_filter.partial_cmp(&level), i1.partial_cmp(&i2));
            }
        }
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_span_level() {
        assert!(SpanLevel::try_from_u8(0).is_none());
        assert!(SpanLevel::try_from_u8(1).is_none());
        assert!(SpanLevel::try_from_u8(2).is_none());
        assert!(SpanLevel::try_from_u8(8).is_none());

        assert!(SpanLevel::try_from_level(Level::Fatal).is_none());
        assert!(SpanLevel::try_from_level(Level::Bug).is_none());
        assert!(SpanLevel::try_from_level_filter(LevelFilter::Fatal).is_none());
        assert!(SpanLevel::try_from_level_filter(LevelFilter::Bug).is_none());

        assert_eq!(
            SpanLevel::from_str_const("off"),
            Err(ParseLevelError::UnknownLevel)
        );
        assert_eq!(
            SpanLevel::from_str_const("fatal"),
            Err(ParseLevelError::UnknownLevel)
        );
        assert_eq!(
            SpanLevel::from_str_const("bug"),
            Err(ParseLevelError::UnknownLevel)
        );
        assert_eq!(SpanLevel::from_str_const("wArN"), Ok(SpanLevel::Warn));
        assert_eq!(
            SpanLevel::from_str_const("warning"),
            Err(ParseLevelError::UnknownLevel)
        );

        for i in 3..=7 {
            let span_level = SpanLevel::try_from_u8(i).unwrap();
            let level = Level::from_span_level(span_level);
            let level_filter = LevelFilter::from_span_level(span_level);

            assert_eq!(span_level.into_u8(), i);
            assert_eq!(u8::from(span_level), i);
            assert_eq!(span_level, level);
            assert_eq!(span_level, level_filter);

            assert_eq!(span_level.into_level(), level);
            assert_eq!(Level::from(span_level), level);
            assert_eq!(LevelFilter::from(span_level), level_filter);
            assert_eq!(SpanLevel::try_from_level(level).unwrap(), span_level);
            assert_eq!(
                SpanLevel::try_from_level_filter(level_filter).unwrap(),
                span_level
            );

            assert_eq!(format!("{span_level}"), span_level.as_uppercase_str());
            assert_eq!(
                span_level.as_lowercase_str(),
                level_filter.as_lowercase_str()
            );
            assert_eq!(
                span_level.as_uppercase_str(),
                level_filter.as_uppercase_str()
            );

            assert_eq!(
                SpanLevel::from_str_const(span_level.as_lowercase_str()),
                Ok(span_level)
            );
            assert_eq!(
                SpanLevel::from_str_const(span_level.as_uppercase_str()),
                Ok(span_level)
            );
            assert_eq!(
                SpanLevel::from_str(span_level.as_lowercase_str()),
                Ok(span_level)
            );
            assert_eq!(
                SpanLevel::from_str(span_level.as_uppercase_str()),
                Ok(span_level)
            );
        }

        for i1 in 0..=7 {
            let level_filter = LevelFilter::try_from_u8(i1).unwrap();
            for i2 in 3..=7 {
                let span_level = SpanLevel::try_from_u8(i2).unwrap();
                assert_eq!(level_filter.partial_cmp(&span_level), i1.partial_cmp(&i2));
            }
        }

        for i1 in 1..=7 {
            let level = Level::try_from_u8(i1).unwrap();
            for i2 in 3..=7 {
                let span_level = SpanLevel::try_from_u8(i2).unwrap();
                assert_eq!(level.partial_cmp(&span_level), i1.partial_cmp(&i2));
            }
        }
    }
}
