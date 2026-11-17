use alloc::string::String;
use core::fmt::{self, Display};
use std::io::{self, BufWriter, Stderr, Stdout, stderr, stdout};

use crate::{
    Formatter, FormatterDurationKind, FormatterEventKind, FormatterIdKind, FormatterSpanIdKind,
    FormatterStrKind, FormatterTimeKind, KnownSpanId, MaybeKnownSpanId, TimePrecision,
};

/// Writes each event as one newline-delimited JSON object.
///
/// # Content and safety
///
/// `NdJsonFormatter` does not sanitize content.
/// Values are emitted under RFC 8259 escaping, so the resulting JSON is always
/// valid and always safe to parse.
/// A consumer that decodes the JSON and prints the resulting strings to a
/// terminal must sanitize them, or route the output through `TextFormatter`
/// instead.
/// See the crate-level documentation for the full contract.
#[derive(Debug)]
pub struct NdJsonFormatter<W> {
    writer: W,
    state: State,
    scratch: Scratch,
}

#[derive(Default, Debug)]
struct State {
    needs_separator: bool,
}

#[derive(Default, Debug)]
struct Scratch {
    string1: String,
}

impl NdJsonFormatter<BufWriter<Stdout>> {
    /// Creates a formatter that writes to a buffered `stdout`.
    #[inline]
    #[must_use]
    pub fn stdout() -> Self {
        Self::new(BufWriter::new(stdout()))
    }
}

impl NdJsonFormatter<BufWriter<Stderr>> {
    /// Creates a formatter that writes to a buffered `stderr`.
    #[inline]
    #[must_use]
    pub fn stderr() -> Self {
        Self::new(BufWriter::new(stderr()))
    }
}

impl<W> NdJsonFormatter<W> {
    /// Creates a formatter over an arbitrary writer.
    #[inline]
    #[must_use]
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            state: State::default(),
            scratch: Scratch::default(),
        }
    }

    /// Returns a reference to the wrapped writer.
    #[inline]
    #[must_use]
    pub const fn writer(&self) -> &W {
        &self.writer
    }

    /// Returns a mutable reference to the wrapped writer.
    #[inline]
    #[must_use]
    pub const fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    #[inline]
    fn write_separator(&mut self) -> io::Result<()>
    where
        W: io::Write,
    {
        if self.state.needs_separator {
            write!(&mut self.writer, ",")?;
        }

        Ok(())
    }
}

impl<W> Formatter for NdJsonFormatter<W>
where
    W: io::Write,
{
    #[inline]
    fn flush_blocking(&mut self) -> io::Result<()> {
        W::flush(&mut self.writer)
    }

    #[inline]
    fn flush_on_panic(&mut self) {
        let result = W::flush(&mut self.writer);
        drop(result);
    }

    #[inline]
    fn update_config(&mut self) {}

    #[inline]
    fn start_entry(&mut self) -> io::Result<()> {
        self.state = State {
            needs_separator: false,
        };
        write!(self.writer, "{{")?;
        Ok(())
    }

    #[inline]
    fn end_entry(&mut self) -> io::Result<()> {
        writeln!(self.writer, "}}")?;
        Ok(())
    }

    #[inline]
    fn write_key(&mut self, key: &'static str) -> io::Result<()> {
        self.write_separator()?;
        write!(self.writer, "\"{key}\":")?;
        self.state.needs_separator = false;
        Ok(())
    }

    #[inline]
    fn begin_array(&mut self) -> io::Result<()> {
        self.write_separator()?;
        write!(self.writer, "[")?;
        self.state.needs_separator = false;
        Ok(())
    }

    #[inline]
    fn end_array(&mut self) -> io::Result<()> {
        write!(self.writer, "]")?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn begin_object(&mut self) -> io::Result<()> {
        self.write_separator()?;
        write!(self.writer, "{{")?;
        self.state.needs_separator = false;
        Ok(())
    }

    #[inline]
    fn end_object(&mut self) -> io::Result<()> {
        write!(self.writer, "}}")?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_time_already_sanitized<T>(
        &mut self,
        value: &T,
        _kind: FormatterTimeKind,
    ) -> io::Result<()>
    where
        T: ?Sized + Display,
    {
        self.write_separator()?;
        write_as_json_escaped_str(&mut self.writer, value, &mut self.scratch)?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_duration(
        &mut self,
        duration: core::time::Duration,
        precision: TimePrecision,
        _kind: FormatterDurationKind,
    ) -> io::Result<()> {
        let secs = duration.as_secs();
        let nanos = duration.subsec_nanos();
        let (subsec_len, subsecs) = match precision {
            TimePrecision::Seconds => (0, 0),
            TimePrecision::Millis => (3, nanos / 1_000_000),
            TimePrecision::Micros => (6, nanos / 1_000),
            TimePrecision::Nanos => (9, nanos),
        };

        self.write_separator()?;
        write!(self.writer, "\"{secs}")?;
        if subsec_len != 0 {
            write!(self.writer, ".{subsecs:0subsec_len$}")?;
        }
        write!(self.writer, "\"")?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_id(&mut self, id: u64, _kind: FormatterIdKind) -> io::Result<()> {
        self.write_separator()?;
        write!(self.writer, "\"{id}\"")?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_str(&mut self, value: &str, _kind: FormatterStrKind) -> io::Result<()> {
        self.write_separator()?;
        write_json_escaped_str(&mut self.writer, value)?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_span_id(
        &mut self,
        span_id: MaybeKnownSpanId,
        _kind: FormatterSpanIdKind,
    ) -> io::Result<()> {
        self.write_separator()?;
        write_span_id_value(&mut self.writer, span_id)?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_kind(&mut self, kind: FormatterEventKind) -> io::Result<()> {
        let text = kind_text(kind);
        self.write_separator()?;
        write!(&mut self.writer, "\"{text}\"")?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_span_path_kind(&mut self, kind: FormatterEventKind) -> io::Result<()> {
        let text = kind_text(kind);
        self.write_separator()?;
        write!(&mut self.writer, "\"{text}\"")?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_span_path_span_id(&mut self, span_id: KnownSpanId) -> io::Result<()> {
        self.write_separator()?;
        write_span_id_value(&mut self.writer, span_id.into())?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_span_path_span_name(&mut self, name: &str) -> io::Result<()> {
        self.write_separator()?;
        write_json_escaped_str(&mut self.writer, name)?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_level(&mut self, level: polog::Level) -> io::Result<()> {
        let level_text = level_text(level);
        self.write_separator()?;
        write!(&mut self.writer, "\"{level_text}\"")?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_target(&mut self, target: &str, _kind: FormatterEventKind) -> io::Result<()> {
        self.write_separator()?;
        write_json_escaped_str(&mut self.writer, target)?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_message<T>(&mut self, message: &T, _kind: FormatterEventKind) -> io::Result<()>
    where
        T: ?Sized + Display,
    {
        self.write_separator()?;
        write_as_json_escaped_str(&mut self.writer, message, &mut self.scratch)?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_argument_key(&mut self, key: &str) -> io::Result<()> {
        self.write_separator()?;
        write_json_escaped_str(&mut self.writer, key)?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_argument_value<T>(&mut self, value: &T) -> io::Result<()>
    where
        T: ?Sized + Display,
    {
        self.write_separator()?;
        write_as_json_escaped_str(&mut self.writer, value, &mut self.scratch)?;
        self.state.needs_separator = true;
        Ok(())
    }

    #[inline]
    fn write_location_arguments(
        &mut self,
        filename: Option<&str>,
        line: Option<u32>,
        column: Option<u32>,
    ) -> io::Result<()> {
        if let Some(filename) = filename {
            self.write_key("filename")?;
            write_json_escaped_str(&mut self.writer, filename)?;
            self.state.needs_separator = true;
        }

        if let Some(line) = line {
            self.write_key("line")?;
            write!(&mut self.writer, "{line}")?;
            self.state.needs_separator = true;
        }

        if let Some(column) = column {
            self.write_key("column")?;
            write!(&mut self.writer, "{column}")?;
            self.state.needs_separator = true;
        }
        Ok(())
    }
}

#[inline]
const fn level_text(level: polog::Level) -> &'static str {
    match level {
        polog::Level::Fatal => "FATAL",
        polog::Level::Bug => "BUG",
        polog::Level::Error => "ERROR",
        polog::Level::Warn => "WARN",
        polog::Level::Info => "INFO",
        polog::Level::Debug => "DEBUG",
        polog::Level::Trace => "TRACE",
    }
}

#[inline]
const fn kind_text(kind: FormatterEventKind) -> &'static str {
    match kind {
        FormatterEventKind::Log => "log",
        FormatterEventKind::NestedEnter => "nested_enter",
        FormatterEventKind::NestedExit => "nested_exit",
        FormatterEventKind::DetachedEnter => "detached_enter",
        FormatterEventKind::DetachedExit => "detached_exit",
        FormatterEventKind::TaskPoll => "task_poll",
        FormatterEventKind::TaskPending => "task_pending",
        FormatterEventKind::TaskReady => "task_ready",
        FormatterEventKind::ExternalNew => "external_new",
        FormatterEventKind::ExternalClose => "external_close",
        FormatterEventKind::ExternalRecord => "external_record",
        FormatterEventKind::ExternalLink => "external_link",
    }
}

#[inline]
fn write_span_id_value<W>(writer: &mut W, span_id: MaybeKnownSpanId) -> io::Result<()>
where
    W: io::Write,
{
    match span_id {
        MaybeKnownSpanId::Unknown => write!(writer, "\"unknown\""),
        MaybeKnownSpanId::Root => write!(writer, "\"root\""),
        MaybeKnownSpanId::SpanId(span_id) => write!(writer, "\"{span_id}\""),
    }
}

fn write_as_json_escaped_str<W, T>(
    writer: &mut W,
    source: &T,
    scratch: &mut Scratch,
) -> io::Result<()>
where
    W: io::Write,
    T: ?Sized + Display,
{
    use core::fmt::Write;
    scratch.string1.clear();
    write!(&mut scratch.string1, "{source}")
        .map_err(|fmt::Error {}| io::Error::other("failed to cache string"))?;
    write_json_escaped_str(writer, scratch.string1.as_str())?;
    scratch.string1.clear();
    Ok(())
}

fn write_json_escaped_str<W>(writer: &mut W, mut source: &str) -> io::Result<()>
where
    W: io::Write,
{
    #[expect(clippy::as_conversions, reason = "only viable option in const expr")]
    static ESCAPE_TABLE: u128 = const {
        let mut table = 0;
        table |= (1 << 0x20) - 1;
        table |= 1 << b'"' as u32;
        table |= 1 << b'\\' as u32;
        table |= 1 << b'\x7F' as u32;
        table
    };

    const HEX: &[u8; 16] = b"0123456789abcdef";

    write!(writer, "\"")?;
    while !source.is_empty() {
        let bytes = source.as_bytes();
        let offset = bytes
            .iter()
            .position(|&byte| byte < 0x80 && (ESCAPE_TABLE & 1 << byte != 0));

        let valid_until = offset.unwrap_or(source.len());
        writer.write_all(&bytes[..valid_until])?;

        let Some(offset) = offset else {
            break;
        };

        match source.as_bytes()[offset] {
            b'"' => writer.write_all(b"\\\"")?,
            b'\\' => writer.write_all(b"\\\\")?,
            b'\n' => writer.write_all(b"\\n")?,
            b'\r' => writer.write_all(b"\\r")?,
            b'\t' => writer.write_all(b"\\t")?,
            0x08 => writer.write_all(b"\\b")?,
            0x0C => writer.write_all(b"\\f")?,
            byte => {
                let buf = [
                    b'\\',
                    b'u',
                    b'0',
                    b'0',
                    HEX[usize::from(byte >> 4)],
                    HEX[usize::from(byte & 0xf)],
                ];
                writer.write_all(&buf)?;
            }
        }

        #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
        (source = &source[offset + 1..]);
    }
    write!(writer, "\"")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "okay in tests")]

    use polog::Level;

    use crate::formatters::ndjson::write_json_escaped_str;
    use crate::{Formatter, NdJsonFormatter};

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_text_formatter_constructors() {
        let mut fmt = NdJsonFormatter::stdout();
        fmt.write_level(Level::Info).unwrap();

        let mut fmt = NdJsonFormatter::stderr();
        fmt.write_level(Level::Info).unwrap();
    }

    #[test]
    fn test_text_formatter_accessors() {
        let mut fmt = NdJsonFormatter::new(Vec::<u8>::new());

        assert_eq!(fmt.writer(), &[]);
        assert_eq!(fmt.writer_mut(), &[]);
    }

    #[test]
    fn test_json_escaped_str() {
        fn into_json_escaped_str(source: &str) -> Vec<u8> {
            let mut target = Vec::new();
            write_json_escaped_str(&mut target, source).unwrap();
            target
        }

        assert_eq!(into_json_escaped_str("foo"), b"\"foo\"");
        assert_eq!(into_json_escaped_str("foo\n"), b"\"foo\\n\"");
        assert_eq!(into_json_escaped_str("foo\nbar"), b"\"foo\\nbar\"");
        assert_eq!(into_json_escaped_str("foo\nbar\n"), b"\"foo\\nbar\\n\"");
        assert_eq!(
            into_json_escaped_str("foo\nbar\nbaz"),
            b"\"foo\\nbar\\nbaz\""
        );

        assert_eq!(
            into_json_escaped_str("a\x00\x1F\"\n\x7Fb"),
            b"\"a\\u0000\\u001f\\\"\\n\\u007fb\""
        );
    }
}
