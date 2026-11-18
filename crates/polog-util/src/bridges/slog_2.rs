//! Bridge from `slog` 2.x to `polog`.

use core::error::Error;

use polog::SharedHandler;
use slog_2::KV;

use crate::util::MaybeStaticStr;
use crate::{
    ExternalArgValue, ExternalArgs, ExternalArgsVisitor, ExternalDescriptor, ExternalHandler,
    ExternalMessage, conv,
};

/// A [`slog::Drain`] adapter over a [`polog::Handler`] backend.
///
/// Forwards `slog` events into a `polog` handler chain.
///
/// [`polog::Handler`]: polog::Handler
/// [`slog::Drain`]: slog_2::Drain
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct SlogBridge<H> {
    handler: H,
}

impl<H> SlogBridge<H> {
    /// Creates a new bridge from `slog` 2.x to `polog`.
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

    /// Creates a `slog` logger that drains into this handler.
    ///
    /// The returned `slog::Logger` can be passed to any code that expects a
    /// `slog::Logger`.
    #[inline]
    pub fn into_slog_logger(self) -> slog_2::Logger
    where
        H: SharedHandler<()> + ExternalHandler<()>,
    {
        slog_2::Logger::root(self, slog_2::o!())
    }
}

impl<H> slog_2::Drain for SlogBridge<H>
where
    H: SharedHandler<()> + ExternalHandler<()>,
{
    type Ok = ();
    type Err = slog_2::Never;

    #[inline]
    fn log(
        &self,
        record: &slog_2::Record<'_>,
        values: &slog_2::OwnedKVList,
    ) -> Result<Self::Ok, Self::Err> {
        if let Some(context) = H::try_enable_external(&self.handler, record, ()) {
            H::log_external(
                &mut self.handler.clone(),
                record,
                &(record, values),
                None,
                context,
            );
        }
        Ok(())
    }

    #[inline]
    fn flush(&self) -> Result<(), slog_2::FlushError> {
        H::flush_blocking(&mut self.handler.clone());
        Ok(())
    }

    #[inline]
    fn is_enabled(&self, level: slog_2::Level) -> bool {
        H::try_enable_external(&self.handler, &level, ()).is_some()
    }
}

impl ExternalDescriptor for slog_2::Level {
    #[inline]
    fn message(&self) -> Option<ExternalMessage<'_>> {
        None
    }

    #[inline]
    fn level(&self) -> polog::Level {
        conv::slog_2::level_from_foreign_approx(*self)
    }

    #[inline]
    fn target(&self) -> Option<MaybeStaticStr<'_>> {
        None
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

impl ExternalDescriptor for slog_2::Record<'_> {
    #[inline]
    fn message(&self) -> Option<ExternalMessage<'_>> {
        Some(ExternalMessage::Fmt(slog_2::Record::msg(self)))
    }

    #[inline]
    fn level(&self) -> polog::Level {
        conv::slog_2::level_from_foreign_approx(slog_2::Record::level(self))
    }

    #[inline]
    fn target(&self) -> Option<MaybeStaticStr<'_>> {
        let tag = slog_2::Record::tag(self);
        if tag.is_empty() {
            self.module_path()
        } else {
            Some(MaybeStaticStr::Borrowed(tag))
        }
    }

    #[inline]
    fn module_path(&self) -> Option<MaybeStaticStr<'_>> {
        Some(MaybeStaticStr::Static(slog_2::Record::module(self)))
    }

    #[inline]
    fn file(&self) -> Option<MaybeStaticStr<'_>> {
        Some(MaybeStaticStr::Static(slog_2::Record::file(self)))
    }

    #[inline]
    fn line(&self) -> Option<u32> {
        Some(slog_2::Record::line(self))
    }

    #[inline]
    fn column(&self) -> Option<u32> {
        Some(slog_2::Record::column(self))
    }
}

impl ExternalArgs for (&slog_2::Record<'_>, &slog_2::OwnedKVList) {
    #[inline]
    fn visit<Visitor>(&self, visitor: &mut Visitor)
    where
        Visitor: ExternalArgsVisitor,
    {
        let mut visitor = ArgsVisitor(visitor);
        // `polog` does not support argument visitor failures.
        // Silently ignore errors.
        let _ = slog_2::BorrowedKV::serialize(&self.0.kv(), self.0, &mut visitor);
        let _ = slog_2::OwnedKVList::serialize(self.1, self.0, &mut visitor);
    }
}

struct ArgsVisitor<'a, V>(&'a mut V);

impl<V> slog_2::Serializer for ArgsVisitor<'_, V>
where
    V: ExternalArgsVisitor,
{
    #[inline]
    fn emit_arguments(
        &mut self,
        key: slog_2::Key,
        value: &core::fmt::Arguments<'_>,
    ) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::Fmt(value));
        Ok(())
    }

    #[inline]
    fn emit_unit(&mut self, key: slog_2::Key) -> slog_2::Result {
        self.0.visit_arg(key_as_str(&key), ExternalArgValue::Unit);
        Ok(())
    }

    #[inline]
    fn emit_none(&mut self, key: slog_2::Key) -> slog_2::Result {
        self.0.visit_arg(key_as_str(&key), ExternalArgValue::None);
        Ok(())
    }

    #[inline]
    fn emit_char(&mut self, key: slog_2::Key, ch: char) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::Char(ch));
        Ok(())
    }

    #[inline]
    fn emit_str(&mut self, key: slog_2::Key, value: &str) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::Str(value));
        Ok(())
    }

    #[inline]
    fn emit_u8(&mut self, key: slog_2::Key, value: u8) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::U8(value));
        Ok(())
    }

    #[inline]
    fn emit_i8(&mut self, key: slog_2::Key, value: i8) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::I8(value));
        Ok(())
    }

    #[inline]
    fn emit_u16(&mut self, key: slog_2::Key, value: u16) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::U16(value));
        Ok(())
    }

    #[inline]
    fn emit_i16(&mut self, key: slog_2::Key, value: i16) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::I16(value));
        Ok(())
    }

    #[inline]
    fn emit_u32(&mut self, key: slog_2::Key, value: u32) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::U32(value));
        Ok(())
    }

    #[inline]
    fn emit_i32(&mut self, key: slog_2::Key, value: i32) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::I32(value));
        Ok(())
    }

    #[inline]
    fn emit_u64(&mut self, key: slog_2::Key, value: u64) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::U64(value));
        Ok(())
    }

    #[inline]
    fn emit_i64(&mut self, key: slog_2::Key, value: i64) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::I64(value));
        Ok(())
    }

    #[inline]
    fn emit_u128(&mut self, key: slog_2::Key, value: u128) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::U128(value));
        Ok(())
    }

    #[inline]
    fn emit_i128(&mut self, key: slog_2::Key, value: i128) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::I128(value));
        Ok(())
    }

    #[inline]
    fn emit_usize(&mut self, key: slog_2::Key, value: usize) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::Usize(value));
        Ok(())
    }

    #[inline]
    fn emit_isize(&mut self, key: slog_2::Key, value: isize) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::Isize(value));
        Ok(())
    }

    #[inline]
    fn emit_f32(&mut self, key: slog_2::Key, value: f32) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::F32(value));
        Ok(())
    }

    #[inline]
    fn emit_f64(&mut self, key: slog_2::Key, value: f64) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::F64(value));
        Ok(())
    }

    #[inline]
    fn emit_error(&mut self, key: slog_2::Key, value: &(dyn Error + 'static)) -> slog_2::Result {
        self.0
            .visit_arg(key_as_str(&key), ExternalArgValue::Error(value));
        Ok(())
    }
}

#[inline]
fn key_as_str(key: &slog_2::Key) -> MaybeStaticStr<'_> {
    // Unfortunately no API methods return static str.
    MaybeStaticStr::Borrowed(key.as_str())
}

#[cfg(test)]
mod tests {
    use crate::bridges::slog_2::SlogBridge;

    #[test]
    fn test_slog_bridge_accessors() {
        let mut enricher = SlogBridge::new(123);
        assert_eq!(enricher.handler(), &123);
        assert_eq!(enricher.handler_mut(), &123);
        assert_eq!(enricher.into_handler(), 123);
    }
}
