mod logger;

#[cfg(feature = "log-0-4")]
#[cfg_attr(docsrs, doc(cfg(feature = "log-0-4")))]
mod log_0_4;

#[cfg(feature = "slog-2")]
#[cfg_attr(docsrs, doc(cfg(feature = "slog-2")))]
mod slog_2;

#[cfg(feature = "tracing-0-1")]
#[cfg_attr(docsrs, doc(cfg(feature = "tracing-0-1")))]
mod tracing_0_1;

pub use logger::*;
