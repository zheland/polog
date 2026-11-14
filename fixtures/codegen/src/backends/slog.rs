#[cfg(any(feature = "write", feature = "send"))]
use core::cell::RefCell;
#[cfg(any(feature = "write", feature = "send"))]
use core::fmt::{self, Write};
#[cfg(any(feature = "write", feature = "send"))]
use core::hint::black_box;
#[cfg(all(feature = "write-stderr", feature = "write"))]
use std::io::Write as _;
#[cfg(feature = "write")]
use std::time::SystemTime;

use slog::{Drain, Level, OwnedKVList, Record};
#[cfg(any(feature = "write", feature = "send"))]
use slog::{Key, Serializer, KV};

thread_local! {
    #[cfg(feature = "write")]
    static BUF: RefCell<String> = RefCell::new(String::with_capacity(512));

    #[cfg(feature = "send")]
    static EVENT: RefCell<Event> = const { RefCell::new(Event::None) };
}

pub struct SlogLogger {
    pub max_level: Option<Level>,
}

impl SlogLogger {
    #[inline]
    #[must_use]
    pub const fn new(max_level: Option<Level>) -> Self {
        Self { max_level }
    }
}

#[cfg(feature = "send")]
#[expect(dead_code, reason = "read by the `black_box`")]
#[expect(
    clippy::large_enum_variant,
    reason = "`None` variant is used for each backend"
)]
#[derive(Debug)]
enum Event {
    None,
    Log {
        level: Level,
        module: &'static str,
        /// Formatted message.
        /// `Record::msg()` borrows the record.
        /// Sending requires owning it.
        msg: String,
        fields: EventFields,
    },
}

#[cfg(feature = "send")]
#[derive(Clone, Default, Debug)]
struct EventFields {
    names: smallvec::SmallVec<[&'static str; 4]>,
    values: smallvec::SmallVec<[EventValue; 4]>,
    str_bundle: compact_str::CompactString,
    bytes_bundle: smallvec::SmallVec<[u8; 8]>,
}

#[cfg(feature = "send")]
#[expect(dead_code, reason = "read by the `black_box`")]
#[derive(Clone, Debug)]
enum EventValue {
    Unit,
    None,
    Bool(bool),
    Char(char),
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    U64(u64),
    I64(i64),
    U128(u128),
    I128(i128),
    Usize(usize),
    Isize(isize),
    F32(f32),
    F64(f64),
    Str { len: usize },
    Bytes { len: usize },
    Error { len: usize },
    Display { len: usize },
}

impl Drain for SlogLogger {
    type Ok = ();
    type Err = !;

    #[inline]
    fn is_enabled(&self, level: Level) -> bool {
        self.max_level.is_none_or(|max_level| level <= max_level)
    }

    fn log(&self, record: &Record<'_>, values: &OwnedKVList) -> Result<(), Self::Err> {
        #[cfg(feature = "send")]
        {
            let mut fields = EventFields::default();
            let mut ser = FieldSendSerializer(&mut fields);
            record.kv().serialize(record, &mut ser).unwrap();
            values.serialize(record, &mut ser).unwrap();

            EVENT.with_borrow_mut(|slot| {
                *slot = Event::Log {
                    level: record.level(),
                    module: record.module(),
                    msg: record.msg().to_string(),
                    fields,
                };
                let _ = black_box(slot);
            });
        }

        #[cfg(feature = "write")]
        {
            let time = humantime::format_rfc3339_micros(SystemTime::now());

            BUF.with_borrow_mut(|buf| {
                buf.clear();
                write!(
                    buf,
                    "S {time} {} {} LOG {}",
                    record.level(),
                    record.module(),
                    record.msg(),
                )
                .unwrap();

                let mut ser = FieldWriteSerializer(buf);
                record.kv().serialize(record, &mut ser).unwrap();
                values.serialize(record, &mut ser).unwrap();
                writeln!(buf).unwrap();

                #[cfg(feature = "write-stderr")]
                let mut stderr = std::io::stderr();
                #[cfg(feature = "write-stderr")]
                stderr.write_all(buf.as_bytes()).unwrap();

                let _ = black_box(buf);
            });
        }

        #[cfg(not(any(feature = "write", feature = "send")))]
        let _ = record;
        #[cfg(not(any(feature = "write", feature = "send")))]
        let _ = values;

        Ok(())
    }
}

#[cfg(feature = "send")]
struct FieldSendSerializer<'a>(&'a mut EventFields);

#[cfg(feature = "send")]
impl Serializer for FieldSendSerializer<'_> {
    #[inline]
    fn emit_arguments(&mut self, key: Key, val: &fmt::Arguments<'_>) -> slog::Result {
        self.0.names.push(key);
        let len_before = self.0.str_bundle.len();
        write!(&mut self.0.str_bundle, "{val}").unwrap();
        let len_after = self.0.str_bundle.len();

        self.0.values.push(EventValue::Display {
            len: len_after.saturating_sub(len_before),
        });
        Ok(())
    }

    #[inline]
    fn emit_error(&mut self, key: Key, error: &(dyn core::error::Error + 'static)) -> slog::Result {
        self.0.names.push(key);
        let len_before = self.0.str_bundle.len();
        write!(&mut self.0.str_bundle, "{error}").unwrap();
        let len_after = self.0.str_bundle.len();

        self.0.values.push(EventValue::Error {
            len: len_after.saturating_sub(len_before),
        });
        Ok(())
    }

    #[inline]
    fn emit_unit(&mut self, key: Key) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::Unit);
        Ok(())
    }

    #[inline]
    fn emit_none(&mut self, key: Key) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::None);
        Ok(())
    }

    #[inline]
    fn emit_bool(&mut self, key: Key, val: bool) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::Bool(val));
        Ok(())
    }

    #[inline]
    fn emit_char(&mut self, key: Key, val: char) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::Char(val));
        Ok(())
    }

    #[inline]
    fn emit_u8(&mut self, key: Key, val: u8) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::U8(val));
        Ok(())
    }

    #[inline]
    fn emit_i8(&mut self, key: Key, val: i8) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::I8(val));
        Ok(())
    }

    #[inline]
    fn emit_u16(&mut self, key: Key, val: u16) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::U16(val));
        Ok(())
    }

    #[inline]
    fn emit_i16(&mut self, key: Key, val: i16) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::I16(val));
        Ok(())
    }

    #[inline]
    fn emit_u32(&mut self, key: Key, val: u32) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::U32(val));
        Ok(())
    }

    #[inline]
    fn emit_i32(&mut self, key: Key, val: i32) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::I32(val));
        Ok(())
    }

    #[inline]
    fn emit_u64(&mut self, key: Key, val: u64) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::U64(val));
        Ok(())
    }

    #[inline]
    fn emit_i64(&mut self, key: Key, val: i64) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::I64(val));
        Ok(())
    }

    #[inline]
    fn emit_u128(&mut self, key: Key, val: u128) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::U128(val));
        Ok(())
    }

    #[inline]
    fn emit_i128(&mut self, key: Key, val: i128) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::I128(val));
        Ok(())
    }

    #[inline]
    fn emit_usize(&mut self, key: Key, val: usize) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::Usize(val));
        Ok(())
    }

    #[inline]
    fn emit_isize(&mut self, key: Key, val: isize) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::Isize(val));
        Ok(())
    }

    #[inline]
    fn emit_f32(&mut self, key: Key, val: f32) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::F32(val));
        Ok(())
    }

    #[inline]
    fn emit_f64(&mut self, key: Key, val: f64) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::F64(val));
        Ok(())
    }

    #[inline]
    fn emit_str(&mut self, key: Key, val: &str) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::Str { len: val.len() });
        self.0.str_bundle.push_str(val);
        Ok(())
    }

    #[inline]
    fn emit_bytes(&mut self, key: Key, bytes: &[u8], _kind: slog::BytesKind) -> slog::Result {
        self.0.names.push(key);
        self.0.values.push(EventValue::Bytes { len: bytes.len() });
        self.0.bytes_bundle.extend_from_slice(bytes);
        Ok(())
    }
}

#[cfg(feature = "write")]
struct FieldWriteSerializer<'a, W: Write>(&'a mut W);

#[cfg(feature = "write")]
impl<W: Write> Serializer for FieldWriteSerializer<'_, W> {
    fn emit_arguments(&mut self, key: Key, value: &fmt::Arguments<'_>) -> slog::Result {
        write!(self.0, " {key}={value}").unwrap();
        Ok(())
    }
}
