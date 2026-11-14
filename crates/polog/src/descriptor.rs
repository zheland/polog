use crate::Level;

/// Static metadata describing a log event or span.
///
/// A `Descriptor` is generated once per macro call site and contains the
/// event's level, module path, optional target, message, parameter names,
/// and source location.
/// Because it is `'static`, it can be used for early filtering before
/// constructing log arguments.
///
/// The logging and span macros produce a `static` descriptor per call site,
/// so every event from the same site shares the same `&'static Descriptor`.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Descriptor {
    /// Indicates whether the descriptor is a span or event.
    pub kind: DescriptorKind,

    /// The severity level of this log event.
    pub level: Level,

    /// The module path (e.g., `crate::io::network`) where the log macro was
    /// invoked.
    ///
    /// This is determined at compile time via [`module_path!()`]. It is **not**
    /// affected by `#[track_caller]` and always reflects the module that
    /// contains the macro invocation.
    pub module_path: &'static str,

    /// An optional user-configurable category or target for this log event or
    /// span.
    ///
    /// When this is `None`, backends should treat
    /// [`module_path`](Self::module_path) as the target.
    pub target: Option<&'static str>,

    /// The static human-readable text of this event or span.
    ///
    /// This is the call-site string supplied to the logging or span macro:
    /// a log message for [`DescriptorKind::Log`] events and a span name for
    /// [`DescriptorKind::Span`] spans.
    /// For spans this is the text conventionally exposed as the span's name
    /// by other logging crates.
    ///
    /// When a call site omits the text, backends may fall back to the `@fn`
    /// meta-attribute or to [`location`](Self::location).
    pub message: &'static str,

    /// The names of the structured parameters captured with this log event or
    /// span.
    pub param_names: &'static [&'static str],

    /// The source code location (file, line, column) where this log event or
    /// span was emitted.
    ///
    /// This is typically set to [`Location::caller()`] at the macro expansion
    /// site and respects `#[track_caller]` annotations on the logging
    /// function.
    ///
    /// [`Location::caller()`]: ::core::panic::Location::caller
    pub location: core::panic::Location<'static>,
}

/// The kind of a [`Descriptor`].
///
/// Distinguishes a log event from a span at the descriptor level.
/// Backends that process both kinds through the same handler can branch on this
/// value when formatting or when deciding whether to track a span lifecycle.

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum DescriptorKind {
    /// A log event produced by [`log!`] or a level-specific logging macro such
    /// as [`info!`](crate::info!).
    ///
    /// [`log!`]: crate::log!
    Log,

    /// A span produced by [`span!`], [`owning_span!`], or
    /// [`deferred_span!`](crate::deferred_span!).
    ///
    /// [`span!`]: crate::span!
    ///
    /// [`owning_span!`]: crate::owning_span!
    Span,
}
