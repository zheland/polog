//! Middleware‑injected per‑event data.
//!
//! Types in this module describe values that a middleware [`Handler`] can
//! attach to the context of each event and span as it flows through the chain.
//! They are opaque newtypes: `Enricher` fills them in, backends read them via
//! their `context_fields` configuration.
//!
//! [`Handler`]: polog::Handler

use core::time::Duration;

/// Wall‑clock‑independent elapsed time since an arbitrary reference point.
///
/// Produced by [`MonotonicEnricherFn`] and rendered by backends under the
/// `monotonic_time` key.
///
/// [`MonotonicEnricherFn`]: crate::MonotonicEnricherFn
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct MonotonicTime(pub Duration);

/// Total CPU time consumed by the process.
///
/// Reported by the platform in units of [`Duration`] and rendered by backends
/// under the `process_cpu_time` key.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct ProcessCpuTime(pub Duration);

/// Total CPU time consumed by the current thread.
///
/// Reported by the platform in units of [`Duration`] and rendered by backends
/// under the `thread_cpu_time` key.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct ThreadCpuTime(pub Duration);

/// OS process identifier, as reported by the platform.
///
/// Rendered by backends under the `process_id` key.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct ProcessId(pub u64);

/// OS process name, as reported by the platform.
///
/// Rendered by backends under the `process_name` key.
#[cfg(feature = "alloc")]
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct ProcessName(pub alloc::borrow::Cow<'static, str>);

/// OS thread identifier, as reported by the platform.
///
/// Rendered by backends under the `thread_id` key.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct ThreadId(pub u64);

/// OS thread name, as reported by the platform.
///
/// Rendered by backends under the `thread_name` key.
#[cfg(feature = "alloc")]
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct ThreadName(pub alloc::borrow::Cow<'static, str>);

/// Async task identifier.
///
/// Rendered by backends under the `task_id` key.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct TaskId(pub u64);

/// Async task name.
///
/// Rendered by backends under the `task_name` key.
#[cfg(feature = "alloc")]
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct TaskName(pub alloc::borrow::Cow<'static, str>);

/// Identifier of the CPU core the event was recorded on.
///
/// Rendered by backends under the `cpu_id` key.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct CpuId(pub u32);
