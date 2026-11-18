//! Delegators for different log backends to forwards events from `polog` to
//! other log backends.

#[cfg(feature = "log-0-4")]
#[cfg_attr(docsrs, doc(cfg(feature = "log-0-4")))]
pub mod log_0_4;

#[cfg(feature = "slog-2")]
#[cfg_attr(docsrs, doc(cfg(feature = "slog-2")))]
pub mod slog_2;

/// This relies on the doc-hidden `FieldSet::value_set_all`.
/// See the long comment in `tracing_0_1::with_values` function.
#[cfg(all(feature = "std", feature = "tracing-0-1-semver-unstable"))]
#[cfg_attr(
    docsrs,
    doc(cfg(all(feature = "std", feature = "tracing-0-1-semver-unstable")))
)]
pub mod tracing_0_1;
