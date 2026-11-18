use core::fmt;
use std::borrow::Cow;

use polog::{BaseHandler, ContextArgs, Level};
use polog_util::util::{DisplayAndDebug, MaybeStaticStr};
use polog_util::{
    ExternalArgValue, ExternalArgs, ExternalArgsVisitor, ExternalHandler,
    TypeErasedExternalDescriptor,
};

const SIMPLE_DESCRIPTOR: TypeErasedExternalDescriptor = TypeErasedExternalDescriptor {
    message: Some(Cow::Borrowed("message")),
    level: Level::Info,
    target: Some(Cow::Borrowed("target")),
    module_path: Some(Cow::Borrowed("module_path")),
    file: Some(Cow::Borrowed("file.rs")),
    line: Some(12),
    column: Some(23),
};

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
        let display: &dyn DisplayAndDebug = &123_i32;
        let debug: &dyn fmt::Debug = &"debug";
        let fmt_args = format_args!("-{}-", 123);

        visitor.visit_message(ExternalArgValue::Str("message-override"));

        let s = |value| MaybeStaticStr::Static(value);

        visitor.visit_arg(s("unit"), ExternalArgValue::Unit);
        visitor.visit_arg(s("none"), ExternalArgValue::None);
        visitor.visit_arg(s("bool"), ExternalArgValue::Bool(true));
        visitor.visit_arg(s("char"), ExternalArgValue::Char('a'));
        visitor.visit_arg(s("u8"), ExternalArgValue::U8(123));
        visitor.visit_arg(s("i8"), ExternalArgValue::I8(123));
        visitor.visit_arg(s("u16"), ExternalArgValue::U16(123));
        visitor.visit_arg(s("i16"), ExternalArgValue::I16(123));
        visitor.visit_arg(s("u32"), ExternalArgValue::U32(123));
        visitor.visit_arg(s("i32"), ExternalArgValue::I32(123));
        visitor.visit_arg(s("u64"), ExternalArgValue::U64(123));
        visitor.visit_arg(s("i64"), ExternalArgValue::I64(123));
        visitor.visit_arg(s("u128"), ExternalArgValue::U128(123));
        visitor.visit_arg(s("i128"), ExternalArgValue::I128(123));
        visitor.visit_arg(s("usize"), ExternalArgValue::Usize(123));
        visitor.visit_arg(s("isize"), ExternalArgValue::Isize(123));
        visitor.visit_arg(s("f32"), ExternalArgValue::F32(123.234));
        visitor.visit_arg(s("f64"), ExternalArgValue::F64(123.234));
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

#[inline]
pub fn check_external_handler_fns<L, Context>(logger: &mut L, context: &Context)
where
    L: BaseHandler + ExternalHandler<Context>,
    Context: Clone + ContextArgs,
{
    let span_id1 = logger.generate_span_id();
    let span_id2 = logger.generate_span_id();
    let span_id3 = logger.generate_span_id();

    assert!(logger.is_enabled_external(&SIMPLE_DESCRIPTOR));

    logger.log_external(
        &SIMPLE_DESCRIPTOR,
        &ExampleArgs,
        Some(span_id1),
        context.clone(),
    );

    logger.new_span_external(
        &SIMPLE_DESCRIPTOR,
        &ExampleArgs,
        Some(span_id1),
        span_id2,
        context.clone(),
    );

    logger.enter_span_external(
        &SIMPLE_DESCRIPTOR,
        &ExampleArgs,
        Some(span_id2),
        span_id3,
        context.clone(),
    );

    logger.record_span_external(&SIMPLE_DESCRIPTOR, &ExampleArgs, span_id2, context.clone());
    logger.link_span_external(&SIMPLE_DESCRIPTOR, span_id1, span_id3, context.clone());
    logger.exit_span_external(&SIMPLE_DESCRIPTOR, span_id3, context.clone());
    logger.close_span_external(&SIMPLE_DESCRIPTOR, span_id2, context.clone());
}
