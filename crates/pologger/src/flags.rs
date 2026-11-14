//! Bitflag-based configuration for [`Pologger`] rendering.
//!
//! Each flag set is a distinct `bitflags`-generated type. A [`LoggerConfig`]
//! bundles one value from each set. Backends that want a single flag
//! namespace per visual concern use the individual flag types directly;
//! backends that want a single configuration value use `LoggerConfig`.
//!
//! All flags are pure data: changing them has no side effect until the next
//! call to [`Pologger`]'s rendering path, which reads them from the backend's
//! live configuration.
//!
//! [`LoggerConfig`]: crate::LoggerConfig
//! [`Pologger`]: crate::Pologger

bitflags::bitflags! {
    /// Event kinds that a [`Pologger`] can emit.
    ///
    /// Controls which lifecycle events reach the output. Turning off
    /// `NESTED_ENTER`, for example, silences span entries while keeping the
    /// events logged inside the spans.
    ///
    /// [`Pologger`]: crate::Pologger
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct EventKinds: u32 {
        /// Log event emitted by `polog::log!` or a level-specific logging
        /// macro.
        const LOG = 1 << 0;

        /// Nested span entry.
        ///
        /// Emitted when a span created with `polog::LoggerExt::borrowing_span` is entered.
        const NESTED_ENTER = 1 << 1;

        /// Nested span exit.
        ///
        /// Emitted when a `BorrowingSpan` or `OwningSpan` of kind `Nested` is dropped.
        const NESTED_EXIT = 1 << 2;

        /// Detached span entry.
        ///
        /// Emitted when a span created from a `DetachedLogger` or materialized from a `DeferredSpan` is entered.
        const DETACHED_ENTER = 1 << 3;

        /// Detached span exit.
        const DETACHED_EXIT = 1 << 4;

        /// Foreign span creation.
        ///
        /// Emitted by `SharedPologger` when it receives a
        /// `tracing::Subscriber::new_span` call.
        const FOREIGN_NEW = 1 << 5;

        /// Foreign span close.
        ///
        /// Emitted by `SharedPologger` when it receives a
        /// `tracing::Subscriber::try_close` call.
        const FOREIGN_CLOSE = 1 << 6;

        /// Instrumented future poll started.
        ///
        /// Emitted by `PollInstrumented` before the inner future is polled.
        const TASK_POLL = 1 << 7;

        /// Instrumented future returned `Poll::Pending`.
        const TASK_PENDING = 1 << 8;

        /// Instrumented future returned `Poll::Ready`.
        const TASK_READY = 1 << 9;
    }
}

impl EventKinds {
    /// Default set: every event kind except the per-poll signals.
    pub const DEFAULT: Self = Self::empty()
        .union(Self::LOG)
        .union(Self::NESTED_ENTER)
        .union(Self::NESTED_EXIT)
        .union(Self::DETACHED_ENTER)
        .union(Self::DETACHED_EXIT)
        .union(Self::FOREIGN_NEW)
        .union(Self::FOREIGN_CLOSE);

    /// Every event kind, including per-poll signals.
    ///
    /// A convenient preset for backends that render the full `tracing`-style
    /// lifecycle. Coincides with [`ALL`] today.
    /// Kept as a named preset because the two may diverge when new event kinds
    /// are added.
    ///
    /// [`ALL`]: Self::ALL
    pub const TRACING: Self = Self::empty()
        .union(Self::LOG)
        .union(Self::NESTED_ENTER)
        .union(Self::NESTED_EXIT)
        .union(Self::DETACHED_ENTER)
        .union(Self::DETACHED_EXIT)
        .union(Self::FOREIGN_NEW)
        .union(Self::FOREIGN_CLOSE)
        .union(Self::TASK_POLL)
        .union(Self::TASK_PENDING)
        .union(Self::TASK_READY);

    /// Every declared event-kind bit set.
    pub const ALL: Self = Self::TRACING;
}

impl Default for EventKinds {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

bitflags::bitflags! {
    /// Descriptor fields that a [`Pologger`] can render.
    ///
    /// Controls which parts of the static `polog::Descriptor` reach the
    /// output.
    ///
    /// [`Pologger`]: crate::Pologger
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct DescriptorFields: u32 {
        /// `DescriptorKind` (log event or span).
        const KIND = 1 << 0;

        /// `Level` severity.
        const LEVEL = 1 << 1;

        /// `Descriptor::target`, falling back to `module_path` when
        /// [`BehaviorFlags::TARGET_DEFAULT_MODULE_PATH`] is set.
        ///
        /// [`BehaviorFlags::TARGET_DEFAULT_MODULE_PATH`]:
        ///     BehaviorFlags::TARGET_DEFAULT_MODULE_PATH
        const TARGET = 1 << 2;

        /// `Descriptor::module_path`.
        const MODULE_PATH = 1 << 3;

        /// `Descriptor::message`. For spans, the message is the span name.
        const MESSAGE = 1 << 4;

        /// Source file from `Descriptor::location`.
        const FILENAME = 1 << 5;

        /// Source line from `Descriptor::location`.
        const LINE = 1 << 6;

        /// Source column from `Descriptor::location`.
        const COLUMN = 1 << 7;
    }
}

impl DescriptorFields {
    /// Minimum useful set: kind, level, and message.
    pub const MINIMAL: Self = Self::empty()
        .union(Self::KIND)
        .union(Self::LEVEL)
        .union(Self::MESSAGE);

    /// Default set: kind, level, target, and message.
    pub const DEFAULT: Self = Self::empty()
        .union(Self::KIND)
        .union(Self::LEVEL)
        .union(Self::TARGET)
        .union(Self::MESSAGE);

    /// Every declared descriptor-field bit set.
    pub const ALL: Self = Self::empty()
        .union(Self::KIND)
        .union(Self::LEVEL)
        .union(Self::TARGET)
        .union(Self::MODULE_PATH)
        .union(Self::MESSAGE)
        .union(Self::FILENAME)
        .union(Self::LINE)
        .union(Self::COLUMN);
}

impl Default for DescriptorFields {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

bitflags::bitflags! {
    /// Per-call-site fields that a [`Pologger`] can render.
    ///
    /// [`Pologger`]: crate::Pologger
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct SiteFields: u32 {
        /// Parent span ID.
        const PARENT_SPAN_ID = 1 << 0;

        /// The event's own span ID, or `Root` for events logged outside any
        /// span.
        const SPAN_ID = 1 << 1;

        /// The structured key-value argument list.
        const ARGUMENTS = 1 << 2;
    }
}

impl SiteFields {
    /// Only the argument list.
    pub const MINIMAL: Self = Self::ARGUMENTS;

    /// Every site field.
    pub const DEFAULT: Self = Self::empty()
        .union(Self::PARENT_SPAN_ID)
        .union(Self::SPAN_ID)
        .union(Self::ARGUMENTS);

    /// Every declared site-field bit set.
    pub const ALL: Self = Self::DEFAULT;
}

impl Default for SiteFields {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

bitflags::bitflags! {
    /// Middleware-injected context fields that a [`Pologger`] can render.
    ///
    /// Context values are supplied by `polog_util::Enricher` middleware.
    /// A field is silently omitted when the corresponding enricher is not in
    /// the chain.
    ///
    /// [`Pologger`]: crate::Pologger
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct ContextFields: u64 {
        /// Wall-clock time in UTC.
        const UTC_TIME = 1 << 0;
        /// UTC time with millisecond precision.
        const UTC_TIME_MILLIS = 1 << 1;
        /// UTC time with microsecond precision.
        const UTC_TIME_MICROS = 1 << 2;
        /// UTC time with nanosecond precision.
        const UTC_TIME_NANOS = 1 << 3;

        /// Wall-clock time in the local timezone.
        const LOCAL_TIME = 1 << 4;
        /// Local time with millisecond precision.
        const LOCAL_TIME_MILLIS = 1 << 5;
        /// Local time with microsecond precision.
        const LOCAL_TIME_MICROS = 1 << 6;
        /// Local time with nanosecond precision.
        const LOCAL_TIME_NANOS = 1 << 7;

        /// Monotonic time since a per-enricher reference point.
        ///
        /// Only meaningful when a `MonotonicEnricher` is in the chain.
        const MONOTONIC_TIME = 1 << 8;
        /// Monotonic time with millisecond precision.
        const MONOTONIC_TIME_MILLIS = 1 << 9;
        /// Monotonic time with microsecond precision.
        const MONOTONIC_TIME_MICROS = 1 << 10;
        /// Monotonic time with nanosecond precision.
        const MONOTONIC_TIME_NANOS = 1 << 11;

        /// Process CPU time.
        ///
        /// Reading this value is expensive on most platforms. Capture it
        /// only when this flag is enabled.
        const PROCESS_CPU_TIME = 1 << 12;
        /// Process CPU time with millisecond precision.
        const PROCESS_CPU_TIME_MILLIS = 1 << 13;
        /// Process CPU time with microsecond precision.
        const PROCESS_CPU_TIME_MICROS = 1 << 14;
        /// Process CPU time with nanosecond precision.
        const PROCESS_CPU_TIME_NANOS = 1 << 15;

        /// Thread CPU time.
        ///
        /// Reading this value is expensive on most platforms. Capture it
        /// only when this flag is enabled.
        const THREAD_CPU_TIME = 1 << 16;
        /// Thread CPU time with millisecond precision.
        const THREAD_CPU_TIME_MILLIS = 1 << 17;
        /// Thread CPU time with microsecond precision.
        const THREAD_CPU_TIME_MICROS = 1 << 18;
        /// Thread CPU time with nanosecond precision.
        const THREAD_CPU_TIME_NANOS = 1 << 19;

        /// Local timezone offset from UTC.
        const LOCAL_UTC_OFFSET = 1 << 25;

        /// OS process ID.
        const PROCESS_ID = 1 << 32;
        /// OS process name.
        const PROCESS_NAME = 1 << 33;
        /// OS thread ID.
        const THREAD_ID = 1 << 34;
        /// OS thread name.
        const THREAD_NAME = 1 << 35;
        /// Async task ID.
        const TASK_ID = 1 << 36;
        /// Async task name.
        const TASK_NAME = 1 << 37;
        /// CPU core identifier.
        const CPU_ID = 1 << 38;
    }
}

impl ContextFields {
    /// Default set: UTC time with microsecond precision.
    pub const DEFAULT: Self = Self::empty()
        .union(Self::UTC_TIME)
        .union(Self::UTC_TIME_MICROS);

    /// Cheap-to-capture fields: identity and time, but no per-process CPU.
    pub const CHEAP: Self = Self::empty()
        .union(Self::UTC_TIME)
        .union(Self::LOCAL_TIME)
        .union(Self::LOCAL_UTC_OFFSET)
        .union(Self::MONOTONIC_TIME)
        .union(Self::PROCESS_ID)
        .union(Self::THREAD_ID)
        .union(Self::THREAD_NAME)
        .union(Self::TASK_ID)
        .union(Self::TASK_NAME)
        .union(Self::CPU_ID);

    /// Fields whose capture is expensive on most platforms.
    pub const COSTLY: Self = Self::empty()
        .union(Self::PROCESS_CPU_TIME)
        .union(Self::THREAD_CPU_TIME);

    /// Every declared context-field bit set.
    pub const ALL: Self = Self::empty()
        .union(Self::UTC_TIME)
        .union(Self::UTC_TIME_MILLIS)
        .union(Self::UTC_TIME_MICROS)
        .union(Self::UTC_TIME_NANOS)
        .union(Self::LOCAL_TIME)
        .union(Self::LOCAL_TIME_MILLIS)
        .union(Self::LOCAL_TIME_MICROS)
        .union(Self::LOCAL_TIME_NANOS)
        .union(Self::MONOTONIC_TIME)
        .union(Self::MONOTONIC_TIME_MILLIS)
        .union(Self::MONOTONIC_TIME_MICROS)
        .union(Self::MONOTONIC_TIME_NANOS)
        .union(Self::PROCESS_CPU_TIME)
        .union(Self::PROCESS_CPU_TIME_MILLIS)
        .union(Self::PROCESS_CPU_TIME_MICROS)
        .union(Self::PROCESS_CPU_TIME_NANOS)
        .union(Self::THREAD_CPU_TIME)
        .union(Self::THREAD_CPU_TIME_MILLIS)
        .union(Self::THREAD_CPU_TIME_MICROS)
        .union(Self::THREAD_CPU_TIME_NANOS)
        .union(Self::LOCAL_UTC_OFFSET)
        .union(Self::PROCESS_ID)
        .union(Self::PROCESS_NAME)
        .union(Self::THREAD_ID)
        .union(Self::THREAD_NAME)
        .union(Self::TASK_ID)
        .union(Self::TASK_NAME)
        .union(Self::CPU_ID);
}

impl Default for ContextFields {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

bitflags::bitflags! {
    /// Derived fields that a [`Pologger`] can render.
    ///
    /// These are not read from a `Descriptor` or from a context value. They
    /// are computed by the backend from cached span state, so they are
    /// useful only when [`BehaviorFlags::CACHED_PARENT_SPAN_IDS`] is also
    /// enabled.
    ///
    /// [`Pologger`]: crate::Pologger
    /// [`BehaviorFlags::CACHED_PARENT_SPAN_IDS`]:
    ///     BehaviorFlags::CACHED_PARENT_SPAN_IDS
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct DerivedFields: u32 {
        /// Kind glyph for each span in the `span_path` array.
        const SPAN_PATH_KIND = 1 << 0;
        /// Span ID for each span in the `span_path` array.
        const SPAN_PATH_ID = 1 << 1;
        /// Span name for each span in the `span_path` array.
        const SPAN_PATH_NAME = 1 << 2;
    }
}

impl DerivedFields {
    /// No derived fields.
    pub const DEFAULT: Self = Self::empty();

    /// Every `span_path` component.
    pub const SPAN_PATH: Self = Self::empty()
        .union(Self::SPAN_PATH_KIND)
        .union(Self::SPAN_PATH_ID)
        .union(Self::SPAN_PATH_NAME);

    /// Every declared derived-field bit set.
    pub const ALL: Self = Self::SPAN_PATH;
}

impl Default for DerivedFields {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

bitflags::bitflags! {
    /// Non-visual behaviors of a [`Pologger`].
    ///
    /// These flags change what is computed, not just what is rendered. They
    /// are typically left at `DEFAULT`.
    ///
    /// [`Pologger`]: crate::Pologger
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct BehaviorFlags: u32 {
        /// Fall back to `SystemTime::now()` when no `SystemTime` is present
        /// in the context.
        ///
        /// Enables wall-clock output for the default `LoggerConfig` without
        /// requiring a `SystemTimeEnricher` in the chain.
        const DEFAULT_SYSTEM_TIME = 1 << 0;

        /// Fall back to `Descriptor::module_path` when
        /// `Descriptor::target` is `None`.
        const TARGET_DEFAULT_MODULE_PATH = 1 << 1;

        /// Cache parent span IDs across the span lifecycle so that
        /// `exit_span` can render `parent_span_id` without re-entering the
        /// parent.
        const CACHED_PARENT_SPAN_IDS = 1 << 2;

        /// Use the `@fn` meta-attribute as the message when the message is
        /// empty.
        const USE_FN_META_ATTRIBUTE = 1 << 3;

        /// Use the source location as the message when both the message and
        /// `@fn` are unavailable.
        const USE_LOCATION_AS_DEFAULT_MESSAGE = 1 << 4;
    }
}

impl BehaviorFlags {
    /// All behaviors enabled.
    pub const DEFAULT: Self = Self::empty()
        .union(Self::DEFAULT_SYSTEM_TIME)
        .union(Self::TARGET_DEFAULT_MODULE_PATH)
        .union(Self::CACHED_PARENT_SPAN_IDS)
        .union(Self::USE_FN_META_ATTRIBUTE)
        .union(Self::USE_LOCATION_AS_DEFAULT_MESSAGE);

    /// Every declared behavior bit set.
    pub const ALL: Self = Self::DEFAULT;
}

impl Default for BehaviorFlags {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}
