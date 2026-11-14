use core::cell::RefCell;
#[cfg(any(feature = "write", feature = "send"))]
use core::fmt::{self, Write};
#[cfg(any(feature = "write", feature = "send"))]
use core::hint::black_box;
use core::sync::atomic::{AtomicU64, Ordering};
use std::collections::HashMap;
#[cfg(all(feature = "write-stderr", feature = "write"))]
use std::io::Write as _;
use std::sync::Mutex;
#[cfg(feature = "write")]
use std::time::SystemTime;

use tracing::span::Id;
use tracing_core::LevelFilter;
#[cfg(any(feature = "write", feature = "send"))]
use tracing_core::field::{Field, Visit};
use tracing_core::span::{Attributes, Current, Record};
use tracing_core::{Metadata, Subscriber};

thread_local! {
    #[cfg(feature = "write")]
    static BUF: RefCell<String> = RefCell::new(String::with_capacity(512));

    #[cfg(feature = "send")]
    static EVENT: RefCell<Event> = const { RefCell::new(Event::None) };

    static SPAN_STACK: RefCell<Vec<SpanData>> = const { RefCell::new(Vec::new()) };
}

#[cfg(feature = "send")]
#[expect(dead_code, reason = "read by the `black_box`")]
#[expect(clippy::enum_variant_names, reason = "okay in fixture")]
#[derive(Debug)]
enum Event {
    None,
    Event {
        meta: &'static Metadata<'static>,
        fields: EventFields,
        parent: Option<u64>,
    },
    NewSpan {
        id: u64,
        meta: &'static Metadata<'static>,
        fields: EventFields,
        parent: Option<u64>,
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
    Bool(bool),
    Char(char),
    U64(u64),
    I64(i64),
    U128(u128),
    I128(i128),
    F64(f64),
    Str { len: usize },
    Bytes { len: usize },
    Debug { len: usize },
}

#[derive(Clone, Copy)]
struct SpanData {
    id: u64,
    meta: &'static Metadata<'static>,
}

/// Global per-span state (needed at `close` time, which may run on any thread).
struct SpanState {
    meta: &'static Metadata<'static>,
    refs: usize,
}

/// Optimized `tracing` backend.
///
/// Bypasses `tracing-subscriber` and writes fully formatted lines to `stderr`
/// through a thread-local buffer, one `write_all` per line.
pub struct TracingLogger {
    pub max_level: LevelFilter,
}

impl TracingLogger {
    #[inline]
    #[must_use]
    pub const fn new(max_level: LevelFilter) -> Self {
        Self { max_level }
    }

    /// Install as the global tracing subscriber.
    ///
    /// # Panics
    ///
    /// Panics if another global tracing subscriber is already installed.
    #[inline]
    pub fn set_as_global_logger(self) {
        let subscriber = SimpleTracingSubscriber {
            max_level: self.max_level,
            next_id: AtomicU64::new(1),
            spans: Mutex::new(HashMap::new()),
        };
        tracing::subscriber::set_global_default(subscriber)
            .expect("failed to install global tracing subscriber");
    }
}

struct SimpleTracingSubscriber {
    max_level: LevelFilter,
    next_id: AtomicU64,
    spans: Mutex<HashMap<u64, SpanState>>,
}

impl Subscriber for SimpleTracingSubscriber {
    #[inline]
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= &self.max_level
    }

    #[inline]
    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(self.max_level)
    }

    #[inline]
    fn new_span(&self, attrs: &Attributes<'_>) -> Id {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let meta = attrs.metadata();

        if meta.level() > &self.max_level {
            // Still return a fresh id so enter/exit/try_close have something
            // consistent to work with; we just don't track it.
            return Id::from_u64(id);
        }

        let parent = SPAN_STACK.with_borrow(|stack| stack.last().map(|s| s.id));

        #[cfg(feature = "send")]
        {
            let mut fields = EventFields::default();
            let mut visitor = FieldSendVisitor(&mut fields);
            attrs.record(&mut visitor);

            EVENT.with_borrow_mut(|slot| {
                *slot = Event::NewSpan {
                    id,
                    meta,
                    fields,
                    parent,
                };
                let _ = black_box(slot);
            });
        }

        #[cfg(feature = "write")]
        {
            BUF.with_borrow_mut(|buf| {
                buf.clear();
                fmt_meta(
                    buf,
                    *meta.level(),
                    meta.module_path().unwrap_or(""),
                    "ENTER",
                );
                write!(buf, " {}", meta.name()).unwrap();
                fmt_attrs(buf, attrs);
                fmt_spans(buf, &[parent, Some(id)]);
                buf.push('\n');

                #[cfg(feature = "write-stderr")]
                let mut stderr = std::io::stderr();
                #[cfg(feature = "write-stderr")]
                stderr.write_all(buf.as_bytes()).unwrap();

                let _ = black_box(buf);
            });
        }

        #[cfg(not(any(feature = "write", feature = "send")))]
        let _ = parent;

        let _prev = self
            .spans
            .lock()
            .unwrap()
            .insert(id, SpanState { meta, refs: 1 });

        Id::from_u64(id)
    }

    #[inline]
    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    #[inline]
    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    #[inline]
    fn event(&self, event: &tracing_core::Event<'_>) {
        let meta = event.metadata();
        if meta.level() > &self.max_level {
            return;
        }

        let parent = SPAN_STACK.with_borrow(|stack| stack.last().map(|s| s.id));

        #[cfg(feature = "send")]
        {
            let mut fields = EventFields::default();
            let mut visitor = FieldSendVisitor(&mut fields);
            event.record(&mut visitor);

            EVENT.with_borrow_mut(|slot| {
                *slot = Event::Event {
                    meta,
                    fields,
                    parent,
                };
                let _ = black_box(slot);
            });
        }

        #[cfg(feature = "write")]
        BUF.with_borrow_mut(|buf| {
            buf.clear();
            fmt_meta(buf, *meta.level(), meta.module_path().unwrap_or(""), "LOG");
            fmt_event_fields(buf, event);
            if let Some(id) = parent {
                write!(buf, " #{id}").unwrap();
            }
            buf.push('\n');

            #[cfg(feature = "write-stderr")]
            let mut stderr = std::io::stderr();
            #[cfg(feature = "write-stderr")]
            stderr.write_all(buf.as_bytes()).unwrap();

            let _ = black_box(buf);
        });

        #[cfg(not(any(feature = "write", feature = "send")))]
        let _ = parent;
    }

    #[inline]
    fn enter(&self, span: &Id) {
        let id = Id::clone(span).into_u64();
        let data = {
            let guard = self.spans.lock().unwrap();
            guard.get(&id).map(|state| SpanData {
                id,
                meta: state.meta,
            })
        };
        if let Some(data) = data {
            SPAN_STACK.with_borrow_mut(|stack| stack.push(data));
        }
    }

    #[inline]
    fn exit(&self, _span: &Id) {
        SPAN_STACK.with_borrow_mut(|stack| {
            let _popped = stack.pop();
        });
    }

    #[inline]
    fn current_span(&self) -> Current {
        SPAN_STACK.with_borrow(|stack| {
            stack.last().map_or_else(Current::none, |data| {
                Current::new(Id::from_u64(data.id), data.meta)
            })
        })
    }

    #[inline]
    fn clone_span(&self, id: &Id) -> Id {
        let id_u64 = Id::clone(id).into_u64();
        if let Some(state) = self.spans.lock().unwrap().get_mut(&id_u64) {
            state.refs += 1;
        }
        Id::from_u64(id_u64)
    }

    fn try_close(&self, id: Id) -> bool {
        let id_u64 = id.into_u64();
        let state = {
            let mut guard = self.spans.lock().unwrap();
            let Some(state) = guard.get_mut(&id_u64) else {
                return true;
            };
            state.refs -= 1;
            if state.refs > 0 {
                return false;
            }
            guard.remove(&id_u64).unwrap()
        };

        #[cfg(not(feature = "write"))]
        let _ = state;

        #[cfg(feature = "write")]
        BUF.with_borrow_mut(|buf| {
            buf.clear();
            fmt_meta(
                buf,
                *state.meta.level(),
                state.meta.module_path().unwrap_or(""),
                "EXIT",
            );
            write!(buf, " {} #{id_u64}", state.meta.name()).unwrap();
            buf.push('\n');

            #[cfg(feature = "write-stderr")]
            let mut stderr = std::io::stderr();
            #[cfg(feature = "write-stderr")]
            stderr.write_all(buf.as_bytes()).unwrap();

            let _ = black_box(buf);
        });

        true
    }
}

#[cfg(feature = "send")]
struct FieldSendVisitor<'a>(&'a mut EventFields);

#[cfg(feature = "send")]
impl Visit for FieldSendVisitor<'_> {
    #[inline]
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0.names.push(field.name());
        let len_before = self.0.str_bundle.len();
        write!(&mut self.0.str_bundle, "{value:?}").unwrap();
        let len_after = self.0.str_bundle.len();

        self.0.values.push(EventValue::Str {
            len: len_after.saturating_sub(len_before),
        });
    }

    #[inline]
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.names.push(field.name());
        self.0.values.push(EventValue::Bool(value));
    }

    #[inline]
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.names.push(field.name());
        self.0.values.push(EventValue::U64(value));
    }

    #[inline]
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.names.push(field.name());
        self.0.values.push(EventValue::I64(value));
    }

    #[inline]
    fn record_u128(&mut self, field: &Field, value: u128) {
        self.0.names.push(field.name());
        self.0.values.push(EventValue::U128(value));
    }

    #[inline]
    fn record_i128(&mut self, field: &Field, value: i128) {
        self.0.names.push(field.name());
        self.0.values.push(EventValue::I128(value));
    }

    #[inline]
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.names.push(field.name());
        self.0.values.push(EventValue::F64(value));
    }

    #[inline]
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.names.push(field.name());
        self.0.values.push(EventValue::Str { len: value.len() });
        self.0.str_bundle.push_str(value);
    }

    #[inline]
    fn record_bytes(&mut self, field: &Field, value: &[u8]) {
        self.0.names.push(field.name());
        self.0.values.push(EventValue::Bytes { len: value.len() });
        self.0.bytes_bundle.extend_from_slice(value);
    }
}

#[cfg(feature = "write")]
struct FieldWriteVisitor<'a>(&'a mut String);

#[cfg(feature = "write")]
impl Visit for FieldWriteVisitor<'_> {
    #[inline]
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            let _ = write!(self.0, " {value}");
        } else {
            let _ = write!(self.0, " {}={value}", field.name());
        }
    }

    #[inline]
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, " {value:?}");
        } else {
            let _ = write!(self.0, " {}={value:?}", field.name());
        }
    }
}

#[cfg(feature = "write")]
#[inline]
fn fmt_meta(buf: &mut String, level: tracing::Level, module: &str, kind: &str) {
    let time = humantime::format_rfc3339_micros(SystemTime::now());
    write!(buf, "T {time} {level} {module} {kind}").unwrap();
}

#[cfg(feature = "write")]
#[inline]
fn fmt_event_fields(buf: &mut String, event: &tracing_core::Event<'_>) {
    let mut visitor = FieldWriteVisitor(buf);
    event.record(&mut visitor);
}

#[cfg(feature = "write")]
#[inline]
fn fmt_attrs(buf: &mut String, attrs: &Attributes<'_>) {
    let mut visitor = FieldWriteVisitor(buf);
    attrs.record(&mut visitor);
}

#[cfg(feature = "write")]
#[inline]
fn fmt_spans(buf: &mut String, span_ids: &[Option<u64>]) {
    for span_id in span_ids.iter().flatten() {
        write!(buf, " #{span_id}").unwrap();
    }
}
