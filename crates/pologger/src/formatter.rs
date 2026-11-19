use core::fmt::Display;
use core::time::Duration;
use std::io;

use polog::{Level, SpanId};

/// Rendering backend for a [`Pologger`](crate::Pologger).
///
/// A `Formatter` describes how to write one event to some output sink.
/// It is driven by `Pologger`: `start_entry` is called once, then a sequence
/// of `write_*` calls for each field that the backend decides to render,
/// then `end_entry`.
/// The exact sequence depends on the `LoggerFlags` and on what fields the event
/// carries.
///
/// # Method families
///
/// - Lifecycle: [`start_entry`], [`end_entry`], [`flush_blocking`],
///   [`flush_on_panic`], [`update_config`].
/// - Structure: [`write_key`], [`begin_array`], [`end_array`],
///   [`begin_object`], [`end_object`].
/// - Leaf values: `write_time_already_sanitized`, `write_duration`, `write_id`,
///   `write_str`, `write_span_id`, `write_kind`, `write_span_path_kind`,
///   `write_span_path_span_id`, `write_span_path_span_name`, `write_level`,
///   `write_target`, `write_message`, `write_argument_key`,
///   `write_argument_value`.
///
/// # Sanitization
///
/// `write_message`, `write_argument_value`, `write_str`, `write_target`,
/// `write_argument_key`, and `write_span_path_span_name` carry
/// caller-supplied content.
/// Implementations are responsible for making that content safe for their
/// target: the reference `TextFormatter` replaces control characters with
/// control pictures.
/// The reference `NdJsonFormatter` relies on JSON escaping and documents the
/// residual risk.
///
/// The `*_already_sanitized` suffix on `write_time_already_sanitized` marks
/// a value that the caller has already made safe (typically a formatted
/// timestamp) and that implementations must not re-sanitize.
///
/// [`start_entry`]: Self::start_entry
/// [`end_entry`]: Self::end_entry
/// [`flush_blocking`]: Self::flush_blocking
/// [`flush_on_panic`]: Self::flush_on_panic
/// [`update_config`]: Self::update_config
/// [`write_key`]: Self::write_key
/// [`begin_array`]: Self::begin_array
/// [`end_array`]: Self::end_array
/// [`begin_object`]: Self::begin_object
/// [`end_object`]: Self::end_object
pub trait Formatter {
    /// Flushes any buffered output.
    ///
    /// # Errors
    ///
    /// Returns whatever `io::Write::flush` on the underlying sink returns.
    fn flush_blocking(&mut self) -> io::Result<()>;

    /// Flushes any buffered output on a failure path.
    ///
    /// Must not panic. Failures are dropped silently because the caller is
    /// already in a panic.
    fn flush_on_panic(&mut self);

    /// Re-reads the formatter's external flag set, if any.
    ///
    /// Called by the backend at the start of every entry, so that a runtime
    /// flag change propagates to the next event.
    fn update_config(&mut self);

    /// Begins a new entry.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn start_entry(&mut self) -> io::Result<()>;

    /// Ends the current entry.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn end_entry(&mut self) -> io::Result<()>;

    /// Writes a field key.
    ///
    /// For structural formatters (`NdJsonFormatter`), this emits the key
    /// prefix.
    /// For flat formatters (`TextFormatter`), this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_key(&mut self, key: &'static str) -> io::Result<()>;

    /// Begins a nested array.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn begin_array(&mut self) -> io::Result<()>;

    /// Ends a nested array.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn end_array(&mut self) -> io::Result<()>;

    /// Begins a nested object.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn begin_object(&mut self) -> io::Result<()>;

    /// Ends a nested object.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn end_object(&mut self) -> io::Result<()>;

    /// Writes a pre-sanitized time value.
    ///
    /// The value is expected to be free of control characters.
    /// Implementations must not re-sanitize it.
    /// The backend uses this method for formatted timestamps that originate
    /// from `jiff`, which emit only printable ASCII.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_time_already_sanitized<T>(
        &mut self,
        value: &T,
        kind: FormatterTimeKind,
    ) -> io::Result<()>
    where
        T: ?Sized + Display;

    /// Writes a duration with the given precision.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_duration(
        &mut self,
        duration: Duration,
        precision: TimePrecision,
        kind: FormatterDurationKind,
    ) -> io::Result<()>;

    /// Writes an integer identifier (process, thread, task, or CPU ID).
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_id(&mut self, id: u64, kind: FormatterIdKind) -> io::Result<()>;

    /// Writes a caller-supplied string.
    ///
    /// Implementations must sanitize or otherwise make the value safe for
    /// their target.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_str(&mut self, value: &str, kind: FormatterStrKind) -> io::Result<()>;

    /// Writes a span ID.
    ///
    /// `MaybeKnownSpanId::Unknown` means the backend could not determine
    /// the ID.
    /// Implementations typically render it as an opaque marker.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_span_id(
        &mut self,
        span_id: MaybeKnownSpanId,
        kind: FormatterSpanIdKind,
    ) -> io::Result<()>;

    /// Writes an event-kind glyph.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_kind(&mut self, kind: FormatterEventKind) -> io::Result<()>;

    /// Writes a span-path kind glyph.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_span_path_kind(&mut self, kind: FormatterEventKind) -> io::Result<()>;

    /// Writes a span-path span ID.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_span_path_span_id(&mut self, span_id: KnownSpanId) -> io::Result<()>;

    /// Writes a caller-supplied span name.
    ///
    /// Implementations must sanitize or otherwise make the value safe for
    /// their target.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_span_path_span_name(&mut self, name: &str) -> io::Result<()>;

    /// Writes a level label.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_level(&mut self, level: Level) -> io::Result<()>;

    /// Writes a caller-supplied target.
    ///
    /// Implementations must sanitize or otherwise make the value safe for
    /// their target.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_target(&mut self, target: &str, kind: FormatterEventKind) -> io::Result<()>;

    /// Writes a caller-supplied message.
    ///
    /// Implementations must sanitize or otherwise make the value safe for
    /// their target.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_message<T>(&mut self, message: &T, kind: FormatterEventKind) -> io::Result<()>
    where
        T: ?Sized + Display;

    /// Writes a caller-supplied argument key.
    ///
    /// Implementations must sanitize or otherwise make the value safe for
    /// their target.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_argument_key(&mut self, key: &str) -> io::Result<()>;

    /// Writes a caller-supplied argument value.
    ///
    /// Implementations must sanitize or otherwise make the value safe for
    /// their target.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_argument_value<T>(&mut self, value: &T) -> io::Result<()>
    where
        T: ?Sized + Display;

    /// Writes location-related argument keys and values.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying sink returns.
    fn write_location_arguments(
        &mut self,
        filename: Option<&str>,
        line: Option<u32>,
        column: Option<u32>,
    ) -> io::Result<()>;
}

/// The kind of event that a `Formatter` is rendering.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum FormatterEventKind {
    /// A log event.
    Log,
    /// A nested span entry.
    NestedEnter,
    /// A nested span exit.
    NestedExit,
    /// A detached span entry.
    DetachedEnter,
    /// A detached span exit.
    DetachedExit,
    /// An instrumented future poll started.
    TaskPoll,
    /// An instrumented future returned `Pending`.
    TaskPending,
    /// An instrumented future returned `Ready`.
    TaskReady,
    /// An external span creation.
    ExternalNew,
    /// An external span close.
    ExternalClose,
    /// An external span record.
    ExternalRecord,
    /// An external span link.
    ExternalLink,
}

/// Which kind of wall-clock time a `Formatter` is writing.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum FormatterTimeKind {
    /// UTC time.
    Utc = 0,
    /// Local time.
    Local = 1,
    /// UTC offset of the local timezone.
    UtcOffset = 2,
}

/// Which kind of duration a `Formatter` is writing.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum FormatterDurationKind {
    /// Monotonic time since a per-enricher reference point.
    Monotonic = 0,
    /// Total CPU time consumed by the process.
    ProcessCpu = 1,
    /// Total CPU time consumed by the current thread.
    ThreadCpu = 2,
}

/// Which kind of integer identifier a `Formatter` is writing.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum FormatterIdKind {
    /// OS process ID.
    ProcessId = 0,
    /// OS thread ID.
    ThreadId = 1,
    /// Async task ID.
    TaskId = 2,
    /// CPU core identifier.
    CpuId = 3,
}

/// Which kind of caller-supplied string a `Formatter` is writing.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum FormatterStrKind {
    /// OS process name.
    ProcessName = 0,
    /// OS thread name.
    ThreadName = 1,
    /// Async task name.
    TaskName = 2,
}

/// Which span-ID slot a `Formatter` is writing.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum FormatterSpanIdKind {
    /// The parent span ID column.
    ParentSpanId,
    /// The event's own span ID column.
    SpanId,
}

/// Precision for a duration or time value.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum TimePrecision {
    /// Whole seconds, no fractional part.
    Seconds,
    /// Millisecond precision (three fractional digits).
    Millis,
    /// Microsecond precision (six fractional digits).
    Micros,
    /// Nanosecond precision (nine fractional digits).
    Nanos,
}

impl From<KnownSpanId> for MaybeKnownSpanId {
    #[inline]
    fn from(value: KnownSpanId) -> Self {
        match value {
            KnownSpanId::Root => Self::Root,
            KnownSpanId::SpanId(span_id) => Self::SpanId(span_id),
        }
    }
}

/// A span ID that the backend is sure about.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum KnownSpanId {
    /// The event is not associated with any span.
    Root,
    /// The event is associated with the given span.
    SpanId(SpanId),
}

/// A span ID that may or may not be known to the backend.
///
/// Backends that cannot determine the parent span ID at the moment of
/// rendering use [`Unknown`] rather than fabricating a `Root` value.
///
/// [`Unknown`]: Self::Unknown
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum MaybeKnownSpanId {
    /// The backend does not know the span ID.
    Unknown,
    /// The event is not associated with any span.
    Root,
    /// The event is associated with the given span.
    SpanId(SpanId),
}

/// Forwards every `Formatter` method to the active variant.
#[cfg(feature = "either-1")]
impl<H1, H2> Formatter for either_1::Either<H1, H2>
where
    H1: Formatter,
    H2: Formatter,
{
    #[inline]
    fn flush_blocking(&mut self) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::flush_blocking(fmt),
            Self::Right(fmt) => H2::flush_blocking(fmt),
        }
    }

    #[inline]
    fn flush_on_panic(&mut self) {
        match self {
            Self::Left(fmt) => H1::flush_on_panic(fmt),
            Self::Right(fmt) => H2::flush_on_panic(fmt),
        }
    }

    #[inline]
    fn update_config(&mut self) {
        match self {
            Self::Left(fmt) => H1::update_config(fmt),
            Self::Right(fmt) => H2::update_config(fmt),
        }
    }

    #[inline]
    fn start_entry(&mut self) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::start_entry(fmt),
            Self::Right(fmt) => H2::start_entry(fmt),
        }
    }

    #[inline]
    fn end_entry(&mut self) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::end_entry(fmt),
            Self::Right(fmt) => H2::end_entry(fmt),
        }
    }

    #[inline]
    fn write_key(&mut self, key: &'static str) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::write_key(fmt, key),
            Self::Right(fmt) => H2::write_key(fmt, key),
        }
    }

    #[inline]
    fn begin_array(&mut self) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::begin_array(fmt),
            Self::Right(fmt) => H2::begin_array(fmt),
        }
    }

    #[inline]
    fn end_array(&mut self) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::end_array(fmt),
            Self::Right(fmt) => H2::end_array(fmt),
        }
    }

    #[inline]
    fn begin_object(&mut self) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::begin_object(fmt),
            Self::Right(fmt) => H2::begin_object(fmt),
        }
    }

    #[inline]
    fn end_object(&mut self) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::end_object(fmt),
            Self::Right(fmt) => H2::end_object(fmt),
        }
    }

    #[inline]
    fn write_time_already_sanitized<T>(
        &mut self,
        value: &T,
        kind: FormatterTimeKind,
    ) -> io::Result<()>
    where
        T: ?Sized + Display,
    {
        match self {
            Self::Left(fmt) => H1::write_time_already_sanitized(fmt, value, kind),
            Self::Right(fmt) => H2::write_time_already_sanitized(fmt, value, kind),
        }
    }

    #[inline]
    fn write_duration(
        &mut self,
        duration: Duration,
        precision: TimePrecision,
        kind: FormatterDurationKind,
    ) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::write_duration(fmt, duration, precision, kind),
            Self::Right(fmt) => H2::write_duration(fmt, duration, precision, kind),
        }
    }

    #[inline]
    fn write_id(&mut self, id: u64, kind: FormatterIdKind) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::write_id(fmt, id, kind),
            Self::Right(fmt) => H2::write_id(fmt, id, kind),
        }
    }

    #[inline]
    fn write_str(&mut self, value: &str, kind: FormatterStrKind) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::write_str(fmt, value, kind),
            Self::Right(fmt) => H2::write_str(fmt, value, kind),
        }
    }

    #[inline]
    fn write_span_id(
        &mut self,
        span_id: MaybeKnownSpanId,
        kind: FormatterSpanIdKind,
    ) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::write_span_id(fmt, span_id, kind),
            Self::Right(fmt) => H2::write_span_id(fmt, span_id, kind),
        }
    }

    #[inline]
    fn write_kind(&mut self, kind: FormatterEventKind) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::write_kind(fmt, kind),
            Self::Right(fmt) => H2::write_kind(fmt, kind),
        }
    }

    #[inline]
    fn write_span_path_kind(&mut self, kind: FormatterEventKind) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::write_span_path_kind(fmt, kind),
            Self::Right(fmt) => H2::write_span_path_kind(fmt, kind),
        }
    }

    #[inline]
    fn write_span_path_span_id(&mut self, span_id: KnownSpanId) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::write_span_path_span_id(fmt, span_id),
            Self::Right(fmt) => H2::write_span_path_span_id(fmt, span_id),
        }
    }

    #[inline]
    fn write_span_path_span_name(&mut self, name: &str) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::write_span_path_span_name(fmt, name),
            Self::Right(fmt) => H2::write_span_path_span_name(fmt, name),
        }
    }

    #[inline]
    fn write_level(&mut self, level: Level) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::write_level(fmt, level),
            Self::Right(fmt) => H2::write_level(fmt, level),
        }
    }

    #[inline]
    fn write_target(&mut self, target: &str, kind: FormatterEventKind) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::write_target(fmt, target, kind),
            Self::Right(fmt) => H2::write_target(fmt, target, kind),
        }
    }

    #[inline]
    fn write_message<T>(&mut self, message: &T, kind: FormatterEventKind) -> io::Result<()>
    where
        T: ?Sized + Display,
    {
        match self {
            Self::Left(fmt) => H1::write_message(fmt, message, kind),
            Self::Right(fmt) => H2::write_message(fmt, message, kind),
        }
    }

    #[inline]
    fn write_argument_key(&mut self, key: &str) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::write_argument_key(fmt, key),
            Self::Right(fmt) => H2::write_argument_key(fmt, key),
        }
    }

    #[inline]
    fn write_argument_value<T>(&mut self, value: &T) -> io::Result<()>
    where
        T: ?Sized + Display,
    {
        match self {
            Self::Left(fmt) => H1::write_argument_value(fmt, value),
            Self::Right(fmt) => H2::write_argument_value(fmt, value),
        }
    }

    #[inline]
    fn write_location_arguments(
        &mut self,
        filename: Option<&str>,
        line: Option<u32>,
        column: Option<u32>,
    ) -> io::Result<()> {
        match self {
            Self::Left(fmt) => H1::write_location_arguments(fmt, filename, line, column),
            Self::Right(fmt) => H2::write_location_arguments(fmt, filename, line, column),
        }
    }
}

impl Formatter for () {
    #[inline]
    fn flush_blocking(&mut self) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn flush_on_panic(&mut self) {}

    #[inline]
    fn update_config(&mut self) {}

    #[inline]
    fn start_entry(&mut self) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn end_entry(&mut self) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn write_key(&mut self, _key: &'static str) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn begin_array(&mut self) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn end_array(&mut self) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn begin_object(&mut self) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn end_object(&mut self) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn write_time_already_sanitized<T>(
        &mut self,
        _value: &T,
        _kind: FormatterTimeKind,
    ) -> io::Result<()>
    where
        T: ?Sized + Display,
    {
        Ok(())
    }

    #[inline]
    fn write_duration(
        &mut self,
        _duration: Duration,
        _precision: TimePrecision,
        _kind: FormatterDurationKind,
    ) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn write_id(&mut self, _id: u64, _kind: FormatterIdKind) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn write_str(&mut self, _value: &str, _kind: FormatterStrKind) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn write_span_id(
        &mut self,
        _span_id: MaybeKnownSpanId,
        _kind: FormatterSpanIdKind,
    ) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn write_kind(&mut self, _kind: FormatterEventKind) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn write_span_path_kind(&mut self, _kind: FormatterEventKind) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn write_span_path_span_id(&mut self, _span_id: KnownSpanId) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn write_span_path_span_name(&mut self, _name: &str) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn write_level(&mut self, _level: Level) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn write_target(&mut self, _target: &str, _kind: FormatterEventKind) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn write_message<T>(&mut self, _message: &T, _kind: FormatterEventKind) -> io::Result<()>
    where
        T: ?Sized + Display,
    {
        Ok(())
    }

    #[inline]
    fn write_argument_key(&mut self, _key: &str) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn write_argument_value<T>(&mut self, _value: &T) -> io::Result<()>
    where
        T: ?Sized + Display,
    {
        Ok(())
    }

    #[inline]
    fn write_location_arguments(
        &mut self,
        _filename: Option<&str>,
        _line: Option<u32>,
        _column: Option<u32>,
    ) -> io::Result<()> {
        Ok(())
    }
}

macro_rules! impl_variadics {
    ( [ $( $args:tt )* ] [] ) => {};
    ( [ $( $args:tt )* ] [ $next:tt $( $rest:tt )* ] ) => {
        impl_variadic!( $( $args )* $next );
        impl_variadics!( [ $( $args )* $next ] [ $( $rest )* ] );
    };
}

macro_rules! impl_variadic {
    ( $( ( $field:tt $type:ident ) )* ) => {
        /// Writes every event to both formatters, in order.
        ///
        /// If the first formatter returns an error, the second is not called and the
        /// error is propagated.
        impl<$( $type ),*> Formatter for  ( $( $type ),*, )
        where
            $( $type: Formatter, )*
        {
            #[inline]
            fn flush_blocking(&mut self) -> io::Result<()> {
                $( $type::flush_blocking(&mut self.$field)?; )*
                Ok(())
            }

            #[inline]
            fn flush_on_panic(&mut self) {
                $( $type::flush_on_panic(&mut self.$field); )*
            }

            #[inline]
            fn update_config(&mut self) {
                $( $type::update_config(&mut self.$field); )*
            }

            #[inline]
            fn start_entry(&mut self) -> io::Result<()> {
                $( $type::start_entry(&mut self.$field)?; )*
                Ok(())
            }

            #[inline]
            fn end_entry(&mut self) -> io::Result<()> {
                $( $type::end_entry(&mut self.$field)?; )*
                Ok(())
            }

            #[inline]
            fn write_key(&mut self, key: &'static str) -> io::Result<()> {
                $( $type::write_key(&mut self.$field, key)?; )*
                Ok(())
            }

            #[inline]
            fn begin_array(&mut self) -> io::Result<()> {
                $( $type::begin_array(&mut self.$field)?; )*
                Ok(())
            }

            #[inline]
            fn end_array(&mut self) -> io::Result<()> {
                $( $type::end_array(&mut self.$field)?; )*
                Ok(())
            }

            #[inline]
            fn begin_object(&mut self) -> io::Result<()> {
                $( $type::begin_object(&mut self.$field)?; )*
                Ok(())
            }

            #[inline]
            fn end_object(&mut self) -> io::Result<()> {
                $( $type::end_object(&mut self.$field)?; )*
                Ok(())
            }

            #[inline]
            fn write_time_already_sanitized<T>(
                &mut self,
                value: &T,
                kind: FormatterTimeKind,
            ) -> io::Result<()>
            where
                T: ?Sized + Display,
            {
                $( $type::write_time_already_sanitized(&mut self.$field, value, kind)?; )*
                Ok(())
            }

            #[inline]
            fn write_duration(
                &mut self,
                duration: Duration,
                precision: TimePrecision,
                kind: FormatterDurationKind,
            ) -> io::Result<()> {
                $( $type::write_duration(&mut self.$field, duration, precision, kind)?; )*
                Ok(())
            }

            #[inline]
            fn write_id(&mut self, id: u64, kind: FormatterIdKind) -> io::Result<()> {
                $( $type::write_id(&mut self.$field, id, kind)?; )*
                Ok(())
            }

            #[inline]
            fn write_str(&mut self, value: &str, kind: FormatterStrKind) -> io::Result<()> {
                $( $type::write_str(&mut self.$field, value, kind)?; )*
                Ok(())
            }

            #[inline]
            fn write_span_id(
                &mut self,
                span_id: MaybeKnownSpanId,
                kind: FormatterSpanIdKind,
            ) -> io::Result<()> {
                $( $type::write_span_id(&mut self.$field, span_id, kind)?; )*
                Ok(())
            }

            #[inline]
            fn write_kind(&mut self, kind: FormatterEventKind) -> io::Result<()> {
                $( $type::write_kind(&mut self.$field, kind)?; )*
                Ok(())
            }

            #[inline]
            fn write_span_path_kind(&mut self, kind: FormatterEventKind) -> io::Result<()> {
                $( $type::write_span_path_kind(&mut self.$field, kind)?; )*
                Ok(())
            }

            #[inline]
            fn write_span_path_span_id(&mut self, span_id: KnownSpanId) -> io::Result<()> {
                $( $type::write_span_path_span_id(&mut self.$field, span_id)?; )*
                Ok(())
            }

            #[inline]
            fn write_span_path_span_name(&mut self, name: &str) -> io::Result<()> {
                $( $type::write_span_path_span_name(&mut self.$field, name)?; )*
                Ok(())
            }

            #[inline]
            fn write_level(&mut self, level: Level) -> io::Result<()> {
                $( $type::write_level(&mut self.$field, level)?; )*
                Ok(())
            }

            #[inline]
            fn write_target(&mut self, target: &str, kind: FormatterEventKind) -> io::Result<()> {
                $( $type::write_target(&mut self.$field, target, kind)?; )*
                Ok(())
            }

            #[inline]
            fn write_message<T>(&mut self, message: &T, kind: FormatterEventKind) -> io::Result<()>
            where
                T: ?Sized + Display,
            {
                $( $type::write_message(&mut self.$field, message, kind)?; )*
                Ok(())
            }

            #[inline]
            fn write_argument_key(&mut self, key: &str) -> io::Result<()> {
                $( $type::write_argument_key(&mut self.$field, key)?; )*
                Ok(())
            }

            #[inline]
            fn write_argument_value<T>(&mut self, value: &T) -> io::Result<()>
            where
                T: ?Sized + Display,
            {
                $( $type::write_argument_value(&mut self.$field, value)?; )*
                Ok(())
            }

            #[inline]
            fn write_location_arguments(
                &mut self,
                filename: Option<&str>,
                line: Option<u32>,
                column: Option<u32>,
            ) -> io::Result<()> {
                $( $type::write_location_arguments(&mut self.$field, filename, line, column)?; )*
                Ok(())
            }
        }
    };
}

impl_variadics! {
    [] [
        (0 T1) (1 T2) (2 T3) (3 T4)
        (4 T5) (5 T6) (6 T7) (7 T8)
        (8 T9) (9 T10) (10 T11) (11 T12)
    ]
}

#[expect(clippy::unwrap_used, reason = "okay in tests")]
#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use alloc::vec::Vec;
    use core::num::NonZero;
    use core::time::Duration;
    use std::io;

    use parking_lot::RwLock;
    use polog::Level;

    use crate::{
        Formatter, FormatterDurationKind, FormatterEventKind, FormatterIdKind, FormatterSpanIdKind,
        FormatterStrKind, FormatterTimeKind, KnownSpanId, MaybeKnownSpanId, NdJsonFormatter,
        TextFormatter, TextFormatterColorMode, TextFormatterConfig, TextFormatterFlags,
        TimePrecision,
    };

    #[rustfmt::skip] // bad formatting
    const EXPECTED_EXAMPLE_ENTRY_TEXT_NO_COLOR: &str = "\
        2000-01-01T00:00:00Z 2000-01-02T00:00:00Z \
        2000-01-01T00:00:00+01:00 2000-01-02T00:00:00+01:00 +01:00 \
        100 100.000 100.000000 100.000000000 \
        123 my-app 234 main 345 worker 456 \
        root unkn #123 root unkn #123    \
        >> << |> |< ?? ?. ?! :> :< :+ :@    \
        >> << |> |< ?? ?. ?! :> :< :+ :@ \
        FATAL BUG   ERROR WARN  INFO  DEBUG TRACE \
        >> root mod1 |> #123 mod2 << #123 mod3 \
        my-target my::module::path my-target my::module::path \
        src/main.rs:12:23 src/main.rs:12 src/main.rs::23 \
        src/main.rs :12:23 :12 ::23 \
        Hello, world! \
        Hello, world!: \
        string = value, \
        with\nnewline = \"with\ttab\", \
        with\\backslash = with␍carriage return, \
        with␈backspace = with␌form feed, \
        with␛escape = with␡del\n";

    #[rustfmt::skip] // bad formatting
    const EXPECTED_EXAMPLE_ENTRY_TEXT_COLORED: &str = "\
        2000-01-01T00:00:00Z \
        \u{1b}[90m2000-01-0\u{1b}[0m2T00:00:00Z \
        2000-01-01T00:00:00+01:00 \
        \u{1b}[90m2000-01-0\u{1b}[0m2T00:00:00+01:00 \
        +01:00 \
        100 100.000 100.000000 \u{1b}[90m100.000000\u{1b}[0m000 \
        123 my-app 234 main 345 worker 456 \
        \u{1b}[90mroot\u{1b}[0m \u{1b}[90munkn\u{1b}[0m \u{1b}[32m#123\u{1b}[0m \
        \u{1b}[90mroot\u{1b}[0m \u{1b}[90munkn\u{1b}[0m \u{1b}[32m#123\u{1b}[0m    \
        \u{1b}[32m>>\u{1b}[0m \u{1b}[90m<<\u{1b}[0m \
        \u{1b}[92m|>\u{1b}[0m \u{1b}[90m|<\u{1b}[0m \
        \u{1b}[90m??\u{1b}[0m \u{1b}[90m?.\u{1b}[0m \u{1b}[90m?!\u{1b}[0m \
        \u{1b}[92m:>\u{1b}[0m \u{1b}[90m:<\u{1b}[0m \
        :+ :@    \
        \u{1b}[32m>>\u{1b}[0m \u{1b}[90m<<\u{1b}[0m \
        \u{1b}[92m|>\u{1b}[0m \u{1b}[90m|<\u{1b}[0m \
        \u{1b}[90m??\u{1b}[0m \u{1b}[90m?.\u{1b}[0m \u{1b}[90m?!\u{1b}[0m \
        \u{1b}[92m:>\u{1b}[0m \u{1b}[90m:<\u{1b}[0m \
        :+ :@ \
        \u{1b}[1;101mFATAL\u{1b}[0m \u{1b}[1;41mBUG\u{1b}[0m   \
        \u{1b}[1;31mERROR\u{1b}[0m \u{1b}[1;33mWARN\u{1b}[0m  \
        \u{1b}[32mINFO\u{1b}[0m  \u{1b}[36mDEBUG\u{1b}[0m \u{1b}[90mTRACE\u{1b}[0m \
        \u{1b}[32m>>\u{1b}[0m \u{1b}[90mroot\u{1b}[0m mod1 \
        \u{1b}[92m|>\u{1b}[0m \u{1b}[32m#123\u{1b}[0m mod2 \
        \u{1b}[90m<<\u{1b}[0m \u{1b}[32m#123\u{1b}[0m mod3 \
        \u{1b}[34mmy-target\u{1b}[0m \u{1b}[34mmy::module::path\u{1b}[0m \
        \u{1b}[90mmy-target\u{1b}[0m \u{1b}[90mmy::module::path\u{1b}[0m \
        \u{1b}[36msrc/main.rs:12:23\u{1b}[0m \
        \u{1b}[36msrc/main.rs:12\u{1b}[0m \
        \u{1b}[36msrc/main.rs::23\u{1b}[0m \
        \u{1b}[36msrc/main.rs\u{1b}[0m \
        \u{1b}[36m:12:23\u{1b}[0m \u{1b}[36m:12\u{1b}[0m \u{1b}[36m::23\u{1b}[0m \
        Hello, world! \u{1b}[90mHello, world!\u{1b}[0m: \
        \u{1b}[35mstring\u{1b}[0m = \u{1b}[34mvalue\u{1b}[0m, \
        \u{1b}[35mwith\nnewline\u{1b}[0m = \u{1b}[34m\"with\ttab\"\u{1b}[0m, \
        \u{1b}[35mwith\\backslash\u{1b}[0m = \
            \u{1b}[34mwith\u{1b}[0m\
            \u{1b}[31m␍\u{1b}[0m\
            \u{1b}[34mcarriage return\u{1b}[0m, \
        \u{1b}[35mwith\u{1b}[0m\u{1b}[31m␈\u{1b}[0m\u{1b}[35mbackspace\u{1b}[0m = \
            \u{1b}[34mwith\u{1b}[0m\
            \u{1b}[31m␌\u{1b}[0m\
            \u{1b}[34mform feed\u{1b}[0m, \
        \u{1b}[35mwith\u{1b}[0m\u{1b}[31m␛\u{1b}[0m\u{1b}[35mescape\u{1b}[0m = \
            \u{1b}[34mwith\u{1b}[0m\
            \u{1b}[31m␡\u{1b}[0m\
            \u{1b}[34mdel\u{1b}[0m\
        \n";

    #[rustfmt::skip] // bad formatting
    const EXPECTED_EXAMPLE_ENTRY_NDJSON: &str = "{\
        \"utc_time\":\"2000-01-01T00:00:00Z\",\
        \"utc_time\":\"2000-01-02T00:00:00Z\",\
        \"local_time\":\"2000-01-01T00:00:00+01:00\",\
        \"local_time\":\"2000-01-02T00:00:00+01:00\",\
        \"utc_offset\":\"+01:00\",\
        \"monotonic_time\":\"100\",\
        \"process_cpu_time\":\"100.000\",\
        \"thread_cpu_time\":\"100.000000\",\
        \"thread_cpu_time\":\"100.000000000\",\
        \"process_id\":\"123\",\"process_name\":\"my-app\",\
        \"thread_id\":\"234\",\"thread_name\":\"main\",\
        \"task_id\":\"345\",\"task_name\":\"worker\",\
        \"cpu_id\":\"456\",\
        \"parent_span_id\":\"root\",\
        \"parent_span_id\":\"unknown\",\
        \"parent_span_id\":\"123\",\
        \"span_id\":\"root\",\
        \"span_id\":\"unknown\",\
        \"span_id\":\"123\",\
        \"kind\":\"log\",\
        \"kind\":\"nested_enter\",\"kind\":\"nested_exit\",\
        \"kind\":\"detached_enter\",\"kind\":\"detached_exit\",\
        \"kind\":\"task_poll\",\
        \"kind\":\"task_pending\",\
        \"kind\":\"task_ready\",\
        \"kind\":\"external_new\",\"kind\":\"external_close\",\
        \"kind\":\"external_record\",\"kind\":\"external_link\",\
        \"span_path_kind\":\"log\",\
        \"span_path_kind\":\"nested_enter\",\
        \"span_path_kind\":\"nested_exit\",\
        \"span_path_kind\":\"detached_enter\",\
        \"span_path_kind\":\"detached_exit\",\
        \"span_path_kind\":\"task_poll\",\
        \"span_path_kind\":\"task_pending\",\
        \"span_path_kind\":\"task_ready\",\
        \"span_path_kind\":\"external_new\",\
        \"span_path_kind\":\"external_close\",\
        \"span_path_kind\":\"external_record\",\
        \"span_path_kind\":\"external_link\",\
        \"level\":\"FATAL\",\"level\":\"BUG\",\
        \"level\":\"ERROR\",\"level\":\"WARN\",\
        \"level\":\"INFO\",\"level\":\"DEBUG\",\"level\":\"TRACE\",\
        \"span_path\":[\
            {\"kind\":\"nested_enter\",\"span_id\":\"root\",\"message\":\"mod1\"},\
            {\"kind\":\"detached_enter\",\"span_id\":\"123\",\"message\":\"mod2\"},\
            {\"kind\":\"nested_exit\",\"span_id\":\"123\",\"message\":\"mod3\"}\
        ],\
        \"target\":\"my-target\",\
        \"module_path\":\"my::module::path\",\
        \"target\":\"my-target\",\
        \"module_path\":\"my::module::path\",\
        \"filename\":\"src/main.rs\",\"line\":12,\"column\":23,\
        \"filename\":\"src/main.rs\",\"line\":12,\
        \"filename\":\"src/main.rs\",\"column\":23,\
        \"filename\":\"src/main.rs\",\"line\":12,\"column\":23,\
        \"line\":12,\"column\":23,\
        \"message\":\"Hello, world!\",\
        \"message\":\"Hello, world!\",\
        \"args\":[\
            [\"string\",\"value\"],\
            [\"with\\nnewline\",\"\\\"with\\ttab\\\"\"],\
            [\"with\\\\backslash\",\"with\\rcarriage return\"],\
            [\"with\\bbackspace\",\"with\\fform feed\"],\
            [\"with\\u001bescape\",\"with\\u007fdel\"]\
        ]\
    }\n";

    #[expect(clippy::too_many_lines, reason = "better to keep together")]
    fn example_entry<F>(fmt: &mut F) -> Result<(), io::Error>
    where
        F: Formatter,
    {
        use FormatterDurationKind as Fdk;
        use FormatterEventKind as Fek;
        use FormatterIdKind as Fik;
        use FormatterSpanIdKind as Fsik;
        use FormatterStrKind as Fsk;
        use FormatterTimeKind as Ftk;
        use KnownSpanId as Ksi;
        use MaybeKnownSpanId as Mksi;
        use TimePrecision as Tp;

        enum IdOrStr<'a> {
            Id(u64, FormatterIdKind),
            Str(&'a str, FormatterStrKind),
        }

        fmt.update_config();
        fmt.start_entry()?;

        for (key, time, kind) in [
            ("utc_time", "2000-01-01T00:00:00Z", Ftk::Utc),
            ("utc_time", "2000-01-02T00:00:00Z", Ftk::Utc),
            ("local_time", "2000-01-01T00:00:00+01:00", Ftk::Local),
            ("local_time", "2000-01-02T00:00:00+01:00", Ftk::Local),
            ("utc_offset", "+01:00", Ftk::UtcOffset),
        ] {
            fmt.write_key(key)?;
            fmt.write_time_already_sanitized(time, kind)?;
        }

        for (key, precision, kind) in [
            ("monotonic_time", Tp::Seconds, Fdk::Monotonic),
            ("process_cpu_time", Tp::Millis, Fdk::ProcessCpu),
            ("thread_cpu_time", Tp::Micros, Fdk::ThreadCpu),
            ("thread_cpu_time", Tp::Nanos, Fdk::ThreadCpu),
        ] {
            fmt.write_key(key)?;
            fmt.write_duration(Duration::from_secs(100), precision, kind)?;
        }

        for (key, id_or_str) in [
            ("process_id", IdOrStr::Id(123, Fik::ProcessId)),
            ("process_name", IdOrStr::Str("my-app", Fsk::ProcessName)),
            ("thread_id", IdOrStr::Id(234, Fik::ThreadId)),
            ("thread_name", IdOrStr::Str("main", Fsk::ThreadName)),
            ("task_id", IdOrStr::Id(345, Fik::TaskId)),
            ("task_name", IdOrStr::Str("worker", Fsk::TaskName)),
            ("cpu_id", IdOrStr::Id(456, Fik::CpuId)),
        ] {
            fmt.write_key(key)?;
            match id_or_str {
                IdOrStr::Id(value, kind) => fmt.write_id(value, kind)?,
                IdOrStr::Str(value, kind) => fmt.write_str(value, kind)?,
            }
        }

        let span_id = polog::SpanId(NonZero::new(123).unwrap());
        for (key, id, kind) in [
            ("parent_span_id", Mksi::Root, Fsik::ParentSpanId),
            ("parent_span_id", Mksi::Unknown, Fsik::ParentSpanId),
            ("parent_span_id", Mksi::SpanId(span_id), Fsik::ParentSpanId),
            ("span_id", Mksi::Root, Fsik::SpanId),
            ("span_id", Mksi::Unknown, Fsik::SpanId),
            ("span_id", Mksi::SpanId(span_id), Fsik::SpanId),
        ] {
            fmt.write_key(key)?;
            fmt.write_span_id(id, kind)?;
        }

        let all_kinds = [
            Fek::Log,
            Fek::NestedEnter,
            Fek::NestedExit,
            Fek::DetachedEnter,
            Fek::DetachedExit,
            Fek::TaskPoll,
            Fek::TaskPending,
            Fek::TaskReady,
            Fek::ExternalNew,
            Fek::ExternalClose,
            Fek::ExternalRecord,
            Fek::ExternalLink,
        ];
        for kind in all_kinds {
            fmt.write_key("kind")?;
            fmt.write_kind(kind)?;
        }
        for kind in all_kinds {
            fmt.write_key("span_path_kind")?;
            fmt.write_span_path_kind(kind)?;
        }

        for level in [
            Level::Fatal,
            Level::Bug,
            Level::Error,
            Level::Warn,
            Level::Info,
            Level::Debug,
            Level::Trace,
        ] {
            fmt.write_key("level")?;
            fmt.write_level(level)?;
        }

        fmt.write_key("span_path")?;
        fmt.begin_array()?;
        for (id, kind, name) in [
            (Ksi::Root, Fek::NestedEnter, "mod1"),
            (Ksi::SpanId(span_id), Fek::DetachedEnter, "mod2"),
            (Ksi::SpanId(span_id), Fek::NestedExit, "mod3"),
        ] {
            fmt.begin_object()?;
            fmt.write_key("kind")?;
            fmt.write_span_path_kind(kind)?;
            fmt.write_key("span_id")?;
            fmt.write_span_path_span_id(id)?;
            fmt.write_key("message")?;
            fmt.write_span_path_span_name(name)?;
            fmt.end_object()?;
        }
        fmt.end_array()?;

        for kind in [Fek::NestedEnter, Fek::NestedExit] {
            fmt.write_key("target")?;
            fmt.write_target("my-target", kind)?;
            fmt.write_key("module_path")?;
            fmt.write_target("my::module::path", kind)?;
        }

        for (filename, line, column) in [
            (Some("src/main.rs"), Some(12), Some(23)),
            (Some("src/main.rs"), Some(12), None),
            (Some("src/main.rs"), None, Some(23)),
            (Some("src/main.rs"), None, None),
            (None, Some(12), Some(23)),
            (None, Some(12), None),
            (None, None, Some(23)),
            (None, None, None),
        ] {
            fmt.write_location_arguments(filename, line, column)?;
        }

        for kind in [Fek::NestedEnter, Fek::NestedExit] {
            fmt.write_key("message")?;
            fmt.write_message(&"Hello, world!", kind)?;
        }

        fmt.write_key("args")?;
        fmt.begin_array()?;
        for (key, value) in [
            ("string", "value"),
            ("with\nnewline", "\"with\ttab\""),
            ("with\\backslash", "with\rcarriage return"),
            ("with\u{08}backspace", "with\u{0c}form feed"),
            ("with\x1bescape", "with\x7fdel"),
        ] {
            fmt.begin_array()?;
            fmt.write_argument_key(key)?;
            fmt.write_argument_value(&value)?;
            fmt.end_array()?;
        }
        fmt.end_array()?;

        fmt.end_entry()?;
        fmt.flush_blocking()?;
        fmt.flush_on_panic();
        Ok(())
    }

    #[test]
    fn text_formatter_for_unit_is_noop() {
        let mut fmt = ();
        example_entry(&mut fmt).unwrap();
    }

    #[test]
    fn text_formatter_tuple2_forward() {
        let config =
            TextFormatterConfig::new(TextFormatterFlags::default(), TextFormatterColorMode::Off);
        let config = Arc::new(RwLock::new(config));
        let mut fmt = (
            TextFormatter::new(Vec::<u8>::new(), Arc::clone(&config)),
            NdJsonFormatter::new(Vec::<u8>::new()),
        );
        example_entry(&mut fmt).unwrap();

        pretty_assertions::assert_eq!(
            str::from_utf8(fmt.0.writer()).unwrap(),
            EXPECTED_EXAMPLE_ENTRY_TEXT_NO_COLOR,
        );
        pretty_assertions::assert_eq!(
            str::from_utf8(fmt.1.writer()).unwrap(),
            EXPECTED_EXAMPLE_ENTRY_NDJSON,
        );
    }

    #[test]
    fn text_formatter_tuple4_forward() {
        let config_no_color =
            TextFormatterConfig::new(TextFormatterFlags::default(), TextFormatterColorMode::Off);
        let config_no_color = Arc::new(RwLock::new(config_no_color));

        let config_colored =
            TextFormatterConfig::new(TextFormatterFlags::default(), TextFormatterColorMode::On);
        let config_colored = Arc::new(RwLock::new(config_colored));

        let mut fmt = (
            TextFormatter::new(Vec::<u8>::new(), config_no_color),
            NdJsonFormatter::new(Vec::<u8>::new()),
            TextFormatter::new(Vec::<u8>::new(), config_colored),
            NdJsonFormatter::new(Vec::<u8>::new()),
        );
        example_entry(&mut fmt).unwrap();

        pretty_assertions::assert_eq!(
            str::from_utf8(fmt.0.writer()).unwrap(),
            EXPECTED_EXAMPLE_ENTRY_TEXT_NO_COLOR,
        );
        pretty_assertions::assert_eq!(
            str::from_utf8(fmt.1.writer()).unwrap(),
            EXPECTED_EXAMPLE_ENTRY_NDJSON,
        );
        println!("{:?}", str::from_utf8(fmt.2.writer()).unwrap());
        pretty_assertions::assert_eq!(
            str::from_utf8(fmt.2.writer()).unwrap(),
            EXPECTED_EXAMPLE_ENTRY_TEXT_COLORED,
        );
        pretty_assertions::assert_eq!(
            str::from_utf8(fmt.3.writer()).unwrap(),
            EXPECTED_EXAMPLE_ENTRY_NDJSON,
        );
    }

    #[cfg(feature = "either-1")]
    #[test]
    fn text_formatter_either_forward() {
        use either_1::Either;

        let config =
            TextFormatterConfig::new(TextFormatterFlags::default(), TextFormatterColorMode::Off);
        let config = Arc::new(RwLock::new(config));

        for mut fmt in [
            Either::Left(TextFormatter::new(Vec::<u8>::new(), Arc::clone(&config))),
            Either::Right(NdJsonFormatter::new(Vec::<u8>::new())),
        ] {
            example_entry(&mut fmt).unwrap();

            match fmt {
                Either::Left(fmt) => pretty_assertions::assert_eq!(
                    str::from_utf8(fmt.writer()).unwrap(),
                    EXPECTED_EXAMPLE_ENTRY_TEXT_NO_COLOR,
                ),
                Either::Right(fmt) => pretty_assertions::assert_eq!(
                    str::from_utf8(fmt.writer()).unwrap(),
                    EXPECTED_EXAMPLE_ENTRY_NDJSON,
                ),
            }
        }
    }

    #[test]
    fn test_formatter_failing_partial_fmt() {
        struct SmallWriter(usize);

        impl io::Write for SmallWriter {
            #[inline]
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                let result = if self.0 < buf.len() {
                    Err(io::Error::from(io::ErrorKind::WriteZero))
                } else {
                    Ok(buf.len())
                };
                self.0 = self.0.saturating_sub(buf.len());
                result
            }

            #[inline]
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        #[cfg(not(miri))]
        let step = 1;
        #[cfg(miri)]
        let step = 16;

        let config =
            TextFormatterConfig::new(TextFormatterFlags::default(), TextFormatterColorMode::Off);
        let config = Arc::new(RwLock::new(config));

        let mut fmt = TextFormatter::new(Vec::<u8>::new(), Arc::clone(&config));
        example_entry(&mut fmt).unwrap();
        let len = fmt.writer().len();
        for len in (0..len).step_by(step) {
            let mut fmt = TextFormatter::new(SmallWriter(len), Arc::clone(&config));
            assert!(example_entry(&mut fmt).is_err());
        }
        let mut fmt = TextFormatter::new(SmallWriter(len), Arc::clone(&config));
        assert!(example_entry(&mut fmt).is_ok());

        let mut fmt = NdJsonFormatter::new(Vec::<u8>::new());
        example_entry(&mut fmt).unwrap();
        let len = fmt.writer().len();

        for len in (0..len).step_by(step) {
            let mut fmt = NdJsonFormatter::new(SmallWriter(len));
            assert!(example_entry(&mut fmt).is_err());
        }
        let mut fmt = NdJsonFormatter::new(SmallWriter(len));
        assert!(example_entry(&mut fmt).is_ok());
    }
}
