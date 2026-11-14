//! Conversions between `polog` types and types from other logging ecosystems.

#[cfg(feature = "log-0-4")]
#[cfg_attr(docsrs, doc(cfg(feature = "log-0-4")))]
pub mod log_0_4;

#[cfg(feature = "slog-2")]
#[cfg_attr(docsrs, doc(cfg(feature = "slog-2")))]
pub mod slog_2;

#[cfg(feature = "tracing-core-0-1")]
#[cfg_attr(docsrs, doc(cfg(feature = "tracing-core-0-1")))]
pub mod tracing_core_0_1;
