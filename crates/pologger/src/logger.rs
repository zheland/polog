use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Debug;
use core::hint::cold_path;
use core::panic::Location;
use core::time::Duration;
use core::{fmt, mem};
use std::collections::{HashMap, hash_map};
use std::io;
use std::sync::Arc;
use std::time::SystemTime;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use parking_lot::RwLock;
use polog::{
    BaseHandler, BorrowingHandler, ContextArgs, Descriptor, DisplayableMixedArgs, DynHandler,
    Handler, Level, LevelFilter, LogArgs, SpanId, SpanIdGenerator, SpanKind, StaticLogArgs,
};
use polog_util::{
    BaseExternalHandler, ExternalArgs, ExternalArgsVisitor, ExternalDescriptor, ExternalHandler,
    MixedArgsExt, context,
};

use crate::{
    Formatter, FormatterDurationKind, FormatterEventKind, FormatterIdKind, FormatterSpanIdKind,
    FormatterStrKind, FormatterTimeKind, KnownSpanId, MaybeKnownSpanId, TimePrecision, flags,
};

/// The core logging backend.
///
/// `Pologger` owns a [`Formatter`], a shared [`SpanIdGenerator`], and the
/// [`LoggerConfig`] that decide what gets rendered.
/// It implements the `polog::Handler` family and is intended to be wrapped in a
/// `SharedHandler` before use as a `polog::Logger`: either
/// `polog_util::Shared` for synchronous use, or
/// `polog_util::OffloaderHandle` for asynchronous use.
///
/// [`Formatter`]: crate::Formatter
/// [`SpanIdGenerator`]: polog::SpanIdGenerator
/// [`LoggerConfig`]: crate::LoggerConfig
#[derive(Debug)]
pub struct Pologger<F> {
    span_id_generator: Arc<SpanIdGenerator>,
    formatter: F,
    config: Arc<RwLock<LoggerConfig>>,
    cache: Cache,
    scratch: Scratch,
}

/// Rendering configuration for a [`Pologger`].
///
/// Each field is an independent flag set.
/// The field names match the flag modules under [`crate::flags`].
/// Use the presets for the common shapes, or construct the struct
/// field-by-field for full control.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct LoggerConfig {
    /// Which event kinds to emit.
    pub event_kinds: flags::EventKinds,
    /// Which `Descriptor` fields to render.
    pub descriptor_fields: flags::DescriptorFields,
    /// Which per-call-site fields to render.
    pub site_fields: flags::SiteFields,
    /// Which middleware-injected context fields to render.
    pub context_fields: flags::ContextFields,
    /// Which derived fields to render.
    pub derived_fields: flags::DerivedFields,
    /// Non-visual behaviors.
    pub behavior_flags: flags::BehaviorFlags,
}

#[derive(Clone, Debug)]
struct Entry<'a, Args, Context> {
    descriptor: &'static Descriptor,
    args: &'a Args,
    parent_span_id: MaybeKnownSpanId,
    span_id: KnownSpanId,
    kind: FormatterEventKind,
    context: &'a Context,
}

#[derive(Clone, Debug)]
struct ExternalEntry<'a, Desc, Args, Context> {
    descriptor: &'a Desc,
    args: &'a Args,
    parent_span_id: MaybeKnownSpanId,
    span_id: KnownSpanId,
    kind: FormatterEventKind,
    context: &'a Context,
}

#[derive(Clone, Eq, PartialEq, Default, Debug)]
struct Cache {
    parent_span_ids: HashMap<SpanId, Option<SpanId>>,
    span_paths: HashMap<SpanId, SpanPathCacheEntry>,
    fn_meta_attributes: HashMap<&'static Descriptor, Cow<'static, str>>,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
struct SpanPathCacheEntry {
    descriptor: Option<&'static Descriptor>,
    parent_span_id: Option<SpanId>,
    kind: SpanPathSpanKind,
    users: usize,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
enum SpanPathSpanKind {
    Nested,
    Detached,
    External,
}

#[derive(Default, Debug)]
struct Scratch {
    string1: String,
    span_id_vec: Vec<SpanId>,
}

impl LoggerConfig {
    /// Creates a default set of flags.
    ///
    /// Identical to `LoggerConfig::default()`.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The default compact preset.
    ///
    /// Equivalent to `LoggerConfig::default()`.
    #[inline]
    #[must_use]
    pub fn compact() -> Self {
        Self::default()
    }

    /// Compact rendering with inline span ID/name paths.
    ///
    /// Drops the standalone parent-span-ID, span-ID, and kind-glyph columns
    /// because the path already emits them.
    #[inline]
    #[must_use]
    pub fn compact_span_path() -> Self {
        let mut this = Self::compact();
        this.derived_fields |= flags::DerivedFields::SPAN_PATH_KIND
            | flags::DerivedFields::SPAN_PATH_ID
            | flags::DerivedFields::SPAN_PATH_NAME;

        // Path renderer emits its own IDs and |> / >> glyphs, so the standalone
        // columns are redundant.
        this.site_fields &= !(flags::SiteFields::PARENT_SPAN_ID | flags::SiteFields::SPAN_ID);
        this.descriptor_fields &= !flags::DescriptorFields::KIND;
        this
    }

    /// Extends the current flags with the per-poll event kinds.
    ///
    /// Adds `TASK_POLL`, `TASK_PENDING`, and `TASK_READY` to `event_kinds`.
    /// Combine with [`compact`] or [`compact_span_path`].
    ///
    /// [`compact`]: Self::compact
    /// [`compact_span_path`]: Self::compact_span_path
    #[inline]
    #[must_use]
    pub fn with_task_poll_events(mut self) -> Self {
        self.event_kinds |= flags::EventKinds::TASK_POLL
            | flags::EventKinds::TASK_PENDING
            | flags::EventKinds::TASK_READY;
        self
    }
}

impl<F> Pologger<F> {
    /// Creates a backend that reads its rendering configuration from
    /// `flags`.
    ///
    /// Sharing the generator is what keeps span IDs unique across `polog`
    /// events and foreign-facade events.
    ///
    /// `flags` is shared: every clone of this handle — held here, by a
    /// [`LoggerConfig`], by a config-watcher task, or by another `Pologger` —
    /// observes the same value. A write through any handle takes effect on the
    /// next event of every reader.
    ///
    /// For the common case — one `Pologger` with default flags and no
    /// dynamic reconfiguration — prefer the builder, which wraps the value
    /// internally and returns a [`LoggerConfig`] handle for the dynamic case.
    ///
    /// [`LoggerConfig`]: crate::LoggerConfig
    #[inline]
    #[must_use]
    pub fn new(
        span_id_generator: Arc<SpanIdGenerator>,
        formatter: F,
        config: Arc<RwLock<LoggerConfig>>,
    ) -> Self {
        Self {
            span_id_generator,
            formatter,
            config,
            cache: Cache::default(),
            scratch: Scratch::default(),
        }
    }

    /// Returns a reference to the wrapped formatter.
    #[inline]
    #[must_use]
    pub const fn formatter(&self) -> &F {
        &self.formatter
    }

    /// Returns a mutable reference to the wrapped formatter.
    #[inline]
    #[must_use]
    pub const fn formatter_mut(&mut self) -> &mut F {
        &mut self.formatter
    }

    /// Returns the shared logger config handle.
    ///
    /// Mutating the inner `LoggerConfig` takes effect on the next event
    /// processed.
    #[inline]
    #[must_use]
    pub const fn config(&self) -> &Arc<RwLock<LoggerConfig>> {
        &self.config
    }

    #[inline]
    fn log_entry<'a, Args, Context>(
        &mut self,
        entry: &Entry<'_, Args, Context>,
        flags: LoggerConfig,
    ) where
        F: Formatter,
        Args: DisplayableMixedArgs<'a>,
        Context: ContextArgs,
    {
        // Silently ignore errors.
        let _result = self.try_log_entry(entry, flags);
    }

    #[inline]
    fn try_log_entry<'a, Args, Context>(
        &mut self,
        entry: &Entry<'_, Args, Context>,
        flags: LoggerConfig,
    ) -> Result<(), io::Error>
    where
        F: Formatter,
        Args: DisplayableMixedArgs<'a>,
        Context: ContextArgs,
    {
        if !is_event_enabled(entry.kind, flags) {
            return Ok(());
        }

        let fmt = &mut self.formatter;
        fmt.update_config();
        fmt.start_entry()?;

        write_time(fmt, entry.context, &flags)?;
        write_durations(fmt, entry.context, &flags)?;
        write_context_units(fmt, entry.context, &flags)?;

        write_parent_span_id(
            fmt,
            entry.parent_span_id,
            entry.span_id,
            &flags,
            &self.cache,
        )?;
        write_kind(fmt, entry.kind, &flags)?;
        write_span_id(fmt, entry.span_id, &flags)?;
        write_level(fmt, entry.descriptor.level, &flags)?;
        write_span_path(
            fmt,
            entry.span_id,
            entry.kind,
            &flags,
            &self.cache,
            &mut self.scratch,
        )?;
        write_target(
            fmt,
            entry.descriptor.target,
            Some(entry.descriptor.module_path),
            entry.kind,
            &flags,
        )?;
        write_location(
            fmt,
            Some(entry.descriptor.location.file()),
            Some(entry.descriptor.location.line()),
            Some(entry.descriptor.location.column()),
            &flags,
        )?;
        write_message(fmt, entry, &flags, &self.cache)?;
        write_args(fmt, entry, &flags)?;

        fmt.end_entry()?;
        Ok(())
    }

    /// Renders a foreign event through this backend.
    ///
    /// The event's fields are visited in order and formatted exactly as a
    /// native `polog` event would be.
    #[inline]
    fn log_external_entry<Desc, Args, Context>(
        &mut self,
        entry: &ExternalEntry<'_, Desc, Args, Context>,
        flags: LoggerConfig,
    ) where
        F: Formatter,
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
        Context: ContextArgs,
    {
        // Silently ignore errors.
        let _result = self.try_log_external_entry(entry, flags);
    }

    #[inline]
    fn try_log_external_entry<Desc, Args, Context>(
        &mut self,
        entry: &ExternalEntry<'_, Desc, Args, Context>,
        flags: LoggerConfig,
    ) -> Result<(), io::Error>
    where
        F: Formatter,
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
        Context: ContextArgs,
    {
        if !is_event_enabled(entry.kind, flags) {
            return Ok(());
        }

        self.formatter.update_config();
        self.formatter.start_entry()?;
        let fmt = &mut self.formatter;

        write_time(fmt, entry.context, &flags)?;
        write_durations(fmt, entry.context, &flags)?;
        write_context_units(fmt, entry.context, &flags)?;

        write_parent_span_id(
            fmt,
            entry.parent_span_id,
            entry.span_id,
            &flags,
            &self.cache,
        )?;
        write_kind(fmt, entry.kind, &flags)?;
        write_span_id(fmt, entry.span_id, &flags)?;
        write_level(fmt, entry.descriptor.level(), &flags)?;
        write_span_path(
            fmt,
            entry.span_id,
            entry.kind,
            &flags,
            &self.cache,
            &mut self.scratch,
        )?;
        write_target(
            fmt,
            entry.descriptor.target().map(|value| value.as_str()),
            entry.descriptor.module_path().map(|value| value.as_str()),
            entry.kind,
            &flags,
        )?;
        write_location(
            fmt,
            entry.descriptor.file().map(|value| value.as_str()),
            entry.descriptor.line(),
            entry.descriptor.column(),
            &flags,
        )?;
        write_external_message_fmt(fmt, entry, &flags)?;
        write_external_args(fmt, entry.args, &flags)?;

        fmt.end_entry()?;
        Ok(())
    }

    #[inline]
    fn parse_fn_meta_attr<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        flags: LoggerConfig,
    ) where
        Args: LogArgs<'a>,
    {
        if !descriptor.message.is_empty() {
            return;
        }

        let behavior_flags = flags.behavior_flags;
        if !behavior_flags.contains(flags::BehaviorFlags::USE_FN_META_ATTRIBUTE) {
            return;
        }

        for param_idx in 0..args.num_args() {
            let Some(&arg_name) = descriptor.param_names.get(param_idx) else {
                break;
            };

            if arg_name == "@fn" {
                self.scratch.string1.clear();
                let result = args.display_fmt_nth(&mut self.scratch.string1, param_idx);

                if result == Some(Ok(())) {
                    self.cache.cache_fn_meta_attribute(
                        descriptor,
                        Cow::Owned(mem::take(&mut self.scratch.string1)),
                    );
                }
            }
        }
    }

    #[inline]
    fn parse_static_fn_meta_attr<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        flags: LoggerConfig,
    ) where
        Args: StaticLogArgs,
    {
        if !descriptor.message.is_empty() {
            return;
        }

        let behavior_flags = flags.behavior_flags;
        if !behavior_flags.contains(flags::BehaviorFlags::USE_FN_META_ATTRIBUTE) {
            return;
        }

        for param_idx in 0..args.num_args() {
            let Some(&arg_name) = descriptor.param_names.get(param_idx) else {
                break;
            };

            if arg_name == "@fn" {
                let value: Option<&&'static str> = args.try_nth_arg_as(param_idx);
                if let Some(value) = value {
                    self.cache
                        .cache_fn_meta_attribute(descriptor, Cow::Borrowed(*value));
                } else {
                    self.scratch.string1.clear();
                    let result = args.display_fmt_nth(&mut self.scratch.string1, param_idx);

                    if result == Some(Ok(())) {
                        self.cache.cache_fn_meta_attribute(
                            descriptor,
                            Cow::Owned(mem::take(&mut self.scratch.string1)),
                        );
                    }
                }
            }
        }
    }

    /// Returns the shared span-ID generator used by this backend.
    #[inline]
    #[must_use]
    pub const fn span_id_generator(&self) -> &Arc<SpanIdGenerator> {
        &self.span_id_generator
    }
}

impl<F> BaseHandler for Pologger<F>
where
    F: Formatter,
{
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        self.span_id_generator.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {
        let result = self.formatter.flush_blocking();
        drop(result);
    }

    #[inline]
    fn flush_on_panic(&mut self) {
        self.formatter.flush_on_panic();
    }
}

impl<F, Context> DynHandler<Context> for Pologger<F>
where
    F: Formatter,
    Context: ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    ) {
        let flags = *self.config.read();

        self.log_entry(
            &Entry {
                descriptor,
                args: &(),
                parent_span_id: MaybeKnownSpanId::Unknown,
                span_id: KnownSpanId::SpanId(span_id),
                kind: match span_kind {
                    SpanKind::Nested => FormatterEventKind::NestedExit,
                    SpanKind::Detached => FormatterEventKind::DetachedExit,
                },
                context: &context,
            },
            flags,
        );

        if flags
            .behavior_flags
            .contains(flags::BehaviorFlags::CACHED_PARENT_SPAN_IDS)
        {
            self.cache.remove_parent_span_id_on_span_exit(span_id);
        }

        if flags.derived_fields.intersects(
            flags::DerivedFields::SPAN_PATH_KIND
                | flags::DerivedFields::SPAN_PATH_ID
                | flags::DerivedFields::SPAN_PATH_NAME,
        ) {
            self.cache.remove_parent_span_path_on_span_exit(span_id);
        }
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        let flags = *self.config.read();

        self.log_entry(
            &Entry {
                descriptor,
                args: &(),
                parent_span_id: MaybeKnownSpanId::Unknown,
                span_id: KnownSpanId::SpanId(span_id),
                kind: FormatterEventKind::TaskPoll,
                context: &context,
            },
            flags,
        );
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        let flags = *self.config.read();

        self.log_entry(
            &Entry {
                descriptor,
                args: &(),
                parent_span_id: MaybeKnownSpanId::Unknown,
                span_id: KnownSpanId::SpanId(span_id),
                kind: FormatterEventKind::TaskPending,
                context: &context,
            },
            flags,
        );
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        let flags = *self.config.read();

        self.log_entry(
            &Entry {
                descriptor,
                args: &(),
                parent_span_id: MaybeKnownSpanId::Unknown,
                span_id: KnownSpanId::SpanId(span_id),
                kind: FormatterEventKind::TaskReady,
                context: &context,
            },
            flags,
        );
    }
}

impl<F, Context> Handler<Context> for Pologger<F>
where
    F: Formatter,
    Context: ContextArgs,
{
    type EnrichedContext = Context;

    #[inline]
    fn try_enable(
        &self,
        _descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        Some(context)
    }

    #[inline]
    fn log<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        context: Context,
    ) where
        Args: LogArgs<'a>,
    {
        let flags = *self.config.read();

        let known_span_id = span_id.map_or(KnownSpanId::Root, KnownSpanId::SpanId);
        self.parse_fn_meta_attr(descriptor, &args, flags);

        self.log_entry(
            &Entry {
                descriptor,
                args: &args,
                parent_span_id: MaybeKnownSpanId::Unknown,
                span_id: known_span_id,
                kind: FormatterEventKind::Log,
                context: &context,
            },
            flags,
        );
    }

    #[inline]
    fn enter_span<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    ) where
        Args: LogArgs<'a>,
    {
        let flags = *self.config.read();

        self.parse_fn_meta_attr(descriptor, &args, flags);

        if flags
            .behavior_flags
            .contains(flags::BehaviorFlags::CACHED_PARENT_SPAN_IDS)
        {
            self.cache
                .cache_parent_span_id_on_span_enter(parent_span_id, span_id);
        }

        if flags.derived_fields.intersects(
            flags::DerivedFields::SPAN_PATH_KIND
                | flags::DerivedFields::SPAN_PATH_ID
                | flags::DerivedFields::SPAN_PATH_NAME,
        ) {
            self.cache.cache_parent_span_path_on_span_enter(
                Some(descriptor),
                parent_span_id,
                span_id,
                span_kind.into(),
            );
        }

        self.log_entry(
            &Entry {
                descriptor,
                args: &args,
                parent_span_id: parent_span_id
                    .map_or(MaybeKnownSpanId::Root, MaybeKnownSpanId::SpanId),
                span_id: KnownSpanId::SpanId(span_id),
                kind: match span_kind {
                    SpanKind::Nested => FormatterEventKind::NestedEnter,
                    SpanKind::Detached => FormatterEventKind::DetachedEnter,
                },

                context: &context,
            },
            flags,
        );
    }

    #[inline]
    fn log_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        context: Context,
    ) where
        Args: StaticLogArgs,
    {
        let flags = *self.config.read();

        let known_span_id = span_id.map_or(KnownSpanId::Root, KnownSpanId::SpanId);
        self.parse_static_fn_meta_attr(descriptor, &args, flags);

        self.log_entry(
            &Entry {
                descriptor,
                args: &args,
                parent_span_id: MaybeKnownSpanId::Unknown,
                span_id: known_span_id,
                kind: FormatterEventKind::Log,
                context: &context,
            },
            flags,
        );
    }

    #[inline]
    fn enter_span_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    ) where
        Args: StaticLogArgs,
    {
        let flags = *self.config.read();

        self.parse_static_fn_meta_attr(descriptor, &args, flags);

        if flags
            .behavior_flags
            .contains(flags::BehaviorFlags::CACHED_PARENT_SPAN_IDS)
        {
            self.cache
                .cache_parent_span_id_on_span_enter(parent_span_id, span_id);
        }

        if flags.derived_fields.intersects(
            flags::DerivedFields::SPAN_PATH_KIND
                | flags::DerivedFields::SPAN_PATH_ID
                | flags::DerivedFields::SPAN_PATH_NAME,
        ) {
            self.cache.cache_parent_span_path_on_span_enter(
                Some(descriptor),
                parent_span_id,
                span_id,
                span_kind.into(),
            );
        }

        self.log_entry(
            &Entry {
                descriptor,
                args: &args,
                parent_span_id: parent_span_id
                    .map_or(MaybeKnownSpanId::Root, MaybeKnownSpanId::SpanId),
                span_id: KnownSpanId::SpanId(span_id),
                kind: match span_kind {
                    SpanKind::Nested => FormatterEventKind::NestedEnter,
                    SpanKind::Detached => FormatterEventKind::DetachedEnter,
                },

                context: &context,
            },
            flags,
        );
    }
}

impl<F, Context> BorrowingHandler<Context> for Pologger<F>
where
    F: Formatter,
    Context: ContextArgs,
{
    #[inline]
    fn log_borrowed<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        span_id: Option<SpanId>,
        context: &Context,
    ) where
        Args: StaticLogArgs,
    {
        let flags = *self.config.read();

        let known_span_id = span_id.map_or(KnownSpanId::Root, KnownSpanId::SpanId);

        self.log_entry(
            &Entry {
                descriptor,
                args,
                parent_span_id: MaybeKnownSpanId::Unknown,
                span_id: known_span_id,
                kind: FormatterEventKind::Log,
                context,
            },
            flags,
        );
    }

    #[inline]
    fn enter_span_borrowed<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: &Context,
    ) where
        Args: StaticLogArgs,
    {
        let flags = *self.config.read();

        self.parse_static_fn_meta_attr(descriptor, args, flags);

        if flags
            .behavior_flags
            .contains(flags::BehaviorFlags::CACHED_PARENT_SPAN_IDS)
        {
            self.cache
                .cache_parent_span_id_on_span_enter(parent_span_id, span_id);
        }

        if flags.derived_fields.intersects(
            flags::DerivedFields::SPAN_PATH_KIND
                | flags::DerivedFields::SPAN_PATH_ID
                | flags::DerivedFields::SPAN_PATH_NAME,
        ) {
            self.cache.cache_parent_span_path_on_span_enter(
                Some(descriptor),
                parent_span_id,
                span_id,
                span_kind.into(),
            );
        }

        self.log_entry(
            &Entry {
                descriptor,
                args,
                parent_span_id: parent_span_id
                    .map_or(MaybeKnownSpanId::Root, MaybeKnownSpanId::SpanId),
                span_id: KnownSpanId::SpanId(span_id),
                kind: match span_kind {
                    SpanKind::Nested => FormatterEventKind::NestedEnter,
                    SpanKind::Detached => FormatterEventKind::DetachedEnter,
                },

                context,
            },
            flags,
        );
    }
}

impl<F> BaseExternalHandler for Pologger<F>
where
    F: Formatter,
{
    #[inline]
    fn max_level_external(&self) -> LevelFilter {
        LevelFilter::ALL
    }
}

impl<F, Context> ExternalHandler<Context> for Pologger<F>
where
    F: Formatter,
    Context: ContextArgs,
{
    #[inline]
    fn try_enable_external<Desc>(
        &self,
        _descriptor: &Desc,
        context: Context,
    ) -> Option<Self::EnrichedContext>
    where
        Desc: ExternalDescriptor,
    {
        Some(context)
    }

    #[inline]
    fn log_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        span_id: Option<SpanId>,
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let flags = *self.config.read();

        self.log_external_entry(
            &ExternalEntry {
                descriptor,
                args,
                parent_span_id: MaybeKnownSpanId::Unknown,
                span_id: span_id.map_or(KnownSpanId::Root, KnownSpanId::SpanId),
                kind: FormatterEventKind::Log,
                context: &context,
            },
            flags,
        );
    }

    #[inline]
    fn new_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let flags = *self.config.read();

        if flags
            .behavior_flags
            .contains(flags::BehaviorFlags::CACHED_PARENT_SPAN_IDS)
        {
            self.cache
                .cache_parent_span_id_on_span_enter(parent_span_id, span_id);
        }

        if flags
            .derived_fields
            .intersects(flags::DerivedFields::SPAN_PATH_KIND | flags::DerivedFields::SPAN_PATH_ID)
        {
            self.cache.cache_parent_span_path_on_span_enter(
                None,
                parent_span_id,
                span_id,
                SpanPathSpanKind::External,
            );
        }

        self.log_external_entry(
            &ExternalEntry {
                descriptor,
                args,
                parent_span_id: parent_span_id
                    .map_or(MaybeKnownSpanId::Root, MaybeKnownSpanId::SpanId),
                span_id: KnownSpanId::SpanId(span_id),
                kind: FormatterEventKind::ExternalNew,
                context: &context,
            },
            flags,
        );
    }

    #[inline]
    fn enter_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Self::EnrichedContext,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let flags = *self.config.read();

        if flags
            .behavior_flags
            .contains(flags::BehaviorFlags::CACHED_PARENT_SPAN_IDS)
        {
            self.cache
                .cache_parent_span_id_on_span_enter(parent_span_id, span_id);
        }

        if flags
            .derived_fields
            .intersects(flags::DerivedFields::SPAN_PATH_KIND | flags::DerivedFields::SPAN_PATH_ID)
        {
            self.cache.cache_parent_span_path_on_span_enter(
                None,
                parent_span_id,
                span_id,
                SpanPathSpanKind::External,
            );
        }

        self.log_external_entry(
            &ExternalEntry {
                descriptor,
                args,
                parent_span_id: parent_span_id
                    .map_or(MaybeKnownSpanId::Root, MaybeKnownSpanId::SpanId),
                span_id: KnownSpanId::SpanId(span_id),
                kind: FormatterEventKind::DetachedEnter,
                context: &context,
            },
            flags,
        );
    }

    #[inline]
    fn exit_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        let flags = *self.config.read();

        self.log_external_entry(
            &ExternalEntry {
                descriptor,
                args: &(),
                parent_span_id: MaybeKnownSpanId::Unknown,
                span_id: KnownSpanId::SpanId(span_id),
                kind: FormatterEventKind::DetachedExit,
                context: &context,
            },
            flags,
        );

        if flags
            .behavior_flags
            .contains(flags::BehaviorFlags::CACHED_PARENT_SPAN_IDS)
        {
            self.cache.remove_parent_span_id_on_span_exit(span_id);
        }

        if flags
            .derived_fields
            .intersects(flags::DerivedFields::SPAN_PATH_KIND | flags::DerivedFields::SPAN_PATH_ID)
        {
            self.cache.remove_parent_span_path_on_span_exit(span_id);
        }
    }

    #[inline]
    fn close_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        let flags = *self.config.read();

        self.log_external_entry(
            &ExternalEntry {
                descriptor,
                args: &(),
                parent_span_id: MaybeKnownSpanId::Unknown,
                span_id: KnownSpanId::SpanId(span_id),
                kind: FormatterEventKind::ExternalClose,
                context: &context,
            },
            flags,
        );
    }

    #[inline]
    fn record_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let flags = *self.config.read();

        self.log_external_entry(
            &ExternalEntry {
                descriptor,
                args: &args,
                parent_span_id: MaybeKnownSpanId::Unknown,
                span_id: KnownSpanId::SpanId(span_id),
                kind: FormatterEventKind::ExternalRecord,
                context: &context,
            },
            flags,
        );
    }

    #[expect(clippy::similar_names, reason = "better than alternatives")]
    #[inline]
    fn link_span_external<Desc>(
        &mut self,
        descriptor: &Desc,
        followed_span_id: SpanId,
        follower_span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
    {
        let flags = *self.config.read();

        self.log_external_entry(
            &ExternalEntry {
                descriptor,
                args: &(),
                parent_span_id: MaybeKnownSpanId::SpanId(followed_span_id),
                span_id: KnownSpanId::SpanId(follower_span_id),
                kind: FormatterEventKind::ExternalLink,
                context: &context,
            },
            flags,
        );
    }
}

impl Cache {
    #[inline]
    pub fn cache_parent_span_id_on_span_enter(
        &mut self,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
    ) {
        let _prev = self.parent_span_ids.insert(span_id, parent_span_id);
    }

    #[inline]
    pub fn cache_parent_span_path_on_span_enter(
        &mut self,
        descriptor: Option<&'static Descriptor>,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        kind: SpanPathSpanKind,
    ) {
        let _prev = self.span_paths.insert(
            span_id,
            SpanPathCacheEntry {
                descriptor,
                parent_span_id,
                kind,
                users: 1,
            },
        );

        if let Some(parent_span_id) = parent_span_id
            && let Some(SpanPathCacheEntry {
                descriptor: _,
                parent_span_id: _,
                kind: _,
                users,
            }) = self.span_paths.get_mut(&parent_span_id)
        {
            *users = users.saturating_add(1);
        }
    }

    #[inline]
    pub fn cache_fn_meta_attribute(
        &mut self,
        descriptor: &'static Descriptor,
        value: Cow<'static, str>,
    ) {
        let _prev = self.fn_meta_attributes.insert(descriptor, value);
    }

    #[inline]
    pub fn remove_parent_span_id_on_span_exit(&mut self, span_id: SpanId) {
        let _prev = self.parent_span_ids.remove(&span_id);
    }

    #[inline]
    pub fn remove_parent_span_path_on_span_exit(&mut self, span_id: SpanId) {
        let mut next_span_id = Some(span_id);

        while let Some(span_id) = next_span_id.take() {
            match self.span_paths.entry(span_id) {
                hash_map::Entry::Occupied(mut entry) => {
                    let SpanPathCacheEntry {
                        descriptor: _,
                        parent_span_id: _,
                        kind: _,
                        users,
                    } = entry.get_mut();

                    *users = users.saturating_sub(1);

                    if *users == 0 {
                        next_span_id = entry.get().parent_span_id;
                        let _ = entry.remove();
                    }
                }
                hash_map::Entry::Vacant(_) => {}
            }
        }
    }
}

#[inline]
fn write_time<F, Context>(
    fmt: &mut F,
    context: &Context,
    flags: &LoggerConfig,
) -> Result<(), io::Error>
where
    F: Formatter,
    Context: ContextArgs,
{
    let print_utc_time = flags
        .context_fields
        .contains(flags::ContextFields::UTC_TIME);
    let print_local_time = flags
        .context_fields
        .contains(flags::ContextFields::LOCAL_TIME);
    let print_utc_offset = flags
        .context_fields
        .contains(flags::ContextFields::LOCAL_UTC_OFFSET);

    if !print_utc_time && !print_local_time && !print_utc_offset {
        return Ok(());
    }

    let time: Option<SystemTime> = context.type_filtered_nth_arg(0).copied();
    let time = time.or_else(|| {
        let is_auto = flags
            .behavior_flags
            .contains(flags::BehaviorFlags::DEFAULT_SYSTEM_TIME);

        #[cfg(not(all(test, miri)))]
        let time = is_auto.then(SystemTime::now);
        #[cfg(all(test, miri))]
        let time = is_auto.then_some(SystemTime::UNIX_EPOCH);

        time
    });

    let Some(time) = time else {
        return Ok(());
    };

    // Convert SystemTime to jiff::Timestamp once.
    let timestamp =
        Timestamp::try_from(time).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    if print_utc_time {
        let precision = time_precision_from_flags(
            flags.context_fields,
            [
                flags::ContextFields::UTC_TIME_MILLIS,
                flags::ContextFields::UTC_TIME_MICROS,
                flags::ContextFields::UTC_TIME_NANOS,
            ],
        );

        fmt.write_key("utc_time")?;
        // jiff's Timestamp implements Display with RFC 3339 formatting.
        match precision {
            TimePrecision::Seconds => fmt.write_time_already_sanitized(
                &format_args!("{timestamp:.0}"),
                FormatterTimeKind::Utc,
            )?,
            TimePrecision::Millis => fmt.write_time_already_sanitized(
                &format_args!("{timestamp:.3}"),
                FormatterTimeKind::Utc,
            )?,
            TimePrecision::Micros => fmt.write_time_already_sanitized(
                &format_args!("{timestamp:.6}"),
                FormatterTimeKind::Utc,
            )?,
            TimePrecision::Nanos => fmt.write_time_already_sanitized(
                &format_args!("{timestamp:.9}"),
                FormatterTimeKind::Utc,
            )?,
        }
    }

    let zoned = (print_local_time || print_utc_offset)
        .then(|| timestamp.to_zoned_datetime(TimeZone::system()));

    if let Some(zoned) = &zoned
        && print_local_time
    {
        let precision = time_precision_from_flags(
            flags.context_fields,
            [
                flags::ContextFields::LOCAL_TIME_MILLIS,
                flags::ContextFields::LOCAL_TIME_MICROS,
                flags::ContextFields::LOCAL_TIME_NANOS,
            ],
        );
        let print_local_utc_offset = flags
            .context_fields
            .contains(flags::ContextFields::LOCAL_UTC_OFFSET);

        let fmt_str = match (print_local_utc_offset, precision) {
            (false, TimePrecision::Seconds) => "%Y-%m-%dT%H:%M:%S%.0f",
            (false, TimePrecision::Millis) => "%Y-%m-%dT%H:%M:%S%.3f",
            (false, TimePrecision::Micros) => "%Y-%m-%dT%H:%M:%S%.6f",
            (false, TimePrecision::Nanos) => "%Y-%m-%dT%H:%M:%S%.9f",
            (true, TimePrecision::Seconds) => "%Y-%m-%dT%H:%M:%S%.0f%:z",
            (true, TimePrecision::Millis) => "%Y-%m-%dT%H:%M:%S%.3f%:z",
            (true, TimePrecision::Micros) => "%Y-%m-%dT%H:%M:%S%.6f%:z",
            (true, TimePrecision::Nanos) => "%Y-%m-%dT%H:%M:%S%.9f%:z",
        };

        fmt.write_key("local_time")?;
        fmt.write_time_already_sanitized(
            &format_args!("{}", zoned.strftime(fmt_str)),
            FormatterTimeKind::Local,
        )?;
    }

    if let Some(zoned) = &zoned
        && print_utc_offset
    {
        fmt.write_key("utc_offset")?;
        fmt.write_time_already_sanitized(
            &format_args!("{}", zoned.strftime("%:z")),
            FormatterTimeKind::UtcOffset,
        )?;
    }

    Ok(())
}

#[inline]
fn is_event_enabled(kind: FormatterEventKind, flags: LoggerConfig) -> bool {
    if flags.event_kinds == flags::EventKinds::ALL {
        return true;
    }

    let flag = match kind {
        FormatterEventKind::Log => flags::EventKinds::LOG,
        FormatterEventKind::NestedEnter => flags::EventKinds::NESTED_ENTER,
        FormatterEventKind::NestedExit => flags::EventKinds::NESTED_EXIT,
        FormatterEventKind::DetachedEnter => flags::EventKinds::DETACHED_ENTER,
        FormatterEventKind::DetachedExit => flags::EventKinds::DETACHED_EXIT,
        FormatterEventKind::TaskPoll => flags::EventKinds::TASK_POLL,
        FormatterEventKind::TaskPending => flags::EventKinds::TASK_PENDING,
        FormatterEventKind::TaskReady => flags::EventKinds::TASK_READY,
        FormatterEventKind::ExternalNew => flags::EventKinds::EXTERNAL_NEW,
        FormatterEventKind::ExternalClose => flags::EventKinds::EXTERNAL_CLOSE,
        FormatterEventKind::ExternalRecord => flags::EventKinds::EXTERNAL_RECORD,
        FormatterEventKind::ExternalLink => flags::EventKinds::EXTERNAL_LINK,
    };
    flags.event_kinds.contains(flag)
}

#[inline]
fn write_durations<F, Context>(
    fmt: &mut F,
    context: &Context,
    flags: &LoggerConfig,
) -> Result<(), io::Error>
where
    F: Formatter,
    Context: ContextArgs,
{
    write_context_duration(
        fmt,
        context,
        flags,
        FormatterDurationKind::Monotonic,
        |duration: context::MonotonicTime| duration.0,
    )?;

    write_context_duration(
        fmt,
        context,
        flags,
        FormatterDurationKind::ProcessCpu,
        |duration: context::ProcessCpuTime| duration.0,
    )?;

    write_context_duration(
        fmt,
        context,
        flags,
        FormatterDurationKind::ThreadCpu,
        |duration: context::ThreadCpuTime| duration.0,
    )?;

    Ok(())
}

#[inline]
fn write_context_duration<Context, F, T, G>(
    fmt: &mut F,
    context: &Context,
    flags: &LoggerConfig,
    kind: FormatterDurationKind,
    getter_fn: G,
) -> Result<(), io::Error>
where
    F: Formatter,
    Context: ContextArgs,
    T: 'static + Copy,
    G: FnOnce(T) -> Duration,
{
    let field = match kind {
        FormatterDurationKind::Monotonic => flags::ContextFields::MONOTONIC_TIME,
        FormatterDurationKind::ProcessCpu => flags::ContextFields::PROCESS_CPU_TIME,
        FormatterDurationKind::ThreadCpu => flags::ContextFields::THREAD_CPU_TIME,
    };

    if !flags.context_fields.contains(field) {
        return Ok(());
    }

    cold_path(); // Unlikely in default config.

    let duration: Option<T> = context.type_filtered_nth_arg(0).copied();
    let Some(duration) = duration else {
        return Ok(());
    };
    let duration = getter_fn(duration);

    let [millis, micros, nanos] = match kind {
        FormatterDurationKind::Monotonic => [
            flags::ContextFields::MONOTONIC_TIME_MILLIS,
            flags::ContextFields::MONOTONIC_TIME_MICROS,
            flags::ContextFields::MONOTONIC_TIME_NANOS,
        ],
        FormatterDurationKind::ProcessCpu => [
            flags::ContextFields::PROCESS_CPU_TIME_MILLIS,
            flags::ContextFields::PROCESS_CPU_TIME_MICROS,
            flags::ContextFields::PROCESS_CPU_TIME_NANOS,
        ],
        FormatterDurationKind::ThreadCpu => [
            flags::ContextFields::THREAD_CPU_TIME_MILLIS,
            flags::ContextFields::THREAD_CPU_TIME_MICROS,
            flags::ContextFields::THREAD_CPU_TIME_NANOS,
        ],
    };

    let precision = time_precision_from_flags(flags.context_fields, [millis, micros, nanos]);

    let key = match kind {
        FormatterDurationKind::Monotonic => "monotonic_time",
        FormatterDurationKind::ProcessCpu => "process_cpu_time",
        FormatterDurationKind::ThreadCpu => "thread_cpu_time",
    };
    fmt.write_key(key)?;
    fmt.write_duration(duration, precision, kind)?;
    Ok(())
}

#[inline]
fn write_context_units<F, Context>(
    fmt: &mut F,
    context: &Context,
    flags: &LoggerConfig,
) -> Result<(), io::Error>
where
    F: Formatter,
    Context: ContextArgs,
{
    write_context_id(
        fmt,
        context,
        flags,
        FormatterIdKind::ProcessId,
        |id: context::ProcessId| id.0,
    )?;
    write_context_str(
        fmt,
        context,
        flags,
        FormatterStrKind::ProcessName,
        |name: &context::ProcessName| &name.0,
    )?;

    write_context_id(
        fmt,
        context,
        flags,
        FormatterIdKind::ThreadId,
        |id: context::ThreadId| id.0,
    )?;
    write_context_str(
        fmt,
        context,
        flags,
        FormatterStrKind::ThreadName,
        |name: &context::ThreadName| &name.0,
    )?;

    write_context_id(
        fmt,
        context,
        flags,
        FormatterIdKind::TaskId,
        |id: context::TaskId| id.0,
    )?;
    write_context_str(
        fmt,
        context,
        flags,
        FormatterStrKind::TaskName,
        |name: &context::TaskName| &name.0,
    )?;

    write_context_id(
        fmt,
        context,
        flags,
        FormatterIdKind::CpuId,
        |id: context::CpuId| u64::from(id.0),
    )?;

    Ok(())
}

#[inline]
fn write_context_id<Context, F, T, G>(
    fmt: &mut F,
    context: &Context,
    flags: &LoggerConfig,
    kind: FormatterIdKind,
    getter_fn: G,
) -> Result<(), io::Error>
where
    F: Formatter,
    Context: ContextArgs,
    T: 'static + Copy,
    G: FnOnce(T) -> u64,
{
    let field = match kind {
        FormatterIdKind::ProcessId => flags::ContextFields::PROCESS_ID,
        FormatterIdKind::ThreadId => flags::ContextFields::THREAD_ID,
        FormatterIdKind::TaskId => flags::ContextFields::TASK_ID,
        FormatterIdKind::CpuId => flags::ContextFields::CPU_ID,
    };

    if !flags.context_fields.contains(field) {
        return Ok(());
    }

    cold_path(); // Unlikely in default config.

    let id: Option<T> = context.type_filtered_nth_arg(0).copied();
    let Some(id) = id else {
        return Ok(());
    };
    let id = getter_fn(id);

    let key = match kind {
        FormatterIdKind::ProcessId => "process_id",
        FormatterIdKind::ThreadId => "thread_id",
        FormatterIdKind::TaskId => "task_id",
        FormatterIdKind::CpuId => "cpu_id",
    };
    fmt.write_key(key)?;
    fmt.write_id(id, kind)?;
    Ok(())
}

#[inline]
fn write_context_str<Context, F, T, G>(
    fmt: &mut F,
    context: &Context,
    flags: &LoggerConfig,
    kind: FormatterStrKind,
    getter_fn: G,
) -> Result<(), io::Error>
where
    F: Formatter,
    Context: ContextArgs,
    T: 'static,
    G: FnOnce(&T) -> &str,
{
    let field = match kind {
        FormatterStrKind::ProcessName => flags::ContextFields::PROCESS_NAME,
        FormatterStrKind::ThreadName => flags::ContextFields::THREAD_NAME,
        FormatterStrKind::TaskName => flags::ContextFields::TASK_NAME,
    };

    if !flags.context_fields.contains(field) {
        return Ok(());
    }

    cold_path(); // Unlikely in default config.

    let value: Option<&T> = context.type_filtered_nth_arg(0);
    let Some(value) = value else {
        return Ok(());
    };
    let value = getter_fn(value);

    let key = match kind {
        FormatterStrKind::ProcessName => "process_name",
        FormatterStrKind::ThreadName => "thread_name",
        FormatterStrKind::TaskName => "task_name",
    };
    fmt.write_key(key)?;
    fmt.write_str(value, kind)?;
    Ok(())
}

#[inline]
fn write_parent_span_id<F>(
    fmt: &mut F,
    parent_span_id: MaybeKnownSpanId,
    span_id: KnownSpanId,
    flags: &LoggerConfig,
    cache: &Cache,
) -> Result<(), io::Error>
where
    F: Formatter,
{
    if !flags
        .site_fields
        .contains(flags::SiteFields::PARENT_SPAN_ID)
    {
        return Ok(());
    }

    let parent_span_id = match parent_span_id {
        MaybeKnownSpanId::Unknown => match span_id {
            KnownSpanId::Root => MaybeKnownSpanId::Root,
            KnownSpanId::SpanId(span_id) => {
                if flags
                    .behavior_flags
                    .contains(flags::BehaviorFlags::CACHED_PARENT_SPAN_IDS)
                {
                    cache
                        .parent_span_ids
                        .get(&span_id)
                        .copied()
                        .map_or(MaybeKnownSpanId::Unknown, |span_id| {
                            span_id.map_or(MaybeKnownSpanId::Root, MaybeKnownSpanId::SpanId)
                        })
                } else {
                    MaybeKnownSpanId::Unknown
                }
            }
        },
        MaybeKnownSpanId::Root => MaybeKnownSpanId::Root,
        MaybeKnownSpanId::SpanId(span_id) => MaybeKnownSpanId::SpanId(span_id),
    };

    fmt.write_key("parent_span_id")?;
    fmt.write_span_id(parent_span_id, FormatterSpanIdKind::ParentSpanId)?;
    Ok(())
}

#[inline]
fn write_kind<F>(
    fmt: &mut F,
    kind: FormatterEventKind,
    flags: &LoggerConfig,
) -> Result<(), io::Error>
where
    F: Formatter,
{
    if !flags
        .descriptor_fields
        .contains(flags::DescriptorFields::KIND)
    {
        return Ok(());
    }

    fmt.write_key("kind")?;
    fmt.write_kind(kind)?;
    Ok(())
}

#[inline]
fn write_span_id<F>(
    fmt: &mut F,
    span_id: KnownSpanId,
    flags: &LoggerConfig,
) -> Result<(), io::Error>
where
    F: Formatter,
{
    if !flags.site_fields.contains(flags::SiteFields::SPAN_ID) {
        return Ok(());
    }

    fmt.write_key("span_id")?;
    fmt.write_span_id(span_id.into(), FormatterSpanIdKind::SpanId)?;
    Ok(())
}

#[inline]
fn write_span_path<F>(
    fmt: &mut F,
    span_id: KnownSpanId,
    event_kind: FormatterEventKind,
    flags: &LoggerConfig,
    cache: &Cache,
    scratch: &mut Scratch,
) -> Result<(), io::Error>
where
    F: Formatter,
{
    let print_kind = flags
        .derived_fields
        .contains(flags::DerivedFields::SPAN_PATH_KIND);
    let print_id = flags
        .derived_fields
        .contains(flags::DerivedFields::SPAN_PATH_ID);
    let print_name = flags
        .derived_fields
        .contains(flags::DerivedFields::SPAN_PATH_NAME);

    if !print_kind && !print_id && !print_name {
        return Ok(());
    }

    let mut next_span_id = span_id;
    scratch.span_id_vec.clear();

    loop {
        let span_id = match next_span_id {
            KnownSpanId::Root => break,
            KnownSpanId::SpanId(span_id) => span_id,
        };
        scratch.span_id_vec.push(span_id);
        let Some(entry) = cache.span_paths.get(&span_id) else {
            break;
        };
        next_span_id = entry
            .parent_span_id
            .map_or(KnownSpanId::Root, KnownSpanId::SpanId);
    }

    fmt.write_key("span_path")?;
    fmt.begin_array()?;

    let len = scratch.span_id_vec.len();
    for (i, span_id) in scratch.span_id_vec.iter().rev().enumerate() {
        #[expect(clippy::arithmetic_side_effects, reason = "never overflows")]
        let is_last = i == len - 1;
        if let Some(SpanPathCacheEntry {
            descriptor,
            parent_span_id: _,
            kind,
            users: _,
        }) = cache.span_paths.get(span_id)
        {
            fmt.begin_object()?;

            if print_kind {
                let last_kind = if is_last {
                    match event_kind {
                        FormatterEventKind::Log => None,
                        FormatterEventKind::NestedEnter
                        | FormatterEventKind::NestedExit
                        | FormatterEventKind::DetachedEnter
                        | FormatterEventKind::DetachedExit
                        | FormatterEventKind::TaskPoll
                        | FormatterEventKind::TaskPending
                        | FormatterEventKind::TaskReady
                        | FormatterEventKind::ExternalNew
                        | FormatterEventKind::ExternalClose
                        | FormatterEventKind::ExternalRecord
                        | FormatterEventKind::ExternalLink => Some(event_kind),
                    }
                } else {
                    None
                };
                let kind = last_kind.unwrap_or(match kind {
                    SpanPathSpanKind::Nested => FormatterEventKind::NestedEnter,
                    SpanPathSpanKind::Detached => FormatterEventKind::DetachedEnter,
                    SpanPathSpanKind::External => FormatterEventKind::ExternalNew,
                });

                fmt.write_key("kind")?;
                fmt.write_span_path_kind(kind)?;
            }

            if print_id {
                fmt.write_key("span_id")?;
                fmt.write_span_path_span_id(KnownSpanId::SpanId(*span_id))?;
            }

            if print_name
                && !is_last
                && let Some(descriptor) = descriptor
            {
                fmt.write_key("message")?;
                write_message_str(fmt, descriptor, event_kind, flags, cache)?;
            }

            fmt.end_object()?;
        }
    }

    fmt.end_array()?;
    scratch.span_id_vec.clear();

    Ok(())
}

#[inline]
fn write_level<F>(fmt: &mut F, level: Level, flags: &LoggerConfig) -> Result<(), io::Error>
where
    F: Formatter,
{
    if !flags
        .descriptor_fields
        .contains(flags::DescriptorFields::LEVEL)
    {
        return Ok(());
    }

    fmt.write_key("level")?;
    fmt.write_level(level)?;
    Ok(())
}

fn write_target<F>(
    fmt: &mut F,
    target: Option<&str>,
    module_path: Option<&str>,
    kind: FormatterEventKind,
    flags: &LoggerConfig,
) -> Result<(), io::Error>
where
    F: Formatter,
{
    if flags
        .descriptor_fields
        .contains(flags::DescriptorFields::TARGET)
    {
        let target = target.unwrap_or_else(|| {
            if flags
                .behavior_flags
                .contains(flags::BehaviorFlags::TARGET_DEFAULT_MODULE_PATH)
            {
                module_path.unwrap_or("")
            } else {
                ""
            }
        });

        fmt.write_key("target")?;
        fmt.write_target(target, kind)?;
    }

    if flags
        .descriptor_fields
        .contains(flags::DescriptorFields::MODULE_PATH)
    {
        let target = module_path.unwrap_or("");
        fmt.write_key("module_path")?;
        fmt.write_target(target, kind)?;
    }
    Ok(())
}

#[inline]
fn write_location<F>(
    fmt: &mut F,
    filename: Option<&str>,
    line: Option<u32>,
    column: Option<u32>,
    flags: &LoggerConfig,
) -> Result<(), io::Error>
where
    F: Formatter,
{
    let filename = flags
        .descriptor_fields
        .contains(flags::DescriptorFields::FILENAME)
        .then_some(filename)
        .flatten();
    let line = flags
        .descriptor_fields
        .contains(flags::DescriptorFields::LINE)
        .then_some(line)
        .flatten();
    let column = flags
        .descriptor_fields
        .contains(flags::DescriptorFields::COLUMN)
        .then_some(column)
        .flatten();

    if filename.is_none() && line.is_none() && column.is_none() {
        return Ok(());
    }

    fmt.write_location_arguments(filename, line, column)
}

#[inline]
fn write_message<'a, F, Args, Context>(
    fmt: &mut F,
    entry: &Entry<'_, Args, Context>,
    flags: &LoggerConfig,
    cache: &Cache,
) -> Result<(), io::Error>
where
    F: Formatter,
    Args: DisplayableMixedArgs<'a>,
    Context: ContextArgs,
{
    if !flags
        .descriptor_fields
        .contains(flags::DescriptorFields::MESSAGE)
    {
        return Ok(());
    }

    let descriptor = entry.descriptor;

    fmt.write_key("message")?;
    write_message_str(fmt, descriptor, entry.kind, flags, cache)?;
    Ok(())
}

#[inline]
fn write_external_message_fmt<F, Desc, Args, Context>(
    fmt: &mut F,
    external_entry: &ExternalEntry<'_, Desc, Args, Context>,
    flags: &LoggerConfig,
) -> Result<(), io::Error>
where
    F: Formatter,
    Desc: ExternalDescriptor,
    Args: ExternalArgs,
    Context: ContextArgs,
{
    struct MessageVisitor<F> {
        result: Option<Result<(), io::Error>>,
        fmt: F,
        kind: FormatterEventKind,
    }

    impl<F> ExternalArgsVisitor for MessageVisitor<&mut F>
    where
        F: Formatter,
    {
        fn visit_message(&mut self, value: polog_util::ExternalArgValue<'_>) {
            if self.result.is_some() {
                return;
            }

            self.result = Some((|| {
                self.fmt.write_key("message")?;
                self.fmt.write_message(&value, self.kind)?;
                Ok(())
            })());
        }

        fn visit_arg(
            &mut self,
            _key: polog_util::util::MaybeStaticStr<'_>,
            _value: polog_util::ExternalArgValue<'_>,
        ) {
        }
    }

    if !flags
        .descriptor_fields
        .contains(flags::DescriptorFields::MESSAGE)
    {
        return Ok(());
    }

    let mut visitor = MessageVisitor {
        result: None,
        fmt: &mut *fmt,
        kind: external_entry.kind,
    };
    external_entry.args.visit(&mut visitor);
    if let Some(result) = visitor.result {
        return result;
    }

    if let Some(message) = external_entry.descriptor.message() {
        fmt.write_key("message")?;
        fmt.write_message(&message, external_entry.kind)?;
    }
    Ok(())
}

#[inline]
fn write_args<'a, F, Args, Context>(
    fmt: &mut F,
    entry: &Entry<'_, Args, Context>,
    flags: &LoggerConfig,
) -> Result<(), io::Error>
where
    F: Formatter,
    Args: DisplayableMixedArgs<'a>,
    Context: ContextArgs,
{
    if !flags.site_fields.contains(flags::SiteFields::ARGUMENTS) {
        return Ok(());
    }

    let descriptor = entry.descriptor;

    fmt.write_key("args")?;
    fmt.begin_array()?;
    for param_idx in 0..entry.args.num_args() {
        let key = descriptor
            .param_names
            .get(param_idx)
            .copied()
            .unwrap_or("?");
        if key.starts_with('@') {
            continue;
        }

        let value = fmt::from_fn(|f| {
            let result = entry.args.display_fmt_nth(f, param_idx);
            result.unwrap_or(Ok(()))
        });
        fmt.begin_array()?;
        fmt.write_argument_key(key)?;
        fmt.write_argument_value(&value)?;
        fmt.end_array()?;
    }
    fmt.end_array()?;
    Ok(())
}

#[inline]
fn write_external_args<F, Args>(
    fmt: &mut F,
    args: &Args,
    flags: &LoggerConfig,
) -> Result<(), io::Error>
where
    F: Formatter,
    Args: ExternalArgs,
{
    struct ArgsVisitor<F> {
        result: Result<(), io::Error>,
        fmt: F,
    }

    impl<F> ExternalArgsVisitor for ArgsVisitor<&mut F>
    where
        F: Formatter,
    {
        fn visit_message(&mut self, _value: polog_util::ExternalArgValue<'_>) {}

        fn visit_arg(
            &mut self,
            key: polog_util::util::MaybeStaticStr<'_>,
            value: polog_util::ExternalArgValue<'_>,
        ) {
            if self.result.is_ok() {
                self.result = (|| {
                    self.fmt.begin_array()?;
                    self.fmt.write_argument_key(key.as_str())?;
                    self.fmt.write_argument_value(&value)?;
                    self.fmt.end_array()?;
                    Ok(())
                })();
            }
        }
    }

    if !flags.site_fields.contains(flags::SiteFields::ARGUMENTS) {
        return Ok(());
    }

    fmt.write_key("args")?;
    fmt.begin_array()?;

    let mut visitor = ArgsVisitor {
        result: Ok(()),
        fmt: &mut *fmt,
    };
    args.visit(&mut visitor);

    let () = visitor.result?;

    fmt.end_array()?;
    Ok(())
}

#[inline]
fn write_message_str<F>(
    fmt: &mut F,
    descriptor: &Descriptor,
    kind: FormatterEventKind,
    flags: &LoggerConfig,
    cache: &Cache,
) -> Result<(), io::Error>
where
    F: Formatter,
{
    #[derive(Copy, Clone, Eq, PartialEq, Debug)]
    enum Message<'a> {
        Str(&'a str),
        Location(Location<'static>),
    }

    let message = if !descriptor.message.is_empty() {
        Message::Str(descriptor.message)
    } else if let Some(cached_message) = cache.fn_meta_attributes.get(&descriptor) {
        Message::Str(cached_message)
    } else if flags
        .behavior_flags
        .contains(flags::BehaviorFlags::USE_LOCATION_AS_DEFAULT_MESSAGE)
    {
        Message::Location(descriptor.location)
    } else {
        Message::Str("")
    };

    let message = fmt::from_fn(|f| match message {
        Message::Str(value) => write!(f, "{value}"),
        Message::Location(location) => write!(f, "{location}"),
    });

    fmt.write_message(&message, kind)?;
    Ok(())
}

#[inline]
const fn time_precision_from_flags(
    context_flags: flags::ContextFields,
    [millis, micros, nanos]: [flags::ContextFields; 3],
) -> TimePrecision {
    if context_flags.contains(nanos) {
        TimePrecision::Nanos
    } else if context_flags.contains(micros) {
        TimePrecision::Micros
    } else if context_flags.contains(millis) {
        TimePrecision::Millis
    } else {
        TimePrecision::Seconds
    }
}

impl From<SpanKind> for SpanPathSpanKind {
    #[inline]
    fn from(span_kind: SpanKind) -> Self {
        match span_kind {
            SpanKind::Nested => Self::Nested,
            SpanKind::Detached => Self::Detached,
        }
    }
}

#[expect(clippy::unwrap_used, reason = "okay in tests")]
#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use core::mem;
    use std::io;
    use std::time::SystemTime;

    use parking_lot::RwLock;
    use polog::{Descriptor, Handler, Level, SpanId, SpanIdGenerator, log_descriptor_ref};

    use crate::logger::Entry;
    use crate::test_util::{assert_eq_with_wildcards, emit_handler_events};
    use crate::{
        FormatterEventKind, KnownSpanId, LoggerConfig, MaybeKnownSpanId, Pologger, TextFormatter,
        TextFormatterColorMode, TextFormatterConfig, TextFormatterFlags, flags, non_zero,
    };

    #[inline]
    fn new_pologger(config: Arc<RwLock<LoggerConfig>>) -> Pologger<TextFormatter<Vec<u8>>> {
        let formatter_config =
            TextFormatterConfig::new(TextFormatterFlags::default(), TextFormatterColorMode::Off);
        let formatter_config = Arc::new(RwLock::new(formatter_config));
        let formatter = TextFormatter::new(Vec::<u8>::new(), formatter_config);

        let span_id_generator = Arc::new(SpanIdGenerator::new());
        Pologger::new(span_id_generator, formatter, config)
    }

    #[test]
    fn test_logger_accessors() {
        let config = Arc::new(RwLock::new(LoggerConfig::default()));
        let mut logger = new_pologger(config);

        let _unused: &TextFormatter<Vec<u8>> = logger.formatter();
        let _unused: &mut TextFormatter<Vec<u8>> = logger.formatter_mut();
        assert_eq!(*logger.config().read(), LoggerConfig::default());
    }

    #[test]
    fn test_logger_compact() {
        assert_eq!(LoggerConfig::new(), LoggerConfig::compact());
        assert_eq!(LoggerConfig::default(), LoggerConfig::compact());

        let config = Arc::new(RwLock::new(LoggerConfig::compact()));
        let mut logger = new_pologger(Arc::clone(&config));

        emit_handler_events(&mut logger);
        let written = mem::take(logger.formatter_mut().writer_mut());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "\
????-??-??T??:??:??.??????Z root    root INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root    root INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root    root INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root    root INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root >> #1   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root << #1   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root |> #1   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root |< #1   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root >> #1   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root << #1   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root >> #1   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root << #1   INFO  pologger::test_util Message
",
        );

        *config.write() = LoggerConfig::compact().with_task_poll_events();
        emit_handler_events(&mut logger);
        let written = mem::take(logger.formatter_mut().writer_mut());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "\
????-??-??T??:??:??.??????Z root    root INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root    root INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root    root INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root    root INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root >> #2   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root << #2   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root |> #2   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root |< #2   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root >> #2   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root << #2   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root >> #2   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root ?? #2   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root ?. #2   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root ?? #2   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root ?! #2   INFO  pologger::test_util Message
????-??-??T??:??:??.??????Z root << #2   INFO  pologger::test_util Message
",
        );
    }

    #[test]
    fn test_logger_compact_span_path() {
        let config = Arc::new(RwLock::new(LoggerConfig::compact_span_path()));
        let logger = new_pologger(config);
        let mut logger = polog_util::Shared::new(logger);

        let mut span1 = polog::info_span!(&mut logger, "span_1");
        let mut span2 = polog::info_span!(&mut span1, "span_2");
        let span3 = polog::info_span!(&mut span2, "span_3");
        drop(span3);
        drop(span2);
        drop(span1);

        let written = mem::take(logger.inner().lock().formatter_mut().writer_mut());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "\
????-??-??T??:??:??.??????Z INFO  >> #1 pologger::logger::tests span_1
????-??-??T??:??:??.??????Z INFO  >> #1 span_1 >> #2 pologger::logger::tests span_2
????-??-??T??:??:??.??????Z INFO  >> #1 span_1 >> #2 span_2 >> #3 pologger::logger::tests span_3
????-??-??T??:??:??.??????Z INFO  >> #1 span_1 >> #2 span_2 << #3 pologger::logger::tests span_3
????-??-??T??:??:??.??????Z INFO  >> #1 span_1 << #2 pologger::logger::tests span_2
????-??-??T??:??:??.??????Z INFO  << #1 pologger::logger::tests span_1
",
        );
    }

    #[test]
    fn test_logger_log_variants() {
        let config = Arc::new(RwLock::new(LoggerConfig::compact_span_path()));
        let logger = new_pologger(config);
        let mut logger = polog_util::Shared::new(logger);

        polog::info!(&mut logger; @fn);
        let written = mem::take(logger.inner().lock().formatter_mut().writer_mut());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "????-??-??T??:??:??.??????Z INFO  pologger::logger::tests test_logger_log_variants\n",
        );

        polog::info!(&mut logger);
        let written = mem::take(logger.inner().lock().formatter_mut().writer_mut());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "????-??-??T??:??:??.??????Z INFO  pologger::logger::tests \
             crates/pologger/src/logger.rs:????:?\n",
        );
    }

    #[test]
    fn test_logger_log_almost_no_flags() {
        let mut config = LoggerConfig::new();
        config.descriptor_fields &= !flags::DescriptorFields::ALL;
        config.site_fields &= !flags::SiteFields::ALL;
        config.context_fields &= !flags::ContextFields::ALL;
        config.derived_fields &= !flags::DerivedFields::ALL;
        config.behavior_flags &= !flags::BehaviorFlags::ALL;

        let config = Arc::new(RwLock::new(config));
        let logger = new_pologger(Arc::clone(&config));
        let mut logger = polog_util::Shared::new(logger);

        polog::trace!(&mut logger, "Message"; key1 = "value1", key2 = "value2");
        polog::trace!(&mut logger, "Message"; key1 = "value1", key2 = "value2");
        let written = mem::take(logger.inner().lock().formatter_mut().writer_mut());
        assert_eq_with_wildcards(str::from_utf8(&written).unwrap(), "\n\n");

        config.write().descriptor_fields |= flags::DescriptorFields::LEVEL;
        polog::trace!(&mut logger, "Message"; key1 = "value1", key2 = "value2");
        polog::trace!(&mut logger, "Message"; key1 = "value1", key2 = "value2");
        let written = mem::take(logger.inner().lock().formatter_mut().writer_mut());
        assert_eq_with_wildcards(str::from_utf8(&written).unwrap(), "TRACE\nTRACE\n");
    }

    #[test]
    fn test_logger_log_all_flags() {
        let mut config = LoggerConfig::new();
        config.event_kinds |= flags::EventKinds::ALL;
        config.descriptor_fields |= flags::DescriptorFields::ALL;
        config.site_fields |= flags::SiteFields::ALL;
        config.context_fields |= flags::ContextFields::ALL;
        config.derived_fields |= flags::DerivedFields::ALL;
        config.behavior_flags |= flags::BehaviorFlags::ALL;

        let config = Arc::new(RwLock::new(config));
        let logger = new_pologger(config);
        let mut logger = polog_util::Shared::new(logger);

        polog::info!(&mut logger, "Message"; key1 = "value1", key2 = "value2");

        let written = mem::take(logger.inner().lock().formatter_mut().writer_mut());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "\
                ????-??-??T??:??:??.?????????Z ????-??-??T??:??:??.????????????:?? ???:?? root    \
             root INFO  pologger::logger::tests pologger::logger::tests \
             crates/pologger/src/logger.rs:????:? Message: key1 = value1, key2 = value2\n",
        );
    }

    #[test]
    fn test_logger_context() {
        const INFO_DESC: &Descriptor = log_descriptor_ref!(Level::Info, "Message");

        let config = LoggerConfig::new();
        let config = Arc::new(RwLock::new(config));
        let mut logger = new_pologger(config);

        let time = SystemTime::UNIX_EPOCH;

        logger.log(INFO_DESC, (), None, (polog::Arg(time),));

        let written = mem::take(logger.formatter_mut().writer_mut());
        assert_eq_with_wildcards(
            str::from_utf8(&written).unwrap(),
            "1970-01-01T00:00:00.000000Z root    root INFO  pologger::logger::tests Message\n",
        );
    }

    #[test]
    fn test_formatter_failing_partial_fmt() {
        struct SmallWriter(usize);

        impl io::Write for SmallWriter {
            #[inline]
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                let result = if self.0 < buf.len() {
                    Err(io::Error::from(io::ErrorKind::WriteZero))
                } else {
                    Ok(buf.len())
                };
                self.0 = self.0.saturating_sub(buf.len());
                result
            }

            #[inline]
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let descriptor = polog::log_descriptor_ref!(Level::Info, "", keys: &["key1", "key2"]);
        let entry = Entry {
            descriptor,
            args: &(polog::Arg("value1"), polog::Arg("value2")),
            parent_span_id: MaybeKnownSpanId::SpanId(SpanId(non_zero!(1))),
            span_id: KnownSpanId::SpanId(SpanId(non_zero!(2))),
            kind: FormatterEventKind::NestedEnter,
            context: &(),
        };

        let config = Arc::new(RwLock::new(LoggerConfig::compact()));
        let mut logger = new_pologger(Arc::clone(&config));
        let flags = *logger.config.read();

        logger.try_log_entry(&entry, flags).unwrap();
        let written = mem::take(logger.formatter_mut().writer_mut());
        let internal_len = written.len();

        #[cfg(not(miri))]
        let step = 1;
        #[cfg(miri)]
        let step = 4;

        for len in (0..internal_len.saturating_add(step)).step_by(step) {
            let formatter_config = TextFormatterConfig::new(
                TextFormatterFlags::default(),
                TextFormatterColorMode::Off,
            );
            let formatter_config = Arc::new(RwLock::new(formatter_config));
            let formatter = TextFormatter::new(SmallWriter(len), formatter_config);

            let span_id_generator = Arc::new(SpanIdGenerator::new());
            let config = Arc::new(RwLock::new(LoggerConfig::compact()));
            let mut logger = Pologger::new(span_id_generator, formatter, config);
            let flags = *logger.config.read();

            let result = logger.try_log_entry(&entry, flags);
            if len < internal_len {
                assert!(result.is_err());
            } else {
                assert!(result.is_ok());
            }
        }
    }
}
