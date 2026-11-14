/// Returns the contained `Option::Some` value, consuming the self value.
///
/// This will invoke the [`fatal!`] macro if the provided expression is
/// `Option::None`.
///
/// # Panics
///
/// Panics if the value is an `None`.
///
/// # Examples
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let result = polog::unwrap_some_or_fatal!(Some(123), &mut logger);
/// assert_eq!(result, 123);
/// ```
///
/// ```rust,should_panic
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let _ = polog::unwrap_some_or_fatal!(None, &mut logger);
/// ```
///
/// [`fatal!`]: crate::fatal!
#[macro_export]
macro_rules! unwrap_some_or_fatal {
    ( $expr:expr, $logger:expr $(, target: $target:expr $(,)? )? ) => {
        match $expr {
            ::core::option::Option::Some(ok) => ok,
            ::core::option::Option::None => {
                ::core::hint::cold_path();
                $crate::fatal!(
                    $logger, $( target: $target, )?
                    ::core::concat!(
                        "called `unwrap_some_or_fatal!(",
                        ::core::stringify!($expr),
                        ")` on an `None` value",
                    )
                );
            },
        }
    };
    ( $expr:expr, $logger:expr, $message:expr $(; $($args:tt)* )? ) => {
        match $expr {
            ::core::option::Option::Some(ok) => ok,
            ::core::option::Option::None => {
                ::core::hint::cold_path();
                $crate::fatal!( $logger, $message $(; $($args)* )? );
            },
        }
    };
    (
        $expr:expr, $logger:expr, target: $target:expr,
        $message:expr $(; $($args:tt)* )?
    ) => {
        match $expr {
            ::core::option::Option::Some(ok) => ok,
            ::core::option::Option::None => {
                ::core::hint::cold_path();
                $crate::fatal!( $logger, target: $target, $message $(; $($args)* )? );
            },
        }
    };
}

/// Consuming the self value and panics if any.
///
/// This will invoke the [`fatal!`] macro if the provided expression is
/// `Option::Some`.
///
/// # Panics
///
/// Panics if the value is an `Some`.
///
/// # Examples
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// polog::unwrap_none_or_fatal!(None::<()>, &mut logger);
/// ```
///
/// ```rust,should_panic
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// polog::unwrap_none_or_fatal!(Some(123), &mut logger);
/// ```
///
/// ```rust,should_panic
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let owned = String::from("abc");
/// let temp = owned.as_str();
/// polog::unwrap_none_or_fatal!(:t Some(temp), &mut logger);
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
/// ```rust,should_panic
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll;
/// do_smth(Some("abc"), "some path", &mut logger);
///
/// fn do_smth(smth: Option<&str>, path: &str, logger: &mut impl polog::Logger) {
///     let _value = polog::unwrap_none_or_fatal!(
///         smth, logger, "Unexpected Some in something";
///         for Some(some): :t ? some, :t path
///     );
/// }
/// ```
///
/// The binding lives only in the argument list: it is not visible in the
/// surrounding code and does not shadow existing bindings outside the macro.
///
/// [`fatal!`]: crate::fatal!
#[macro_export]
macro_rules! unwrap_none_or_fatal {
    ( $( :$expr_send:tt )? $expr:expr, $logger:expr $(, target: $target:expr $(,)? )? ) => {
        match $expr {
            ::core::option::Option::None => {},
            ::core::option::Option::Some(ok) => {
                ::core::hint::cold_path();
                $crate::fatal!(
                    $logger, $( target: $target, )?
                    ::core::concat!(
                        "called `unwrap_none_or_fatal!(",
                        ::core::stringify!($expr),
                        ")` on an `Some` value",
                    );
                    $( :$expr_send )? ? ok
                );
            },
        }
    };
    ( $expr:expr, $logger:expr, $message:expr; for Some($some:pat): $($args:tt)* ) => {
        match $expr {
            ::core::option::Option::None => {},
            ::core::option::Option::Some($some) => {
                ::core::hint::cold_path();
                $crate::fatal!( $logger, $message; $($args)* );
            },
        }
    };
    ( $expr:expr, $logger:expr, $message:expr $(; $($args:tt)* )? ) => {
        match $expr {
            ::core::option::Option::None => {},
            ::core::option::Option::Some(_some) => {
                ::core::hint::cold_path();
                $crate::fatal!( $logger, $message $(; $($args)* )? );
            },
        }
    };
    (
        $expr:expr, $logger:expr, target: $target:expr,
        $message:expr; for Some($some:pat): $($args:tt)*
    ) => {
        match $expr {
            ::core::option::Option::None => {},
            ::core::option::Option::Some($some) => {
                ::core::hint::cold_path();
                $crate::fatal!( $logger, target: $target, $message ; $($args)* );
            },
        }
    };
    (
        $expr:expr, $logger:expr, target: $target:expr,
        $message:expr $(; $($args:tt)* )?
    ) => {
        match $expr {
            ::core::option::Option::None => {},
            ::core::option::Option::Some(_some) => {
                ::core::hint::cold_path();
                $crate::fatal!( $logger, target: $target, $message $(; $($args)* )? );
            },
        }
    };
}

/// Returns the contained `Result::Ok` value, consuming the self value.
///
/// This will invoke the [`fatal!`] macro if the provided expression is
/// `Result::Err`.
///
/// # Panics
///
/// Panics if the value is an `Err`.
///
/// # Examples
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let result = polog::unwrap_ok_or_fatal!(Ok::<u32, ()>(123), &mut logger);
/// assert_eq!(result, 123);
/// ```
///
/// ```rust,should_panic
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let _ = polog::unwrap_ok_or_fatal!(Err::<(), u32>(123), &mut logger);
/// ```
///
/// ```rust,should_panic
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let owned = String::from("abc");
/// let temp = owned.as_str();
/// let _ = polog::unwrap_ok_or_fatal!(:t Err::<(), &str>(temp), &mut logger);
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
/// ```rust,should_panic
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll;
/// do_smth(Err("abc"), "some path", &mut logger);
///
/// fn do_smth(smth: Result<u32, &str>, path: &str, logger: &mut impl polog::Logger) {
///     let _value = polog::unwrap_ok_or_fatal!(
///         smth, logger, "Unexpected Err in something";
///         for Err(err): :t ? err, :t path
///     );
/// }
/// ```
///
/// The binding lives only in the argument list: it is not visible in the
/// surrounding code and does not shadow existing bindings outside the macro.
///
/// [`fatal!`]: crate::fatal!
#[macro_export]
macro_rules! unwrap_ok_or_fatal {
    ( $( :$expr_send:tt )? $expr:expr, $logger:expr $(, target: $target:expr $(,)? )? ) => {
        match $expr {
            ::core::result::Result::Ok(ok) => ok,
            ::core::result::Result::Err(err) => {
                ::core::hint::cold_path();
                $crate::fatal!(
                    $logger, $( target: $target, )?
                    ::core::concat!(
                        "called `unwrap_ok_or_fatal!(",
                        ::core::stringify!($expr),
                        ")` on an `Err` value",
                    );
                    $( :$expr_send )? ? err
                );
            },
        }
    };
    ( $expr:expr, $logger:expr, $message:expr; for Err($err:pat): $($args:tt)* ) => {
        match $expr {
            ::core::result::Result::Ok(ok) => ok,
            ::core::result::Result::Err($err) => {
                ::core::hint::cold_path();
                $crate::fatal!( $logger, $message; $($args)* );
            },
        }
    };
    ( $expr:expr, $logger:expr, $message:expr $(; $($args:tt)* )? ) => {
        match $expr {
            ::core::result::Result::Ok(ok) => ok,
            ::core::result::Result::Err(_err) => {
                ::core::hint::cold_path();
                $crate::fatal!( $logger, $message $(; $($args)* )? );
            },
        }
    };
    (
        $expr:expr, $logger:expr, target: $target:expr,
        $message:expr; for Err($err:pat): $($args:tt)*
    ) => {
        match $expr {
            ::core::result::Result::Ok(ok) => ok,
            ::core::result::Result::Err($err) => {
                ::core::hint::cold_path();
                $crate::fatal!( $logger, target: $target, $message; $($args)* );
            },
        }
    };
    (
        $expr:expr, $logger:expr,
        target: $target:expr, $message:expr $(; $($args:tt)* )?
    ) => {
        match $expr {
            ::core::result::Result::Ok(ok) => ok,
            ::core::result::Result::Err(_err) => {
                ::core::hint::cold_path();
                $crate::fatal!( $logger, target: $target, $message $(; $($args)* )? );
            },
        }
    };
}

/// Returns the contained `Result::Err` value, consuming the self value.
///
/// This will invoke the [`fatal!`] macro if the provided expression is
/// `Result::Ok`.
///
/// # Panics
///
/// Panics if the value is an `Ok`.
///
/// # Examples
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let result = polog::unwrap_err_or_fatal!(Err::<(), u32>(123), &mut logger);
/// assert_eq!(result, 123);
/// ```
///
/// ```rust,should_panic
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let _ = polog::unwrap_err_or_fatal!(Ok::<u32, ()>(123), &mut logger);
/// ```
///
/// ```rust,should_panic
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let owned = String::from("abc");
/// let temp = owned.as_str();
/// let _ = polog::unwrap_err_or_fatal!(:t Ok::<&str, ()>(temp), &mut logger);
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
/// ```rust,should_panic
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll;
/// do_smth(Ok("abc"), "some path", &mut logger);
///
/// fn do_smth(smth: Result<&str, u32>, path: &str, logger: &mut impl polog::Logger) {
///     let _value = polog::unwrap_err_or_fatal!(
///         smth, logger, "Unexpected Ok in something";
///         for Ok(ok): :t ? ok, :t path
///     );
/// }
/// ```
///
/// The binding lives only in the argument list: it is not visible in the
/// surrounding code and does not shadow existing bindings outside the macro.
///
/// [`fatal!`]: crate::fatal!
#[macro_export]
macro_rules! unwrap_err_or_fatal {
    ( $( :$expr_send:tt )? $expr:expr, $logger:expr $(, target: $target:expr $(,)? )? ) => {
        match $expr {
            ::core::result::Result::Err(err) => err,
            ::core::result::Result::Ok(ok) => {
                ::core::hint::cold_path();
                $crate::fatal!(
                    $logger, $( target: $target, )?
                    ::core::concat!(
                        "called `unwrap_err_or_fatal!(",
                        ::core::stringify!($expr),
                        ")` on an `Ok` value",
                    );
                    $( :$expr_send )? ? ok
                );
            },
        }
    };
    ( $expr:expr, $logger:expr, $message:expr; for Ok($ok:pat): $($args:tt)* ) => {
        match $expr {
            ::core::result::Result::Err(err) => err,
            ::core::result::Result::Ok($ok) => {
                ::core::hint::cold_path();
                $crate::fatal!( $logger, $message; $($args)* );
            },
        }
    };
    ( $expr:expr, $logger:expr, $message:expr $(; $($args:tt)* )? ) => {
        match $expr {
            ::core::result::Result::Err(err) => err,
            ::core::result::Result::Ok(_ok) => {
                ::core::hint::cold_path();
                $crate::fatal!( $logger, $message $(; $($args)* )? );
            },
        }
    };
    (
        $expr:expr, $logger:expr, target: $target:expr,
        $message:expr; for Ok($ok:pat): $($args:tt)*
    ) => {
        match $expr {
            ::core::result::Result::Err(err) => err,
            ::core::result::Result::Ok($ok) => {
                ::core::hint::cold_path();
                $crate::fatal!( $logger, target: $target, $message; $($args)* );
            },
        }
    };
    (
        $expr:expr, $logger:expr, target: $target:expr,
        $message:expr $(; $($args:tt)* )?
    ) => {
        match $expr {
            ::core::result::Result::Err(err) => err,
            ::core::result::Result::Ok(_ok) => {
                ::core::hint::cold_path();
                $crate::fatal!( $logger, target: $target, $message $(; $($args)* )? );
            },
        }
    };
}
