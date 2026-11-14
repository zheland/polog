//! Conversions between `polog` and `slog` 2.

use polog::{Level, LevelFilter};

/// Converts a `slog::Level` into the closest `polog::Level`.
///
/// Approximate: `slog::Level::Critical` maps to `polog::Level::Error` because
/// `polog` does not model a level stricter than `Error` for non‑panic cases.
#[inline]
#[must_use]
pub const fn level_from_foreign_approx(level: slog_2::Level) -> Level {
    match level {
        slog_2::Level::Critical | slog_2::Level::Error => Level::Error,
        slog_2::Level::Warning => Level::Warn,
        slog_2::Level::Info => Level::Info,
        slog_2::Level::Debug => Level::Debug,
        slog_2::Level::Trace => Level::Trace,
    }
}

/// Converts a `polog::Level` into the closest `slog::Level`.
///
/// Approximate: `Level::Fatal` and `Level::Bug` are mapped to
/// `slog::Level::Error`, because `slog` has no more severe level.
#[inline]
#[must_use]
pub const fn level_into_foreign_approx(level: Level) -> slog_2::Level {
    match level {
        Level::Fatal | Level::Bug | Level::Error => slog_2::Level::Error,
        Level::Warn => slog_2::Level::Warning,
        Level::Info => slog_2::Level::Info,
        Level::Debug => slog_2::Level::Debug,
        Level::Trace => slog_2::Level::Trace,
    }
}

/// Converts a `slog::FilterLevel` into the closest `polog::LevelFilter`.
///
/// Approximate: `slog::Level::Critical` maps to `polog::Level::Error` because
/// `polog` does not model a level stricter than `Error` for non‑panic cases.
#[inline]
#[must_use]
pub const fn level_filter_from_foreign_approx(filter: slog_2::FilterLevel) -> LevelFilter {
    match filter {
        slog_2::FilterLevel::Off => LevelFilter::Off,
        slog_2::FilterLevel::Critical | slog_2::FilterLevel::Error => LevelFilter::Error,
        slog_2::FilterLevel::Warning => LevelFilter::Warn,
        slog_2::FilterLevel::Info => LevelFilter::Info,
        slog_2::FilterLevel::Debug => LevelFilter::Debug,
        slog_2::FilterLevel::Trace => LevelFilter::Trace,
    }
}

/// Converts a `polog::LevelFilter` into the closest `slog::FilterLevel`.
///
/// Approximate: `LevelFilter::Fatal` and `LevelFilter::Bug` are mapped to
/// `slog::FilterLevel::Off`, because `slog` has no stricter filter.
#[inline]
pub const fn level_filter_into_foreign_approx(level: LevelFilter) -> slog_2::FilterLevel {
    match level {
        LevelFilter::Off | LevelFilter::Fatal | LevelFilter::Bug => slog_2::FilterLevel::Off,
        LevelFilter::Error => slog_2::FilterLevel::Error,
        LevelFilter::Warn => slog_2::FilterLevel::Warning,
        LevelFilter::Info => slog_2::FilterLevel::Info,
        LevelFilter::Debug => slog_2::FilterLevel::Debug,
        LevelFilter::Trace => slog_2::FilterLevel::Trace,
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
            let foreign = conv::slog_2::level_filter_into_foreign_approx(level_filter);
            assert_eq!(
                conv::slog_2::level_filter_from_foreign_approx(foreign),
                LevelFilter::Off
            );
        }

        for i in 3..=7 {
            let level_filter = LevelFilter::try_from_u8(i).unwrap();
            let foreign = conv::slog_2::level_filter_into_foreign_approx(level_filter);
            assert_eq!(
                conv::slog_2::level_filter_from_foreign_approx(foreign),
                level_filter
            );
        }
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_conv_log_level() {
        for i in 1..=2 {
            let level = Level::try_from_u8(i).unwrap();
            let foreign = conv::slog_2::level_into_foreign_approx(level);
            assert_eq!(
                conv::slog_2::level_from_foreign_approx(foreign),
                Level::Error
            );
        }

        for i in 3..=7 {
            let level = Level::try_from_u8(i).unwrap();
            let foreign = conv::slog_2::level_into_foreign_approx(level);
            assert_eq!(conv::slog_2::level_from_foreign_approx(foreign), level);
        }
    }
}
