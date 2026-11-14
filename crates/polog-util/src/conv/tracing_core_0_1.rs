//! Conversions between `polog` and `tracing_core` 0.1.

use polog::{Level, LevelFilter};

/// Converts a `tracing_core::Level` into the equivalent `polog::Level`.
///
/// Lossless: every `tracing_core::Level` constant has a `polog::Level`
/// counterpart.
#[inline]
#[must_use]
pub const fn level_from_foreign(level: tracing_core_0_1::Level) -> Level {
    match level {
        tracing_core_0_1::Level::ERROR => Level::Error,
        tracing_core_0_1::Level::WARN => Level::Warn,
        tracing_core_0_1::Level::INFO => Level::Info,
        tracing_core_0_1::Level::DEBUG => Level::Debug,
        tracing_core_0_1::Level::TRACE => Level::Trace,
    }
}

/// Converts a `polog::Level` into the closest `tracing_core::Level`.
///
/// Approximate: `Level::Fatal` and `Level::Bug` are mapped to
/// `tracing_core::Level::ERROR`, because `tracing` has no more severe level.
#[inline]
#[must_use]
pub const fn level_into_foreign_approx(level: Level) -> tracing_core_0_1::Level {
    match level {
        Level::Fatal | Level::Bug | Level::Error => tracing_core_0_1::Level::ERROR,
        Level::Warn => tracing_core_0_1::Level::WARN,
        Level::Info => tracing_core_0_1::Level::INFO,
        Level::Debug => tracing_core_0_1::Level::DEBUG,
        Level::Trace => tracing_core_0_1::Level::TRACE,
    }
}

/// Converts a `tracing_core::LevelFilter` into the equivalent
/// `polog::LevelFilter`.
///
/// Lossless: every `tracing_core::LevelFilter` constant has a
/// `polog::LevelFilter` counterpart.
#[inline]
#[must_use]
pub const fn level_filter_from_foreign(filter: tracing_core_0_1::LevelFilter) -> LevelFilter {
    match filter {
        tracing_core_0_1::LevelFilter::OFF => LevelFilter::Off,
        tracing_core_0_1::LevelFilter::ERROR => LevelFilter::Error,
        tracing_core_0_1::LevelFilter::WARN => LevelFilter::Warn,
        tracing_core_0_1::LevelFilter::INFO => LevelFilter::Info,
        tracing_core_0_1::LevelFilter::DEBUG => LevelFilter::Debug,
        tracing_core_0_1::LevelFilter::TRACE => LevelFilter::Trace,
    }
}

/// Converts a `polog::LevelFilter` into the closest
/// `tracing_core::LevelFilter`.
///
/// Approximate: `LevelFilter::Fatal` and `LevelFilter::Bug` are mapped to
/// `tracing_core::LevelFilter::ERROR`, because `tracing` has no stricter
/// filter.
#[inline]
#[must_use]
pub const fn level_filter_into_foreign_approx(level: LevelFilter) -> tracing_core_0_1::LevelFilter {
    match level {
        LevelFilter::Off | LevelFilter::Fatal | LevelFilter::Bug => {
            tracing_core_0_1::LevelFilter::OFF
        }
        LevelFilter::Error => tracing_core_0_1::LevelFilter::ERROR,
        LevelFilter::Warn => tracing_core_0_1::LevelFilter::WARN,
        LevelFilter::Info => tracing_core_0_1::LevelFilter::INFO,
        LevelFilter::Debug => tracing_core_0_1::LevelFilter::DEBUG,
        LevelFilter::Trace => tracing_core_0_1::LevelFilter::TRACE,
    }
}

#[cfg(test)]
mod tests {
    use polog::{Level, LevelFilter};

    use crate::conv;

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_conv_log_level_filter() {
        for i in 0..=2 {
            let level_filter = LevelFilter::try_from_u8(i).unwrap();
            let foreign = conv::tracing_core_0_1::level_filter_into_foreign_approx(level_filter);
            assert_eq!(
                conv::tracing_core_0_1::level_filter_from_foreign(foreign),
                LevelFilter::Off
            );
        }

        for i in 3..=7 {
            let level_filter = LevelFilter::try_from_u8(i).unwrap();
            let foreign = conv::tracing_core_0_1::level_filter_into_foreign_approx(level_filter);
            assert_eq!(
                conv::tracing_core_0_1::level_filter_from_foreign(foreign),
                level_filter
            );
        }
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_conv_log_level() {
        for i in 1..=2 {
            let level = Level::try_from_u8(i).unwrap();
            let foreign = conv::tracing_core_0_1::level_into_foreign_approx(level);
            assert_eq!(
                conv::tracing_core_0_1::level_from_foreign(foreign),
                Level::Error
            );
        }

        for i in 3..=7 {
            let level = Level::try_from_u8(i).unwrap();
            let foreign = conv::tracing_core_0_1::level_into_foreign_approx(level);
            assert_eq!(conv::tracing_core_0_1::level_from_foreign(foreign), level);
        }
    }
}
