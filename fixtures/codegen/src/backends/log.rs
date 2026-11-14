#[cfg(any(feature = "write", feature = "send"))]
use core::cell::RefCell;
#[cfg(any(feature = "write", feature = "send"))]
use core::fmt::Write;
#[cfg(any(feature = "write", feature = "send"))]
use core::hint::black_box;
#[cfg(any(feature = "write", feature = "send"))]
use log::kv;
use log::{LevelFilter, Log, Metadata, Record};
#[cfg(feature = "write-stderr")]
use std::io::Write as _;
#[cfg(feature = "write")]
use std::time::SystemTime;

thread_local! {
    #[cfg(feature = "write")]
    static BUF: RefCell<String> = RefCell::new(String::with_capacity(512));

    #[cfg(feature = "send")]
    static EVENT: RefCell<Event> = const { RefCell::new(Event::None) };
}

pub struct LogLogger {
    pub max_level: LevelFilter,
}

impl LogLogger {
    #[inline]
    #[must_use]
    pub const fn new(max_level: LevelFilter) -> Self {
        Self { max_level }
    }

    #[inline]
    pub fn set_as_global_logger(self) {
        let logger = Box::leak(Box::new(self));
        log::set_logger(logger).unwrap();
        log::set_max_level(LevelFilter::Trace);
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
    Log {
        level: log::Level,
        target: String,
        module_path: Option<&'static str>,
        file: Option<&'static str>,
        line: Option<u32>,
        /// Formatted message.
        /// Unlike `polog`, `log`'s `fmt::Arguments<'_>` cannot be made
        /// `'static + Send` without formatting to `String`.
        args: String,
        fields: EventFields,
    },
    None,
}

#[cfg(feature = "send")]
#[derive(Clone, Default, Debug)]
struct EventFields {
    names: smallvec::SmallVec<[EventKey; 4]>,
    values: smallvec::SmallVec<[EventValue; 4]>,
    str_bundle: compact_str::CompactString,
}

#[cfg(feature = "send")]
#[expect(dead_code, reason = "read by the `black_box`")]
#[derive(Clone, Debug)]
pub enum EventKey {
    StaticStr(&'static str),
    OwnedStr { len: usize },
}

#[cfg(feature = "send")]
#[expect(dead_code, reason = "read by the `black_box`")]
#[derive(Clone, Debug)]
enum EventValue {
    Null,
    Bool(bool),
    Char(char),
    U64(u64),
    I64(i64),
    U128(u128),
    I128(i128),
    F64(f64),
    Str { len: usize },
    Error { len: usize },
    Display { len: usize },
}

impl Log for LogLogger {
    #[inline]
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.max_level
    }

    #[inline]
    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }

        #[cfg(feature = "send")]
        {
            let metadata = record.metadata();

            let mut args = String::with_capacity(64);
            write!(args, "{}", record.args()).expect("write to String");

            let mut fields = EventFields::default();
            let mut visitor = FieldSendVisitor(&mut fields);
            let _ = record.key_values().visit(&mut visitor).ok();

            EVENT.with_borrow_mut(|slot| {
                *slot = Event::Log {
                    level: record.level(),
                    target: metadata.target().to_owned(),
                    module_path: record.module_path_static(),
                    file: record.file_static(),
                    line: record.line(),
                    args,
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
                    "L {time} {} {} LOG {}",
                    record.level(),
                    record.module_path().unwrap_or(""),
                    record.args(),
                )
                .unwrap();

                let mut visitor = FieldWriteVisitor(buf);
                record.key_values().visit(&mut visitor).unwrap();
                writeln!(buf).unwrap();

                #[cfg(feature = "write-stderr")]
                let mut stderr = std::io::stderr();
                #[cfg(feature = "write-stderr")]
                stderr.write_all(buf.as_bytes()).unwrap();

                let _ = black_box(buf);
            });
        }
    }

    #[inline]
    fn flush(&self) {
        #[cfg(feature = "write-stderr")]
        {
            let mut stderr = std::io::stderr();
            stderr.flush().unwrap();
        }
    }
}

#[cfg(feature = "send")]
struct FieldSendVisitor<'a>(&'a mut EventFields);

#[cfg(feature = "send")]
impl<'a> kv::VisitSource<'a> for FieldSendVisitor<'_> {
    #[inline]
    fn visit_pair(&mut self, key: kv::Key<'a>, value: kv::Value<'a>) -> Result<(), kv::Error> {
        if let Some(key) = key.to_static_str() {
            self.0.names.push(EventKey::StaticStr(key));
        } else {
            self.0.names.push(EventKey::OwnedStr {
                len: key.as_str().len(),
            });
            self.0.str_bundle.push_str(key.as_str());
        }
        value.visit(self)
    }
}

#[cfg(feature = "send")]
impl<'a> kv::VisitValue<'a> for FieldSendVisitor<'_> {
    fn visit_any(&mut self, value: kv::Value<'_>) -> Result<(), kv::Error> {
        let len_before = self.0.str_bundle.len();
        write!(&mut self.0.str_bundle, "{value}").unwrap();
        let len_after = self.0.str_bundle.len();

        self.0.values.push(EventValue::Display {
            len: len_after.saturating_sub(len_before),
        });
        Ok(())
    }

    fn visit_error(&mut self, err: &(dyn core::error::Error + 'static)) -> Result<(), kv::Error> {
        let len_before = self.0.str_bundle.len();
        write!(&mut self.0.str_bundle, "{err}").unwrap();
        let len_after = self.0.str_bundle.len();

        self.0.values.push(EventValue::Error {
            len: len_after.saturating_sub(len_before),
        });
        Ok(())
    }

    fn visit_borrowed_error(
        &mut self,
        err: &'a (dyn core::error::Error + 'static),
    ) -> Result<(), kv::Error> {
        let len_before = self.0.str_bundle.len();
        write!(&mut self.0.str_bundle, "{err}").unwrap();
        let len_after = self.0.str_bundle.len();

        self.0.values.push(EventValue::Error {
            len: len_after.saturating_sub(len_before),
        });
        Ok(())
    }

    fn visit_null(&mut self) -> Result<(), kv::Error> {
        self.0.values.push(EventValue::Null);
        Ok(())
    }

    fn visit_bool(&mut self, value: bool) -> Result<(), kv::Error> {
        self.0.values.push(EventValue::Bool(value));
        Ok(())
    }

    fn visit_char(&mut self, value: char) -> Result<(), kv::Error> {
        self.0.values.push(EventValue::Char(value));
        Ok(())
    }

    fn visit_u64(&mut self, value: u64) -> Result<(), kv::Error> {
        self.0.values.push(EventValue::U64(value));
        Ok(())
    }

    fn visit_i64(&mut self, value: i64) -> Result<(), kv::Error> {
        self.0.values.push(EventValue::I64(value));
        Ok(())
    }

    fn visit_u128(&mut self, value: u128) -> Result<(), kv::Error> {
        self.0.values.push(EventValue::U128(value));
        Ok(())
    }

    fn visit_i128(&mut self, value: i128) -> Result<(), kv::Error> {
        self.0.values.push(EventValue::I128(value));
        Ok(())
    }

    fn visit_f64(&mut self, value: f64) -> Result<(), kv::Error> {
        self.0.values.push(EventValue::F64(value));
        Ok(())
    }

    fn visit_str(&mut self, value: &str) -> Result<(), kv::Error> {
        self.0.values.push(EventValue::Str { len: value.len() });
        self.0.str_bundle.push_str(value);
        Ok(())
    }

    fn visit_borrowed_str(&mut self, value: &'a str) -> Result<(), kv::Error> {
        self.0.values.push(EventValue::Str { len: value.len() });
        self.0.str_bundle.push_str(value);
        Ok(())
    }
}

#[cfg(feature = "write")]
struct FieldWriteVisitor<'a, W>(&'a mut W)
where
    W: Write;

#[cfg(feature = "write")]
impl<'a, W> kv::VisitSource<'a> for FieldWriteVisitor<'_, W>
where
    W: Write,
{
    #[inline]
    fn visit_pair(&mut self, key: kv::Key<'a>, value: kv::Value<'a>) -> Result<(), kv::Error> {
        write!(self.0, " {key}={value}").unwrap();
        Ok(())
    }
}
