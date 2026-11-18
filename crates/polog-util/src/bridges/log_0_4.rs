//! Bridge from `log` 0.4.x to `polog`.

use polog::SharedHandler;

use crate::util::MaybeStaticStr;
use crate::{
    ExternalArgValue, ExternalArgs, ExternalArgsVisitor, ExternalDescriptor, ExternalHandler,
    ExternalMessage, conv,
};

/// A [`log::Log`] adapter over a [`polog::Handler`] backend.
///
/// Forwards `log` events into a `polog` handler chain.
///
/// Register a `LogBridge` as the global `log` logger to receive
/// `log!` macros from the whole process into a `polog` backend.
///
/// [`polog::Handler`]: polog::Handler
/// [`log::Log`]: log_0_4::Log
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct LogBridge<H> {
    handler: H,
}

impl<H> LogBridge<H> {
    /// Creates a new bridge from `log` 0.4.x to `polog`.
    #[inline]
    #[must_use]
    pub const fn new(handler: H) -> Self {
        Self { handler }
    }

    /// Returns a reference to the wrapped handler.
    #[inline]
    #[must_use]
    pub const fn handler(&self) -> &H {
        &self.handler
    }

    /// Returns a mutable reference to the wrapped handler.
    #[inline]
    #[must_use]
    pub const fn handler_mut(&mut self) -> &mut H {
        &mut self.handler
    }

    /// Consumes the middleware, returning the wrapped handler.
    #[inline]
    #[must_use]
    pub fn into_handler(self) -> H {
        self.handler
    }

    /// Consumes and registers this `LogBridge` as the process-wide `log`
    /// backend.
    ///
    /// This method does not update `log` max-level filter.
    ///
    /// # Errors
    ///
    /// Returns the same error as `log::set_boxed_logger`: the global
    /// logger has already been set.
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn try_install_global(self) -> Result<(), log_0_4::SetLoggerError>
    where
        H: SharedHandler<()> + ExternalHandler<()>,
    {
        use alloc::boxed::Box;

        let max_level = self.handler.max_level_external();
        let max_level = conv::log_0_4::level_filter_into_foreign_approx(max_level);
        log_0_4::set_max_level(max_level);
        log_0_4::set_boxed_logger(Box::new(self))
    }
}

impl<H> log_0_4::Log for LogBridge<H>
where
    H: SharedHandler<()> + ExternalHandler<()>,
{
    #[inline]
    fn enabled(&self, metadata: &log_0_4::Metadata<'_>) -> bool {
        H::try_enable_external(&self.handler, metadata, ()).is_some()
    }

    #[inline]
    fn log(&self, record: &log_0_4::Record<'_>) {
        // See <https://docs.rs/log/latest/log/trait.Log.html#tymethod.log>
        //
        // > This method isn't called automatically by the log! macros.
        // > It can be called manually on shut-down to ensure any in-flight
        // > records are flushed.
        if let Some(context) = H::try_enable_external(&self.handler, record, ()) {
            H::log_external(&mut self.handler.clone(), record, record, None, context);
        }
    }

    #[inline]
    fn flush(&self) {
        H::flush_blocking(&mut self.handler.clone());
    }
}

impl ExternalDescriptor for log_0_4::Metadata<'_> {
    #[inline]
    fn message(&self) -> Option<ExternalMessage<'_>> {
        None
    }

    #[inline]
    fn level(&self) -> polog::Level {
        conv::log_0_4::level_from_foreign(log_0_4::Metadata::level(self))
    }

    #[inline]
    fn target(&self) -> Option<MaybeStaticStr<'_>> {
        Some(MaybeStaticStr::Borrowed(log_0_4::Metadata::target(self)))
    }

    #[inline]
    fn module_path(&self) -> Option<MaybeStaticStr<'_>> {
        None
    }

    #[inline]
    fn file(&self) -> Option<MaybeStaticStr<'_>> {
        None
    }

    #[inline]
    fn line(&self) -> Option<u32> {
        None
    }

    #[inline]
    fn column(&self) -> Option<u32> {
        None
    }
}

impl ExternalDescriptor for log_0_4::Record<'_> {
    #[inline]
    fn message(&self) -> Option<ExternalMessage<'_>> {
        Some(ExternalMessage::Fmt(self.args()))
    }

    #[inline]
    fn level(&self) -> polog::Level {
        conv::log_0_4::level_from_foreign(log_0_4::Metadata::level(self.metadata()))
    }

    #[inline]
    fn target(&self) -> Option<MaybeStaticStr<'_>> {
        Some(MaybeStaticStr::Borrowed(log_0_4::Metadata::target(
            self.metadata(),
        )))
    }

    #[inline]
    fn module_path(&self) -> Option<MaybeStaticStr<'_>> {
        #[expect(clippy::manual_map, reason = "better readability")]
        #[expect(clippy::option_if_let_else, reason = "better readability")]
        if let Some(module_path) = log_0_4::Record::module_path_static(self) {
            Some(MaybeStaticStr::Static(module_path))
        } else if let Some(module_path) = log_0_4::Record::module_path(self) {
            Some(MaybeStaticStr::Borrowed(module_path))
        } else {
            None
        }
    }

    #[inline]
    fn file(&self) -> Option<MaybeStaticStr<'_>> {
        #[expect(clippy::manual_map, reason = "better readability")]
        #[expect(clippy::option_if_let_else, reason = "better readability")]
        if let Some(file) = log_0_4::Record::file_static(self) {
            Some(MaybeStaticStr::Static(file))
        } else if let Some(file) = log_0_4::Record::file(self) {
            Some(MaybeStaticStr::Borrowed(file))
        } else {
            None
        }
    }

    #[inline]
    fn line(&self) -> Option<u32> {
        log_0_4::Record::line(self)
    }

    #[inline]
    fn column(&self) -> Option<u32> {
        None
    }
}

impl ExternalArgs for log_0_4::Record<'_> {
    #[inline]
    fn visit<Visitor>(&self, visitor: &mut Visitor)
    where
        Visitor: ExternalArgsVisitor,
    {
        // `polog` does not support argument visitor failures.
        // Silently ignore errors.
        let _result = self.key_values().visit(&mut ArgsVisitor(visitor));
    }
}

struct ArgsVisitor<'a, V>(&'a mut V);

impl<'a, V> log_0_4::kv::VisitSource<'a> for ArgsVisitor<'_, V>
where
    V: ExternalArgsVisitor,
{
    #[inline]
    fn visit_pair(
        &mut self,
        key: log_0_4::kv::Key<'a>,
        value: log_0_4::kv::Value<'a>,
    ) -> Result<(), log_0_4::kv::Error> {
        let key = key.to_static_str().map_or_else(
            || MaybeStaticStr::Borrowed(key.as_str()),
            MaybeStaticStr::Static,
        );
        value.visit(ArgVisitor(key, self.0))
    }
}

struct ArgVisitor<'a, V>(MaybeStaticStr<'a>, &'a mut V);

impl<'a, V> log_0_4::kv::VisitValue<'a> for ArgVisitor<'a, V>
where
    V: ExternalArgsVisitor,
{
    #[inline]
    fn visit_any(&mut self, value: log_0_4::kv::Value<'_>) -> Result<(), log_0_4::kv::Error> {
        self.1.visit_arg(self.0, ExternalArgValue::Display(&value));
        Ok(())
    }

    #[inline]
    fn visit_null(&mut self) -> Result<(), log_0_4::kv::Error> {
        self.1.visit_arg(self.0, ExternalArgValue::None);
        Ok(())
    }

    #[inline]
    fn visit_u64(&mut self, value: u64) -> Result<(), log_0_4::kv::Error> {
        self.1.visit_arg(self.0, ExternalArgValue::U64(value));
        Ok(())
    }

    #[inline]
    fn visit_i64(&mut self, value: i64) -> Result<(), log_0_4::kv::Error> {
        self.1.visit_arg(self.0, ExternalArgValue::I64(value));
        Ok(())
    }

    #[inline]
    fn visit_u128(&mut self, value: u128) -> Result<(), log_0_4::kv::Error> {
        self.1.visit_arg(self.0, ExternalArgValue::U128(value));
        Ok(())
    }

    #[inline]
    fn visit_i128(&mut self, value: i128) -> Result<(), log_0_4::kv::Error> {
        self.1.visit_arg(self.0, ExternalArgValue::I128(value));
        Ok(())
    }

    #[inline]
    fn visit_f64(&mut self, value: f64) -> Result<(), log_0_4::kv::Error> {
        self.1.visit_arg(self.0, ExternalArgValue::F64(value));
        Ok(())
    }

    #[inline]
    fn visit_bool(&mut self, value: bool) -> Result<(), log_0_4::kv::Error> {
        self.1.visit_arg(self.0, ExternalArgValue::Bool(value));
        Ok(())
    }

    #[inline]
    fn visit_str(&mut self, value: &str) -> Result<(), log_0_4::kv::Error> {
        self.1.visit_arg(self.0, ExternalArgValue::Str(value));
        Ok(())
    }

    #[inline]
    fn visit_borrowed_str(&mut self, value: &'a str) -> Result<(), log_0_4::kv::Error> {
        self.1.visit_arg(self.0, ExternalArgValue::Str(value));
        Ok(())
    }

    #[inline]
    fn visit_char(&mut self, value: char) -> Result<(), log_0_4::kv::Error> {
        self.1.visit_arg(self.0, ExternalArgValue::Char(value));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::bridges::log_0_4::LogBridge;

    #[test]
    fn test_log_bridge_accessors() {
        let mut enricher = LogBridge::new(123);
        assert_eq!(enricher.handler(), &123);
        assert_eq!(enricher.handler_mut(), &123);
        assert_eq!(enricher.into_handler(), 123);
    }
}
