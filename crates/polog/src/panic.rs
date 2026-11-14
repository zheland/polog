use core::fmt::{self, Display};

use crate::{Descriptor, Level, SpanId};

/// Default panic hook for log-related panics.
///
/// Formats the descriptor level, span ID, module path, and message, then
/// panics with that message.
///
/// # Panics
///
/// Always panics.
#[expect(clippy::panic, reason = "explicit panic function")]
#[inline]
#[track_caller]
pub fn default_log_related_panic(descriptor: &'static Descriptor, span_id: Option<SpanId>) -> ! {
    panic!(
        "{}",
        PanicFmt {
            descriptor,
            span_id,
        }
    );
}

/// Formatting helper for [`default_log_related_panic`].
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
struct PanicFmt {
    descriptor: &'static Descriptor,
    span_id: Option<SpanId>,
}

impl Display for PanicFmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {} in {} at {}: {}",
            level_as_padded_str(self.descriptor.level),
            fmt::from_fn(|f| if let Some(span_id) = self.span_id {
                write!(f, "span {}", span_id.0)
            } else {
                write!(f, "root span")
            }),
            self.descriptor.module_path,
            self.descriptor.location,
            self.descriptor.message,
        )?;

        Ok(())
    }
}

#[inline]
#[must_use]
const fn level_as_padded_str(level: Level) -> &'static str {
    match level {
        Level::Fatal => "FATAL",
        Level::Bug => "BUG  ",
        Level::Error => "ERROR",
        Level::Warn => "WARN ",
        Level::Info => "INFO ",
        Level::Debug => "DEBUG",
        Level::Trace => "TRACE",
    }
}
