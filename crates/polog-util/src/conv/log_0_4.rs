//! Conversions between `polog` and `log` 0.4.

use polog::{Level, LevelFilter};

/// Converts a `log::Level` into the equivalent `polog::Level`.
///
/// Lossless: every `log::Level` has a `polog::Level` counterpart of the same
/// name.
#[inline]
#[must_use]
pub const fn level_from_foreign(level: log_0_4::Level) -> Level {
    match level {
        log_0_4::Level::Error => Level::Error,
        log_0_4::Level::Warn => Level::Warn,
        log_0_4::Level::Info => Level::Info,
        log_0_4::Level::Debug => Level::Debug,
        log_0_4::Level::Trace => Level::Trace,
    }
}

/// Converts a `polog::Level` into the closest `log::Level`.
///
/// Approximate: `Level::Fatal` and `Level::Bug` are mapped to
/// `log::Level::Error`, because `log` has no more severe level.
#[inline]
#[must_use]
pub const fn level_into_foreign_approx(level: Level) -> log_0_4::Level {
    match level {
        Level::Fatal | Level::Bug | Level::Error => log_0_4::Level::Error,
        Level::Warn => log_0_4::Level::Warn,
        Level::Info => log_0_4::Level::Info,
        Level::Debug => log_0_4::Level::Debug,
        Level::Trace => log_0_4::Level::Trace,
    }
}

/// Converts a `log::LevelFilter` into the equivalent `polog::LevelFilter`.
///
/// Lossless: every `log::LevelFilter` has a `polog::LevelFilter` counterpart
/// of the same name.
#[inline]
#[must_use]
pub const fn level_filter_from_foreign(filter: log_0_4::LevelFilter) -> LevelFilter {
    match filter {
        log_0_4::LevelFilter::Off => LevelFilter::Off,
        log_0_4::LevelFilter::Error => LevelFilter::Error,
        log_0_4::LevelFilter::Warn => LevelFilter::Warn,
        log_0_4::LevelFilter::Info => LevelFilter::Info,
        log_0_4::LevelFilter::Debug => LevelFilter::Debug,
        log_0_4::LevelFilter::Trace => LevelFilter::Trace,
    }
}

/// Converts a `polog::LevelFilter` into the closest `log::LevelFilter`.
///
/// Approximate: `LevelFilter::Fatal` and `LevelFilter::Bug` are mapped to
/// `log::LevelFilter::Off`, because `log` has no filter that admits a
/// narrower set of levels than `Error`.
#[inline]
#[must_use]
pub const fn level_filter_into_foreign_approx(level: LevelFilter) -> log_0_4::LevelFilter {
    match level {
        LevelFilter::Off | LevelFilter::Fatal | LevelFilter::Bug => log_0_4::LevelFilter::Off,
        LevelFilter::Error => log_0_4::LevelFilter::Error,
        LevelFilter::Warn => log_0_4::LevelFilter::Warn,
        LevelFilter::Info => log_0_4::LevelFilter::Info,
        LevelFilter::Debug => log_0_4::LevelFilter::Debug,
        LevelFilter::Trace => log_0_4::LevelFilter::Trace,
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
            let foreign = conv::log_0_4::level_filter_into_foreign_approx(level_filter);
            assert_eq!(
                conv::log_0_4::level_filter_from_foreign(foreign),
                LevelFilter::Off
            );
        }

        for i in 3..=7 {
            let level_filter = LevelFilter::try_from_u8(i).unwrap();
            let foreign = conv::log_0_4::level_filter_into_foreign_approx(level_filter);
            assert_eq!(
                conv::log_0_4::level_filter_from_foreign(foreign),
                level_filter
            );
        }
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_conv_log_level() {
        for i in 1..=2 {
            let level = Level::try_from_u8(i).unwrap();
            let foreign = conv::log_0_4::level_into_foreign_approx(level);
            assert_eq!(conv::log_0_4::level_from_foreign(foreign), Level::Error);
        }

        for i in 3..=7 {
            let level = Level::try_from_u8(i).unwrap();
            let foreign = conv::log_0_4::level_into_foreign_approx(level);
            assert_eq!(conv::log_0_4::level_from_foreign(foreign), level);
        }
    }
}
