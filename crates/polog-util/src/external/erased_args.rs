use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::util::MaybeStaticStr;
use crate::{ExternalArgValue, ExternalArgs, ExternalArgsVisitor};

/// A type-erased set of arguments of an event that originates outside `polog`.
#[derive(Clone, PartialEq, PartialOrd, Default, Debug)]
pub struct TypeErasedExternalArgs {
    str_arena: String,
    message: Option<TypeErasedExternalArgValue>,
    args: Vec<(ArenaStrSlice, TypeErasedExternalArgValue)>,
}

/// A byte range into [`TypeErasedExternalArgs::str_arena`].
#[derive(Copy, Clone, PartialEq, PartialOrd, Default, Debug)]
struct ArenaStrSlice {
    offset: usize,
    len: usize,
}

/// An owned, type-erased copy of an [`ExternalArgValue`].
///
/// Primitive variants are copied verbatim.
/// String-like variants (`Str`, `Display`, `Debug`, `Error`, `Fmt`) are stored
/// as an [`ArenaStrSlice`] into the owning
/// [`TypeErasedExternalArgs::str_arena`].
/// Trait-object values are formatted eagerly, since the borrow they came
/// through cannot be stored.
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug)]
enum TypeErasedExternalArgValue {
    /// A unit value.
    Unit,
    /// A `Option::None` value from the foreign facade.
    None,
    /// A boolean value.
    Bool(bool),
    /// A Unicode scalar value.
    Char(char),
    /// An unsigned 8-bit integer.
    U8(u8),
    /// A signed 8-bit integer.
    I8(i8),
    /// An unsigned 16-bit integer.
    U16(u16),
    /// A signed 16-bit integer.
    I16(i16),
    /// An unsigned 32-bit integer.
    U32(u32),
    /// A signed 32-bit integer.
    I32(i32),
    /// An unsigned 64-bit integer.
    U64(u64),
    /// A signed 64-bit integer.
    I64(i64),
    /// An unsigned 128-bit integer.
    U128(u128),
    /// A signed 128-bit integer.
    I128(i128),
    /// An unsigned pointer-sized integer.
    Usize(usize),
    /// A signed pointer-sized integer.
    Isize(isize),
    /// A single-precision float.
    F32(f32),
    /// A double-precision float.
    F64(f64),
    /// A borrowed string.
    Str(ArenaStrSlice),
    /// A value that implements both [`Display`] and [`Debug`].
    ///
    /// [`Display`]: core::fmt::Display
    Display(ArenaStrSlice),
    /// A value that implements only [`Debug`].
    ///
    /// [`Debug`]: core::fmt::Debug
    Debug(ArenaStrSlice),
    /// An error value.
    Error(ArenaStrSlice),
    /// A pre-formatted `format_args!` value.
    Fmt(ArenaStrSlice),
}

impl TypeErasedExternalArgs {
    /// Builds a type-erased argument list.
    #[inline]
    #[must_use]
    pub fn from_args<Args>(args: &Args) -> Self
    where
        Args: ExternalArgs,
    {
        let mut this = Self::default();
        args.visit(&mut ArgsBuilderVisitor(&mut this));
        this
    }

    /// Resolves an arena slice back to its string contents.
    #[inline]
    fn resolve_str(&self, slice: ArenaStrSlice) -> &str {
        let until = slice.offset.saturating_add(slice.len);
        self.str_arena.get(slice.offset..until).unwrap_or("")
    }

    /// Reconstructs a borrowed [`ExternalArgValue`] from its erased form.
    ///
    /// String-like variants (including stringified `Display` / `Debug` /
    /// `Error` / `Fmt` trait objects) are presented as
    /// [`ExternalArgValue::Str`], because the arena only holds plain UTF-8.
    #[inline]
    fn resolve_value(&self, value: &TypeErasedExternalArgValue) -> ExternalArgValue<'_> {
        use ExternalArgValue as V;
        use TypeErasedExternalArgValue as TeV;
        match *value {
            TeV::Unit => V::Unit,
            TeV::None => V::None,
            TeV::Bool(value) => V::Bool(value),
            TeV::Char(value) => V::Char(value),
            TeV::U8(value) => V::U8(value),
            TeV::I8(value) => V::I8(value),
            TeV::U16(value) => V::U16(value),
            TeV::I16(value) => V::I16(value),
            TeV::U32(value) => V::U32(value),
            TeV::I32(value) => V::I32(value),
            TeV::U64(value) => V::U64(value),
            TeV::I64(value) => V::I64(value),
            TeV::U128(value) => V::U128(value),
            TeV::I128(value) => V::I128(value),
            TeV::Usize(value) => V::Usize(value),
            TeV::Isize(value) => V::Isize(value),
            TeV::F32(value) => V::F32(value),
            TeV::F64(value) => V::F64(value),
            TeV::Str(slice)
            | TeV::Display(slice)
            | TeV::Debug(slice)
            | TeV::Error(slice)
            | TeV::Fmt(slice) => V::Str(self.resolve_str(slice)),
        }
    }
}

/// Visitor that fills a [`TypeErasedExternalArgs`] from an [`ExternalArgs`]
/// source.
struct ArgsBuilderVisitor<'a>(&'a mut TypeErasedExternalArgs);

impl ArgsBuilderVisitor<'_> {
    /// Copies `value` into the arena and returns its slice.
    #[inline]
    fn push_str(&mut self, value: &str) -> ArenaStrSlice {
        let offset = self.0.str_arena.len();
        self.0.str_arena.push_str(value);
        ArenaStrSlice {
            offset,
            len: value.len(),
        }
    }

    /// Converts a borrowed [`ExternalArgValue`] into its owning erased form,
    /// formatting trait objects into the arena.
    #[inline]
    fn push_value(&mut self, value: &ExternalArgValue<'_>) -> TypeErasedExternalArgValue {
        use ExternalArgValue as V;
        use TypeErasedExternalArgValue as TeV;
        match value {
            V::Unit => TeV::Unit,
            V::None => TeV::None,
            V::Bool(value) => TeV::Bool(*value),
            V::Char(value) => TeV::Char(*value),
            V::U8(value) => TeV::U8(*value),
            V::I8(value) => TeV::I8(*value),
            V::U16(value) => TeV::U16(*value),
            V::I16(value) => TeV::I16(*value),
            V::U32(value) => TeV::U32(*value),
            V::I32(value) => TeV::I32(*value),
            V::U64(value) => TeV::U64(*value),
            V::I64(value) => TeV::I64(*value),
            V::U128(value) => TeV::U128(*value),
            V::I128(value) => TeV::I128(*value),
            V::Usize(value) => TeV::Usize(*value),
            V::Isize(value) => TeV::Isize(*value),
            V::F32(value) => TeV::F32(*value),
            V::F64(value) => TeV::F64(*value),
            V::Str(value) => TeV::Str(self.push_str(value)),
            V::Display(value) => TeV::Display(self.push_str(&format!("{value}"))),
            V::Debug(value) => TeV::Debug(self.push_str(&format!("{value:?}"))),
            V::Error(value) => TeV::Error(self.push_str(&format!("{value}"))),
            V::Fmt(value) => TeV::Fmt(self.push_str(&format!("{value}"))),
        }
    }
}

impl ExternalArgsVisitor for ArgsBuilderVisitor<'_> {
    #[inline]
    fn visit_message(&mut self, value: ExternalArgValue<'_>) {
        if self.0.message.is_some() {
            self.visit_arg(MaybeStaticStr::Static("message"), value);
        } else {
            self.0.message = Some(self.push_value(&value));
        }
    }

    #[inline]
    fn visit_arg(&mut self, key: MaybeStaticStr<'_>, value: ExternalArgValue<'_>) {
        let key = self.push_str(key.as_str());
        let value = self.push_value(&value);
        self.0.args.push((key, value));
    }
}

impl ExternalArgs for TypeErasedExternalArgs {
    #[inline]
    fn visit<Visitor>(&self, visitor: &mut Visitor)
    where
        Visitor: ExternalArgsVisitor,
    {
        if let Some(message) = &self.message {
            visitor.visit_message(self.resolve_value(message));
        }
        for &(key, value) in &self.args {
            visitor.visit_arg(
                MaybeStaticStr::Borrowed(self.resolve_str(key)),
                self.resolve_value(&value),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use core::fmt::{self, Write};

    use crate::util::{DisplayAndDebug, MaybeStaticStr};
    use crate::{ExternalArgValue, ExternalArgs, ExternalArgsVisitor, TypeErasedExternalArgs};

    struct ExampleArgs;

    impl ExternalArgs for ExampleArgs {
        #[inline]
        fn visit<Visitor>(&self, visitor: &mut Visitor)
        where
            Visitor: ExternalArgsVisitor,
        {
            #[derive(Debug)]
            struct Error;
            impl fmt::Display for Error {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("custom")
                }
            }
            impl core::error::Error for Error {}

            let error = Error;
            let display: &dyn DisplayAndDebug = &321_i32;
            let debug: &dyn fmt::Debug = &"debug";
            let fmt_args = format_args!("-{}-", 123);

            visitor.visit_message(ExternalArgValue::Str("message-override"));

            let s = |value| MaybeStaticStr::Static(value);

            visitor.visit_arg(s("unit"), ExternalArgValue::Unit);
            visitor.visit_arg(s("none"), ExternalArgValue::None);
            visitor.visit_arg(s("bool"), ExternalArgValue::Bool(true));
            visitor.visit_arg(s("char"), ExternalArgValue::Char('a'));
            visitor.visit_arg(s("u8"), ExternalArgValue::U8(12));
            visitor.visit_arg(s("i8"), ExternalArgValue::I8(23));
            visitor.visit_arg(s("u16"), ExternalArgValue::U16(34));
            visitor.visit_arg(s("i16"), ExternalArgValue::I16(45));
            visitor.visit_arg(s("u32"), ExternalArgValue::U32(56));
            visitor.visit_arg(s("i32"), ExternalArgValue::I32(67));
            visitor.visit_arg(s("u64"), ExternalArgValue::U64(78));
            visitor.visit_arg(s("i64"), ExternalArgValue::I64(89));
            visitor.visit_arg(s("u128"), ExternalArgValue::U128(123));
            visitor.visit_arg(s("i128"), ExternalArgValue::I128(234));
            visitor.visit_arg(s("usize"), ExternalArgValue::Usize(345));
            visitor.visit_arg(s("isize"), ExternalArgValue::Isize(456));
            visitor.visit_arg(s("f32"), ExternalArgValue::F32(123.234));
            visitor.visit_arg(s("f64"), ExternalArgValue::F64(234.345));
            visitor.visit_arg(s("str"), ExternalArgValue::Str("abc"));
            visitor.visit_arg(s("display"), ExternalArgValue::Display(display));
            visitor.visit_arg(s("debug"), ExternalArgValue::Debug(debug));
            visitor.visit_arg(s("error"), ExternalArgValue::Error(&error));
            visitor.visit_arg(s("fmt"), ExternalArgValue::Fmt(&fmt_args));

            visitor.visit_arg(
                MaybeStaticStr::Borrowed("foo"),
                ExternalArgValue::Str("bar"),
            );
        }
    }

    struct ToStringVisitor(String);

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    impl ExternalArgsVisitor for ToStringVisitor {
        fn visit_message(&mut self, value: ExternalArgValue<'_>) {
            write!(self.0, "msg: {value}; ").unwrap();
        }

        fn visit_arg(&mut self, key: MaybeStaticStr<'_>, value: ExternalArgValue<'_>) {
            write!(self.0, "arg.{key}: {value}; ").unwrap();
        }
    }

    #[expect(clippy::mut_mut, reason = "explicitly tested")]
    #[test]
    fn test_erased_external_args() {
        const EXPECTED_ARENA: &str = "\
            message-override\
            unitnoneboolchar\
            u8i8u16i16u32i32u64i64u128i128usizeisizef32f64\
            strabcdisplay321debug\"debug\"errorcustomfmt-123-foobar";

        let mut visitor1 = ToStringVisitor(String::new());
        ExampleArgs.visit(&mut visitor1);

        let args = TypeErasedExternalArgs::from_args(&ExampleArgs);
        assert_eq!(args.str_arena, EXPECTED_ARENA);
        let mut visitor2 = ToStringVisitor(String::new());
        args.visit(&mut visitor2);

        let mut args = TypeErasedExternalArgs::from_args(&args);
        assert_eq!(args.str_arena, EXPECTED_ARENA);
        let mut visitor3 = ToStringVisitor(String::new());
        args.visit(&mut &mut visitor3);

        let args = TypeErasedExternalArgs::from_args(&&mut args);
        assert_eq!(args.str_arena, EXPECTED_ARENA);
        let mut visitor4 = ToStringVisitor(String::new());
        args.visit(&mut &mut visitor4);

        assert_eq!(
            visitor1.0,
            "msg: message-override; arg.unit: (); arg.none: None; arg.bool: true; arg.char: a; \
             arg.u8: 12; arg.i8: 23; arg.u16: 34; arg.i16: 45; arg.u32: 56; arg.i32: 67; arg.u64: \
             78; arg.i64: 89; arg.u128: 123; arg.i128: 234; arg.usize: 345; arg.isize: 456; \
             arg.f32: 123.234; arg.f64: 234.345; arg.str: abc; arg.display: 321; arg.debug: \
             \"debug\"; arg.error: custom; arg.fmt: -123-; arg.foo: bar; "
        );
        assert_eq!(visitor1.0, visitor2.0);
        assert_eq!(visitor1.0, visitor3.0);
        assert_eq!(visitor1.0, visitor4.0);
    }
}
