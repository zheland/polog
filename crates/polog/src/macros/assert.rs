/// Asserts that a boolean expression is `true` at runtime.
///
/// This will invoke the [`fatal!`] macro if the provided expression cannot be
/// evaluated to `true` at runtime.
///
/// # Syntax
///
/// The macro syntax is:
/// - `fatal_assert!( $condition:expr, $logger:expr, $( target: $target:expr, )?
///   $message:expr $(; $( $arg ),* $(,)? )? )`.
///
/// This macro syntax is similar to the `log!` macro.
/// For the detailed syntax documentation, please see the [`log!`] macro
/// documentation.
///
/// # Examples
///
/// ```rust
/// let mut logger = polog::Discard;
/// polog::fatal_assert!(1 < 2, &mut logger);
/// ```
///
/// ```rust,should_panic
/// let mut logger = polog::Discard;
/// polog::fatal_assert!(1 > 2, &mut logger);
/// ```
///
/// [`log!`]: crate::log!
/// [`fatal!`]: crate::fatal!
#[macro_export]
macro_rules! fatal_assert {
    ( $($args:tt)* ) => {
        $crate::__log_assert_impl!(fatal, $($args)*);
    };
}
/// Asserts that a boolean expression is `true` at runtime.
///
/// This will invoke the [`bug!`] macro if the provided expression cannot be
/// evaluated to `true` at runtime.
///
/// # Syntax
///
/// The macro syntax is:
/// - `bug_assert!( $condition:expr, $logger:expr, $( target: $target:expr, )?
///   $message:expr $(; $( $arg ),* $(,)? )? )`.
///
/// This macro syntax is similar to the `log!` macro.
/// For the detailed syntax documentation, please see the [`log!`] macro
/// documentation.
///
/// # Examples
///
/// ```rust
/// let mut logger = polog::Discard;
/// polog::bug_assert!(1 < 2, &mut logger);
/// ```
///
/// [`log!`]: crate::log!
/// [`bug!`]: crate::bug!
#[macro_export]
macro_rules! bug_assert {
    ( $($args:tt)* ) => {
        $crate::__log_assert_impl!(bug, $($args)*);
    };
}

/// Asserts that two expressions are equal to each other (using `PartialEq`).
///
/// This will invoke the [`fatal!`] macro if the provided expressions are not
/// equal to each other.
///
/// # Syntax
///
/// The macro syntax is:
/// - `fatal_assert_eq!( $left:expr, $right:expr, $logger:expr, $( target:
///   $target:expr, )? $message:expr $(; $( $arg ),* $(,)? )? )`.
///
/// This macro syntax is similar to the `log!` macro.
/// For the detailed syntax documentation, please see the [`log!`] macro
/// documentation.
///
/// # Examples
///
/// ```rust
/// let mut logger = polog::Discard;
/// polog::fatal_assert_eq!(1 + 2, 3, &mut logger);
/// ```
///
/// ```rust,should_panic
/// let mut logger = polog::Discard;
/// polog::fatal_assert_eq!(1 + 2, 4, &mut logger);
/// ```
///
/// [`log!`]: crate::log!
/// [`fatal!`]: crate::fatal!
#[macro_export]
macro_rules! fatal_assert_eq {
    ( $left:expr, $right:expr, $($args:tt)* ) => {
        $crate::__log_assert_cmp_impl!(fatal, $left, $right, true, " == ", $($args)*);
    };
}

/// Asserts that two expressions are equal to each other (using `PartialEq`).
///
/// This will invoke the [`bug!`] macro if the provided expressions are not
/// equal to each other.
///
/// # Syntax
///
/// The macro syntax is:
/// - `bug_assert_eq!( $left:expr, $right:expr, $logger:expr, $( target:
///   $target:expr, )? $message:expr $(; $( $arg ),* $(,)? )? )`.
///
/// This macro syntax is similar to the `log!` macro.
/// For the detailed syntax documentation, please see the [`log!`] macro
/// documentation.
///
/// # Examples
///
/// ```rust
/// let mut logger = polog::Discard;
/// polog::bug_assert_eq!(1 + 2, 3, &mut logger);
/// ```
///
/// [`log!`]: crate::log!
/// [`bug!`]: crate::bug!
#[macro_export]
macro_rules! bug_assert_eq {
    ( $left:expr, $right:expr, $($args:tt)* ) => {
        $crate::__log_assert_cmp_impl!(bug, $left, $right, true, " == ", $($args)*);
    };
}

/// Asserts that two expressions are not equal to each other (using
/// `PartialEq`).
///
/// This will invoke the [`fatal!`] macro if the provided expressions are equal
/// to each other.
///
/// # Syntax
///
/// The macro syntax is:
/// - `fatal_assert_ne!( $left:expr, $right:expr, $logger:expr, $( target:
///   $target:expr, )? $message:expr $(; $( $arg ),* $(,)? )? )`.
///
/// This macro syntax is similar to the `log!` macro.
/// For the detailed syntax documentation, please see the [`log!`] macro
/// documentation.
///
/// # Examples
///
/// ```rust
/// let mut logger = polog::Discard;
/// polog::fatal_assert_ne!(1 + 2, 4, &mut logger);
/// ```
///
/// ```rust,should_panic
/// let mut logger = polog::Discard;
/// polog::fatal_assert_ne!(1 + 2, 3, &mut logger);
/// ```
///
/// [`log!`]: crate::log!
/// [`fatal!`]: crate::fatal!
#[macro_export]
macro_rules! fatal_assert_ne {
    ( $left:expr, $right:expr, $($args:tt)* ) => {
        $crate::__log_assert_cmp_impl!(fatal, $left, $right, false, " != ", $($args)*);
    };
}

/// Asserts that two expressions are not equal to each other (using
/// `PartialEq`).
///
/// This will invoke the [`bug!`] macro if the provided expressions are equal
/// to each other.
///
/// # Syntax
///
/// The macro syntax is:
/// - `bug_assert_ne!( $left:expr, $right:expr, $logger:expr, $( target:
///   $target:expr, )? $message:expr $(; $( $arg ),* $(,)? )? )`.
///
/// This macro syntax is similar to the `log!` macro.
/// For the detailed syntax documentation, please see the [`log!`] macro
/// documentation.
///
/// # Examples
///
/// ```rust
/// let mut logger = polog::Discard;
/// polog::bug_assert_ne!(1 + 2, 4, &mut logger);
/// ```
///
/// [`log!`]: crate::log!
/// [`bug!`]: crate::bug!
#[macro_export]
macro_rules! bug_assert_ne {
    ( $left:expr, $right:expr, $($args:tt)* ) => {
        $crate::__log_assert_cmp_impl!(bug, $left, $right, false, " != ", $($args)*);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __log_assert_impl {
    (
        $macro:ident, $cond:expr, $logger:expr
        $(, target: $target:expr )?
    ) => {
        if !$cond {
            ::core::hint::cold_path();
            $crate::$macro!(
                $logger, $( target: $target, )?
                ::core::concat!("Assertion `", ::core::stringify!($cond), "` failed")
            );
        }
    };
    (
        $macro:ident, $cond:expr, $logger:expr,
        $message:expr $(; $($args:tt)* )?
    ) => {
        if !$cond {
            ::core::hint::cold_path();
            $crate::$macro!( $logger, $message; $( $($args)* )? );
        }
    };
    (
        $macro:ident, $cond:expr, $logger:expr,
        target: $target:expr, $message:expr $(; $($args:tt)* )?
    ) => {
        if !$cond {
            ::core::hint::cold_path();
            $crate::$macro!(
                $logger, target: $target, $message; $( $($args)* )?
            );
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __log_assert_cmp_impl {
    (
        $macro:ident, $left:expr, $right:expr,
        $should_be_eq:literal, $cond_literal:literal,
        $logger:expr
        $(, target: $target:expr $(,)? )?
    ) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if (*left_val == *right_val) != $should_be_eq {
                    ::core::hint::cold_path();
                    $crate::$macro!(
                        $logger, $( target: $target, )?
                        ::core::concat!(
                            "Assertion `",
                            ::core::stringify!($left),
                            $cond_literal,
                            ::core::stringify!($right)
                            ,"` failed",
                        );
                        (::core::stringify!($left)) = ? left_val,
                        (::core::stringify!($right)) = ? right_val,
                    );
                }
            }
        }
    };
    (
        $macro:ident, $left:expr, $right:expr,
        $should_be_eq:literal, $cond_literal:literal,
        $logger:expr
        $(, target: $target:expr )?, $message:expr $(; $($args:tt)* )?
    ) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if (*left_val == *right_val) != $should_be_eq {
                    ::core::hint::cold_path();
                    $crate::$macro!( $logger, $message; $( $($args)* )? );
                }
            }
        }
    };
    (
        $macro:ident, $left:expr, $right:expr,
        $should_be_eq:literal, $cond_literal:literal,
        $logger:expr,
        target: $target:expr, $message:expr $(; $($args:tt)* )?
    ) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if (*left_val == *right_val) != $should_be_eq {
                    ::core::hint::cold_path();
                    $crate::$macro!(
                        $logger, target: $target, $message; $( $($args)* )?
                    );
                }
            }
        }
    };
}
