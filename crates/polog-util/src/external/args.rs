use core::error::Error;
use core::fmt;

use crate::util::{DisplayAndDebug, MaybeStaticStr};

/// A visitor that receives the fields of a foreign event.
///
/// Called exactly once per event, in source order, by
/// [`ExternalArgs::visit`]. Implementations render, collect, filter, or
/// forward as they see fit; the source walks its fields once and the
/// callback decides what to do with each.
pub trait ExternalArgsVisitor {
    /// Called at most once, when the foreign source distinguishes a
    /// message field from the argument list.
    ///
    /// `log` and `slog` call this for the record's primary message
    /// (`record.args()` / `record.msg()`); `tracing` calls this for an
    /// `event!`'s `message` field if present.
    /// If the source does not distinguish a message, this method is
    /// never called, and any `message`-named field arrives through
    /// [`visit_arg`](Self::visit_arg).
    fn visit_message(&mut self, value: ExternalArgValue<'_>);

    /// Called once per key-value argument, in source order.
    ///
    /// The pair is skipped if it was already reported through
    /// [`visit_message`](Self::visit_message).
    fn visit_arg(&mut self, key: MaybeStaticStr<'_>, value: ExternalArgValue<'_>);
}

/// A borrowed view of the arguments of a foreign event.
///
/// The source walks its fields once and forwards each to the visitor.
/// Implementations exist for `&log::Record`,
/// `(&slog::Record, &slog::OwnedKVList)`, `&tracing::span::Attributes`,
/// `&tracing::Event`, and `&tracing::span::Record`.
/// The borrowed values in [`ExternalArgValue`] are only valid for the
/// duration of the `visit` call.
pub trait ExternalArgs {
    /// Visits every field of the event.
    ///
    /// The order is the source's argument order. `visit_message` runs
    /// before `visit_arg` if a distinguished message exists; otherwise
    /// `visit_message` is not called.
    fn visit<Visitor>(&self, visitor: &mut Visitor)
    where
        Visitor: ExternalArgsVisitor;
}

/// A value from a foreign event.
///
/// The `Str`, `Display`, `Debug`, `Error`, and `Fmt` variants borrow from
/// the foreign event and are only valid for the duration of the adapter
/// call.
#[derive(Clone, Debug)]
pub enum ExternalArgValue<'a> {
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
    Str(&'a str),
    /// A value that implements both [`Display`] and [`Debug`].
    ///
    /// [`Display`]: core::fmt::Display
    Display(&'a dyn DisplayAndDebug),
    /// A value that implements only [`Debug`].
    ///
    /// [`Debug`]: core::fmt::Debug
    Debug(&'a dyn fmt::Debug),
    /// An error value.
    Error(&'a dyn Error),
    /// A pre-formatted `format_args!` value.
    Fmt(&'a fmt::Arguments<'a>),
}

impl<Visitor> ExternalArgsVisitor for &mut Visitor
where
    Visitor: ExternalArgsVisitor,
{
    #[inline]
    fn visit_message(&mut self, value: ExternalArgValue<'_>) {
        Visitor::visit_message(&mut **self, value);
    }

    #[inline]
    fn visit_arg(&mut self, key: MaybeStaticStr<'_>, value: ExternalArgValue<'_>) {
        Visitor::visit_arg(&mut **self, key, value);
    }
}

impl<Args> ExternalArgs for &Args
where
    Args: ExternalArgs,
{
    #[inline]
    fn visit<Visitor>(&self, visitor: &mut Visitor)
    where
        Visitor: ExternalArgsVisitor,
    {
        Args::visit(&**self, visitor);
    }
}

impl<Args> ExternalArgs for &mut Args
where
    Args: ExternalArgs,
{
    #[inline]
    fn visit<Visitor>(&self, visitor: &mut Visitor)
    where
        Visitor: ExternalArgsVisitor,
    {
        Args::visit(&**self, visitor);
    }
}

impl fmt::Display for ExternalArgValue<'_> {
    #[expect(clippy::missing_inline_in_public_items, reason = "bloats binary size")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExternalArgValue::Unit => fmt::Write::write_str(f, "()"),
            ExternalArgValue::None => fmt::Write::write_str(f, "None"),
            ExternalArgValue::Bool(value) => write!(f, "{value}"),
            ExternalArgValue::Char(value) => write!(f, "{value}"),
            ExternalArgValue::U8(value) => write!(f, "{value}"),
            ExternalArgValue::I8(value) => write!(f, "{value}"),
            ExternalArgValue::U16(value) => write!(f, "{value}"),
            ExternalArgValue::I16(value) => write!(f, "{value}"),
            ExternalArgValue::U32(value) => write!(f, "{value}"),
            ExternalArgValue::I32(value) => write!(f, "{value}"),
            ExternalArgValue::U64(value) => write!(f, "{value}"),
            ExternalArgValue::I64(value) => write!(f, "{value}"),
            ExternalArgValue::U128(value) => write!(f, "{value}"),
            ExternalArgValue::I128(value) => write!(f, "{value}"),
            ExternalArgValue::Usize(value) => write!(f, "{value}"),
            ExternalArgValue::Isize(value) => write!(f, "{value}"),
            ExternalArgValue::F32(value) => write!(f, "{value}"),
            ExternalArgValue::F64(value) => write!(f, "{value}"),
            ExternalArgValue::Str(value) => fmt::Write::write_str(f, value),
            ExternalArgValue::Display(display) => write!(f, "{display}"),
            ExternalArgValue::Debug(debug) => write!(f, "{debug:?}"),
            ExternalArgValue::Error(error) => write!(f, "{error}"),
            ExternalArgValue::Fmt(args) => write!(f, "{args}"),
        }
    }
}

impl ExternalArgs for () {
    #[inline]
    fn visit<Visitor>(&self, _visitor: &mut Visitor)
    where
        Visitor: ExternalArgsVisitor,
    {
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;
    use core::fmt;

    use crate::ExternalArgValue;
    use crate::util::DisplayAndDebug;

    #[test]
    fn external_arg_fmt() {
        #[derive(Debug)]
        struct Error;
        impl fmt::Display for Error {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("missing")
            }
        }
        impl core::error::Error for Error {}

        let error = Error;
        let display: &dyn DisplayAndDebug = &123_i32;
        let debug: &dyn fmt::Debug = &"debug";
        let fmt_args = format_args!("-{}-", 123);

        assert_eq!(format!("{}", ExternalArgValue::Bool(true)), "true");
        assert_eq!(format!("{}", ExternalArgValue::Char('a')), "a");
        assert_eq!(format!("{}", ExternalArgValue::U8(1)), "1");
        assert_eq!(format!("{}", ExternalArgValue::I8(-1)), "-1");
        assert_eq!(format!("{}", ExternalArgValue::U16(2)), "2");
        assert_eq!(format!("{}", ExternalArgValue::I16(-2)), "-2");
        assert_eq!(format!("{}", ExternalArgValue::U32(3)), "3");
        assert_eq!(format!("{}", ExternalArgValue::I32(-3)), "-3");
        assert_eq!(format!("{}", ExternalArgValue::U64(4)), "4");
        assert_eq!(format!("{}", ExternalArgValue::I64(-4)), "-4");
        assert_eq!(format!("{}", ExternalArgValue::U128(5)), "5");
        assert_eq!(format!("{}", ExternalArgValue::I128(-5)), "-5");
        assert_eq!(format!("{}", ExternalArgValue::Usize(6)), "6");
        assert_eq!(format!("{}", ExternalArgValue::Isize(-6)), "-6");
        assert_eq!(format!("{}", ExternalArgValue::F32(1.5)), "1.5");
        assert_eq!(format!("{}", ExternalArgValue::F64(2.5)), "2.5");
        assert_eq!(format!("{}", ExternalArgValue::Str("str")), "str");
        assert_eq!(format!("{}", ExternalArgValue::Display(display)), "123");
        assert_eq!(format!("{}", ExternalArgValue::Debug(debug)), "\"debug\"");
        assert_eq!(format!("{}", ExternalArgValue::Error(&error)), "missing");
        assert_eq!(format!("{}", ExternalArgValue::Fmt(&fmt_args)), "-123-");
    }
}
