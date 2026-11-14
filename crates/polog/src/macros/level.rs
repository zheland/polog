// See files:
// - `crates/polog/src/macros/log.rs` for log macro expansion details.
// - `crates/polog/src/macros/span.rs` for span macro expansion details.
// - `crates/polog/src/macros/arguments.rs` for argument parsing details.
// - `crates/polog/src/macros/descriptor.rs` for descriptor expansion details.

/// Defines a level-specific logging macro in terms of a generic macro.
///
/// - `$d` is the `$` token.
/// - `$macro` is the generated macro name;
/// - `$level_kind::$level` is the level path.
/// - `$fn` is the macro to delegate to.
/// - `$desc` and `$and` are doc text segments.
/// - `$extra_doc` is appended verbatim.
macro_rules! define_level_macro {
    (
        $d:tt, $macro:ident, $level_kind:ident::$level:ident,
        $fn:ident, $desc_tail:literal
        $(, $extra_doc:literal, $should_panic:literal )?
    ) => {
        #[doc = concat!(
            "Logs a message at the ", stringify!($level), " level", $desc_tail
        )]
        ///
        /// # Syntax
        ///
        /// The macro syntax is:
        #[doc = concat!(
            "- `",
            stringify!($macro),
            "!( $logger:expr, $( target: $target:expr, )? ",
            "$message:expr $(; $( $arg ),* $(,)? )? )`."
        )]
        ///
        /// This macro syntax is similar to the `log!` macro.
        /// For the detailed syntax documentation, please see the [`log!`] macro documentation.
        ///
        /// # Examples
        ///
        #[doc = concat!("```rust" $(, ",", $should_panic )? )]
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # {
        /// let mut logger = polog::Discard;
        /// # }
        #[doc = concat!(
            "polog::", ::core::stringify!($macro),
            "!(&mut logger, \"Message on ", stringify!($level), " level\");"
        )]
        #[doc = "```"]
        ///
        #[doc = concat!("```rust" $(, ",", $should_panic )? )]
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # {
        /// let mut logger = polog::Discard;
        /// # }
        /// let string = String::from("foo");
        /// let non_static_str_ref = string.as_str();
        #[doc = concat!("polog::", ::core::stringify!($macro), "!(")]
        ///     &mut logger, "Message";
        ///     arg1 = "value",
        ///     arg2 = :t non_static_str_ref,
        ///     arg3 = ? (123, true, 'a')
        /// );
        #[doc = "```"]
        ///
        /// [`log!`]: crate::log!
        $( #[doc = $extra_doc] )?
        #[macro_export]
        macro_rules! $macro {
            ( $d logger:expr $d (, $d ($d tt:tt)* )? ) => {
                $crate::$fn!(
                    $d logger,
                    $crate::$level_kind::$level
                    $d (, $d ($d tt)* )?
                )
            };
            ( $d logger:expr $d (; $d ($d tt:tt)* )? ) => {
                $crate::$fn!(
                    $d logger,
                    $crate::$level_kind::$level
                    $d (; $d ($d tt)* )?
                )
            };
        }
    };
}

define_level_macro!(
    $, fatal, Level::Fatal, __log_fatal_impl, " and panics.",
    "\n# Panics\n\nAlways panics.", "should_panic"
);

define_level_macro!(
    $, bug, Level::Bug, log,
    " and panics if `logger.should_panic_on_bug()` returns `true`.",
    "\n# Panics\n\nPanics if `logger.should_panic_on_bug` returns `true`.\n\
     If not overridden by the log `Handler` implementation it returns \
     `cfg!(debug_assertions)` and therefore panic on dev builds and don't \
     panic on release build when default profile options are used.
    ", "ignore"
);

define_level_macro!($, error, Level::Error, log, ".");
define_level_macro!($, warn, Level::Warn, log, ".");
define_level_macro!($, info, Level::Info, log, ".");
define_level_macro!($, debug, Level::Debug, log, ".");
define_level_macro!($, trace, Level::Trace, log, ".");

/// Defines a level-specific logging or span macro in terms of a generic macro.
///
/// - `$d` is the `$` token.
/// - `$macro` is the generated macro name;
/// - `$level_kind::$level` is the level path.
/// - `$fn` is the macro to delegate to.
/// - `$desc` and `$and` are doc text segments.
/// - `$extra_doc` is appended verbatim.
macro_rules! define_level_macro {
    ( $d:tt, $macro:ident, $level_kind:ident::$level:ident ) => {
        #[doc = concat!("Constructs a borrowing span at the ", stringify!($level), " level.")]
        ///
        /// # Syntax
        ///
        /// The macro syntax is:
        #[doc = concat!(
            "- `",
            stringify!($macro),
            "!( $logger:expr, $( target: $target:expr, )? ",
            "$message:expr $(; $( $arg ),* $(,)? )? )`."
        )]
        ///
        /// This macro syntax is similar to the `log!` macro.
        /// For the detailed syntax documentation, please see the [`log!`] macro documentation.
        ///
        /// # Examples
        ///
        /// ```rust
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # {
        /// let mut logger = polog::Discard;
        /// # }
        #[doc = concat!(
            "let logger = polog::", ::core::stringify!($macro),
            "!(&mut logger, \"Message on ", stringify!($level), " level\");"
        )]
        /// ```
        ///
        /// ```rust
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # {
        /// let mut logger = polog::Discard;
        /// # }
        /// let string = String::from("foo");
        /// let non_static_str_ref = string.as_str();
        #[doc = concat!("let logger = polog::", ::core::stringify!($macro), "!(")]
        ///     &mut logger, "Message";
        ///     arg1 = "value",
        ///     arg2 = :t non_static_str_ref,
        ///     arg3 = ? (123, true, 'a')
        /// );
        /// ```
        ///
        /// [`log!`]: crate::log!
        #[macro_export]
        macro_rules! $macro {
            /* This comment fixes https://github.com/rust-lang/rustfmt/issues/5044 rustfmt issue */
            ( $d logger:expr $d (, $d ($d tt:tt)* )? ) => {
                $crate::span!(
                    $d logger,
                    $crate::$level_kind::$level
                    $d (, $d ($d tt)* )?
                )
            };
            ( $d logger:expr $d (; $d ($d tt:tt)* )? ) => {
                $crate::span!(
                    $d logger,
                    $crate::$level_kind::$level
                    $d (; $d ($d tt)* )?
                )
            };
        }
    };
}

define_level_macro!($, error_span, SpanLevel::Error);
define_level_macro!($, warn_span, SpanLevel::Warn);
define_level_macro!($, info_span, SpanLevel::Info);
define_level_macro!($, debug_span, SpanLevel::Debug);
define_level_macro!($, trace_span, SpanLevel::Trace);

/// Defines a level-specific logging or span macro in terms of a generic macro.
///
/// - `$d` is the `$` token.
/// - `$macro` is the generated macro name;
/// - `$level_kind::$level` is the level path.
/// - `$fn` is the macro to delegate to.
/// - `$desc` and `$and` are doc text segments.
/// - `$extra_doc` is appended verbatim.
macro_rules! define_level_macro {
    ( $d:tt, $macro:ident, $level_kind:ident::$level:ident ) => {
        #[doc = concat!("Constructs an owning span at the ", stringify!($level), " level.")]
        ///
        /// # Syntax
        ///
        /// The macro syntax is:
        #[doc = concat!(
            "- `",
            stringify!($macro),
            "!( $logger:expr, $( target: $target:expr, )? ",
            "$message:expr $(; $( $arg ),* $(,)? )? )`."
        )]
        ///
        /// This macro syntax is similar to the `log!` macro.
        /// For the detailed syntax documentation, please see the [`log!`] macro documentation.
        ///
        /// # Examples
        ///
        /// ```rust
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # {
        /// let mut logger = polog::Discard;
        /// # }
        #[doc = concat!(
            "let span = polog::", ::core::stringify!($macro),
            "!(&logger, \"Message on ", stringify!($level), " level\");"
        )]
        /// ```
        ///
        /// ```rust
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # {
        /// let mut logger = polog::Discard;
        /// # }
        /// let string = String::from("foo");
        /// let non_static_str_ref = string.as_str();
        #[doc = concat!("let span = polog::", ::core::stringify!($macro), "!(")]
        ///     &logger, "Message";
        ///     arg1 = "value",
        ///     arg2 = :t non_static_str_ref,
        ///     arg3 = ? (123, true, 'a')
        /// );
        /// ```
        ///
        /// [`log!`]: crate::log!
        #[macro_export]
        macro_rules! $macro {
            /* This comment fixes https://github.com/rust-lang/rustfmt/issues/5044 rustfmt issue */
            ( $d logger:expr $d (, $d ($d tt:tt)* )? ) => {
                $crate::owning_span!(
                    $d logger,
                    $crate::$level_kind::$level
                    $d (, $d ($d tt)* )?
                )
            };
            ( $d logger:expr $d (; $d ($d tt:tt)* )? ) => {
                $crate::owning_span!(
                    $d logger,
                    $crate::$level_kind::$level
                    $d (; $d ($d tt)* )?
                )
            };
        }
    };
}

define_level_macro!($, owning_error_span, SpanLevel::Error);
define_level_macro!($, owning_warn_span, SpanLevel::Warn);
define_level_macro!($, owning_info_span, SpanLevel::Info);
define_level_macro!($, owning_debug_span, SpanLevel::Debug);
define_level_macro!($, owning_trace_span, SpanLevel::Trace);

/// Defines a level-specific logging or span macro in terms of a generic macro.
///
/// - `$d` is the `$` token.
/// - `$macro` is the generated macro name;
/// - `$level_kind::$level` is the level path.
/// - `$fn` is the macro to delegate to.
/// - `$desc` and `$and` are doc text segments.
/// - `$extra_doc` is appended verbatim.
macro_rules! define_level_macro {
    ( $d:tt, $macro:ident, $level_kind:ident::$level:ident ) => {
        #[doc = concat!("Constructs a deferred span at the ", stringify!($level), " level.")]
        ///
        /// # Syntax
        ///
        /// The macro syntax is:
        #[doc = concat!(
            "- `",
            stringify!($macro),
            "!( $logger:expr, $( target: $target:expr, )? ",
            "$message:expr $(; $( $arg ),* $(,)? )? )`."
        )]
        ///
        /// This macro syntax is similar to the `log!` macro.
        /// For the detailed syntax documentation, please see the [`log!`] macro documentation.
        ///
        /// # Examples
        ///
        /// ```rust
        /// let mut logger = polog::Discard;
        #[doc = concat!(
            "let span = polog::", ::core::stringify!($macro),
            "!(&logger, \"Message on ", stringify!($level), " level\");"
        )]
        /// ```
        ///
        /// ```rust
        /// let mut logger = polog::Discard;
        /// let string = String::from("foo");
        /// let non_static_str_ref = string.as_str();
        #[doc = concat!("let span = polog::", ::core::stringify!($macro), "!(")]
        ///     &logger, "Message";
        ///     arg1 = "value",
        ///     arg2 = :t non_static_str_ref,
        ///     arg3 = ? (123, true, 'a')
        /// );
        /// ```
        ///
        /// [`log!`]: crate::log!
        #[macro_export]
        macro_rules! $macro {
            /* This comment fixes https://github.com/rust-lang/rustfmt/issues/5044 rustfmt issue */
            ( $d logger:expr $d (, $d ($d tt:tt)* )? ) => {
                $crate::deferred_span!(
                    $d logger,
                    $crate::$level_kind::$level
                    $d (, $d ($d tt)* )?
                )
            };
            ( $d logger:expr $d (; $d ($d tt:tt)* )? ) => {
                $crate::deferred_span!(
                    $d logger,
                    $crate::$level_kind::$level
                    $d (; $d ($d tt)* )?
                )
            };
        }
    };
}

define_level_macro!($, deferred_error_span, SpanLevel::Error);
define_level_macro!($, deferred_warn_span, SpanLevel::Warn);
define_level_macro!($, deferred_info_span, SpanLevel::Info);
define_level_macro!($, deferred_debug_span, SpanLevel::Debug);
define_level_macro!($, deferred_trace_span, SpanLevel::Trace);
