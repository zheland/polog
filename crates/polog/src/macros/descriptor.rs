/// Builds a `const` [`Descriptor`] for a log event.
///
/// # Syntax
///
/// Accepted forms include:
/// - `log_descriptor!( $level:expr $(, target: $target:expr)? $(,
///   $message:expr)? $(; $($arg),* $(,)? )? )`
/// - `log_descriptor!( $level:expr $(, target: $target:expr)? $(,
///   $message:expr)? $(, keys: $keys:expr $(,)? )? )`
/// - `log_descriptor!( descriptor: $descriptor:expr $(,)? )`
///
/// This macro syntax is similar to the `log!` macro.
/// For the detailed syntax documentation, please see the [`log!`] macro
/// documentation.
///
/// This macro is intended for defining static descriptors manually, for example
/// when using [`log!`](crate::log!) with `:r(...)` raw arguments.
///
/// # Examples
///
/// ```rust
/// const DESCRIPTOR: polog::Descriptor =
///     polog::log_descriptor!(polog::Level::Debug, "Raw args"; arg1, arg2, arg3);
/// ```
///
/// ```rust
/// const KEYS: &[&str] = &["arg1", "arg2", "arg3"];
/// const DESCRIPTOR: polog::Descriptor =
///     polog::log_descriptor!(polog::Level::Debug, "Raw args", keys: KEYS);
/// ```
///
/// [`Descriptor`]: crate::Descriptor
/// [`log!`]: crate::log!
#[macro_export]
macro_rules! log_descriptor {
    ( $($tt:tt)* ) => {
        $crate::__descriptor_impl!( [ $($tt)* ] [ log ] )
    }
}

/// Builds a `const` [`Descriptor`] for a span event.
///
/// # Syntax
///
/// Accepted forms include:
/// - `span_descriptor!( $level:expr $(, target: $target:expr)? $(,
///   $message:expr)? $(; $($arg),* $(,)? )? )`
/// - `span_descriptor!( $level:expr $(, target: $target:expr)? $(,
///   $message:expr)? $(, keys: $keys:expr $(,)? )? )`
/// - `span_descriptor!( descriptor: $descriptor:expr $(,)? )`
///
/// This macro syntax is similar to the `log!` macro.
/// For the detailed syntax documentation, please see the [`log!`] macro
/// documentation.
///
/// This macro is intended for defining static descriptors manually, for example
/// when using [`log!`](crate::log!) with `:r(...)` raw arguments.
///
/// # Examples
///
/// ```rust
/// const DESCRIPTOR: polog::Descriptor = polog::span_descriptor!(
///     polog::SpanLevel::Debug, "Raw args"; arg1, arg2, arg3
/// );
/// ```
///
/// ```rust
/// const KEYS: &[&str] = &["arg1", "arg2", "arg3"];
/// const DESCRIPTOR: polog::Descriptor =
///     polog::span_descriptor!(polog::SpanLevel::Debug, "Raw args", keys: KEYS);
/// ```
///
/// [`Descriptor`]: crate::Descriptor
/// [`log!`]: crate::log!
#[macro_export]
macro_rules! span_descriptor {
    ( $($tt:tt)* ) => {
        $crate::__descriptor_impl!( [ $($tt)* ] [ span ] )
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! __descriptor_impl {
    // Delegate to given descriptor.
    ( [ descriptor: $descriptor:expr $(, $($args:tt)* )? ] [ $_kind:ident ] ) => {
        $descriptor
    };
    ( [ descriptor: $descriptor:expr $(; $($args:tt)* )? ] [ $_kind:ident ] ) => {
        $descriptor
    };

    // Parse keys from key-value pairs after the semicolon.
    // Should be first to replace `;` to `,` for further parsing steps.
    ( [ $key:ident: $value:expr; $($args:tt)* ] $( $parsed:tt )* ) => {
        $crate::__descriptor_impl!(
            [ $key: $value, keys: &$crate::parse_keys!($($args)*) ]
            $( $parsed )*
        )
    };
    ( [ $value:expr; $($args:tt)* ] $( $parsed:tt )* ) => {
        $crate::__descriptor_impl!(
            [ $value, keys: &$crate::parse_keys!($($args)*) ]
            $( $parsed )*
        )
    };

    // Parse level
    (
        [ $level:expr $(, $($rest:tt)*)? ]
        [ $kind:ident ]
    ) => {
        $crate::__descriptor_impl!(
            [ $($($rest)*)? ]
            [ $kind, $level ]
        )
    };

    // Parse target
    (
        [ target: $target:expr $(, $($rest:tt)*)? ]
        [ $kind:ident, $level:expr ]
    ) => {
        $crate::__descriptor_impl!(
            [ $($($rest)*)? ]
            [ $kind, $level, Some($target) ]
        )
    };
    (
        [ $($rest:tt)* ]
        [ $kind:ident, $level:expr ]
    ) => {
        $crate::__descriptor_impl!(
            [ $($rest)* ]
            [ $kind, $level, None ]
        )
    };

    // Parse message
    (
        [ $message:expr $(, $($rest:tt)*)? ]
        [ $kind:ident, $level:expr, $target:expr ]
    ) => {
        $crate::__descriptor_impl!(
            [ $($($rest)*)? ]
            [ $kind, $level, $target, $message ]
        )
    };
    (
        [ $($rest:tt)* ]
        [ $kind:ident, $level:expr, $target:expr ]
    ) => {
        $crate::__descriptor_impl!(
            [ $($rest)* ]
            [ $kind, $level, $target, "" ]
        )
    };

    // Parse or replace keys.
    (
        [ keys: $keys:expr $(, $($rest:tt)*)? ]
        [ $kind:ident, $level:expr, $target:expr, $message:expr $(, $old_keys:expr )? ]
    ) => {
        $crate::__descriptor_impl!(
            [ $($($rest)*)? ]
            [ $kind, $level, $target, $message, $keys ]
        )
    };
    // Use empty keys list.
    (
        [ $(,)? ]
        [ $kind:ident, $level:expr, $target:expr, $message:expr ]
    ) => {
        $crate::__descriptor_impl!(
            [] [ $kind, $level, $target, $message, &[] ]
        )
    };
    // Handle unexpected syntax.
    (
        [ $($rest:tt)* ]
        [ $kind:ident, $level:expr, $target:expr, $message:expr ]
    ) => {
        ::core::compile_error!(::core::concat!(
            "Unexpected expressions after the comma. \n",
            "Expected one of:\n",
            "- arguments after semicolon: `... ",
            ::core::stringify!($message), " ;       ",
            ::core::stringify!($($rest)*),
            " ...`)\n",
            "- keys descriptor argument:  `... ",
            ::core::stringify!($message), " , keys: ",
            ::core::stringify!($($rest)*),
            " ...`)\n",
        ))
    };

    // Build descriptor
    (
        [ $(,)? ]
        [ log, $level:expr, $target:expr, $message:expr, $keys:expr ]
    ) => {
        $crate::Descriptor {
            kind: $crate::DescriptorKind::Log,
            level: $level,
            module_path: ::core::module_path!(),
            target: $target,
            message: $message,
            param_names: $keys,
            location: *::core::panic::Location::caller(),
        }
    };
    (
        [ $(,)? ]
        [ span, $level:expr, $target:expr, $message:expr, $keys:expr ]
    ) => {
        $crate::Descriptor {
            kind: $crate::DescriptorKind::Span,
            level: $crate::Level::from_span_level($level),
            module_path: ::core::module_path!(),
            target: $target,
            message: $message,
            param_names: $keys,
            location: *::core::panic::Location::caller(),
        }
    };

    // Unexpected tokens error
    (
        [ $token:tt $(, $($rest:tt)*)? ]
        $( $parsed:tt )*
    ) => {
        ::core::compile_error!(::core::concat!(
            "Unexpected token `",
            $( ::core::stringify!($token) )?,
            "`, parsed:",
            $( " `", ::core::stringify!($parsed), "`", )*
        ));
    };
}

/// Builds a `&'static` [`Descriptor`] for a log event.
///
/// # Syntax
///
/// Accepted forms include:
/// - `log_descriptor!( $(:dyn)? $level:expr $(, target: $target:expr)? $(,
///   $message:expr)? $(; $($arg),* $(,)? )? )`
/// - `span_descriptor!( $level:expr $(, target: $target:expr)? $(,
///   $message:expr)? $(, keys: $keys:expr $(,)? )? )`
/// - `log_descriptor!( descriptor: $descriptor:expr $(,)? )`
/// - `log_descriptor!( descriptor_ref: $descriptor_ref:expr $(,)? )`
///
/// This macro syntax is similar to the `log!` macro.
/// For the detailed syntax documentation, please see the [`log!`] macro
/// documentation.
///
/// With a constant level, the descriptor is stored in a `static` and returned
/// by reference.
/// With `:dyn $level:expr`, one static descriptor is generated per built-in
/// [`Level`] and selected at runtime.
///
/// # Examples
///
/// ```rust
/// static DESCRIPTOR_REF: &polog::Descriptor = polog::log_descriptor_ref!(
///     polog::Level::Debug, "Raw args"; arg1, arg2, arg3
/// );
/// ```
///
/// ```rust
/// const KEYS: &[&str] = &["arg1", "arg2", "arg3"];
/// static DESCRIPTOR_REF: &polog::Descriptor =
///     polog::log_descriptor_ref!(polog::Level::Debug, "Raw args", keys: KEYS);
/// ```
///
/// [`Descriptor`]: crate::Descriptor
/// [`log!`]: crate::log!
/// [`Level`]: crate::Level
#[macro_export]
macro_rules! log_descriptor_ref {
    ( :dyn $level:expr, $($tt:tt)* ) => {{
        static __FATAL: $crate::Descriptor = $crate::log_descriptor!($crate::Level::Fatal, $($tt)*);
        static __BUG: $crate::Descriptor = $crate::log_descriptor!($crate::Level::Bug, $($tt)*);
        static __ERROR: $crate::Descriptor = $crate::log_descriptor!($crate::Level::Error, $($tt)*);
        static __WARN: $crate::Descriptor = $crate::log_descriptor!($crate::Level::Warn, $($tt)*);
        static __INFO: $crate::Descriptor = $crate::log_descriptor!($crate::Level::Info, $($tt)*);
        static __DEBUG: $crate::Descriptor = $crate::log_descriptor!($crate::Level::Debug, $($tt)*);
        static __TRACE: $crate::Descriptor = $crate::log_descriptor!($crate::Level::Trace, $($tt)*);
        match $level {
            $crate::Level::Fatal => &__FATAL,
            $crate::Level::Bug => &__BUG,
            $crate::Level::Error => &__ERROR,
            $crate::Level::Warn => &__WARN,
            $crate::Level::Info => &__INFO,
            $crate::Level::Debug => &__DEBUG,
            $crate::Level::Trace => &__TRACE,
        }
    }};
    ( descriptor_ref: $descriptor:expr $(, $($args:tt)* )? ) => {
        $crate::log_descriptor!( descriptor: $descriptor $(, $( $args )* )? )
    };
    ( descriptor_ref: $descriptor:expr $(; $($args:tt)* )? ) => {
        $crate::log_descriptor!( descriptor: $descriptor $(; $( $args )* )? )
    };
    ( $($tt:tt)* ) => {{
        static __DESCRIPTOR: $crate::Descriptor = $crate::log_descriptor!($($tt)*);
        &__DESCRIPTOR
    }};
}

/// Builds a `&'static` [`Descriptor`] for a span event.
///
/// # Syntax
///
/// Accepted forms include:
/// - `span_descriptor!( $(:dyn)? $level:expr $(, target: $target:expr)? $(,
///   $message:expr)? $(; $($arg),* $(,)? )? )`
/// - `span_descriptor!( $level:expr $(, target: $target:expr)? $(,
///   $message:expr)? $(, keys: $keys:expr $(,)? )? )`
/// - `span_descriptor!( descriptor: $descriptor:expr $(,)? )`
/// - `span_descriptor!( descriptor_ref: $descriptor_ref:expr $(,)? )`
///
/// This macro syntax is similar to the `log!` macro.
/// For the detailed syntax documentation, please see the [`log!`] macro
/// documentation.
///
/// With a constant level, the descriptor is stored in a `static` and returned
/// by reference.
/// With `:dyn $level:expr`, one static descriptor is generated per built-in
/// [`Level`] and selected at runtime.
///
/// # Examples
///
/// ```rust
/// static DESCRIPTOR_REF: &polog::Descriptor = polog::span_descriptor_ref!(
///     polog::SpanLevel::Debug, "Raw args"; arg1, arg2, arg3
/// );
/// ```
///
/// ```rust
/// const KEYS: &[&str] = &["arg1", "arg2", "arg3"];
/// static DESCRIPTOR_REF: &polog::Descriptor = polog::span_descriptor_ref!(
///     polog::SpanLevel::Debug, "Raw args", keys: KEYS
/// );
/// ```
///
/// [`Descriptor`]: crate::Descriptor
/// [`log!`]: crate::log!
/// [`Level`]: crate::Level
#[macro_export]
macro_rules! span_descriptor_ref {
    ( :dyn $level:expr, $($tt:tt)* ) => {{
        static __ERROR: $crate::Descriptor =
            $crate::span_descriptor!($crate::SpanLevel::Error, $($tt)*);
        static __WARN: $crate::Descriptor =
            $crate::span_descriptor!($crate::SpanLevel::Warn, $($tt)*);
        static __INFO: $crate::Descriptor =
            $crate::span_descriptor!($crate::SpanLevel::Info, $($tt)*);
        static __DEBUG: $crate::Descriptor =
            $crate::span_descriptor!($crate::SpanLevel::Debug, $($tt)*);
        static __TRACE: $crate::Descriptor =
            $crate::span_descriptor!($crate::SpanLevel::Trace, $($tt)*);
        match $level {
            $crate::SpanLevel::Error => &__ERROR,
            $crate::SpanLevel::Warn => &__WARN,
            $crate::SpanLevel::Info => &__INFO,
            $crate::SpanLevel::Debug => &__DEBUG,
            $crate::SpanLevel::Trace => &__TRACE,
        }
    }};
    ( descriptor_ref: $descriptor:expr $(, $($args:tt)* )? ) => {
        $crate::log_descriptor!( descriptor: $descriptor $(, $( $args )* )? )
    };
    ( descriptor_ref: $descriptor:expr $(; $($args:tt)* )? ) => {
        $crate::log_descriptor!( descriptor: $descriptor $(; $( $args )* )? )
    };
    ( $($tt:tt)* ) => {{
        static DESCRIPTOR: $crate::Descriptor = $crate::span_descriptor!($($tt)*);
        &DESCRIPTOR
    }};
}

#[cfg(test)]
mod tests {
    use crate::{Descriptor, DescriptorKind, Level, SpanLevel};

    #[test]
    fn test_macro_descriptor_without_keys() {
        {
            const DESCRIPTOR1: Descriptor = log_descriptor!(Level::Error, "desc");
            const DESCRIPTOR2: Descriptor = log_descriptor!(descriptor: DESCRIPTOR1);
            assert!(matches!(
                DESCRIPTOR1,
                Descriptor {
                    kind: DescriptorKind::Log,
                    level: Level::Error,
                    module_path: module_path!(),
                    target: None,
                    message: "desc",
                    param_names: &[],
                    location: _,
                }
            ));
            assert_eq!(DESCRIPTOR1, DESCRIPTOR2);
        }
        {
            const DESCRIPTOR: Descriptor = span_descriptor!(SpanLevel::Error, "desc");
            assert!(matches!(
                DESCRIPTOR,
                Descriptor {
                    kind: DescriptorKind::Span,
                    level: Level::Error,
                    ..
                }
            ));
        }
        {
            const DESCRIPTOR: Descriptor = log_descriptor!(Level::Error);
            assert!(matches!(
                DESCRIPTOR,
                Descriptor {
                    target: None,
                    param_names: &[],
                    ..
                }
            ));
        }
        {
            const DESCRIPTOR: Descriptor = log_descriptor!(Level::Error, target: "custom");
            assert!(matches!(
                DESCRIPTOR,
                Descriptor {
                    target: Some("custom"),
                    param_names: &[],
                    ..
                }
            ));
        }
        {
            const DESCRIPTOR: Descriptor = log_descriptor!(Level::Error, "desc");
            assert!(matches!(
                DESCRIPTOR,
                Descriptor {
                    target: None,
                    message: "desc",
                    param_names: &[],
                    ..
                }
            ));
        }
        {
            const DESCRIPTOR: Descriptor = log_descriptor!(Level::Error, target: "custom", "desc");
            assert!(matches!(
                DESCRIPTOR,
                Descriptor {
                    target: Some("custom"),
                    message: "desc",
                    param_names: &[],
                    ..
                }
            ));
        }
    }

    #[test]
    fn test_macro_descriptor_with_keys() {
        {
            const DESCRIPTOR: Descriptor = log_descriptor!(Level::Error, keys: &["a", "b", "c"]);
            assert!(matches!(
                DESCRIPTOR,
                Descriptor {
                    target: None,
                    param_names: &["a", "b", "c"],
                    ..
                }
            ));
        }
        {
            const DESCRIPTOR: Descriptor =
                log_descriptor!(Level::Error, target: "custom", keys: &["a", "b", "c"]);
            assert!(matches!(
                DESCRIPTOR,
                Descriptor {
                    target: Some("custom"),
                    param_names: &["a", "b", "c"],
                    ..
                }
            ));
        }
        {
            const DESCRIPTOR: Descriptor =
                log_descriptor!(Level::Error, "desc", keys: &["a", "b", "c"]);
            assert!(matches!(
                DESCRIPTOR,
                Descriptor {
                    target: None,
                    message: "desc",
                    param_names: &["a", "b", "c"],
                    ..
                }
            ));
        }
        {
            const DESCRIPTOR: Descriptor =
                log_descriptor!(Level::Error, target: "custom", "desc", keys: &["a", "b", "c"]);
            assert!(matches!(
                DESCRIPTOR,
                Descriptor {
                    target: Some("custom"),
                    message: "desc",
                    param_names: &["a", "b", "c"],
                    ..
                }
            ));
        }
    }

    #[test]
    fn test_macro_descriptor_with_arguments() {
        {
            const DESCRIPTOR: Descriptor = log_descriptor!(Level::Error; q = 1, w, e = :a ? e);
            assert!(matches!(
                DESCRIPTOR,
                Descriptor {
                    target: None,
                    param_names: &["q", "w", "e"],
                    ..
                }
            ));
        }
        {
            const DESCRIPTOR: Descriptor =
                log_descriptor!(Level::Error, target: "custom"; q = 1, w, e = :a ? e);
            assert!(matches!(
                DESCRIPTOR,
                Descriptor {
                    target: Some("custom"),
                    param_names: &["q", "w", "e"],
                    ..
                }
            ));
        }
        {
            const DESCRIPTOR: Descriptor =
                log_descriptor!(Level::Error, "desc"; q = 1, w, e = :a ? e);
            assert!(matches!(
                DESCRIPTOR,
                Descriptor {
                    target: None,
                    message: "desc",
                    param_names: &["q", "w", "e"],
                    ..
                }
            ));
        }
        {
            const DESCRIPTOR: Descriptor =
                log_descriptor!(Level::Error, target: "custom", "desc"; q = 1, w, e = :a ? e);
            assert!(matches!(
                DESCRIPTOR,
                Descriptor {
                    target: Some("custom"),
                    message: "desc",
                    param_names: &["q", "w", "e"],
                    ..
                }
            ));
        }
    }
}
