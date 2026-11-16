//! Bridges for different log backends to forwards events from other log
//! backends into `polog`.

#[cfg(feature = "log-0-4")]
#[cfg_attr(docsrs, doc(cfg(feature = "log-0-4")))]
pub mod log_0_4;

#[cfg(feature = "slog-2")]
#[cfg_attr(docsrs, doc(cfg(feature = "slog-2")))]
pub mod slog_2;

#[cfg(all(feature = "std", feature = "tracing-0-1",))]
#[cfg_attr(docsrs, doc(cfg(all(feature = "std", feature = "tracing-0-1",))))]
pub mod tracing_0_1;
