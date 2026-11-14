use alloc::string::String;
use alloc::sync::Arc;
use core::fmt::{self, Display};
use std::io::{self, BufWriter, Stderr, Stdout, stderr, stdout};

use parking_lot::RwLock;

use crate::{
    Formatter, FormatterDurationKind, FormatterEventKind, FormatterIdKind, FormatterSpanIdKind,
    FormatterStrKind, FormatterTimeKind, KnownSpanId, MaybeKnownSpanId, TimePrecision,
};

/// Writes each event as a single line of human-readable text.
///
/// # Content and safety
///
/// `TextFormatter` sanitizes content: every C0 control character and DEL in
/// a caller-supplied string is replaced with its Unicode Control Picture
/// counterpart, and the replacement is colorized in red when colorization is
/// enabled. See the crate-level documentation for the full contract.
///
/// # Flags
///
/// Controlled by [`TextFormatterFlags`].
#[derive(Debug)]
pub struct TextFormatter<W> {
    writer: W,
    cached_config: TextFormatterConfig,
    config: Arc<RwLock<TextFormatterConfig>>,
    state: State,
    cache: Cache,
    scratch: Scratch,
}

/// Runtime configuration of an [`TextFormatter`].
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct TextFormatterConfig {
    /// Rendering flags.
    pub flags: TextFormatterFlags,
    /// Color mode configuration.
    pub color_mode: TextFormatterColorMode,
}

impl TextFormatterConfig {
    /// Creates a runtime configuration of an [`TextFormatter`].
    #[inline]
    #[must_use]
    pub const fn new(flags: TextFormatterFlags, color_mode: TextFormatterColorMode) -> Self {
        Self { flags, color_mode }
    }
}

bitflags::bitflags! {
    /// Flags that control [`TextFormatter`] output.
    ///
    /// [`TextFormatter`]: TextFormatter
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct TextFormatterFlags: u32 {
        /// Pad log levels to same width.
        const PAD_LEVELS = 1 << 0;

        /// Pad span IDs so they align across lines.
        const DYNAMIC_SPAN_ID_PADDING = 1 << 1;

        /// Pad context durations so they align across lines.
        const DYNAMIC_CONTEXT_DURATION_PADDING = 1 << 2;

        /// Pad context IDs so they align across lines.
        const DYNAMIC_CONTEXT_ID_PADDING = 1 << 3;

        /// Pad context names so they align across lines.
        const DYNAMIC_CONTEXT_NAME_PADDING = 1 << 4;

        /// Colorize only the suffix of each timestamp that differs from the
        /// previous timestamp on the same stream.
        const COLORIZE_TIMESTAMP_DIFF = 1 << 5;

        /// Replace C0 control characters and DEL with their Unicode Control
        /// Picture counterpart.
        const SANITIZE_CONTROL_CHARS = 1 << 6;

        /// Also replace `\n` with `␊`.
        const SANITIZE_NEWLINES = 1 << 7;

        /// Also replace `\t` with `␉`.
        const SANITIZE_TABULATION = 1 << 8;

        /// Colorize each sanitized character red, making it visually
        /// distinct from a control picture that appeared in content.
        const COLORIZE_SANITIZED = 1 << 9;
    }
}

impl TextFormatterFlags {
    /// Default set: dynamic padding, timestamp highlighting, and content
    /// sanitization.
    pub const DEFAULT: Self = Self::empty()
        .union(Self::PAD_LEVELS)
        .union(Self::DYNAMIC_SPAN_ID_PADDING)
        .union(Self::DYNAMIC_CONTEXT_DURATION_PADDING)
        .union(Self::DYNAMIC_CONTEXT_ID_PADDING)
        .union(Self::DYNAMIC_CONTEXT_NAME_PADDING)
        .union(Self::COLORIZE_TIMESTAMP_DIFF)
        .union(Self::SANITIZE_CONTROL_CHARS)
        .union(Self::COLORIZE_SANITIZED);

    /// Every declared text-format bit set.
    pub const ALL: Self = Self::empty()
        .union(Self::DYNAMIC_SPAN_ID_PADDING)
        .union(Self::DYNAMIC_CONTEXT_DURATION_PADDING)
        .union(Self::DYNAMIC_CONTEXT_ID_PADDING)
        .union(Self::DYNAMIC_CONTEXT_NAME_PADDING)
        .union(Self::COLORIZE_TIMESTAMP_DIFF)
        .union(Self::SANITIZE_CONTROL_CHARS)
        .union(Self::SANITIZE_NEWLINES)
        .union(Self::SANITIZE_TABULATION)
        .union(Self::COLORIZE_SANITIZED);
}

impl Default for TextFormatterFlags {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// How [`TextFormatter`] should emit ANSI color escapes.
///
/// The `Auto*` variants ask the [`supports-color`] crate whether the named
/// standard stream is a TTY.
/// The answer is cached per stream by `supports-color`, so detection is
/// effectively free after the first call.
///
/// Any destination that is not stdout or stderr should use [`Off`] (or
/// [`On`], if the sink is known to accept ANSI escapes).
/// There is no auto-detection for arbitrary destinations because there is no
/// portable way to ask "is this writer a TTY?".
///
/// [`Off`]: Self::Off
/// [`On`]: Self::On
/// [`supports-color`]: https://docs.rs/supports-color/latest/supports_color/
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum TextFormatterColorMode {
    /// Never emit color escapes.
    Off,

    /// Emit color escapes only when stdout is a TTY.
    AutoStdout,

    /// Emit color escapes only when stderr is a TTY.
    AutoStderr,

    /// Always emit color escapes.
    On,
}

/// Whether `TextFormatter` should emit ANSI color escapes.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub enum ColorizePreset {
    /// Never emit color escapes.
    /// Maps to [`TextFormatterColorMode::Off`].
    Off,

    /// Emit color escapes when the stream detected by the constructor is a
    /// TTY.
    /// [`TextFormatter::stdout`] maps this to
    /// [`TextFormatterColorMode::AutoStdout`].
    /// [`TextFormatter::stderr`] maps it to
    /// [`TextFormatterColorMode::AutoStderr`].
    #[default]
    Auto,

    /// Always emit color escapes.
    /// Maps to [`TextFormatterColorMode::On`].
    On,
}

#[derive(Default, Debug)]
struct State {
    has_anything: bool,
    has_arguments: bool,
    should_colorize: bool,
}

const SPAN_ID_LENGTH_IDX: usize = 0;
const MONOTONIC_TIME_LENGTH_IDX: usize = 1;
const PROCESS_CPU_TIME_LENGTH_IDX: usize = 2;
const THREAD_CPU_TIME_LENGTH_IDX: usize = 3;
const PROCESS_ID_LENGTH_IDX: usize = 4;
const PROCESS_NAME_LENGTH_IDX: usize = 5;
const THREAD_ID_LENGTH_IDX: usize = 6;
const THREAD_NAME_LENGTH_IDX: usize = 7;
const TASK_ID_LENGTH_IDX: usize = 8;
const TASK_NAME_LENGTH_IDX: usize = 9;
const CPU_ID_LENGTH_IDX: usize = 10;
const NUM_CACHE_LENGTHS: usize = 11;

const UTC_TIME_VALUE_IDX: usize = 0;
const LOCAL_TIME_VALUE_IDX: usize = 1;
const UTC_OFFSET_VALUE_IDX: usize = 2;
const MONOTONIC_TIME_VALUE_IDX: usize = 3;
const PROCESS_CPU_TIME_VALUE_IDX: usize = 4;
const THREAD_CPU_TIME_VALUE_IDX: usize = 5;
const NUM_CACHE_VALUES: usize = 6;

#[derive(Default, Debug)]
struct Cache {
    lengths: [u8; NUM_CACHE_LENGTHS],
    values: [String; NUM_CACHE_VALUES],
}

#[derive(Default, Debug)]
struct Scratch {
    string1: String,
}

impl TextFormatter<BufWriter<Stdout>> {
    /// Creates a formatter that writes to a buffered `stdout`.
    ///
    /// [`ColorizePreset::Auto`] is interpreted as
    /// [`TextFormatterColorMode::AutoStdout`], because this constructor's
    /// stream is already known.
    /// For explicit control of the color mode, construct a
    /// [`TextFormatterConfig`] and pass it to [`TextFormatter::new`].
    #[inline]
    #[must_use]
    pub fn stdout(flags: TextFormatterFlags, should_colorize: ColorizePreset) -> Self {
        Self::new(
            BufWriter::new(stdout()),
            Arc::new(RwLock::new(TextFormatterConfig {
                flags,
                color_mode: match should_colorize {
                    ColorizePreset::Off => TextFormatterColorMode::Off,
                    ColorizePreset::Auto => TextFormatterColorMode::AutoStdout,
                    ColorizePreset::On => TextFormatterColorMode::On,
                },
            })),
        )
    }
}

impl TextFormatter<BufWriter<Stderr>> {
    /// Creates a formatter that writes to a buffered `stderr`.
    ///
    /// [`ColorizePreset::Auto`] is interpreted as
    /// [`TextFormatterColorMode::AutoStderr`], because this constructor's
    /// stream is already known.
    /// For explicit control of the color mode, construct a
    /// [`TextFormatterConfig`] and pass it to [`TextFormatter::new`].
    #[inline]
    #[must_use]
    pub fn stderr(flags: TextFormatterFlags, should_colorize: ColorizePreset) -> Self {
        Self::new(
            BufWriter::new(stderr()),
            Arc::new(RwLock::new(TextFormatterConfig {
                flags,
                color_mode: match should_colorize {
                    ColorizePreset::Off => TextFormatterColorMode::Off,
                    ColorizePreset::Auto => TextFormatterColorMode::AutoStderr,
                    ColorizePreset::On => TextFormatterColorMode::On,
                },
            })),
        )
    }
}

impl<W> TextFormatter<W> {
    /// Creates a formatter over an arbitrary writer.
    ///
    /// `config.color_mode` decides whether ANSI color escapes are emitted.
    /// [`TextFormatterColorMode::Off`] is appropriate for destinations that do
    /// not interpret ANSI (files, captured pipes, in-memory buffers).
    /// [`TextFormatterColorMode::On`] is appropriate only when the sink is
    /// known to accept color.
    ///
    /// The `AutoStdout` and `AutoStderr` variants ask the [`supports-color`]
    /// crate whether *the process's* stdout or stderr is a TTY.
    /// They do not inspect `writer`. Use them only when `writer` is (or wraps)
    /// that standard stream.
    /// For any other writer, prefer `Off` or `On`.
    ///
    /// [`supports-color`]: https://docs.rs/supports-color/latest/supports_color/
    #[inline]
    #[must_use]
    pub fn new(writer: W, config: Arc<RwLock<TextFormatterConfig>>) -> Self {
        let cached_config = *config.read();
        Self {
            writer,
            cached_config,
            config,
            state: State::default(),
            cache: Cache::default(),
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

    /// Returns the shared config.
    ///
    /// Mutating the inner `TextFormatterConfig` takes effect on the next call
    /// to `TextFormatter::update_config`, which `Pologger` calls at the start
    /// of every entry.
    #[inline]
    #[must_use]
    pub const fn config(&self) -> &Arc<RwLock<TextFormatterConfig>> {
        &self.config
    }

    #[inline]
    fn write_separator(&mut self) -> io::Result<()>
    where
        W: io::Write,
    {
        if self.state.has_anything {
            write!(&mut self.writer, " ")?;
        }
        self.state.has_anything = true;
        Ok(())
    }

    #[inline]
    fn should_colorize(&self) -> bool {
        match self.cached_config.color_mode {
            TextFormatterColorMode::Off => false,
            TextFormatterColorMode::AutoStdout => {
                supports_color::on_cached(supports_color::Stream::Stdout).is_some()
            }
            TextFormatterColorMode::AutoStderr => {
                supports_color::on_cached(supports_color::Stream::Stderr).is_some()
            }
            TextFormatterColorMode::On => true,
        }
    }

    #[inline]
    fn write_timestamp_with_colorized_diff<T>(
        &mut self,
        value: &T,
        cache_value_idx: usize,
    ) -> Result<(), io::Error>
    where
        W: io::Write,
        T: ?Sized + Display,
    {
        use core::fmt::Write;

        if !self.state.should_colorize
            || !self
                .cached_config
                .flags
                .contains(TextFormatterFlags::COLORIZE_TIMESTAMP_DIFF)
        {
            write!(self.writer, "{value}")?;
            return Ok(());
        }

        self.scratch.string1.clear();
        let _ = write!(&mut self.scratch.string1, "{value}");
        let cached = &mut self.cache.values[cache_value_idx];
        let common_prefix_len = (self.scratch.string1.bytes())
            .zip(cached.bytes())
            .take_while(|(x, y)| x == y)
            .count();

        if common_prefix_len > 0 {
            write!(
                &mut self.writer,
                "\x1b[90m{}\x1b[0m",
                &self.scratch.string1[..common_prefix_len],
            )?;
        }
        write!(
            &mut self.writer,
            "{}",
            &self.scratch.string1[common_prefix_len..],
        )?;

        cached.clear();
        cached.push_str(&self.scratch.string1);
        self.scratch.string1.clear();

        Ok(())
    }
}

impl<W> Formatter for TextFormatter<W>
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
    fn update_config(&mut self) {
        self.cached_config = *self.config.read();
    }

    #[inline]
    fn start_entry(&mut self) -> io::Result<()> {
        self.state = State {
            has_anything: false,
            has_arguments: false,
            should_colorize: self.should_colorize(),
        };
        Ok(())
    }

    #[inline]
    fn end_entry(&mut self) -> io::Result<()> {
        writeln!(self.writer)
    }

    #[inline]
    fn write_key(&mut self, _: &'static str) -> io::Result<()> {
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
        value: &T,
        kind: FormatterTimeKind,
    ) -> io::Result<()>
    where
        T: ?Sized + Display,
    {
        let cache_value_idx = match kind {
            FormatterTimeKind::Utc => UTC_TIME_VALUE_IDX,
            FormatterTimeKind::Local => LOCAL_TIME_VALUE_IDX,
            FormatterTimeKind::UtcOffset => UTC_OFFSET_VALUE_IDX,
        };
        self.write_separator()?;
        self.write_timestamp_with_colorized_diff(value, cache_value_idx)?;
        Ok(())
    }

    #[inline]
    fn write_duration(
        &mut self,
        duration: core::time::Duration,
        precision: TimePrecision,
        kind: FormatterDurationKind,
    ) -> io::Result<()> {
        let (cache_value_idx, cache_length_idx) = match kind {
            FormatterDurationKind::Monotonic => {
                (MONOTONIC_TIME_VALUE_IDX, MONOTONIC_TIME_LENGTH_IDX)
            }
            FormatterDurationKind::ProcessCpu => {
                (PROCESS_CPU_TIME_VALUE_IDX, PROCESS_CPU_TIME_LENGTH_IDX)
            }
            FormatterDurationKind::ThreadCpu => {
                (THREAD_CPU_TIME_VALUE_IDX, THREAD_CPU_TIME_LENGTH_IDX)
            }
        };

        let secs = duration.as_secs();
        let nanos = duration.subsec_nanos();
        let (period_len, subsec_len, subsecs) = match precision {
            TimePrecision::Seconds => (0, 0, 0),
            TimePrecision::Millis => (1, 3, nanos / 1_000_000),
            TimePrecision::Micros => (1, 6, nanos / 1_000),
            TimePrecision::Nanos => (1, 9, nanos),
        };

        let cached_len = self
            .cached_config
            .flags
            .contains(TextFormatterFlags::DYNAMIC_CONTEXT_DURATION_PADDING)
            .then_some(&mut self.cache.lengths[cache_length_idx]);

        #[expect(clippy::option_if_let_else, reason = "better because of side-effects")]
        #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
        let secs_len = if let Some(cached_len) = cached_len {
            (*cached_len = (*cached_len).max(num_digits(secs) + period_len + subsec_len));
            Some(usize::from(
                cached_len.saturating_sub(period_len + subsec_len),
            ))
        } else {
            None
        };

        let value = fmt::from_fn(move |f| {
            if let Some(secs_len) = secs_len {
                write!(f, "{secs:secs_len$}")?;
            } else {
                write!(f, "{secs}")?;
            }
            if subsec_len != 0 {
                let subsec_len = usize::from(subsec_len);
                write!(f, ".{subsecs:0subsec_len$}")?;
            }
            Ok(())
        });

        self.write_separator()?;
        self.write_timestamp_with_colorized_diff(&value, cache_value_idx)?;
        Ok(())
    }

    #[inline]
    fn write_id(&mut self, id: u64, kind: FormatterIdKind) -> io::Result<()> {
        let cache_length_idx = match kind {
            FormatterIdKind::ProcessId => PROCESS_ID_LENGTH_IDX,
            FormatterIdKind::ThreadId => THREAD_ID_LENGTH_IDX,
            FormatterIdKind::TaskId => TASK_ID_LENGTH_IDX,
            FormatterIdKind::CpuId => CPU_ID_LENGTH_IDX,
        };

        let cached_len = self
            .cached_config
            .flags
            .contains(TextFormatterFlags::DYNAMIC_CONTEXT_ID_PADDING)
            .then_some(&mut self.cache.lengths[cache_length_idx]);

        #[expect(clippy::option_if_let_else, reason = "better because of side-effects")]
        let len = if let Some(cached_len) = cached_len {
            *cached_len = (*cached_len).max(num_digits(id));
            Some(usize::from(*cached_len))
        } else {
            None
        };

        self.write_separator()?;
        if let Some(len) = len {
            write!(&mut self.writer, "{id:len$}")?;
        } else {
            write!(&mut self.writer, "{id}")?;
        }
        Ok(())
    }

    #[inline]
    fn write_str(&mut self, value: &str, kind: FormatterStrKind) -> io::Result<()> {
        let cache_length_idx = match kind {
            FormatterStrKind::ProcessName => PROCESS_NAME_LENGTH_IDX,
            FormatterStrKind::ThreadName => THREAD_NAME_LENGTH_IDX,
            FormatterStrKind::TaskName => TASK_NAME_LENGTH_IDX,
        };

        let cached_len = self
            .cached_config
            .flags
            .contains(TextFormatterFlags::DYNAMIC_CONTEXT_NAME_PADDING)
            .then_some(&mut self.cache.lengths[cache_length_idx]);

        #[expect(clippy::option_if_let_else, reason = "better because of side-effects")]
        let len = if let Some(cached_len) = cached_len {
            let len = u8::try_from(value.len()).unwrap_or(255);
            *cached_len = (*cached_len).max(len);
            Some(usize::from(*cached_len))
        } else {
            None
        };

        self.write_separator()?;
        if let Some(len) = len {
            use core::fmt::Write;
            self.scratch.string1.clear();
            write!(&mut self.scratch.string1, "{value:len$}")
                .map_err(|fmt::Error {}| io::Error::other("failed to cache string"))?;
            write_with_sanitized_control_chars(
                &mut self.writer,
                &self.scratch.string1,
                b"",
                b"",
                self.cached_config.flags,
                self.state.should_colorize,
            )?;
            self.scratch.string1.clear();
        } else {
            write_with_sanitized_control_chars(
                &mut self.writer,
                value,
                b"",
                b"",
                self.cached_config.flags,
                self.state.should_colorize,
            )?;
        }
        Ok(())
    }

    #[inline]
    fn write_span_id(
        &mut self,
        span_id: MaybeKnownSpanId,
        kind: FormatterSpanIdKind,
    ) -> io::Result<()> {
        let mut len = self
            .cached_config
            .flags
            .contains(TextFormatterFlags::DYNAMIC_SPAN_ID_PADDING)
            .then_some(&mut self.cache.lengths[SPAN_ID_LENGTH_IDX]);

        if let Some(len) = len.as_mut() {
            **len = (**len).max(span_id_len(span_id));
        }
        let len = len.copied();

        let pad_kind = match kind {
            FormatterSpanIdKind::ParentSpanId => PadKind::Left,
            FormatterSpanIdKind::SpanId => PadKind::Right,
        };
        let span_id = StyledSpanId::new(span_id, len, self.state.should_colorize, pad_kind);

        self.write_separator()?;
        write!(&mut self.writer, "{span_id}")?;
        Ok(())
    }

    #[inline]
    fn write_kind(&mut self, kind: FormatterEventKind) -> io::Result<()> {
        let glyph = kind_glyph(kind, self.state.should_colorize);

        self.write_separator()?;
        write!(&mut self.writer, "{glyph}")?;
        Ok(())
    }

    #[inline]
    fn write_span_path_kind(&mut self, kind: FormatterEventKind) -> io::Result<()> {
        let glyph = kind_glyph(kind, self.state.should_colorize);

        self.write_separator()?;
        write!(&mut self.writer, "{glyph}")?;
        Ok(())
    }

    #[inline]
    fn write_span_path_span_id(&mut self, span_id: KnownSpanId) -> io::Result<()> {
        let span_id = StyledSpanId::new(
            span_id.into(),
            None,
            self.state.should_colorize,
            PadKind::Left,
        );

        self.write_separator()?;
        write!(&mut self.writer, "{span_id}")?;
        Ok(())
    }

    #[inline]
    fn write_span_path_span_name(&mut self, name: &str) -> io::Result<()> {
        self.write_separator()?;
        write_with_sanitized_control_chars(
            &mut self.writer,
            name,
            b"",
            b"",
            self.cached_config.flags,
            self.state.should_colorize,
        )?;
        Ok(())
    }

    #[inline]
    fn write_level(&mut self, level: polog::Level) -> io::Result<()> {
        let (level_text, padding) = level_text(level, self.state.should_colorize);

        self.write_separator()?;
        write!(&mut self.writer, "{level_text}")?;
        if self
            .cached_config
            .flags
            .contains(TextFormatterFlags::PAD_LEVELS)
        {
            write!(&mut self.writer, "{padding}")?;
        }
        Ok(())
    }

    #[inline]
    fn write_target(&mut self, target: &str, kind: FormatterEventKind) -> io::Result<()> {
        if target.is_empty() {
            return Ok(());
        }

        let (prefix, suffix) = if self.state.should_colorize {
            if is_primary_kind(kind) {
                ("\x1b[34m", "\x1b[0m")
            } else {
                ("\x1b[90m", "\x1b[0m")
            }
        } else {
            ("", "")
        };

        self.write_separator()?;
        write!(&mut self.writer, "{prefix}")?;
        write_with_sanitized_control_chars(
            &mut self.writer,
            target,
            suffix.as_bytes(),
            prefix.as_bytes(),
            self.cached_config.flags,
            self.state.should_colorize,
        )?;
        write!(&mut self.writer, "{suffix}")?;
        Ok(())
    }

    #[inline]
    fn write_message<T>(&mut self, message: &T, kind: FormatterEventKind) -> io::Result<()>
    where
        T: ?Sized + Display,
    {
        let (prefix, suffix) = if self.state.should_colorize && !is_primary_kind(kind) {
            ("\x1b[90m", "\x1b[0m")
        } else {
            ("", "")
        };

        self.write_separator()?;
        write!(&mut self.writer, "{prefix}")?;
        write_as_str_with_sanitized_control_chars(
            &mut self.writer,
            message,
            suffix.as_bytes(),
            prefix.as_bytes(),
            self.cached_config.flags,
            &mut self.scratch,
            self.state.should_colorize,
        )?;
        write!(&mut self.writer, "{suffix}")?;
        Ok(())
    }

    #[inline]
    fn write_argument_key(&mut self, key: &str) -> io::Result<()> {
        if self.state.has_anything {
            if self.state.has_arguments {
                write!(&mut self.writer, ", ")?;
            } else {
                write!(&mut self.writer, ": ")?;
            }
        }
        self.state.has_arguments = true;

        let (prefix, suffix) = if self.state.should_colorize {
            ("\x1b[35m", "\x1b[0m")
        } else {
            ("", "")
        };

        write!(&mut self.writer, "{prefix}")?;
        write_with_sanitized_control_chars(
            &mut self.writer,
            key,
            suffix.as_bytes(),
            prefix.as_bytes(),
            self.cached_config.flags,
            self.state.should_colorize,
        )?;
        write!(&mut self.writer, "{suffix}")?;
        Ok(())
    }

    #[inline]
    fn write_argument_value<T>(&mut self, value: &T) -> io::Result<()>
    where
        T: ?Sized + Display,
    {
        let (prefix, suffix) = if self.state.should_colorize {
            ("\x1b[34m", "\x1b[0m")
        } else {
            ("", "")
        };

        self.write_separator()?;
        write!(&mut self.writer, "= {prefix}")?;
        write_as_str_with_sanitized_control_chars(
            &mut self.writer,
            value,
            suffix.as_bytes(),
            prefix.as_bytes(),
            self.cached_config.flags,
            &mut self.scratch,
            self.state.should_colorize,
        )?;
        write!(&mut self.writer, "{suffix}")?;
        Ok(())
    }

    #[inline]
    fn write_location_arguments(
        &mut self,
        filename: Option<&str>,
        line: Option<u32>,
        column: Option<u32>,
    ) -> io::Result<()> {
        if filename.is_none() && line.is_none() && column.is_none() {
            return Ok(());
        }

        self.write_separator()?;
        let (prefix, suffix) = if self.state.should_colorize {
            ("\x1b[36m", "\x1b[0m")
        } else {
            ("", "")
        };
        write!(&mut self.writer, "{prefix}")?;

        if let Some(filename) = filename {
            write_with_sanitized_control_chars(
                &mut self.writer,
                filename,
                suffix.as_bytes(),
                prefix.as_bytes(),
                self.cached_config.flags,
                self.state.should_colorize,
            )?;
        }
        if line.is_some() || column.is_some() {
            write!(&mut self.writer, ":")?;
            if let Some(line) = line {
                write!(&mut self.writer, "{line}")?;
            }
            if let Some(column) = column {
                write!(&mut self.writer, ":{column}")?;
            }
        }

        write!(&mut self.writer, "{suffix}")?;
        Ok(())
    }
}

#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "never truncates"
)]
#[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
#[inline]
const fn num_digits(value: u64) -> u8 {
    if value == 0 {
        1
    } else {
        value.ilog10() as u8 + 1
    }
}

/// A span ID rendered with its per-span color and optional field padding.
///
/// Implements `Display` so it can be written via `write!` without allocating
/// an intermediate string.
#[derive(Copy, Clone, Debug)]
struct StyledSpanId {
    span_id: MaybeKnownSpanId,
    field_len: Option<u8>,
    should_colorize: bool,
    pad_kind: PadKind,
}

impl StyledSpanId {
    #[inline]
    #[must_use]
    pub const fn new(
        span_id: MaybeKnownSpanId,
        field_len: Option<u8>,
        should_colorize: bool,
        pad_kind: PadKind,
    ) -> Self {
        Self {
            span_id,
            field_len,
            should_colorize,
            pad_kind,
        }
    }
}

/// Which side of a fixed-width span-ID field is padded.
#[derive(Copy, Clone, Debug)]
enum PadKind {
    /// Pad on the left, so the value is right-aligned.
    Left,
    /// Pad on the right, so the value is left-aligned.
    Right,
}

#[inline]
const fn span_id_len(this: MaybeKnownSpanId) -> u8 {
    match this {
        MaybeKnownSpanId::SpanId(span_id) => num_digits(span_id.0.get()).saturating_add(1),
        // Shorten long "unknown" string by default.
        MaybeKnownSpanId::Root | MaybeKnownSpanId::Unknown => 4,
    }
}

impl Display for StyledSpanId {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = match (self.should_colorize, self.span_id) {
            (false, _) => "",
            (true, MaybeKnownSpanId::SpanId(span_id)) => match span_id.0.get().wrapping_sub(1) % 12
            {
                0 => "\x1b[31m",
                1 => "\x1b[33m",
                2 => "\x1b[32m",
                3 => "\x1b[36m",
                4 => "\x1b[34m",
                5 => "\x1b[35m",
                6 => "\x1b[91m",
                7 => "\x1b[93m",
                8 => "\x1b[92m",
                9 => "\x1b[96m",
                10 => "\x1b[94m",
                11 => "\x1b[95m",
                _ => "",
            },
            (true, MaybeKnownSpanId::Root | MaybeKnownSpanId::Unknown) => "\x1b[90m",
        };
        let suffix = if self.should_colorize { "\x1b[0m" } else { "" };

        let (first, second) = match self.span_id {
            MaybeKnownSpanId::SpanId(span_id) => ("#", Some(span_id)),
            MaybeKnownSpanId::Root => ("root", None),
            MaybeKnownSpanId::Unknown => ("unknown", None),
        };

        let Some(field_len) = self.field_len else {
            write!(f, "{prefix}{first}")?;
            if let Some(second) = second {
                write!(f, "{second}")?;
            }
            write!(f, "{suffix}")?;
            return Ok(());
        };

        let first = &first[..first.len().min(usize::from(field_len))];
        let len = u8::try_from(first.len())
            .unwrap_or(u8::MAX)
            .saturating_add(second.map_or(0, |span_id| num_digits(span_id.0.get())));

        write!(f, "{prefix}")?;
        let pad_len = usize::from(field_len.saturating_sub(len));
        match self.pad_kind {
            PadKind::Left => write!(f, "{0:pad_len$}", "")?,
            PadKind::Right => {}
        }
        write!(f, "{first}")?;
        if let Some(second) = second {
            write!(f, "{}", second.0)?;
        }
        match self.pad_kind {
            PadKind::Left => {}
            PadKind::Right => write!(f, "{0:pad_len$}", "")?,
        }
        write!(f, "{suffix}")?;
        Ok(())
    }
}

#[inline]
const fn level_text(level: polog::Level, should_colorize: bool) -> (&'static str, &'static str) {
    if should_colorize {
        match level {
            polog::Level::Fatal => ("\x1b[1;101mFATAL\x1b[0m", ""),
            polog::Level::Bug => ("\x1b[1;41mBUG\x1b[0m", "  "),
            polog::Level::Error => ("\x1b[1;31mERROR\x1b[0m", ""),
            polog::Level::Warn => ("\x1b[1;33mWARN\x1b[0m", " "),
            polog::Level::Info => ("\x1b[32mINFO\x1b[0m", " "),
            polog::Level::Debug => ("\x1b[36mDEBUG\x1b[0m", ""),
            polog::Level::Trace => ("\x1b[90mTRACE\x1b[0m", ""),
        }
    } else {
        match level {
            polog::Level::Fatal => ("FATAL", ""),
            polog::Level::Bug => ("BUG", "  "),
            polog::Level::Error => ("ERROR", ""),
            polog::Level::Warn => ("WARN", " "),
            polog::Level::Info => ("INFO", " "),
            polog::Level::Debug => ("DEBUG", ""),
            polog::Level::Trace => ("TRACE", ""),
        }
    }
}

#[inline]
const fn kind_glyph(kind: FormatterEventKind, should_colorize: bool) -> &'static str {
    if should_colorize {
        match kind {
            FormatterEventKind::Log => "  ",
            FormatterEventKind::NestedEnter => "\x1b[32m>>\x1b[0m",
            FormatterEventKind::NestedExit => "\x1b[90m<<\x1b[0m",
            FormatterEventKind::DetachedEnter => "\x1b[92m|>\x1b[0m",
            FormatterEventKind::DetachedExit => "\x1b[90m|<\x1b[0m",
            FormatterEventKind::ForeignNew => "\x1b[92m:>\x1b[0m",
            FormatterEventKind::ForeignClose => "\x1b[90m:<\x1b[0m",
            FormatterEventKind::TaskPoll => "\x1b[90m??\x1b[0m",
            FormatterEventKind::TaskPending => "\x1b[90m?.\x1b[0m",
            FormatterEventKind::TaskReady => "\x1b[90m?!\x1b[0m",
        }
    } else {
        match kind {
            FormatterEventKind::Log => "  ",
            FormatterEventKind::NestedEnter => ">>",
            FormatterEventKind::NestedExit => "<<",
            FormatterEventKind::DetachedEnter => "|>",
            FormatterEventKind::DetachedExit => "|<",
            FormatterEventKind::ForeignNew => ":>",
            FormatterEventKind::ForeignClose => ":<",
            FormatterEventKind::TaskPoll => "??",
            FormatterEventKind::TaskPending => "?.",
            FormatterEventKind::TaskReady => "?!",
        }
    }
}

#[inline]
const fn is_primary_kind(kind: FormatterEventKind) -> bool {
    match kind {
        FormatterEventKind::Log
        | FormatterEventKind::NestedEnter
        | FormatterEventKind::DetachedEnter
        | FormatterEventKind::ForeignNew => true,

        FormatterEventKind::NestedExit
        | FormatterEventKind::DetachedExit
        | FormatterEventKind::ForeignClose
        | FormatterEventKind::TaskPoll
        | FormatterEventKind::TaskPending
        | FormatterEventKind::TaskReady => false,
    }
}

fn write_as_str_with_sanitized_control_chars<W, T>(
    writer: &mut W,
    source: &T,
    prefix: &[u8],
    suffix: &[u8],
    flags: TextFormatterFlags,
    scratch: &mut Scratch,
    should_colorize: bool,
) -> io::Result<()>
where
    W: io::Write,
    T: ?Sized + Display,
{
    use core::fmt::Write;
    scratch.string1.clear();
    write!(&mut scratch.string1, "{source}")
        .map_err(|fmt::Error {}| io::Error::other("failed to cache string"))?;
    write_with_sanitized_control_chars(
        writer,
        scratch.string1.as_str(),
        prefix,
        suffix,
        flags,
        should_colorize,
    )?;
    scratch.string1.clear();
    Ok(())
}

/// Writes `source` to `writer`, replacing every C0 control character and DEL
/// with its Unicode Control Picture counterpart.
///
/// # Outer color
///
/// `prefix` and `suffix` bracket the entire value: `prefix` is written
/// before any content and `suffix` after the last content byte. Callers use
/// them to apply an outer color to the whole value - blue for argument
/// values, gray for non-primary messages, or empty for uncolored text.
/// Bracketing is applied regardless of [`COLORIZE_SANITIZED`].
/// The flag only affects how each picture is rendered inside the brackets.
///
/// # Highlighting pictures
///
/// When [`COLORIZE_SANITIZED`] is set, each picture is emitted as:
///
/// ```text
/// suffix  "\x1b[31m"  <picture>  "\x1b[0m"  prefix
/// ```
///
/// `suffix` cancels the outer color, `\x1b[31m` starts red, the picture is
/// emitted in red, `\x1b[0m` cancels red, and `prefix` restores the outer
/// color for the content that follows.
/// The picture therefore appears in red while the surrounding text keeps its
/// original color, and the original color is restored the moment the picture
/// ends.
///
/// The red start and cancel are hardcoded as `\x1b[31m` and `\x1b[0m`,
/// independent of `prefix` and `suffix`.
///
/// When [`COLORIZE_SANITIZED`] is not set, each picture is emitted directly
/// and inherits the outer color.
/// The Control Picture glyph still marks the replacement visibly, but it is not
/// highlighted in red.
///
/// # Sanitizable bytes
///
/// A byte is replaced when it is:
/// - a C0 control character (`0x00`-`0x1F`) other than `\n` and `\t`,
/// - DEL (`0x7F`), or
/// - `\n` while [`SANITIZE_NEWLINES`] is set, or
/// - `\t` while [`SANITIZE_TABULATION`] is set.
///
/// Every other byte flows through unchanged, including non-sanitizable
/// control characters and the surrounding text.
/// Because every sanitizable byte is ASCII, the substitution always consumes
/// exactly one byte.
///
/// [`COLORIZE_SANITIZED`]: TextFormatterFlags::COLORIZE_SANITIZED
/// [`SANITIZE_NEWLINES`]: TextFormatterFlags::SANITIZE_NEWLINES
/// [`SANITIZE_TABULATION`]: TextFormatterFlags::SANITIZE_TABULATION
#[inline]
fn write_with_sanitized_control_chars<W>(
    writer: &mut W,
    mut source: &str,
    prefix: &[u8],
    suffix: &[u8],
    flags: TextFormatterFlags,
    should_colorize: bool,
) -> io::Result<()>
where
    W: io::Write,
{
    if !flags.contains(TextFormatterFlags::SANITIZE_CONTROL_CHARS) {
        writer.write_all(source.as_bytes())?;
        return Ok(());
    }

    while !source.is_empty() {
        let bytes = source.as_bytes();
        let offset = bytes
            .iter()
            .position(|&byte| is_sanitizable_byte(byte, flags));

        let valid_until = offset.unwrap_or(source.len());
        writer.write_all(&bytes[..valid_until])?;

        let Some(offset) = offset else {
            break;
        };

        let byte = source.as_bytes()[offset];
        let mut utf8_buf = [0u8; 4];
        let encoded = control_picture(byte).encode_utf8(&mut utf8_buf);
        if should_colorize && flags.contains(TextFormatterFlags::COLORIZE_SANITIZED) {
            writer.write_all(prefix)?;
            writer.write_all(b"\x1b[31m")?;
            writer.write_all(encoded.as_bytes())?;
            writer.write_all(b"\x1b[0m")?;
            writer.write_all(suffix)?;
        } else {
            writer.write_all(encoded.as_bytes())?;
        }

        #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
        (source = &source[offset + 1..]);
    }
    Ok(())
}

/// Returns whether `byte` is replaced under the given flags.
///
/// - Bytes below `0x20` are control characters.
/// - `0x7F` (DEL) is treated as a control character.
/// - `\n` and `\t` are control characters only when their dedicated flag is
///   set.
#[inline]
const fn is_sanitizable_byte(byte: u8, flags: TextFormatterFlags) -> bool {
    if byte == b'\n' {
        flags.contains(TextFormatterFlags::SANITIZE_NEWLINES)
    } else if byte == b'\t' {
        flags.contains(TextFormatterFlags::SANITIZE_TABULATION)
    } else {
        byte < 0x20 || byte == 0x7f
    }
}

/// Maps a C0 control byte or DEL to its Unicode Control Picture.
///
/// U+2400 through U+241F correspond one-to-one to `0x00` through `0x1F`.
/// U+2421 corresponds to DEL.
#[inline]
const fn control_picture(byte: u8) -> char {
    if byte <= 0x1f {
        #[expect(clippy::as_conversions, reason = "const fn")]
        #[expect(clippy::arithmetic_side_effects, reason = "can not overflow")]
        char::from_u32(('\u{2400}' as u32) + byte as u32).expect("should never fail")
    } else {
        // The `else` branch is reached only for `0x7f`, which
        // `is_sanitized_byte` is the sole gate for.
        '\u{2421}'
    }
}

#[expect(clippy::unwrap_used, reason = "okay in tests")]
#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec::Vec;
    use core::mem;

    use parking_lot::RwLock;
    use polog::Level;

    use crate::formatters::text::write_with_sanitized_control_chars;
    use crate::{
        ColorizePreset, Formatter, FormatterSpanIdKind, MaybeKnownSpanId, TextFormatter,
        TextFormatterColorMode, TextFormatterConfig, TextFormatterFlags, non_zero,
    };

    fn sanitize(
        source: &str,
        (prefix, suffix, newline, tab, colorize): (&[u8], &[u8], bool, bool, bool),
    ) -> String {
        let mut flags: TextFormatterFlags = TextFormatterFlags::SANITIZE_CONTROL_CHARS;
        if newline {
            flags |= TextFormatterFlags::SANITIZE_NEWLINES;
        }
        if tab {
            flags |= TextFormatterFlags::SANITIZE_TABULATION;
        }
        if colorize {
            flags |= TextFormatterFlags::COLORIZE_SANITIZED;
        }

        let mut buf = Vec::new();
        write_with_sanitized_control_chars(&mut buf, source, prefix, suffix, flags, colorize)
            .unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn test_text_formatter_constructors() {
        use TextFormatterColorMode as Tfcm;

        let mut fmt = TextFormatter::stdout(TextFormatterFlags::default(), ColorizePreset::Off);
        fmt.write_level(Level::Info).unwrap();
        assert_eq!(fmt.config.read().color_mode, Tfcm::Off);

        let mut fmt = TextFormatter::stdout(TextFormatterFlags::default(), ColorizePreset::Auto);
        fmt.write_level(Level::Info).unwrap();
        assert_eq!(fmt.config.read().color_mode, Tfcm::AutoStdout);

        let mut fmt = TextFormatter::stdout(TextFormatterFlags::default(), ColorizePreset::On);
        fmt.write_level(Level::Info).unwrap();
        assert_eq!(fmt.config.read().color_mode, Tfcm::On);

        let mut fmt = TextFormatter::stderr(TextFormatterFlags::default(), ColorizePreset::Off);
        fmt.write_level(Level::Info).unwrap();
        assert_eq!(fmt.config.read().color_mode, Tfcm::Off);

        let mut fmt = TextFormatter::stderr(TextFormatterFlags::default(), ColorizePreset::Auto);
        fmt.write_level(Level::Info).unwrap();
        assert_eq!(fmt.config.read().color_mode, Tfcm::AutoStderr);

        let mut fmt = TextFormatter::stderr(TextFormatterFlags::default(), ColorizePreset::On);
        fmt.write_level(Level::Info).unwrap();
        assert_eq!(fmt.config.read().color_mode, Tfcm::On);
    }

    #[test]
    fn test_text_formatter_accessors() {
        let config =
            TextFormatterConfig::new(TextFormatterFlags::default(), TextFormatterColorMode::Off);
        let config = Arc::new(RwLock::new(config));
        let mut fmt = TextFormatter::new(Vec::<u8>::new(), Arc::clone(&config));

        assert_eq!(fmt.writer(), &[]);
        assert_eq!(fmt.writer_mut(), &[]);
    }

    #[test]
    fn test_span_id_padding() {
        use FormatterSpanIdKind as Fsik;
        use MaybeKnownSpanId as Mksi;

        let config =
            TextFormatterConfig::new(TextFormatterFlags::default(), TextFormatterColorMode::Off);
        let config = Arc::new(RwLock::new(config));
        let mut fmt = TextFormatter::new(Vec::<u8>::new(), Arc::clone(&config));

        fmt.start_entry().unwrap();
        fmt.write_span_id(Mksi::Root, Fsik::SpanId).unwrap();
        fmt.end_entry().unwrap();
        assert_eq!(mem::take(fmt.writer_mut()), b"root\n");

        let id = Mksi::SpanId(polog::SpanId(non_zero!(1)));
        fmt.start_entry().unwrap();
        fmt.write_span_id(id, Fsik::ParentSpanId).unwrap();
        fmt.write_span_id(id, Fsik::SpanId).unwrap();
        fmt.end_entry().unwrap();
        assert_eq!(mem::take(fmt.writer_mut()), b"  #1 #1  \n");

        let id = Mksi::SpanId(polog::SpanId(non_zero!(123)));
        fmt.start_entry().unwrap();
        fmt.write_span_id(id, Fsik::ParentSpanId).unwrap();
        fmt.write_span_id(id, Fsik::SpanId).unwrap();
        fmt.end_entry().unwrap();
        assert_eq!(mem::take(fmt.writer_mut()), b"#123 #123\n");

        let id = Mksi::SpanId(polog::SpanId(non_zero!(1_234_567)));
        fmt.start_entry().unwrap();
        fmt.write_span_id(id, Fsik::ParentSpanId).unwrap();
        fmt.write_span_id(id, Fsik::SpanId).unwrap();
        fmt.end_entry().unwrap();
        assert_eq!(mem::take(fmt.writer_mut()), b"#1234567 #1234567\n");

        let id = Mksi::SpanId(polog::SpanId(non_zero!(1)));
        fmt.start_entry().unwrap();
        fmt.write_span_id(id, Fsik::ParentSpanId).unwrap();
        fmt.write_span_id(id, Fsik::SpanId).unwrap();
        fmt.end_entry().unwrap();
        assert_eq!(mem::take(fmt.writer_mut()), b"      #1 #1      \n");
    }

    #[test]
    fn test_write_with_sanitized_control_chars() {
        // Empty.
        assert_eq!(sanitize("", (b"", b"", false, false, false)), "");
        assert_eq!(sanitize("", (b"", b"", true, true, true)), "");

        // Plain string.
        let str = "English, Español, Русский, 汉语, 日本語, ქართული";
        assert_eq!(sanitize(str, (b"", b"", false, false, false)), str);
        assert_eq!(sanitize(str, (b"", b"", false, false, true)), str);

        // Plain escape sequence.
        assert_eq!(
            sanitize("\x1b[31m", (b"", b"", false, false, false)),
            "␛[31m"
        );
        assert_eq!(
            sanitize("\x1b[31m", (b"", b"", false, false, true)),
            "\x1b[31m␛\x1b[0m[31m",
        );

        // Newlines and tabs
        assert_eq!(
            sanitize("a\nb\tc", (b"", b"", false, false, false)),
            "a\nb\tc"
        );
        assert_eq!(
            sanitize("a\nb\tc", (b"", b"", true, false, false)),
            "a␊b\tc"
        );
        assert_eq!(
            sanitize("a\nb\tc", (b"", b"", false, true, false)),
            "a\nb␉c"
        );
        assert_eq!(sanitize("a\nb\tc", (b"", b"", true, true, false)), "a␊b␉c");

        // Del
        assert_eq!(
            sanitize("abc\x7fdef", (b"", b"", false, false, false)),
            "abc␡def"
        );
        assert_eq!(
            sanitize("abc\x7fdef", (b"", b"", false, false, true)),
            "abc\x1b[31m␡\x1b[0mdef"
        );

        // Control sequences.

        assert_eq!(
            sanitize(
                "a\x00b\x01c\x02d\x03e\x04f\x05g\x06h\x07i",
                (b"", b"", false, false, false),
            ),
            "a␀b␁c␂d␃e␄f␅g␆h␇i"
        );
        assert_eq!(
            sanitize(
                "a\x08b\x09c\x0ad\x0be\x0cf\x0dg\x0eh\x0fi",
                (b"", b"", false, false, false),
            ),
            "a␈b\tc\nd␋e␌f␍g␎h␏i"
        );
        assert_eq!(
            sanitize(
                "a\x10b\x11c\x12d\x13e\x14f\x15g\x16h\x17i",
                (b"", b"", false, false, false),
            ),
            "a␐b␑c␒d␓e␔f␕g␖h␗i"
        );
        assert_eq!(
            sanitize(
                "a\x18b\x19c\x1ad\x1be\x1cf\x1dg\x1eh\x1fi",
                (b"", b"", false, false, false),
            ),
            "a␘b␙c␚d␛e␜f␝g␞h␟i"
        );

        // Already colorized.
        assert_eq!(
            sanitize(
                "abc\x1b[31mdef",
                (b"\x1b[0m", b"\x1b[34m", false, false, true)
            ),
            "abc\x1b[0m\x1b[31m␛\x1b[0m\x1b[34m[31mdef",
        );
    }
}
