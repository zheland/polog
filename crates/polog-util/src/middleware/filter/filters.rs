//! Filters and filter combinators.

use polog::{Descriptor, Level, LevelFilter};

use crate::{ExternalDescriptor, ExternalFilterFn, FilterFn};

/// Filter that accepts every descriptor.
///
/// Useful as a placeholder when a filter is required by the type system but
/// no actual filtering is desired.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct Always;

impl FilterFn for Always {
    #[inline]
    fn is_enabled(&self, _descriptor: &'static Descriptor) -> bool {
        true
    }

    #[inline]
    fn max_level(&self) -> LevelFilter {
        LevelFilter::ALL
    }
}

impl ExternalFilterFn for Always {
    #[inline]
    fn is_enabled_external<Desc>(&self, _descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor,
    {
        true
    }
}

/// Filter that rejects every descriptor.
///
/// Mostly useful for tests and benchmarks.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct Never;

impl FilterFn for Never {
    #[inline]
    fn is_enabled(&self, _descriptor: &'static Descriptor) -> bool {
        false
    }

    #[inline]
    fn max_level(&self) -> LevelFilter {
        LevelFilter::NONE
    }
}

impl ExternalFilterFn for Never {
    #[inline]
    fn is_enabled_external<Desc>(&self, _descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor,
    {
        false
    }
}

/// Filter that enables descriptors at or above a maximum verbosity level.
///
/// A descriptor passes when `descriptor.level <= max_level`, i.e. when its
/// level is not more verbose than `max_level`.
/// Use [`LevelFilter::Off`] to reject all levels.
///
/// # Example
///
/// ```rust
/// use polog::{Level, LevelFilter};
/// use polog_util::FilterFn;
/// use polog_util::filters::MaxLevel;
///
/// let filter = MaxLevel(LevelFilter::Info);
/// assert!(filter.is_enabled(polog::log_descriptor_ref!(Level::Error, "")));
/// assert!(filter.is_enabled(polog::log_descriptor_ref!(Level::Info, "")));
/// assert!(!filter.is_enabled(polog::log_descriptor_ref!(Level::Debug, "")));
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct MaxLevel(pub LevelFilter);

impl FilterFn for MaxLevel {
    #[inline]
    fn is_enabled(&self, descriptor: &'static Descriptor) -> bool {
        descriptor.level <= self.0
    }

    #[inline]
    fn max_level(&self) -> LevelFilter {
        self.0
    }
}

impl ExternalFilterFn for MaxLevel {
    #[inline]
    fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor,
    {
        descriptor.level() <= self.0
    }
}

/// A const-generic filter that accepts descriptors whose level is no more
/// verbose than `MAX_LEVEL`.
///
/// A descriptor passes when `descriptor.level <= MAX_LEVEL`, i.e. when its
/// level is not more verbose than `MAX_LEVEL`.
///
/// `MAX_LEVEL` is the raw [`Level::into_u8`] value: `1` for [`Level::Fatal`]
/// through `7` for [`Level::Trace`].
/// The aliases [`FatalFilter`], [`BugFilter`], [`ErrorFilter`], [`WarnFilter`],
/// [`InfoFilter`], [`DebugFilter`] and [`TraceFilter`] cover every built-in
/// level.
/// Use the raw type when writing a filter through a macro or code generator.
///
/// # Example
///
/// ```rust
/// use polog::Level;
/// use polog_util::FilterFn;
/// use polog_util::filters::{ConstMaxLevel, InfoFilter};
///
/// let filter: InfoFilter = ConstMaxLevel;
/// assert!(filter.is_enabled(polog::log_descriptor_ref!(Level::Error, "")));
/// assert!(filter.is_enabled(polog::log_descriptor_ref!(Level::Info, "")));
/// assert!(!filter.is_enabled(polog::log_descriptor_ref!(Level::Debug, "")));
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct ConstMaxLevel<const MAX_LEVEL: u8>;

impl<const MAX_LEVEL: u8> FilterFn for ConstMaxLevel<MAX_LEVEL> {
    #[inline]
    fn is_enabled(&self, descriptor: &'static Descriptor) -> bool {
        descriptor.level.into_u8() <= MAX_LEVEL
    }

    #[inline]
    fn max_level(&self) -> LevelFilter {
        LevelFilter::try_from_u8(MAX_LEVEL).unwrap_or(LevelFilter::ALL)
    }
}

impl<const MAX_LEVEL: u8> ExternalFilterFn for ConstMaxLevel<MAX_LEVEL> {
    #[inline]
    fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor,
    {
        descriptor.level().into_u8() <= MAX_LEVEL
    }
}

/// Filter that permits only [`Level::Fatal`] events.
/// More verbose levels [`Level::Bug`] and below are rejected.
pub type FatalFilter = ConstMaxLevel<{ Level::Fatal.into_u8() }>;

/// Filter that permits [`Level::Fatal`] and [`Level::Bug`] events.
/// More verbose levels [`Level::Error`] and below are rejected.
pub type BugFilter = ConstMaxLevel<{ Level::Bug.into_u8() }>;

/// Filter that permits [`Level::Fatal`], [`Level::Bug`], and [`Level::Error`]
/// events.
/// More verbose levels [`Level::Warn`] and below are rejected.
pub type ErrorFilter = ConstMaxLevel<{ Level::Error.into_u8() }>;

/// Filter that permits [`Level::Fatal`], [`Level::Bug`], [`Level::Error`], and
/// [`Level::Warn`] events.
/// More verbose levels [`Level::Info`] and below are rejected.
pub type WarnFilter = ConstMaxLevel<{ Level::Warn.into_u8() }>;

/// Filter that permits [`Level::Fatal`], [`Level::Bug`], [`Level::Error`],
/// [`Level::Warn`], and [`Level::Info`] events.
/// More verbose levels [`Level::Debug`] and [`Level::Trace`] are rejected.
pub type InfoFilter = ConstMaxLevel<{ Level::Info.into_u8() }>;

/// Filter that permits [`Level::Fatal`], [`Level::Bug`], [`Level::Error`],
/// [`Level::Warn`], [`Level::Info`], [`Level::Debug`] events.
/// More verbose level [`Level::Trace`] is rejected.
pub type DebugFilter = ConstMaxLevel<{ Level::Debug.into_u8() }>;

/// Filter that permits all [`Level::Fatal`], [`Level::Bug`], [`Level::Error`],
/// [`Level::Warn`], [`Level::Info`], [`Level::Debug`] and [`Level::Trace`]
/// events.
pub type TraceFilter = ConstMaxLevel<{ Level::Trace.into_u8() }>;

/// Filter that matches descriptors by their *effective target*.
///
/// The effective target is [`Descriptor::target`] if it is set, or
/// [`Descriptor::module_path`] otherwise.
/// This mirrors how delegators typically derive the target from a descriptor.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Target(pub &'static str);

impl FilterFn for Target {
    #[inline]
    fn is_enabled(&self, descriptor: &'static Descriptor) -> bool {
        effective_target(descriptor) == self.0
    }

    #[inline]
    fn max_level(&self) -> LevelFilter {
        LevelFilter::ALL
    }
}

impl ExternalFilterFn for Target {
    #[inline]
    fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor,
    {
        external_effective_target(descriptor) == self.0
    }
}

/// Filter that matches descriptors by their *effective target* prefix.
///
/// See [`Target`] for the definition of the effective target.
/// A descriptor passes when the effective target equals the prefix or starts
/// with `prefix::`.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct TargetPrefix(pub &'static str);

impl FilterFn for TargetPrefix {
    #[inline]
    fn is_enabled(&self, descriptor: &'static Descriptor) -> bool {
        path_prefix_match(effective_target(descriptor), self.0)
    }

    #[inline]
    fn max_level(&self) -> LevelFilter {
        LevelFilter::ALL
    }
}

impl ExternalFilterFn for TargetPrefix {
    #[inline]
    fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor,
    {
        path_prefix_match(external_effective_target(descriptor), self.0)
    }
}

/// Negation of a filter.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Not<F>(pub F);

impl<F> FilterFn for Not<F>
where
    F: FilterFn,
{
    #[inline]
    fn is_enabled(&self, descriptor: &'static Descriptor) -> bool {
        !self.0.is_enabled(descriptor)
    }

    #[inline]
    fn max_level(&self) -> LevelFilter {
        LevelFilter::ALL
    }
}

impl<F> ExternalFilterFn for Not<F>
where
    F: ExternalFilterFn,
{
    #[inline]
    fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor,
    {
        !self.0.is_enabled_external(descriptor)
    }
}

/// Conjunction of multiple filters: all must accept.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct All<T>(pub T);

/// Disjunction of multiple filters: at least one must accept.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Any<T>(pub T);

macro_rules! impl_variadics {
    ( [ $( $args:tt )* ] [] ) => {};
    ( [ $( $args:tt )* ] [ $next:tt $( $rest:tt )* ] ) => {
        impl_variadic!( $( $args )* $next );
        impl_variadics!( [ $( $args )* $next ] [ $( $rest )* ] );
    };
}

macro_rules! impl_variadic {
    ( $( ( $field:tt $type:ident ) )* ) => {
        impl<$( $type ),*> FilterFn for All<( $( $type ),*, )>
        where
            $( $type: FilterFn, )*
        {
            #[inline]
            fn is_enabled(&self, descriptor: &'static Descriptor) -> bool {
                true $( && self.0.$field.is_enabled(descriptor) )*
            }

            #[inline]
            fn max_level(&self) -> LevelFilter {
                [ $( self.0.$field.max_level(), )* ]
                    .into_iter().min().unwrap_or(LevelFilter::ALL)
            }
        }

        impl<$( $type ),*> ExternalFilterFn for All<( $( $type ),*, )>
        where
            $( $type: ExternalFilterFn, )*
        {
            #[inline]
            fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
            where
                Desc: ExternalDescriptor,
            {
                true $( && self.0.$field.is_enabled_external(descriptor) )*
            }
        }

        impl<$( $type ),*> FilterFn for Any<( $( $type ),*, )>
        where
            $( $type: FilterFn, )*
        {
            #[inline]
            fn is_enabled(&self, descriptor: &'static Descriptor) -> bool {
                false $( || self.0.$field.is_enabled(descriptor) )*
            }

            #[inline]
            fn max_level(&self) -> LevelFilter {
                [ $( self.0.$field.max_level(), )* ]
                    .into_iter().max().unwrap_or(LevelFilter::NONE)
            }
        }

        impl<$( $type ),*> ExternalFilterFn for Any<( $( $type ),*, )>
        where
            $( $type: ExternalFilterFn, )*
        {
            #[inline]
            fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
            where
                Desc: ExternalDescriptor,
            {
                false $( || self.0.$field.is_enabled_external(descriptor) )*
            }
        }
    };
}

impl_variadics! {
    [] [
        (0 T1) (1 T2) (2 T3) (3 T4)
        (4 T5) (5 T6) (6 T7) (7 T8)
        (8 T9) (9 T10) (10 T11) (11 T12)
    ]
}

// Crate-internal helpers.

/// Returns `true` if `path` equals `prefix` or is a descendant path of it
/// (`prefix::...`).
#[inline]
#[must_use]
pub(crate) fn path_prefix_match(path: &str, prefix: &str) -> bool {
    path.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with("::"))
}

/// Returns the effective target of a descriptor: explicit target if set,
/// module path otherwise.
#[inline]
#[must_use]
pub(crate) fn effective_target(descriptor: &'static Descriptor) -> &'static str {
    descriptor.target.unwrap_or(descriptor.module_path)
}

/// Returns the effective target of a descriptor: explicit target if set,
/// module path otherwise.
#[inline]
#[must_use]
pub(crate) fn external_effective_target<Desc>(descriptor: &Desc) -> &str
where
    Desc: ExternalDescriptor,
{
    descriptor
        .target()
        .or(descriptor.module_path())
        .map_or("", |value| value.as_str())
}

#[cfg(test)]
mod tests {
    use alloc::borrow::Cow;
    use core::panic::Location;

    use polog::{Descriptor, DescriptorKind, Level, LevelFilter};
    use polog_test_helpers::tracker::Tracker;

    use crate::filters::{
        All, Always, Any, ConstMaxLevel, MaxLevel, Never, Not, Target, TargetPrefix,
    };
    use crate::{ExternalFilterFn, Filter, FilterFn, TypeErasedExternalDescriptor};

    #[inline]
    const fn descriptor(
        level: Level,
        module_path: &'static str,
        target: Option<&'static str>,
    ) -> Descriptor {
        Descriptor {
            kind: DescriptorKind::Log,
            level,
            module_path,
            target,
            message: "",
            param_names: &[],
            location: *Location::caller(),
        }
    }

    #[test]
    fn test_filters_descriptor() {
        let desc = descriptor(Level::Error, "module", Some("target"));
        assert_eq!(desc.level, Level::Error);
        assert_eq!(desc.module_path, "module");
        assert_eq!(desc.target, Some("target"));
    }

    #[test]
    fn test_filters_always_and_never_desc() {
        const DESC: Descriptor = descriptor(Level::Error, "any::module", None);

        assert!(Always.is_enabled(&DESC));
        assert_eq!(Always.max_level(), LevelFilter::Trace);

        assert!(!Never.is_enabled(&DESC));
        assert_eq!(Never.max_level(), LevelFilter::Off);
    }

    #[test]
    fn test_filters_max_level_desc() {
        const FATAL: Descriptor = descriptor(Level::Fatal, "", None);
        const BUG: Descriptor = descriptor(Level::Bug, "", None);
        const ERROR: Descriptor = descriptor(Level::Error, "", None);
        const WARN: Descriptor = descriptor(Level::Warn, "", None);
        const INFO: Descriptor = descriptor(Level::Info, "", None);
        const DEBUG: Descriptor = descriptor(Level::Debug, "", None);
        const TRACE: Descriptor = descriptor(Level::Trace, "", None);

        let filter = MaxLevel(LevelFilter::Off);
        assert_eq!(filter.max_level(), LevelFilter::Off);
        for desc in [&FATAL, &BUG, &ERROR, &WARN, &INFO, &DEBUG, &TRACE] {
            assert!(!filter.is_enabled(desc));
        }

        let filter = MaxLevel(LevelFilter::Info);
        assert_eq!(filter.max_level(), LevelFilter::Info);
        for desc in [&FATAL, &BUG, &ERROR, &WARN, &INFO] {
            assert!(filter.is_enabled(desc));
        }
        for desc in [&DEBUG, &TRACE] {
            assert!(!filter.is_enabled(desc));
        }

        let filter = MaxLevel(LevelFilter::Trace);
        assert_eq!(filter.max_level(), LevelFilter::Trace);
        for desc in [&FATAL, &BUG, &ERROR, &WARN, &INFO, &DEBUG, &TRACE] {
            assert!(filter.is_enabled(desc));
        }
    }

    #[test]
    fn test_filters_const_max_level_desc() {
        const FATAL: Descriptor = descriptor(Level::Fatal, "", None);
        const BUG: Descriptor = descriptor(Level::Bug, "", None);
        const ERROR: Descriptor = descriptor(Level::Error, "", None);
        const WARN: Descriptor = descriptor(Level::Warn, "", None);
        const INFO: Descriptor = descriptor(Level::Info, "", None);
        const DEBUG: Descriptor = descriptor(Level::Debug, "", None);
        const TRACE: Descriptor = descriptor(Level::Trace, "", None);

        let filter = ConstMaxLevel::<{ LevelFilter::Off.into_u8() }>;
        assert_eq!(filter.max_level(), LevelFilter::Off);
        for desc in [&FATAL, &BUG, &ERROR, &WARN, &INFO, &DEBUG, &TRACE] {
            assert!(!filter.is_enabled(desc));
        }

        let filter = ConstMaxLevel::<{ LevelFilter::Info.into_u8() }>;
        assert_eq!(filter.max_level(), LevelFilter::Info);
        for desc in [&FATAL, &BUG, &ERROR, &WARN, &INFO] {
            assert!(filter.is_enabled(desc));
        }
        for desc in [&DEBUG, &TRACE] {
            assert!(!filter.is_enabled(desc));
        }

        let filter = ConstMaxLevel::<{ LevelFilter::Trace.into_u8() }>;
        assert_eq!(filter.max_level(), LevelFilter::Trace);
        for desc in [&FATAL, &BUG, &ERROR, &WARN, &INFO, &DEBUG, &TRACE] {
            assert!(filter.is_enabled(desc));
        }
    }

    #[test]
    fn test_filters_target_and_target_prefix_desc() {
        const DESC: Descriptor = descriptor(Level::Info, "foo::bar", Some("qwe::asd"));

        assert!(Target("qwe::asd").is_enabled(&DESC));
        assert!(!Target("qwe").is_enabled(&DESC));
        assert!(!Target("foo::bar").is_enabled(&DESC));
        assert!(!Target("foo").is_enabled(&DESC));

        assert!(TargetPrefix("qwe::asd").is_enabled(&DESC));
        assert!(TargetPrefix("qwe").is_enabled(&DESC));
        assert!(!TargetPrefix("qweeee").is_enabled(&DESC));
        assert!(!TargetPrefix("foo::bar").is_enabled(&DESC));
        assert!(!TargetPrefix("foo").is_enabled(&DESC));
        assert!(!TargetPrefix("fooooo").is_enabled(&DESC));

        assert_eq!(Target("").max_level(), LevelFilter::Trace);
        assert_eq!(Target("any").max_level(), LevelFilter::Trace);
    }

    #[test]
    fn test_filters_module_and_module_prefix_desc() {
        const DESC: Descriptor = descriptor(Level::Info, "foo::bar", None);

        assert!(!Target("qwe::asd").is_enabled(&DESC));
        assert!(!Target("qwe").is_enabled(&DESC));
        assert!(Target("foo::bar").is_enabled(&DESC));
        assert!(!Target("foo").is_enabled(&DESC));

        assert!(!TargetPrefix("qwe::asd").is_enabled(&DESC));
        assert!(!TargetPrefix("qwe").is_enabled(&DESC));
        assert!(!TargetPrefix("qweeee").is_enabled(&DESC));
        assert!(TargetPrefix("foo::bar").is_enabled(&DESC));
        assert!(TargetPrefix("foo").is_enabled(&DESC));
        assert!(!TargetPrefix("fooooo").is_enabled(&DESC));

        assert_eq!(TargetPrefix("").max_level(), LevelFilter::Trace);
        assert_eq!(TargetPrefix("any").max_level(), LevelFilter::Trace);
    }

    #[test]
    fn test_filters_combinators_desc() {
        const DESC: Descriptor = descriptor(Level::Debug, "foo::bar", None);

        assert!(Not(MaxLevel(LevelFilter::Info)).is_enabled(&DESC));
        assert!(!Not(MaxLevel(LevelFilter::Trace)).is_enabled(&DESC));

        let both = All((MaxLevel(LevelFilter::Debug), TargetPrefix("foo")));
        assert!(both.is_enabled(&DESC));
        let fail = All((MaxLevel(LevelFilter::Info), TargetPrefix("foo")));
        assert!(!fail.is_enabled(&DESC));

        let either = Any((MaxLevel(LevelFilter::Info), TargetPrefix("other")));
        assert!(!either.is_enabled(&DESC));
        let pass = Any((MaxLevel(LevelFilter::Info), TargetPrefix("foo")));
        assert!(pass.is_enabled(&DESC));

        let not = Not(MaxLevel(LevelFilter::Info));
        assert_eq!(not.max_level(), LevelFilter::Trace);

        let all = All((
            MaxLevel(LevelFilter::Debug),
            MaxLevel(LevelFilter::Info),
            TargetPrefix("foo"),
        ));
        assert_eq!(all.max_level(), LevelFilter::Info);

        let any = Any((
            MaxLevel(LevelFilter::Debug),
            MaxLevel(LevelFilter::Info),
            TargetPrefix("foo"),
        ));
        assert_eq!(any.max_level(), LevelFilter::Trace);

        let any = Any((MaxLevel(LevelFilter::Debug), MaxLevel(LevelFilter::Info)));
        assert_eq!(any.max_level(), LevelFilter::Debug);
    }

    #[test]
    fn test_filters_closure_filter_desc() {
        const EVEN1: Descriptor = descriptor(Level::Info, "mod", None);
        const EVEN2: Descriptor = descriptor(Level::Info, "mod", None);

        let filter = |d: &'static Descriptor| d.location.line() % 2 == 0;

        assert_eq!(filter.is_enabled(&EVEN1), EVEN1.location.line() % 2 == 0);
        assert_eq!(filter.is_enabled(&EVEN2), EVEN2.location.line() % 2 == 0);
    }

    #[test]
    fn test_filters_max_level() {
        let tracker = Tracker::default();
        let mut logger = Filter::new(tracker.clone(), MaxLevel(LevelFilter::Info));

        polog::info!(&mut logger);
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);
        polog::debug!(&mut logger);
        assert_eq!(tracker.take_events(), [""; 0]);
    }

    #[test]
    fn test_filters_const_max_level() {
        let tracker = Tracker::default();
        let mut logger = Filter::new(
            tracker.clone(),
            ConstMaxLevel::<{ LevelFilter::Info.into_u8() }>,
        );

        polog::info!(&mut logger);
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);
        polog::debug!(&mut logger);
        assert_eq!(tracker.take_events(), [""; 0]);
    }

    #[test]
    fn test_filters_target() {
        let tracker = Tracker::default();
        let mut logger = Filter::new(tracker.clone(), Target("specific_target"));

        polog::info!(&mut logger, target: "specific_target");
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);
        polog::info!(&mut logger, target: "other");
        assert_eq!(tracker.take_events(), [""; 0]);
    }

    #[test]
    fn test_filters_target_prefix() {
        let tracker = Tracker::default();
        let mut logger = Filter::new(tracker.clone(), TargetPrefix("app"));

        polog::info!(&mut logger, target: "app");
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);
        polog::info!(&mut logger, target: "app::inner");
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);
        polog::info!(&mut logger, target: "other");
        assert_eq!(tracker.take_events(), [""; 0]);
    }

    #[test]
    fn test_filters_not() {
        let tracker = Tracker::default();
        let mut logger = Filter::new(tracker.clone(), Not(MaxLevel(LevelFilter::Info)));

        polog::warn!(&mut logger);
        assert_eq!(tracker.take_events(), [""; 0]);
        polog::info!(&mut logger);
        assert_eq!(tracker.take_events(), [""; 0]);
        polog::debug!(&mut logger);
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);
    }

    #[test]
    fn test_filters_all() {
        let tracker = Tracker::default();
        let mut logger = Filter::new(
            tracker.clone(),
            All((MaxLevel(LevelFilter::Info), TargetPrefix("app"))),
        );

        polog::info!(&mut logger, target: "app");
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);
        polog::trace!(&mut logger, target: "app");
        assert_eq!(tracker.take_events(), [""; 0]);
        polog::info!(&mut logger, target: "other");
        assert_eq!(tracker.take_events(), [""; 0]);
        polog::trace!(&mut logger, target: "other");
        assert_eq!(tracker.take_events(), [""; 0]);
    }

    #[test]
    fn test_filters_any() {
        let tracker = Tracker::default();
        let mut logger = Filter::new(
            tracker.clone(),
            Any((MaxLevel(LevelFilter::Info), TargetPrefix("app"))),
        );

        polog::info!(&mut logger, target: "app");
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);
        polog::trace!(&mut logger, target: "app");
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);
        polog::info!(&mut logger, target: "other");
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);
        polog::trace!(&mut logger, target: "other");
        assert_eq!(tracker.take_events(), [""; 0]);
    }

    #[test]
    fn test_filters_always_and_never2() {
        let tracker = Tracker::default();

        let mut logger = Filter::new(tracker.clone(), Always);
        polog::trace!(&mut logger, "Always");
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);

        let mut logger = Filter::new(tracker.clone(), Never);
        polog::trace!(&mut logger, "Never");
        assert_eq!(tracker.take_events(), [""; 0]);
    }

    #[test]
    fn test_filters_external_filter() {
        let desc = |level| TypeErasedExternalDescriptor {
            message: Some(Cow::Borrowed("name")),
            level,
            target: Some(Cow::Borrowed("target::path")),
            module_path: Some(Cow::Borrowed("module")),
            file: Some(Cow::Borrowed("file")),
            line: Some(1),
            column: Some(1),
        };

        assert!(Always.is_enabled_external(&desc(Level::Debug)));
        assert!(!Never.is_enabled_external(&desc(Level::Debug)));

        assert!(MaxLevel(LevelFilter::Info).is_enabled_external(&desc(Level::Info)));
        assert!(!MaxLevel(LevelFilter::Info).is_enabled_external(&desc(Level::Debug)));

        assert!(
            ConstMaxLevel::<{ LevelFilter::Info.into_u8() }>
                .is_enabled_external(&desc(Level::Info))
        );
        assert!(
            !ConstMaxLevel::<{ LevelFilter::Info.into_u8() }>
                .is_enabled_external(&desc(Level::Debug))
        );

        assert!(!Target("target").is_enabled_external(&desc(Level::Info)));
        assert!(Target("target::path").is_enabled_external(&desc(Level::Info)));
        assert!(!Target("other").is_enabled_external(&desc(Level::Info)));

        assert!(TargetPrefix("target").is_enabled_external(&desc(Level::Info)));
        assert!(!TargetPrefix("tar").is_enabled_external(&desc(Level::Info)));

        assert!(Not(MaxLevel(LevelFilter::Info)).is_enabled_external(&desc(Level::Debug)));
        assert!(
            !All((MaxLevel(LevelFilter::Info), TargetPrefix("target")))
                .is_enabled_external(&desc(Level::Debug))
        );
        assert!(
            All((MaxLevel(LevelFilter::Info), TargetPrefix("target")))
                .is_enabled_external(&desc(Level::Info))
        );
        assert!(
            Any((MaxLevel(LevelFilter::Info), TargetPrefix("target")))
                .is_enabled_external(&desc(Level::Debug))
        );
    }
}
