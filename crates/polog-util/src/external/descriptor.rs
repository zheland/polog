use core::fmt;

use polog::Level;

use crate::util::MaybeStaticStr;

/// A description of an event that originates outside `polog`.
///
/// All fields except `level` are `Option`, because foreign facades differ
/// in which metadata they carry:
///
/// - `log` provides `target`, `module_path`, `file`, and `line`, but no `name`
///   or `column`.
/// - `slog` provides `module_path`, `file`, `line`, and `column`, but no `name`
///   and no target.
/// - `tracing` provides all fields except `column`.
///
/// A filter that needs to match on a field the foreign facade does not
/// supply should treat the missing value as "does not match".
pub trait ExternalDescriptor {
    /// Event or span message or name, if the foreign facade has one.
    ///
    /// `tracing` supplies a name for every span and every event.
    /// `log` sets it for the primary record message.
    /// `slog` sets it for the primary `msg` argument.
    fn message(&self) -> Option<ExternalMessage<'_>>;

    /// Event level, mapped to `polog::Level` by the ingesting backend.
    fn level(&self) -> Level;

    /// Event target, if the foreign facade has one.
    ///
    /// `log` and `tracing` set a target on every event.
    /// `slog` has no target concept.
    fn target(&self) -> Option<MaybeStaticStr<'_>>;

    /// Module path of the emitting call site, if the foreign facade has one.
    ///
    /// `log` and `slog` always provide a module path.
    /// `tracing` may provide a module path.
    fn module_path(&self) -> Option<MaybeStaticStr<'_>>;

    /// Source file of the emitting call site, if the foreign facade has one.
    fn file(&self) -> Option<MaybeStaticStr<'_>>;

    /// Source line of the emitting call site, if the foreign facade has one.
    fn line(&self) -> Option<u32>;

    /// Source column of the emitting call site, if the foreign facade has one.
    fn column(&self) -> Option<u32>;
}

/// The message of a foreign event, as returned by
/// [`ExternalDescriptor::message`].
///
/// A foreign facade may supply its message either as a plain string  or as a
/// pre-built [`fmt::Arguments`] value.
///
/// [`ExternalDescriptor::message`]: crate::ExternalDescriptor::message
/// [`fmt::Arguments`]: core::fmt::Arguments
#[derive(Clone, Debug)]
pub enum ExternalMessage<'a> {
    /// A borrowed string message.
    Str(MaybeStaticStr<'a>),
    /// A pre-formatted message.
    Fmt(&'a fmt::Arguments<'a>),
}

impl<Desc> ExternalDescriptor for &Desc
where
    Desc: ExternalDescriptor,
{
    #[inline]
    fn message(&self) -> Option<ExternalMessage<'_>> {
        Desc::message(&**self)
    }

    #[inline]
    fn level(&self) -> Level {
        Desc::level(&**self)
    }

    #[inline]
    fn target(&self) -> Option<MaybeStaticStr<'_>> {
        Desc::target(&**self)
    }

    #[inline]
    fn module_path(&self) -> Option<MaybeStaticStr<'_>> {
        Desc::module_path(&**self)
    }

    #[inline]
    fn file(&self) -> Option<MaybeStaticStr<'_>> {
        Desc::file(&**self)
    }

    #[inline]
    fn line(&self) -> Option<u32> {
        Desc::line(&**self)
    }

    #[inline]
    fn column(&self) -> Option<u32> {
        Desc::column(&**self)
    }
}

impl<Desc> ExternalDescriptor for &mut Desc
where
    Desc: ExternalDescriptor,
{
    #[inline]
    fn message(&self) -> Option<ExternalMessage<'_>> {
        Desc::message(&**self)
    }

    #[inline]
    fn level(&self) -> Level {
        Desc::level(&**self)
    }

    #[inline]
    fn target(&self) -> Option<MaybeStaticStr<'_>> {
        Desc::target(&**self)
    }

    #[inline]
    fn module_path(&self) -> Option<MaybeStaticStr<'_>> {
        Desc::module_path(&**self)
    }

    #[inline]
    fn file(&self) -> Option<MaybeStaticStr<'_>> {
        Desc::file(&**self)
    }

    #[inline]
    fn line(&self) -> Option<u32> {
        Desc::line(&**self)
    }

    #[inline]
    fn column(&self) -> Option<u32> {
        Desc::column(&**self)
    }
}

impl fmt::Display for ExternalMessage<'_> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Str(value) => fmt::Display::fmt(value, f),
            Self::Fmt(args) => fmt::Display::fmt(args, f),
        }
    }
}
