use alloc::string::String;
use alloc::vec::Vec;
use core::error::Error;
use core::fmt;
use core::marker::PhantomData;

use polog::{Descriptor, LevelFilter};

use crate::filters::{effective_target, external_effective_target, path_prefix_match};
use crate::{ExternalDescriptor, ExternalFilterFn, FilterFn};

/// One prefix and its matching maximum‑level mapping within a [`FilterRules`].
///
/// The `prefix` is matched against a descriptor's effective target using
/// path prefix matching: the rule matches if the prefix equals the target or is
/// a `::`‑separated ancestor of it.
/// Longer prefixes take precedence, so a rule with prefix
/// `module::inner` overrides `module` for descriptors under
/// `module::inner`.
///
/// [`FilterRules`]: crate::FilterRules.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct FilterRule<T> {
    /// Target prefix this rule applies to.
    pub prefix: T,
    /// Maximum level (inclusive) this rule permits.
    pub max_level: LevelFilter,
}

/// A set of per-module maximum-level rules.
///
/// Similar to the `RUST_LOG` environment variable, maps module-path prefixes to
/// a maximum enabled level, falling back to a configurable default for
/// unmatched targets.
///
/// Rules are matched against the descriptor's *effective target* (see
/// [`Target`]) by longest-prefix first, so more specific rules override less
/// specific ones.
///
/// # Syntax
///
/// [`FilterRules<T>::parse`] accepts a `RUST_LOG`-style string:
///
/// ```text
/// [default_level][,module1=level1][,module2=level2]...
/// ```
///
/// Items are separated by `,` and whitespace around items is ignored.
/// A bare level sets the default for unmatched targets.
/// A `prefix=level` pair adds a rule for the given prefix.
///
/// Accepted level names (case-insensitive): `off`, `fatal`, `bug`,
/// `error`, `warn`, `info`, `debug`, `trace`.
///
/// # Example
///
/// ```rust
/// use polog::LevelFilter;
/// use polog_util::FilterRules;
///
/// let rules = FilterRules::parse("warn,module=debug,module::inner=trace").unwrap();
///
/// // Default level applies to unmatched targets.
/// assert_eq!(rules.max_level_for("other"), LevelFilter::Warn);
///
/// // Longest matching prefix wins.
/// assert_eq!(rules.max_level_for("module"), LevelFilter::Debug);
/// assert_eq!(rules.max_level_for("module::inner"), LevelFilter::Trace);
/// assert_eq!(rules.max_level_for("module::other"), LevelFilter::Debug);
/// ```
///
/// [`Target`]: crate::filters::Target
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FilterRules<T, U> {
    default: LevelFilter,
    max_level: LevelFilter,
    /// Sorted by descending prefix length so the first matching rule is the
    /// most specific one.
    rules: T,
    _prefix_ty: PhantomData<U>,
}

/// The borrowing form of [`FilterRules`].
///
/// `BorrowingFilterRules` is a `FilterRules` whose rules and prefix type are
/// stored as a `Vec<FilterRule<&'a str>>, &'a str>`.
/// The plain `FilterRules` may borrow the input string during parsing.
/// call [`FilterRules::to_owning`] to obtain a `'static` rule set that
/// can be stored inside a handler.
pub type BorrowingFilterRules<'a> = FilterRules<Vec<FilterRule<&'a str>>, &'a str>;

/// The owning form of [`FilterRules`].
///
/// `OwningFilterRules` is a `FilterRules` whose rules and prefix type are
/// stored as a `Vec<FilterRule<String>>`.
/// The plain `FilterRules` may borrow the input string during parsing.
/// call [`FilterRules::to_owning`] to obtain a `'static` rule set that
/// can be stored inside a handler.
pub type OwningFilterRules = FilterRules<Vec<FilterRule<String>>, String>;

/// Builder for [`FilterRules`].
///
/// Use [`FilterRulesBuilder::with_default`] to create a builder, then
/// [`add_rule`] one rule per prefix and call [`build`] to obtain a
/// [`FilterRules`].
///
/// Prefer [`FilterRules::parse`] for parsing a `RUST_LOG`‑style string.
/// The builder is for programmatic construction where the input does not come
/// from a string.
///
/// [`add_rule`]: FilterRulesBuilder::add_rule
/// [`build`]: FilterRulesBuilder::build
/// [`FilterRules::parse`]: FilterRules::parse
#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct FilterRulesBuilder<T> {
    default: LevelFilter,
    rules: Vec<FilterRule<T>>,
}

#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
impl<T> FilterRule<T> {
    /// Returns an owning copy of this rule.
    ///
    /// The returned `FilterRule<String>` owns its prefix and can outlive the
    /// input the original rule borrowed from.
    /// Use this to make a `'static` rule out of a rule parsed from a
    /// `RUST_LOG`‑style string.
    #[inline]
    pub fn to_owning(&self) -> FilterRule<::alloc::string::String>
    where
        T: AsRef<str>,
    {
        use alloc::borrow::ToOwned;

        let Self { prefix, max_level } = self;
        FilterRule {
            prefix: prefix.as_ref().to_owned(),
            max_level: *max_level,
        }
    }
}

impl<T, U> FilterRules<T, U> {
    /// Creates an empty rule set with the given default level.
    ///
    /// Use [`LevelFilter::Off`] to reject everything by default.
    #[inline]
    #[must_use]
    pub fn with_default(default: LevelFilter) -> Self
    where
        T: Default,
    {
        Self {
            default,
            max_level: default,
            rules: T::default(),
            _prefix_ty: PhantomData,
        }
    }

    /// Returns the default level applied to targets not matched by any rule.
    #[inline]
    #[must_use]
    pub const fn default_level(&self) -> LevelFilter {
        self.default
    }

    /// Sets the default level applied to targets not matched by any rule.
    #[inline]
    pub const fn set_default_level(&mut self, level: LevelFilter) {
        self.default = level;
    }

    /// Returns the rules in effect.
    ///
    /// The rules are ordered by descending prefix length, so the first rule
    /// that matches a target during a linear scan is the most specific one.
    /// The rules are stored as the generic `T` of the `FilterRules`
    /// parameter.
    /// For `FilterRules<T>` this is `Vec<FilterRule<T>>`.
    #[inline]
    #[must_use]
    pub const fn rules(&self) -> &T {
        &self.rules
    }

    /// Replaces the rule set, rejecting it if any later rule is a prefix of
    /// another earlier rule.
    ///
    /// Returns `true` if the rules were accepted and stored, `false` if they
    /// were rejected (the existing rules remain in effect).
    #[inline]
    pub fn try_set_rules(&mut self, rules: T) -> bool
    where
        T: AsRef<[FilterRule<U>]>,
        U: AsRef<str>,
    {
        let rules_ref = rules.as_ref();
        for (i, rule1) in rules_ref.iter().enumerate() {
            for rule2 in rules_ref.iter().skip(i).skip(1) {
                if rule2.prefix.as_ref().starts_with(rule1.prefix.as_ref()) {
                    return false;
                }
            }
        }
        self.rules = rules;
        true
    }

    /// Returns the maximum level that applies to `target`.
    #[inline]
    #[must_use]
    pub fn max_level_for(&self, target: &str) -> LevelFilter
    where
        T: AsRef<[FilterRule<U>]>,
        U: AsRef<str>,
    {
        for rule in self.rules.as_ref() {
            if path_prefix_match(target, rule.prefix.as_ref()) {
                return rule.max_level;
            }
        }
        self.default
    }

    /// Returns an owning copy of this rule set.
    ///
    /// The returned rule set borrows nothing from `self` and can outlive
    /// the string that produced `self`.
    /// Use this to store a parsed `RUST_LOG`‑style rule set in a handler
    /// that must be `'static`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use polog_util::FilterRules;
    ///
    /// let parsed = FilterRules::parse("info,module=debug").unwrap();
    /// let owned = parsed.to_owning();
    /// // `owned` owns its rules and does not borrow from `parsed`.
    /// ```
    #[inline]
    pub fn to_owning<'a>(&'a self) -> OwningFilterRules
    where
        &'a T: IntoIterator<Item = &'a FilterRule<U>>,
        U: 'a + AsRef<str>,
    {
        let Self {
            default,
            max_level,
            rules,
            _prefix_ty,
        } = self;

        FilterRules {
            default: *default,
            max_level: *max_level,
            rules: rules.into_iter().map(FilterRule::to_owning).collect(),
            _prefix_ty: PhantomData,
        }
    }
}

#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
impl<'a> BorrowingFilterRules<'a> {
    /// Parses rules from a `RUST_LOG`-style string.
    ///
    /// # Errors
    ///
    /// Returns [`ParseFilterRulesError::UnknownLevel`] if an unknown level name
    /// is encountered.
    ///
    /// # Example
    ///
    /// ```rust
    /// use polog_util::FilterRules;
    ///
    /// let filter = FilterRules::parse("warn,module=debug").unwrap();
    /// ```
    #[inline]
    pub fn parse(spec: &'a str) -> Result<Self, ParseFilterRulesError> {
        let mut builder = FilterRulesBuilder::with_default(LevelFilter::Error);

        for item in spec.split(',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }

            if let Some((prefix, level)) = item.split_once('=') {
                let prefix = prefix.trim();
                let level = parse_level(level.trim())?;
                builder.add_rule(prefix, level);
            } else {
                builder.set_default_level(parse_level(item)?);
            }
        }

        Ok(builder.build())
    }
}

#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
impl<T> FilterRulesBuilder<T> {
    /// Creates an empty rule set with the given default level.
    ///
    /// Use [`LevelFilter::Off`] to reject everything by default.
    #[inline]
    #[must_use]
    pub const fn with_default(default: LevelFilter) -> Self {
        Self {
            default,
            rules: Vec::new(),
        }
    }

    /// Sets the default level applied to targets not matched by any rule.
    #[inline]
    pub const fn set_default_level(&mut self, level: LevelFilter) {
        self.default = level;
    }

    /// Adds or replaces a rule for `prefix`.
    ///
    /// Matching uses longest-prefix-first semantics: a rule with prefix
    /// `module::inner` takes precedence over `module` for descriptors
    /// whose effective target is `module::inner` or a descendant.
    #[inline]
    pub fn add_rule(&mut self, prefix: T, max_level: LevelFilter) {
        self.rules.push(FilterRule { prefix, max_level });
    }

    /// Adds or replaces a rule for `prefix`.
    ///
    /// Matching uses longest-prefix-first semantics: a rule with prefix
    /// `module::inner` takes precedence over `module` for descriptors
    /// whose effective target is `module::inner` or a descendant.
    #[inline]
    #[must_use]
    pub fn build(self) -> FilterRules<Vec<FilterRule<T>>, T>
    where
        T: AsRef<str>,
    {
        use core::cmp::Reverse;

        let Self { default, mut rules } = self;
        let max_level = rules.iter().map(|rule| rule.max_level).max();
        let max_level = max_level.unwrap_or(default).max(default);
        rules.sort_by_key(|rule| Reverse(rule.prefix.as_ref().len()));
        FilterRules {
            default,
            max_level,
            rules,
            _prefix_ty: PhantomData,
        }
    }
}

impl<T, U> FilterFn for FilterRules<T, U>
where
    T: AsRef<[FilterRule<U>]>,
    U: AsRef<str>,
{
    #[inline]
    fn is_enabled(&self, descriptor: &'static Descriptor) -> bool {
        if descriptor.level > self.max_level {
            return false;
        }

        let target = effective_target(descriptor);
        let max_level = self.max_level_for(target);
        descriptor.level <= max_level
    }

    #[inline]
    fn max_level(&self) -> LevelFilter {
        self.max_level
    }
}

impl<T, U> ExternalFilterFn for FilterRules<T, U>
where
    T: AsRef<[FilterRule<U>]>,
    U: AsRef<str>,
{
    #[inline]
    fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor,
    {
        if descriptor.level() > self.max_level {
            return false;
        }

        let target = external_effective_target(descriptor);
        let max_level = self.max_level_for(target);
        descriptor.level() <= max_level
    }
}

/// Error returned by [`FilterRules<T>::parse`].
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum ParseFilterRulesError {
    /// The level name is not one of the recognized level names.
    ///
    /// Accepted names (case-insensitive): `off`, `fatal`, `bug`, `error`,
    /// `warn`, `info`, `debug`, `trace`.
    UnknownLevel,
}

impl fmt::Display for ParseFilterRulesError {
    #[expect(clippy::missing_inline_in_public_items, reason = "cold path")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownLevel => f.write_str("unknown level name"),
        }
    }
}

impl Error for ParseFilterRulesError {}

impl From<!> for ParseFilterRulesError {
    #[inline]
    fn from(value: !) -> Self {
        match value {}
    }
}

#[inline]
const fn parse_level(s: &str) -> Result<LevelFilter, ParseFilterRulesError> {
    if s.eq_ignore_ascii_case("off") {
        Ok(LevelFilter::Off)
    } else if s.eq_ignore_ascii_case("fatal") {
        Ok(LevelFilter::Fatal)
    } else if s.eq_ignore_ascii_case("bug") {
        Ok(LevelFilter::Bug)
    } else if s.eq_ignore_ascii_case("error") {
        Ok(LevelFilter::Error)
    } else if s.eq_ignore_ascii_case("warn") {
        Ok(LevelFilter::Warn)
    } else if s.eq_ignore_ascii_case("info") {
        Ok(LevelFilter::Info)
    } else if s.eq_ignore_ascii_case("debug") {
        Ok(LevelFilter::Debug)
    } else if s.eq_ignore_ascii_case("trace") {
        Ok(LevelFilter::Trace)
    } else {
        Err(ParseFilterRulesError::UnknownLevel)
    }
}

#[expect(clippy::unwrap_used, reason = "okay in tests")]
#[cfg(test)]
mod tests {
    use alloc::borrow::{Cow, ToOwned};
    use alloc::string::String;
    use alloc::vec::Vec;
    use alloc::{format, vec};
    use core::panic::Location;

    use polog::{Descriptor, DescriptorKind, Level, LevelFilter};
    use polog_test_helpers::tracker::Tracker;

    use crate::{
        ExternalFilterFn, Filter, FilterFn, FilterRule, FilterRules, FilterRulesBuilder,
        ParseFilterRulesError, TypeErasedExternalDescriptor,
    };

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

    #[inline]
    fn is_enabled(filter: &FilterRules<Vec<FilterRule<&str>>, &str>, expected: [u8; 9]) {
        static FATAL_FOO_BAR: Descriptor = descriptor(Level::Fatal, "foo::bar", None);
        static INFO_FOO_BAR: Descriptor = descriptor(Level::Info, "foo::bar", None);
        static TRACE_FOO_BAR: Descriptor = descriptor(Level::Trace, "foo::bar", None);
        static FATAL_FOO_BAZ: Descriptor = descriptor(Level::Fatal, "foo::baz", None);
        static INFO_FOO_BAZ: Descriptor = descriptor(Level::Info, "foo::baz", None);
        static TRACE_FOO_BAZ: Descriptor = descriptor(Level::Trace, "foo::baz", None);
        static FATAL_QWE_ASD: Descriptor = descriptor(Level::Fatal, "qwe::asd", None);
        static INFO_QWE_ASD: Descriptor = descriptor(Level::Info, "qwe::asd", None);
        static TRACE_QWE_ASD: Descriptor = descriptor(Level::Trace, "qwe::asd", None);

        assert_eq!(filter.is_enabled(&FATAL_FOO_BAR), expected[0] != 0);
        assert_eq!(filter.is_enabled(&INFO_FOO_BAR), expected[1] != 0);
        assert_eq!(filter.is_enabled(&TRACE_FOO_BAR), expected[2] != 0);
        assert_eq!(filter.is_enabled(&FATAL_FOO_BAZ), expected[3] != 0);
        assert_eq!(filter.is_enabled(&INFO_FOO_BAZ), expected[4] != 0);
        assert_eq!(filter.is_enabled(&TRACE_FOO_BAZ), expected[5] != 0);
        assert_eq!(filter.is_enabled(&FATAL_QWE_ASD), expected[6] != 0);
        assert_eq!(filter.is_enabled(&INFO_QWE_ASD), expected[7] != 0);
        assert_eq!(filter.is_enabled(&TRACE_QWE_ASD), expected[8] != 0);
    }

    #[test]
    fn test_rules_descriptor() {
        let desc = descriptor(Level::Error, "module", Some("target"));
        assert_eq!(desc.level, Level::Error);
        assert_eq!(desc.module_path, "module");
        assert_eq!(desc.target, Some("target"));
    }

    #[test]
    fn test_rules_parse_simple() {
        let filter = FilterRules::parse("").unwrap();
        assert_eq!(filter.default_level(), LevelFilter::Error);
        is_enabled(&filter, [1, 0, 0, 1, 0, 0, 1, 0, 0]);

        let filter = FilterRules::parse("off").unwrap();
        assert_eq!(filter.default_level(), LevelFilter::Off);
        is_enabled(&filter, [0, 0, 0, 0, 0, 0, 0, 0, 0]);

        let filter = FilterRules::parse("info").unwrap();
        assert_eq!(filter.default_level(), LevelFilter::Info);
        is_enabled(&filter, [1, 1, 0, 1, 1, 0, 1, 1, 0]);

        let filter = FilterRules::parse("warn  ,  foo  =   dEbUg").unwrap();
        assert_eq!(filter.default_level(), LevelFilter::Warn);
        is_enabled(&filter, [1, 1, 0, 1, 1, 0, 1, 0, 0]);

        // Prefix does not match partial segment.
        let filter = FilterRules::parse("warn,fo=debug").unwrap();
        assert_eq!(filter.default_level(), LevelFilter::Warn);
        is_enabled(&filter, [1, 0, 0, 1, 0, 0, 1, 0, 0]);

        // Longest prefix wins.
        let filter = FilterRules::parse("warn,foo=DEBUG,foo::bar=off").unwrap();
        assert_eq!(filter.default_level(), LevelFilter::Warn);
        is_enabled(&filter, [0, 0, 0, 1, 1, 0, 1, 0, 0]);

        // Longest prefix wins.
        let filter = FilterRules::parse("warn,foo::bar=oFf,foo=debug").unwrap();
        assert_eq!(filter.default_level(), LevelFilter::Warn);
        is_enabled(&filter, [0, 0, 0, 1, 1, 0, 1, 0, 0]);

        assert_eq!(
            FilterRules::parse("foo=bar").map_err(|err| format!("{err}")),
            Err("unknown level name".to_owned())
        );

        for i in 0..=7 {
            let level_filter = LevelFilter::try_from_u8(i).unwrap();
            let filter = FilterRules::parse(level_filter.as_lowercase_str()).unwrap();
            assert_eq!(filter.default_level(), level_filter);
        }
    }

    #[test]
    fn test_rules_unknown_level_errors() {
        assert_eq!(
            FilterRules::parse("verbose"),
            Err(ParseFilterRulesError::UnknownLevel),
        );
        assert_eq!(
            FilterRules::parse("info,foo=verbose"),
            Err(ParseFilterRulesError::UnknownLevel),
        );
    }

    #[test]
    fn test_rule_to_owning() {
        let rule: FilterRule<&str> = FilterRule {
            prefix: "abc",
            max_level: LevelFilter::Debug,
        };
        let owned: FilterRule<String> = rule.to_owning();
        assert_eq!(owned.prefix, "abc");
        assert_eq!(owned.max_level, LevelFilter::Debug);

        let parsed = FilterRules::parse("info,module=debug").unwrap();
        let owned = parsed.to_owning();
        assert_eq!(owned.default_level(), LevelFilter::Info);
        assert_eq!(owned.max_level_for("module"), LevelFilter::Debug);
        assert_eq!(owned.max_level_for("other"), LevelFilter::Info);
    }

    #[test]
    fn test_rules_default_level() {
        let mut rules = FilterRules::<Vec<FilterRule<&str>>, &str>::with_default(LevelFilter::Warn);
        assert_eq!(rules.default_level(), LevelFilter::Warn);

        rules.set_default_level(LevelFilter::Error);
        assert_eq!(rules.default_level(), LevelFilter::Error);
    }

    #[test]
    fn test_rules_try_set_rules() {
        let mut rules = FilterRules::<Vec<FilterRule<&str>>, &str>::with_default(LevelFilter::Off);

        // Sorted (longest first): accepted.
        assert!(rules.try_set_rules(vec![
            FilterRule {
                prefix: "foo::bar::baz",
                max_level: LevelFilter::Trace,
            },
            FilterRule {
                prefix: "foo::bar",
                max_level: LevelFilter::Debug,
            },
            FilterRule {
                prefix: "foo",
                max_level: LevelFilter::Info,
            },
        ]));

        // Later prefix is a prefix of an earlier one: rejected.
        assert!(!rules.try_set_rules(vec![
            FilterRule {
                prefix: "foo",
                max_level: LevelFilter::Trace,
            },
            FilterRule {
                prefix: "baz",
                max_level: LevelFilter::Info,
            },
            FilterRule {
                prefix: "foo::bar",
                max_level: LevelFilter::Debug,
            },
        ]));

        // Duplicate prefixes: rejected.
        assert!(!rules.try_set_rules(vec![
            FilterRule {
                prefix: "foo",
                max_level: LevelFilter::Trace,
            },
            FilterRule {
                prefix: "bar",
                max_level: LevelFilter::Info,
            },
            FilterRule {
                prefix: "foo",
                max_level: LevelFilter::Debug,
            },
        ]));
    }

    #[test]
    fn test_rules_max_level_for() {
        let rules = FilterRules::parse("warn,foo::bar=debug,foo::bar::baz=trace")
            .unwrap()
            .to_owning();

        assert_eq!(rules.max_level_for("foo::bar::baz"), LevelFilter::Trace);
        assert_eq!(
            rules.max_level_for("foo::bar::baz::deep"),
            LevelFilter::Trace
        );
        assert_eq!(rules.max_level_for("foo::bar"), LevelFilter::Debug);
        assert_eq!(rules.max_level_for("foo::other"), LevelFilter::Warn);
        assert_eq!(rules.max_level_for("unrelated"), LevelFilter::Warn);
    }

    #[test]
    fn test_rules_builder() {
        let mut b = FilterRulesBuilder::with_default(LevelFilter::Warn);
        b.set_default_level(LevelFilter::Error);
        b.add_rule("foo", LevelFilter::Info);
        b.add_rule("foo::bar", LevelFilter::Debug);
        let rules = b.build();

        assert_eq!(
            rules.rules(),
            &[
                FilterRule {
                    prefix: "foo::bar",
                    max_level: LevelFilter::Debug,
                },
                FilterRule {
                    prefix: "foo",
                    max_level: LevelFilter::Info,
                }
            ]
        );

        assert_eq!(rules.default_level(), LevelFilter::Error);
        assert_eq!(rules.max_level_for("foo"), LevelFilter::Info);
        assert_eq!(rules.max_level_for("foo::bar"), LevelFilter::Debug);
        assert_eq!(rules.max_level_for("other"), LevelFilter::Error);

        assert_eq!(rules.max_level(), LevelFilter::Debug);
    }

    #[test]
    fn test_rules_applied() {
        let tracker = Tracker::default();
        let rules = FilterRules::parse("info,module=debug").unwrap().to_owning();
        let mut logger = Filter::new(tracker.clone(), rules);

        polog::info!(&mut logger, "Info");
        assert_eq!(tracker.take_events(), ["try_enable", "log"]);

        polog::debug!(&mut logger, "Debug");
        assert_eq!(tracker.take_events(), [""; 0]);
    }

    #[test]
    fn test_rules_external_filter() {
        let rules = FilterRules::parse("info,module=debug").unwrap().to_owning();

        let desc = |level| TypeErasedExternalDescriptor {
            message: None,
            level,
            target: Some(Cow::Borrowed("other")),
            module_path: None,
            file: None,
            line: None,
            column: None,
        };
        assert!(rules.is_enabled_external(&desc(Level::Info)));
        assert!(!rules.is_enabled_external(&desc(Level::Debug)));
        assert!(!rules.is_enabled_external(&desc(Level::Trace)));
    }
}
