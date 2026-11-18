use alloc::borrow::Cow;
use alloc::format;
use alloc::string::String;

use polog::Level;

use crate::util::MaybeStaticStr;
use crate::{ExternalDescriptor, ExternalMessage};

/// A type-erased description of an event that originates outside `polog`.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct TypeErasedExternalDescriptor {
    /// Event or span message or name, if the foreign facade has one.
    ///
    /// `tracing` supplies a name for every span and every event.
    ///
    /// `log` sets it for the primary record message.
    /// `slog` sets it for the primary `msg` argument.
    pub message: Option<Cow<'static, str>>,

    /// Event level, mapped to `polog::Level` by the ingesting backend.
    pub level: Level,

    /// Event target, if the foreign facade has one.
    ///
    /// `log` and `tracing` set a target on every event.
    /// `slog` has no target concept.
    pub target: Option<Cow<'static, str>>,

    /// Module path of the emitting call site, if the foreign facade has one.
    ///
    /// `log` and `slog` always provide a module path.
    /// `tracing` may provide a module path.
    pub module_path: Option<Cow<'static, str>>,

    /// Source file of the emitting call site, if the foreign facade has one.
    pub file: Option<Cow<'static, str>>,

    /// Source line of the emitting call site, if the foreign facade has one.
    pub line: Option<u32>,

    /// Source column of the emitting call site, if the foreign facade has one.
    pub column: Option<u32>,
}

impl TypeErasedExternalDescriptor {
    /// Builds a type-erased descriptor.
    #[inline]
    #[must_use]
    pub fn from_descriptor<Desc>(descriptor: &Desc) -> Self
    where
        Desc: ExternalDescriptor,
    {
        let message = descriptor.message().map(|message| match message {
            ExternalMessage::Str(value) => maybe_static_to_cow(value),
            ExternalMessage::Fmt(args) => Cow::Owned(format!("{args}")),
        });

        Self {
            message,
            level: descriptor.level(),
            target: descriptor.target().map(maybe_static_to_cow),
            module_path: descriptor.module_path().map(maybe_static_to_cow),
            file: descriptor.file().map(maybe_static_to_cow),
            line: descriptor.line(),
            column: descriptor.column(),
        }
    }
}

impl ExternalDescriptor for TypeErasedExternalDescriptor {
    #[inline]
    fn message(&self) -> Option<ExternalMessage<'_>> {
        self.message
            .as_ref()
            .map(|value| ExternalMessage::Str(cow_as_maybe_static(value)))
    }

    #[inline]
    fn level(&self) -> Level {
        self.level
    }

    #[inline]
    fn target(&self) -> Option<MaybeStaticStr<'_>> {
        self.target.as_ref().map(cow_as_maybe_static)
    }

    #[inline]
    fn module_path(&self) -> Option<MaybeStaticStr<'_>> {
        self.module_path.as_ref().map(cow_as_maybe_static)
    }

    #[inline]
    fn file(&self) -> Option<MaybeStaticStr<'_>> {
        self.file.as_ref().map(cow_as_maybe_static)
    }

    #[inline]
    fn line(&self) -> Option<u32> {
        self.line
    }

    #[inline]
    fn column(&self) -> Option<u32> {
        self.column
    }
}

#[inline]
fn maybe_static_to_cow(value: MaybeStaticStr<'_>) -> Cow<'static, str> {
    value
        .try_as_static_str()
        .map_or_else(|| Cow::Owned(String::from(value.as_str())), Cow::Borrowed)
}

#[expect(
    clippy::ptr_arg,
    reason = "Intentionally used to preserve static lifetime if possible"
)]
#[inline]
fn cow_as_maybe_static<'a>(value: &'a Cow<'static, str>) -> MaybeStaticStr<'a> {
    match value {
        Cow::Borrowed(value) => MaybeStaticStr::Static(value),
        Cow::Owned(value) => MaybeStaticStr::Borrowed(value),
    }
}

#[cfg(test)]
mod tests {
    use alloc::borrow::Cow;
    use alloc::format;

    use crate::{ExternalDescriptor, TypeErasedExternalDescriptor};

    #[expect(clippy::mut_mut, reason = "explicitly tested")]
    #[test]
    fn test_erased_descriptor() {
        let descriptor1 = TypeErasedExternalDescriptor {
            message: Some(Cow::Borrowed("name")),
            level: polog::Level::Info,
            target: Some(Cow::Borrowed("target")),
            module_path: Some(Cow::Borrowed("module_path")),
            file: Some(Cow::Borrowed("file.rs")),
            line: Some(12),
            column: Some(23),
        };
        let mut descriptor2 = descriptor1.clone();

        let desc1: &dyn ExternalDescriptor = &&descriptor1;
        let desc2: &mut dyn ExternalDescriptor = &mut &mut descriptor2;

        assert_eq!(
            format!("{:?}", desc1.message()),
            format!("{:?}", desc2.message())
        );
        assert_eq!(desc1.level(), desc2.level());
        assert_eq!(desc1.target(), desc2.target());
        assert_eq!(desc1.module_path(), desc2.module_path());
        assert_eq!(desc1.file(), desc2.file());
        assert_eq!(desc1.line(), desc2.line());
        assert_eq!(desc1.column(), desc2.column());
    }
}
