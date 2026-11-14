use polog_util::{ExternalDescriptor, ExternalFilterFn};

use crate::{
    ExternalArg, ExternalArguments, ExternalEntry, Formatter, FormatterEventKind, KnownSpanId,
    MaybeKnownSpanId, SharedPologger,
};

impl<Fmt, Fff> log_0_4::Log for SharedPologger<Fmt, Fff>
where
    Fmt: Send + Formatter,
    Fff: Send + Sync + ExternalFilterFn,
{
    #[inline]
    fn enabled(&self, metadata: &log_0_4::Metadata<'_>) -> bool {
        let level = polog_util::conv::log_0_4::level_from_foreign(metadata.level());
        let descriptor = ExternalDescriptor {
            name: None,
            level,
            target: Some(metadata.target()),
            module_path: None,
            file: None,
            line: None,
            column: None,
        };
        self.filter().is_external_enabled(&descriptor)
    }

    #[inline]
    fn log(&self, record: &log_0_4::Record<'_>) {
        let level = polog_util::conv::log_0_4::level_from_foreign(record.metadata().level());
        let descriptor = ExternalDescriptor {
            name: None,
            level,
            target: Some(record.metadata().target()),
            module_path: record.module_path(),
            file: record.file(),
            line: record.line(),
            column: None,
        };

        // See <https://docs.rs/log/latest/log/trait.Log.html#tymethod.log>
        //
        // > This method isn’t called automatically by the log! macros.
        // > It can be called manually on shut-down to ensure any in-flight
        // > records are flushed.
        if self.filter().is_external_enabled(&descriptor) {
            let entry = ExternalEntry {
                descriptor: &descriptor,
                message_fmt: Some(record.args()),
                args_visitor: &Args(record.key_values()),
                parent_span_id: MaybeKnownSpanId::Unknown,
                span_id: KnownSpanId::Root,
                kind: FormatterEventKind::Log,
            };
            self.log_external_entry(&entry);
        }
    }

    #[inline]
    fn flush(&self) {
        self.flush_blocking_by_ref();
    }
}

/// `ExternalArguments` adapter over `log::kv::Source`.
struct Args<'a>(&'a dyn log_0_4::kv::Source);

impl ExternalArguments for Args<'_> {
    #[inline]
    fn visit_msg<F>(&self, _cb: F)
    where
        F: for<'b> FnOnce(ExternalArg<'b>),
    {
    }

    fn visit_args<F>(&self, cb: F)
    where
        F: for<'b> FnMut(&'b str, ExternalArg<'b>),
    {
        let mut visitor = ArgsVisitor(cb);
        let _ = self.0.visit(&mut visitor);
    }
}

struct ArgsVisitor<F>(F);

impl<'a, F> log_0_4::kv::VisitSource<'a> for ArgsVisitor<F>
where
    F: for<'b> FnMut(&'b str, ExternalArg<'b>),
{
    #[inline]
    fn visit_pair(
        &mut self,
        key: log_0_4::kv::Key<'a>,
        value: log_0_4::kv::Value<'a>,
    ) -> Result<(), log_0_4::kv::Error> {
        self.0(key.as_str(), ExternalArg::Debug(&value));
        Ok(())
    }
}
