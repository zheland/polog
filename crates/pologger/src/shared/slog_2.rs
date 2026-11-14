use core::error::Error;

use polog_util::{ExternalDescriptor, ExternalFilterFn};
use slog_2::KV;

use crate::{
    ExternalArg, ExternalArguments, ExternalEntry, Formatter, FormatterEventKind, KnownSpanId,
    MaybeKnownSpanId, SharedPologger,
};

impl<Fmt, Fff> slog_2::Drain for SharedPologger<Fmt, Fff>
where
    Fmt: Send + Formatter,
    Fff: Send + Sync + ExternalFilterFn,
{
    type Ok = ();
    type Err = slog_2::Never;

    #[inline]
    fn log(
        &self,
        record: &slog_2::Record<'_>,
        values: &slog_2::OwnedKVList,
    ) -> Result<Self::Ok, Self::Err> {
        let level = polog_util::conv::slog_2::level_from_foreign_approx(record.level());

        let descriptor = ExternalDescriptor {
            name: None,
            level,
            target: None,
            module_path: Some(record.module()),
            file: Some(record.file()),
            line: Some(record.line()),
            column: Some(record.column()),
        };

        if self.filter().is_external_enabled(&descriptor) {
            let args = Args { record, values };

            let entry = ExternalEntry {
                descriptor: &descriptor,
                message_fmt: Some(record.msg()),
                args_visitor: &args,
                parent_span_id: MaybeKnownSpanId::Unknown,
                span_id: KnownSpanId::Root,
                kind: FormatterEventKind::Log,
            };

            self.log_external_entry(&entry);
        }

        Ok(())
    }

    #[inline]
    fn flush(&self) -> Result<(), slog_2::FlushError> {
        self.flush_blocking_by_ref();
        Ok(())
    }

    #[inline]
    fn is_enabled(&self, level: slog_2::Level) -> bool {
        let level = polog_util::conv::slog_2::level_from_foreign_approx(level);
        let descriptor = ExternalDescriptor {
            name: None,
            level,
            target: None,
            module_path: None,
            file: None,
            line: None,
            column: None,
        };
        self.filter().is_external_enabled(&descriptor)
    }
}

/// `ExternalArguments` adapter over `slog` key-values.
struct Args<'a> {
    record: &'a slog_2::Record<'a>,
    values: &'a slog_2::OwnedKVList,
}

impl ExternalArguments for Args<'_> {
    #[inline]
    fn visit_msg<F>(&self, _cb: F)
    where
        F: for<'b> FnOnce(ExternalArg<'b>),
    {
    }

    #[inline]
    fn visit_args<F>(&self, cb: F)
    where
        F: for<'b> FnMut(&'b str, ExternalArg<'b>),
    {
        let mut visitor = ArgsVisitor(cb);
        let _ = self.record.kv().serialize(self.record, &mut visitor);
        let _ = slog_2::OwnedKVList::serialize(self.values, self.record, &mut visitor);
    }
}

/// `slog::Serializer` that forwards each `emit_*` call to a
/// `FnMut(&str, ExternalArg<'_>)`.
struct ArgsVisitor<F>(F);

impl<F> slog_2::Serializer for ArgsVisitor<F>
where
    F: for<'b> FnMut(&'b str, ExternalArg<'b>),
{
    #[inline]
    fn emit_arguments(
        &mut self,
        key: slog_2::Key,
        value: &core::fmt::Arguments<'_>,
    ) -> slog_2::Result {
        (self.0)(key, ExternalArg::Fmt(value));
        Ok(())
    }

    #[inline]
    fn emit_unit(&mut self, key: slog_2::Key) -> slog_2::Result {
        (self.0)(key, ExternalArg::Str("()"));
        Ok(())
    }

    #[inline]
    fn emit_none(&mut self, key: slog_2::Key) -> slog_2::Result {
        (self.0)(key, ExternalArg::Str("None"));
        Ok(())
    }

    #[inline]
    fn emit_char(&mut self, key: slog_2::Key, ch: char) -> slog_2::Result {
        (self.0)(key, ExternalArg::Char(ch));
        Ok(())
    }

    #[inline]
    fn emit_str(&mut self, key: slog_2::Key, value: &str) -> slog_2::Result {
        (self.0)(key, ExternalArg::Str(value));
        Ok(())
    }

    #[inline]
    fn emit_u8(&mut self, key: slog_2::Key, value: u8) -> slog_2::Result {
        (self.0)(key, ExternalArg::U8(value));
        Ok(())
    }

    #[inline]
    fn emit_i8(&mut self, key: slog_2::Key, value: i8) -> slog_2::Result {
        (self.0)(key, ExternalArg::I8(value));
        Ok(())
    }

    #[inline]
    fn emit_u16(&mut self, key: slog_2::Key, value: u16) -> slog_2::Result {
        (self.0)(key, ExternalArg::U16(value));
        Ok(())
    }

    #[inline]
    fn emit_i16(&mut self, key: slog_2::Key, value: i16) -> slog_2::Result {
        (self.0)(key, ExternalArg::I16(value));
        Ok(())
    }

    #[inline]
    fn emit_u32(&mut self, key: slog_2::Key, value: u32) -> slog_2::Result {
        (self.0)(key, ExternalArg::U32(value));
        Ok(())
    }

    #[inline]
    fn emit_i32(&mut self, key: slog_2::Key, value: i32) -> slog_2::Result {
        (self.0)(key, ExternalArg::I32(value));
        Ok(())
    }

    #[inline]
    fn emit_u64(&mut self, key: slog_2::Key, value: u64) -> slog_2::Result {
        (self.0)(key, ExternalArg::U64(value));
        Ok(())
    }

    #[inline]
    fn emit_i64(&mut self, key: slog_2::Key, value: i64) -> slog_2::Result {
        (self.0)(key, ExternalArg::I64(value));
        Ok(())
    }

    #[inline]
    fn emit_u128(&mut self, key: slog_2::Key, value: u128) -> slog_2::Result {
        (self.0)(key, ExternalArg::U128(value));
        Ok(())
    }

    #[inline]
    fn emit_i128(&mut self, key: slog_2::Key, value: i128) -> slog_2::Result {
        (self.0)(key, ExternalArg::I128(value));
        Ok(())
    }

    #[inline]
    fn emit_usize(&mut self, key: slog_2::Key, value: usize) -> slog_2::Result {
        (self.0)(key, ExternalArg::Usize(value));
        Ok(())
    }

    #[inline]
    fn emit_isize(&mut self, key: slog_2::Key, value: isize) -> slog_2::Result {
        (self.0)(key, ExternalArg::Isize(value));
        Ok(())
    }

    #[inline]
    fn emit_f32(&mut self, key: slog_2::Key, value: f32) -> slog_2::Result {
        (self.0)(key, ExternalArg::F32(value));
        Ok(())
    }

    #[inline]
    fn emit_f64(&mut self, key: slog_2::Key, value: f64) -> slog_2::Result {
        (self.0)(key, ExternalArg::F64(value));
        Ok(())
    }

    #[inline]
    fn emit_error(&mut self, key: slog_2::Key, value: &(dyn Error + 'static)) -> slog_2::Result {
        (self.0)(key, ExternalArg::Error(value));
        Ok(())
    }
}
