// See files:
// - `crates/polog/src/macros/log.rs` for log macro expansion details.
// - `crates/polog/src/macros/span.rs` for span macro expansion details.
// - `crates/polog/src/macros/arguments.rs` for argument parsing details.
// - `crates/polog/src/macros/descriptor.rs` for descriptor expansion details.

/// Inspects `Option<T>` and logs `None` value if any.
///
/// This will invoke the [`log!`] macro if the provided expression is
/// `Option::None`.
/// If the expression evaluates to `Option::Some(value)`, the macro returns
/// `Option::Some(value)` unchanged without logging.
///
/// # Examples
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let result = polog::some_or_log!(Some::<u32>(123), &mut logger, polog::Level::Error);
/// assert_eq!(result, Some(123));
/// ```
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let result = polog::some_or_log!(None::<u32>, &mut logger, polog::Level::Error);
/// assert_eq!(result, None);
/// ```
///
/// # Panics
///
/// The macro expands to the conditional [`log!`], which calls
/// [`LoggerExt::panic_if_critical`] after emitting the event.
/// Consequently:
/// - If `$level` is [`Level::Fatal`], the macro panics after logging.
/// - If `$level` is [`Level::Bug`] and the backend's
///   [`BaseHandler::should_panic_on_bug`] returns `true`, the macro panics
///   after logging.
///
/// The level-specific variants ([`ok_or_error!`], [`ok_or_warn!`], and so on)
/// fix `$level` to a non-panic level and never panic.
///
/// [`log!`]: crate::log!
/// [`ok_or_error!`]: crate::ok_or_error
/// [`ok_or_warn!`]: crate::ok_or_warn
/// [`Level::Fatal`]: crate::Level::Fatal
/// [`Level::Bug`]: crate::Level::Bug
/// [`BaseHandler::should_panic_on_bug`]: crate::BaseHandler::should_panic_on_bug
/// [`LoggerExt::panic_if_critical`]: crate::LoggerExt::panic_if_critical
#[macro_export]
macro_rules! some_or_log {
    (
        $expr:expr, $logger:expr, $level:expr
        $(, target: $target:expr $(,)? )?
    ) => {
        match $expr {
            ::core::option::Option::Some(ok) => Some(ok),
            ::core::option::Option::None => {
                $crate::log!(
                    $logger, $level, $( target: $target, )?
                    ::core::concat!(
                        "called `some_or_log!(",
                        ::core::stringify!($expr),
                        ")` on an `None` value",
                    )
                );
                None
            },
        }
    };
    (
        $expr:expr, $logger:expr, $level:expr,
        $message:expr $(; $($args:tt)* )?
    ) => {
        match $expr {
            ::core::option::Option::Some(ok) => Some(ok),
            ::core::option::Option::None => {
                $crate::log!( $logger, $level, $message $(; $($args)* )? );
                None
            },
        }
    };
    (
        $expr:expr, $logger:expr, $level:expr, target: $target:expr,
        $message:expr $(; $($args:tt)* )?
    ) => {
        match $expr {
            ::core::option::Option::Some(ok) => Some(ok),
            ::core::option::Option::None => {
                $crate::log!( $logger, $level, target: $target, $message $(; $($args)* )? );
                None
            },
        }
    };
}

/// Consumes `Option<T>` and logs `Some` value if any.
///
/// This will invoke the [`log!`] macro if the provided expression is
/// `Option::Some(value)` consuming it.
///
/// # Examples
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// polog::none_or_log!(None::<u32>, &mut logger, polog::Level::Error);
/// ```
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// polog::none_or_log!(Some::<u32>(123), &mut logger, polog::Level::Error);
/// ```
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let owned = String::from("abc");
/// let temp = owned.as_str();
/// polog::none_or_log!(:t Some::<&str>(temp), &mut logger, polog::Level::Error);
/// ```
///
/// # Payload binding
///
/// The auto-message form automatically includes the `Some` payload as a
/// structured argument.
/// The custom-message form does not: the payload is consumed by the match, and
/// the argument list is under caller control.
/// To reference the payload in `$args`, introduce a binding with a `for`
/// clause between the message and the argument list:
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # do_smth(Some("abc"), "some path", &mut logger);
///
/// fn do_smth(smth: Option<&str>, path: &str, logger: &mut impl polog::Logger) {
///     let _value = polog::none_or_log!(
///         smth, logger, polog::Level::Error,
///         "Unexpected Some in something";
///         for Some(some): :t ? some, :t path
///     );
/// }
/// ```
///
/// The binding lives only in the argument list: it is not visible in the
/// surrounding code and does not shadow existing bindings outside the macro.
///
/// # Panics
///
/// The macro expands to the conditional [`log!`], which calls
/// [`LoggerExt::panic_if_critical`] after emitting the event.
/// Consequently:
/// - If `$level` is [`Level::Fatal`], the macro panics after logging.
/// - If `$level` is [`Level::Bug`] and the backend's
///   [`BaseHandler::should_panic_on_bug`] returns `true`, the macro panics
///   after logging.
///
/// The level-specific variants ([`ok_or_error!`], [`ok_or_warn!`], and so on)
/// fix `$level` to a non-panic level and never panic.
///
/// [`log!`]: crate::log!
/// [`ok_or_error!`]: crate::ok_or_error
/// [`ok_or_warn!`]: crate::ok_or_warn
/// [`Level::Fatal`]: crate::Level::Fatal
/// [`Level::Bug`]: crate::Level::Bug
/// [`BaseHandler::should_panic_on_bug`]: crate::BaseHandler::should_panic_on_bug
/// [`LoggerExt::panic_if_critical`]: crate::LoggerExt::panic_if_critical
#[macro_export]
macro_rules! none_or_log {
    (
        $( :$expr_send:tt )? $expr:expr, $logger:expr, $level:expr
        $(, target: $target:expr $(,)? )?
    ) => {
        match $expr {
            ::core::option::Option::None => {},
            ::core::option::Option::Some(value) => {
                $crate::log!(
                    $logger, $level, $( target: $target, )?
                    ::core::concat!(
                        "called `none_or_log!(",
                        ::core::stringify!($expr),
                        ")` on an `Some` value",
                    );
                    $( :$expr_send )? ? value
                );
            },
        }
    };
    (
        $expr:expr, $logger:expr, $level:expr,
        $message:expr; for Some($some:pat): $($args:tt)*
    ) => {
        match $expr {
            ::core::option::Option::None => {},
            ::core::option::Option::Some($some) => {
                $crate::log!( $logger, $level, $message; $($args)* );
            },
        }
    };
    (
        $expr:expr, $logger:expr, $level:expr,
        $message:expr $(; $($args:tt)* )?
    ) => {
        match $expr {
            ::core::option::Option::None => {},
            ::core::option::Option::Some(_value) => {
                $crate::log!( $logger, $level, $message $(; $($args)* )? );
            },
        }
    };
    (
        $expr:expr, $logger:expr, $level:expr, target: $target:expr,
        $message:expr; for Some($some:pat): $($args:tt)*
    ) => {
        match $expr {
            ::core::option::Option::None => {},
            ::core::option::Option::Some(_value) => {
                $crate::log!( $logger, $level, target: $target, $message; $($args)* );
            },
        }
    };
    (
        $expr:expr, $logger:expr, $level:expr, target: $target:expr,
        $message:expr $(; $($args:tt)* )?
    ) => {
        match $expr {
            ::core::option::Option::None => {},
            ::core::option::Option::Some(_value) => {
                $crate::log!( $logger, $level, target: $target, $message $(; $($args)* )? );
            },
        }
    };
}

/// Converts from `Result<T, E>` to `Option<T>` and logs `Err` value if any.
///
/// This will invoke the [`log!`] macro if the provided expression is
/// `Result::Err(err)`.
/// If the expression evaluates to `Result::Ok(ok)`, the macro returns
/// `Option::Some(ok)` unchanged without logging.
///
/// # Examples
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let result = polog::ok_or_log!(Ok::<u32, ()>(123), &mut logger, polog::Level::Error);
/// assert_eq!(result, Some(123));
/// ```
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let result = polog::ok_or_log!(Err::<(), u32>(123), &mut logger, polog::Level::Error);
/// assert_eq!(result, None);
/// ```
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let owned = String::from("abc");
/// let temp = owned.as_str();
/// let result = polog::ok_or_log!(:t Err::<(), &str>(temp), &mut logger, polog::Level::Error);
/// assert_eq!(result, None);
/// ```
///
/// # Payload binding
///
/// The auto-message form automatically includes the `Err` payload as a
/// structured argument.
/// The custom-message form does not: the payload is consumed by the match, and
/// the argument list is under caller control.
/// To reference the payload in `$args`, introduce a binding with a `for`
/// clause between the message and the argument list:
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # do_smth(Err("abc"), "some path", &mut logger);
///
/// fn do_smth(smth: Result<u32, &str>, path: &str, logger: &mut impl polog::Logger) {
///     let _value = polog::ok_or_log!(
///         smth, logger, polog::Level::Error,
///         "Unexpected Err in something";
///         for Err(err): :t ? err, :t path
///     );
/// }
/// ```
///
/// The binding lives only in the argument list: it is not visible in the
/// surrounding code and does not shadow existing bindings outside the macro.
///
/// # Panics
///
/// The macro expands to the conditional [`log!`], which calls
/// [`LoggerExt::panic_if_critical`] after emitting the event.
/// Consequently:
/// - If `$level` is [`Level::Fatal`], the macro panics after logging.
/// - If `$level` is [`Level::Bug`] and the backend's
///   [`BaseHandler::should_panic_on_bug`] returns `true`, the macro panics
///   after logging.
///
/// The level-specific variants ([`ok_or_error!`], [`ok_or_warn!`], and so on)
/// fix `$level` to a non-panic level and never panic.
///
/// [`log!`]: crate::log!
/// [`ok_or_error!`]: crate::ok_or_error
/// [`ok_or_warn!`]: crate::ok_or_warn
/// [`Level::Fatal`]: crate::Level::Fatal
/// [`Level::Bug`]: crate::Level::Bug
/// [`BaseHandler::should_panic_on_bug`]: crate::BaseHandler::should_panic_on_bug
/// [`LoggerExt::panic_if_critical`]: crate::LoggerExt::panic_if_critical
#[macro_export]
macro_rules! ok_or_log {
    (
        $( :$expr_send:tt )? $expr:expr, $logger:expr, $level:expr
        $(, target: $target:expr $(,)? )?
    ) => {
        match $expr {
            ::core::result::Result::Ok(ok) => Some(ok),
            ::core::result::Result::Err(err) => {
                $crate::log!(
                    $logger, $level, $( target: $target, )?
                    ::core::concat!(
                        "called `ok_or_log!(",
                        ::core::stringify!($expr),
                        ")` on an `Err` value",
                    );
                    $( :$expr_send )? ? err
                );
                None
            },
        }
    };
    (
        $expr:expr, $logger:expr, $level:expr,
        $message:expr; for Err($err:pat): $($args:tt)*
    ) => {
        match $expr {
            ::core::result::Result::Ok(ok) => Some(ok),
            ::core::result::Result::Err($err) => {
                $crate::log!( $logger, $level, $message; $($args)* );
                None
            },
        }
    };
    (
        $expr:expr, $logger:expr, $level:expr,
        $message:expr $(; $($args:tt)* )?
    ) => {
        match $expr {
            ::core::result::Result::Ok(ok) => Some(ok),
            ::core::result::Result::Err(_err) => {
                $crate::log!( $logger, $level, $message $(; $($args)* )? );
                None
            },
        }
    };
    (
        $expr:expr, $logger:expr, $level:expr, target: $target:expr,
        $message:expr; for Err($err:pat): $($args:tt)*
    ) => {
        match $expr {
            ::core::result::Result::Ok(ok) => Some(ok),
            ::core::result::Result::Err($err) => {
                $crate::log!( $logger, $level, target: $target, $message; $($args)* );
                None
            },
        }
    };
    (
        $expr:expr, $logger:expr, $level:expr, target: $target:expr,
        $message:expr $(; $($args:tt)* )?
    ) => {
        match $expr {
            ::core::result::Result::Ok(ok) => Some(ok),
            ::core::result::Result::Err(_err) => {
                $crate::log!( $logger, $level, target: $target, $message $(; $($args)* )? );
                None
            },
        }
    };
}

/// Converts from `Result<T, E>` to `Option<E>` and logs `Ok` value if any.
///
/// This will invoke the [`log!`] macro if the provided expression is
/// `Result::Ok(ok)`.
/// If the expression evaluates to `Result::Err(err)`, the macro returns
/// `Option::Some(err)` unchanged without logging.
///
/// # Examples
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let result = polog::err_or_log!(Err::<(), u32>(123), &mut logger, polog::Level::Error);
/// assert_eq!(result, Some(123));
/// ```
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let result = polog::err_or_log!(Ok::<u32, ()>(123), &mut logger, polog::Level::Error);
/// assert_eq!(result, None);
/// ```
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let owned = String::from("abc");
/// let temp = owned.as_str();
/// let result = polog::err_or_log!(:t Ok::<&str, ()>(temp), &mut logger, polog::Level::Error);
/// assert_eq!(result, None);
/// ```
///
/// # Payload binding
///
/// The auto-message form automatically includes the `Ok` payload as a
/// structured argument.
/// The custom-message form does not: the payload is consumed by the match, and
/// the argument list is under caller control.
/// To reference the payload in `$args`, introduce a binding with a `for`
/// clause between the message and the argument list:
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # do_smth(Ok("abc"), "some path", &mut logger);
///
/// fn do_smth(smth: Result<&str, u32>, path: &str, logger: &mut impl polog::Logger) {
///     let _value = polog::err_or_log!(
///         smth, logger, polog::Level::Error,
///         "Unexpected Ok in something";
///         for Ok(ok): :t ? ok, :t path
///     );
/// }
/// ```
///
/// The binding lives only in the argument list: it is not visible in the
/// surrounding code and does not shadow existing bindings outside the macro.
///
/// # Panics
///
/// The macro expands to the conditional [`log!`], which calls
/// [`LoggerExt::panic_if_critical`] after emitting the event.
/// Consequently:
/// - If `$level` is [`Level::Fatal`], the macro panics after logging.
/// - If `$level` is [`Level::Bug`] and the backend's
///   [`BaseHandler::should_panic_on_bug`] returns `true`, the macro panics
///   after logging.
///
/// The level-specific variants ([`ok_or_error!`], [`ok_or_warn!`], and so on)
/// fix `$level` to a non-panic level and never panic.
///
/// [`log!`]: crate::log!
/// [`ok_or_error!`]: crate::ok_or_error
/// [`ok_or_warn!`]: crate::ok_or_warn
/// [`Level::Fatal`]: crate::Level::Fatal
/// [`Level::Bug`]: crate::Level::Bug
/// [`BaseHandler::should_panic_on_bug`]: crate::BaseHandler::should_panic_on_bug
/// [`LoggerExt::panic_if_critical`]: crate::LoggerExt::panic_if_critical
#[macro_export]
macro_rules! err_or_log {
    (
        $( :$expr_send:tt )? $expr:expr, $logger:expr, $level:expr
        $(, target: $target:expr $(,)? )?
    ) => {
        match $expr {
            ::core::result::Result::Err(err) => Some(err),
            ::core::result::Result::Ok(ok) => {
                $crate::log!(
                    $logger, $level, $( target: $target, )?
                    ::core::concat!(
                        "called `err_or_log!(",
                        ::core::stringify!($expr),
                        ")` on an `Ok` value",
                    );
                    $( :$expr_send )? ? ok
                );
                None
            },
        }
    };
    (
        $expr:expr, $logger:expr, $level:expr,
        $message:expr; for Ok($ok:pat): $($args:tt)*
    ) => {
        match $expr {
            ::core::result::Result::Err(err) => Some(err),
            ::core::result::Result::Ok($ok) => {
                $crate::log!( $logger, $level, $message; $($args)* );
                None
            },
        }
    };
    (
        $expr:expr, $logger:expr, $level:expr,
        $message:expr $(; $($args:tt)* )?
    ) => {
        match $expr {
            ::core::result::Result::Err(err) => Some(err),
            ::core::result::Result::Ok(_ok) => {
                $crate::log!( $logger, $level, $message $(; $($args)* )? );
                None
            },
        }
    };
    (
        $expr:expr, $logger:expr, $level:expr, target: $target:expr,
        $message:expr; for Ok($ok:pat): $($args:tt)*
    ) => {
        match $expr {
            ::core::result::Result::Err(err) => Some(err),
            ::core::result::Result::Ok($ok) => {
                $crate::log!( $logger, $level, target: $target, $message; $($args)* );
                None
            },
        }
    };
    (
        $expr:expr, $logger:expr, $level:expr, target: $target:expr,
        $message:expr $(; $($args:tt)* )?
    ) => {
        match $expr {
            ::core::result::Result::Err(err) => Some(err),
            ::core::result::Result::Ok(_ok) => {
                $crate::log!( $logger, $level, target: $target, $message $(; $($args)* )? );
                None
            },
        }
    };
}

/// Defines a level-specific inspecting log macro in terms of a generic macro.
///
/// - `$d` is the `$` token.
/// - `$macro` is the generated macro name;
/// - `$level` is the level path.
macro_rules! define_level_macro {
    (
        $d:tt, $macro:ident, $level:ident, $level_text:literal
    ) => {
        #[doc = concat!(
            "Inspects `Option<T>` and logs `None` value if any at ",
            $level_text,
            " level."
        )]
        ///
        /// This will invoke the [`log!`] macro if the provided expression is
        /// `Option::None`.
        /// If the expression evaluates to `Option::Some(value)`, the macro returns
        /// `Option::Some(value)` unchanged without logging.
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
            "let result = polog::", ::core::stringify!($macro),
            "!(Some::<u32>(123), &mut logger);"
        )]
        /// assert_eq!(result, Some(123));
        /// ```
        ///
        /// ```rust
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # {
        /// let mut logger = polog::Discard;
        /// # }
        #[doc = concat!(
            "let result = polog::", ::core::stringify!($macro),
            "!(None::<u32>, &mut logger);"
        )]
        /// assert_eq!(result, None);
        /// ```
        ///
        /// [`log!`]: crate::log!
        #[macro_export]
        macro_rules! $macro {
            /* This comment fixes https://github.com/rust-lang/rustfmt/issues/5044 rustfmt issue */
            ( $d expr:expr, $d logger:expr $d (, $d ($d tt:tt)* )? ) => {
                $crate::some_or_log!(
                    $d expr,
                    $d logger,
                    $crate::Level::$level
                    $d (, $d ($d tt)* )?
                )
            };
            ( $d expr:expr, $d logger:expr $d (; $d ($d tt:tt)* )? ) => {
                $crate::some_or_log!(
                    $d expr,
                    $d logger,
                    $crate::Level::$level
                    $d (; $d ($d tt)* )?
                )
            };
        }
    };
}

define_level_macro!($, some_or_error, Error, "ERROR");
define_level_macro!($, some_or_warn, Warn, "WARN");
define_level_macro!($, some_or_info, Info, "INFO");
define_level_macro!($, some_or_debug, Debug, "DEBUG");
define_level_macro!($, some_or_trace, Trace, "TRACE");

/// Defines a level-specific inspecting log macro in terms of a generic macro.
///
/// - `$d` is the `$` token.
/// - `$macro` is the generated macro name;
/// - `$level` is the level path.
macro_rules! define_level_macro {
    (
        $d:tt, $macro:ident, $level:ident, $level_text:literal
    ) => {
        #[doc = concat!(
            "Consumes `Option<T>` and logs `Some` value if any at ",
            $level_text,
            " level."
        )]
        ///
        /// This will invoke the [`log!`] macro if the provided expression is
        /// `Option::Some(value)` consuming it.
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
            "polog::", ::core::stringify!($macro),
            "!(None::<u32>, &mut logger);"
        )]
        /// ```
        ///
        /// ```rust
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # {
        /// let mut logger = polog::Discard;
        /// # }
        #[doc = concat!(
            "polog::", ::core::stringify!($macro),
            "!(Some::<u32>(123), &mut logger);"
        )]
        /// ```
        ///
        /// ```rust
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # {
        /// let mut logger = polog::Discard;
        /// # }
        /// let owned = String::from("abc");
        /// let temp = owned.as_str();
        #[doc = concat!(
            "polog::", ::core::stringify!($macro),
            "!(:t Some::<&str>(temp), &mut logger);"
        )]
        /// ```
        ///
        /// # Payload binding
        ///
        /// The auto-message form automatically includes the `Some` payload as a
        /// structured argument.
        /// The custom-message form does not: the payload is consumed by the match, and
        /// the argument list is under caller control.
        /// To reference the payload in `$args`, introduce a binding with a `for`
        /// clause between the message and the argument list:
        ///
        /// ```rust
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # do_smth(Some("abc"), "some path", &mut logger);
        ///
        /// fn do_smth(smth: Option<&str>, path: &str, logger: &mut impl polog::Logger) {
        #[doc = concat!(
            "    let _value = polog::", ::core::stringify!($macro), "!("
        )]
        ///         smth, logger, "Unexpected Some in something";
        ///         for Some(some): :t ? some, :t path
        ///     );
        /// }
        /// ```
        ///
        /// The binding lives only in the argument list: it is not visible in the
        /// surrounding code and does not shadow existing bindings outside the macro.
        ///
        /// [`log!`]: crate::log!
        #[macro_export]
        macro_rules! $macro {
            ( $d ( :$d expr_send:tt )? $d expr:expr, $d logger:expr $d (, $d ($d tt:tt)* )? ) => {
                $crate::none_or_log!(
                    $d ( :$d expr_send )? $d expr,
                    $d logger,
                    $crate::Level::$level
                    $d (, $d ($d tt)* )?
                )
            };
            ( $d ( :$d expr_send:tt )? $d expr:expr, $d logger:expr $d (; $d ($d tt:tt)* )? ) => {
                $crate::none_or_log!(
                    $d ( :$d expr_send )? $d expr,
                    $d logger,
                    $crate::Level::$level
                    $d (; $d ($d tt)* )?
                )
            };
        }
    };
}

define_level_macro!($, none_or_error, Error, "ERROR");
define_level_macro!($, none_or_warn, Warn, "WARN");
define_level_macro!($, none_or_info, Info, "INFO");
define_level_macro!($, none_or_debug, Debug, "DEBUG");
define_level_macro!($, none_or_trace, Trace, "TRACE");

/// Defines a level-specific inspecting log macro in terms of a generic macro.
///
/// - `$d` is the `$` token.
/// - `$macro` is the generated macro name;
/// - `$level` is the level path.
macro_rules! define_level_macro {
    (
        $d:tt, $macro:ident, $level:ident, $level_text:literal
    ) => {
        #[doc = concat!(
            "Converts from `Result<T, E>` to `Option<E>` and logs `Ok` value if \
             any at ",
            $level_text,
            " level."
        )]
        ///
        /// This will invoke the [`log!`] macro if the provided expression is
        /// `Result::Err(err)`.
        /// If the expression evaluates to `Result::Ok(ok)`, the macro returns
        /// `Option::Some(ok)` unchanged without logging.
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
            "let result = polog::", ::core::stringify!($macro),
            "!(Ok::<u32, ()>(123), &mut logger);"
        )]
        /// assert_eq!(result, Some(123));
        /// ```
        ///
        /// ```rust
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # {
        /// let mut logger = polog::Discard;
        /// # }
        #[doc = concat!(
            "let result = polog::", ::core::stringify!($macro),
            "!(Err::<(), u32>(123), &mut logger);"
        )]
        /// assert_eq!(result, None);
        /// ```
        ///
        /// ```rust
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # {
        /// let mut logger = polog::Discard;
        /// # }
        /// let owned = String::from("abc");
        /// let temp = owned.as_str();
        #[doc = concat!(
            "let result = polog::", ::core::stringify!($macro),
            "!(:t Err::<(), &str>(temp), &mut logger);"
        )]
        /// assert_eq!(result, None);
        /// ```
        ///
        /// # Payload binding
        ///
        /// The auto-message form automatically includes the `Err` payload as a
        /// structured argument.
        /// The custom-message form does not: the payload is consumed by the match, and
        /// the argument list is under caller control.
        /// To reference the payload in `$args`, introduce a binding with a `for`
        /// clause between the message and the argument list:
        ///
        /// ```rust
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # do_smth(Err("abc"), "some path", &mut logger);
        ///
        /// fn do_smth(smth: Result<u32, &str>, path: &str, logger: &mut impl polog::Logger) {
        #[doc = concat!(
            "    let _value = polog::", ::core::stringify!($macro), "!("
        )]
        ///         smth, logger, "Unexpected Err in something";
        ///         for Err(err): :t ? err, :t path
        ///     );
        /// }
        /// ```
        ///
        /// The binding lives only in the argument list: it is not visible in the
        /// surrounding code and does not shadow existing bindings outside the macro.
        ///
        /// [`log!`]: crate::log!
        #[macro_export]
        macro_rules! $macro {
            ( $d ( :$d expr_send:tt )? $d expr:expr, $d logger:expr $d (, $d ($d tt:tt)* )? ) => {
                $crate::ok_or_log!(
                    $d ( :$d expr_send )? $d expr,
                    $d logger,
                    $crate::Level::$level
                    $d (, $d ($d tt)* )?
                )
            };
            ( $d ( :$d expr_send:tt )? $d expr:expr, $d logger:expr $d (; $d ($d tt:tt)* )? ) => {
                $crate::ok_or_log!(
                    $d ( :$d expr_send )? $d expr,
                    $d logger,
                    $crate::Level::$level
                    $d (; $d ($d tt)* )?
                )
            };
        }
    };
}

define_level_macro!($, ok_or_error, Error, "ERROR");
define_level_macro!($, ok_or_warn, Warn, "WARN");
define_level_macro!($, ok_or_info, Info, "INFO");
define_level_macro!($, ok_or_debug, Debug, "DEBUG");
define_level_macro!($, ok_or_trace, Trace, "TRACE");

/// Defines a level-specific inspecting log macro in terms of a generic macro.
///
/// - `$d` is the `$` token.
/// - `$macro` is the generated macro name;
/// - `$level` is the level path.
macro_rules! define_level_macro {
    (
        $d:tt, $macro:ident, $level:ident, $level_text:literal
    ) => {
        #[doc = concat!(
            "Converts from `Result<T, E>` to `Option<E>` and logs `Ok` value if \
             any at ",
            $level_text,
            " level."
        )]
        ///
        /// This will invoke the [`log!`] macro if the provided expression is
        /// `Result::Ok(ok)`.
        /// If the expression evaluates to `Result::Err(err)`, the macro returns
        /// `Option::Some(err)` unchanged without logging.
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
            "let result = polog::", ::core::stringify!($macro),
            "!(Err::<(), u32>(123), &mut logger);"
        )]
        /// assert_eq!(result, Some(123));
        /// ```
        ///
        /// ```rust
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # {
        /// let mut logger = polog::Discard;
        /// # }
        #[doc = concat!(
            "let result = polog::", ::core::stringify!($macro),
            "!(Ok::<u32, ()>(123), &mut logger);"
        )]
        /// assert_eq!(result, None);
        /// ```
        ///
        /// ```rust
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # {
        /// let mut logger = polog::Discard;
        /// # }
        /// let owned = String::from("abc");
        /// let temp = owned.as_str();
        #[doc = concat!(
            "let result = polog::", ::core::stringify!($macro),
            "!(:t Ok::<&str, ()>(temp), &mut logger);"
        )]
        /// assert_eq!(result, None);
        /// ```
        ///
        /// # Payload binding
        ///
        /// The auto-message form automatically includes the `Ok` payload as a
        /// structured argument.
        /// The custom-message form does not: the payload is consumed by the match, and
        /// the argument list is under caller control.
        /// To reference the payload in `$args`, introduce a binding with a `for`
        /// clause between the message and the argument list:
        ///
        /// ```rust
        /// # use polog_test_helpers::accept_all::AcceptAll;
        /// # let mut logger = AcceptAll; // Enables args construction.
        /// # do_smth(Ok("abc"), "some path", &mut logger);
        ///
        /// fn do_smth(smth: Result<&str, u32>, path: &str, logger: &mut impl polog::Logger) {
        #[doc = concat!(
            "    let _value = polog::", ::core::stringify!($macro), "!("
        )]
        ///         smth, logger, "Unexpected Ok in something";
        ///         for Ok(ok): :t ? ok, :t path
        ///     );
        /// }
        /// ```
        ///
        /// The binding lives only in the argument list: it is not visible in the
        /// surrounding code and does not shadow existing bindings outside the macro.
        ///
        /// [`log!`]: crate::log!
        #[macro_export]
        macro_rules! $macro {
            ( $d ( :$d expr_send:tt )? $d expr:expr, $d logger:expr $d (, $d ($d tt:tt)* )? ) => {
                $crate::err_or_log!(
                    $d ( :$d expr_send )? $d expr,
                    $d logger,
                    $crate::Level::$level
                    $d (, $d ($d tt)* )?
                )
            };
            ( $d ( :$d expr_send:tt )? $d expr:expr, $d logger:expr $d (; $d ($d tt:tt)* )? ) => {
                $crate::err_or_log!(
                    $d ( :$d expr_send )? $d expr,
                    $d logger,
                    $crate::Level::$level
                    $d (; $d ($d tt)* )?
                )
            };
        }
    };
}

define_level_macro!($, err_or_error, Error, "ERROR");
define_level_macro!($, err_or_warn, Warn, "WARN");
define_level_macro!($, err_or_info, Info, "INFO");
define_level_macro!($, err_or_debug, Debug, "DEBUG");
define_level_macro!($, err_or_trace, Trace, "TRACE");
