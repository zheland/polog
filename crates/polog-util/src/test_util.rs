#[cfg(feature = "alloc")]
use alloc::borrow::ToOwned;
#[cfg(feature = "alloc")]
use alloc::string::String;

#[cfg(feature = "alloc")]
use polog::Level;

#[cfg(feature = "alloc")]
use crate::util::MaybeStaticStr;
use crate::{ExternalDescriptor, ExternalDescriptorIntoOwning, ExternalMessage};

pub struct SimpleBorrowingExternalDescriptor<'a> {
    pub name: Option<&'a str>,
    pub level: Level,
    pub target: Option<&'a str>,
    pub module_path: Option<&'a str>,
    pub file: Option<&'a str>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[cfg(feature = "alloc")]
pub struct SimpleOwningExternalDescriptor {
    pub name: Option<String>,
    pub level: Level,
    pub target: Option<String>,
    pub module_path: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

impl ExternalDescriptor for SimpleBorrowingExternalDescriptor<'_> {
    fn message(&self) -> Option<ExternalMessage<'_>> {
        self.name
            .map(|name| ExternalMessage::Str(MaybeStaticStr::Borrowed(name)))
    }

    fn level(&self) -> Level {
        self.level
    }

    fn target(&self) -> Option<MaybeStaticStr<'_>> {
        self.target.map(MaybeStaticStr::Borrowed)
    }

    fn module_path(&self) -> Option<MaybeStaticStr<'_>> {
        self.module_path.map(MaybeStaticStr::Borrowed)
    }

    fn file(&self) -> Option<MaybeStaticStr<'_>> {
        self.file.map(MaybeStaticStr::Borrowed)
    }

    fn line(&self) -> Option<u32> {
        self.line
    }

    fn column(&self) -> Option<u32> {
        self.column
    }
}

#[cfg(feature = "alloc")]
impl ExternalDescriptorIntoOwning for SimpleBorrowingExternalDescriptor<'_> {
    type OwningExternalDescriptor = SimpleOwningExternalDescriptor;

    fn into_owning(self) -> Self::OwningExternalDescriptor {
        SimpleOwningExternalDescriptor {
            name: self.name.map(ToOwned::to_owned),
            level: self.level,
            target: self.target.map(ToOwned::to_owned),
            module_path: self.module_path.map(ToOwned::to_owned),
            file: self.file.map(ToOwned::to_owned),
            line: self.line,
            column: self.column,
        }
    }
}

#[cfg(feature = "alloc")]
impl ExternalDescriptor for SimpleOwningExternalDescriptor {
    fn message(&self) -> Option<ExternalMessage<'_>> {
        self.name
            .as_ref()
            .map(|name| ExternalMessage::Str(MaybeStaticStr::Borrowed(name.as_str())))
    }

    fn level(&self) -> Level {
        self.level
    }

    fn target(&self) -> Option<MaybeStaticStr<'_>> {
        self.target.as_deref().map(MaybeStaticStr::Borrowed)
    }

    fn module_path(&self) -> Option<MaybeStaticStr<'_>> {
        self.module_path.as_deref().map(MaybeStaticStr::Borrowed)
    }

    fn file(&self) -> Option<MaybeStaticStr<'_>> {
        self.file.as_deref().map(MaybeStaticStr::Borrowed)
    }

    fn line(&self) -> Option<u32> {
        self.line
    }

    fn column(&self) -> Option<u32> {
        self.column
    }
}

#[cfg(feature = "alloc")]
impl ExternalDescriptorIntoOwning for SimpleOwningExternalDescriptor {
    type OwningExternalDescriptor = Self;

    fn into_owning(self) -> Self::OwningExternalDescriptor {
        self
    }
}
