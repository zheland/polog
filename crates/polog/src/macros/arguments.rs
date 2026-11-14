/// Parses a log argument list and returns its keys as a `&'static
/// [&'static str]`.
///
/// The argument syntax matches the argument list accepted by
/// [`log!`], [`span!`], and related macros.
///
/// The result is suitable for [`Descriptor::param_names`].
///
/// # Examples
///
/// ```rust
/// let baz = "baz_arg_name";
///
/// let args = polog::parse_keys!(
///     foo = foo1 * 2,
///     "bar" = bar2 * 3,
///     (baz) = baz3 * 3,
///     ("qux") = :s % qux4 * 3,
///     quux,
///     :t ? quux + 123,
/// );
///
/// assert_eq!(
///     args,
///     ["foo", "bar", "baz_arg_name", "qux", "quux", "quux + 123"],
/// );
/// ```
///
/// [`log!`]: crate::log!
/// [`span!`]: crate::span!
/// [`Descriptor::param_names`]: crate::Descriptor::param_names
#[macro_export]
macro_rules! parse_keys {
    ( $($tt:tt)* ) => {
        $crate::__parse_args_impl!( [] [] [] [] K [ $($tt)* ] [] )
    };
}

/// Parses a log argument list and returns a heterogeneous tuple of [`Arg`]
/// wrappers.
///
/// Send and formatting modifiers are applied here, so the result implements
/// [`LogArgs`].
///
/// # Examples
///
/// ```rust
/// let baz = "baz_arg_name";
///
/// let args = polog::parse_values!(
///     foo = 123,
///     "bar" = 234,
///     (baz) = "baz",
///     ("qux") = :s % 16 * 4,
/// );
///
/// assert_eq!(
///     args,
///     (
///         polog::Arg(123),
///         polog::Arg(234),
///         polog::Arg("baz"),
///         polog::Arg(polog::Stringifiable(polog::ViaDisplay(64))),
///     ),
/// );
/// ```
///
/// [`Arg`]: crate::Arg
/// [`LogArgs`]: crate::LogArgs
#[macro_export]
macro_rules! parse_values {
    ( $($tt:tt)* ) => {
        $crate::__parse_args_impl!( [] [] [] [] V [ $($tt)* ] [] )
    };
}

/// Skips the descriptor portion of a macro invocation and parses the trailing
/// argument list with [`parse_values!`].
///
/// # Examples
///
/// ```rust
/// let baz = "baz_arg_name";
///
/// let args = polog::locate_and_parse_values!(
///     &mut logger, "Message";
///     foo = 123,
///     "bar" = 234,
///     (baz) = "baz",
///     ("qux") = :s % 16 * 4,
/// );
///
/// assert_eq!(
///     args,
///     (
///         polog::Arg(123),
///         polog::Arg(234),
///         polog::Arg("baz"),
///         polog::Arg(polog::Stringifiable(polog::ViaDisplay(64))),
///     ),
/// );
/// ```
///
/// The macro discards tokens until the first `;` or the end of input, then
/// delegates to [`parse_values!`].
#[macro_export]
macro_rules! locate_and_parse_values {
    () => {
        $crate::parse_values!()
    };
    ( ; $($tt:tt)* ) => {
        $crate::parse_values!($($tt)*)
    };
    ( $_:tt $($tt:tt)* ) => {
        $crate::locate_and_parse_values!($($tt)*)
    };
}

/// The macro syntax is:
/// ```rust,ignore
/// __parse_args_impl!(
///     [ $( $last_key:expr )? ]
///     [ $( $send_modifier:expr )? ]
///     [ $( $fmt_modifier:expr )? ]
///     [ $( $last_expr )? ]
///     $action:tt
///     [ $( $remaining_tokens:tt )* ]
///     [ $( [ $parsed:expr ] )* ]
/// )
/// ```
#[doc(hidden)]
#[macro_export]
macro_rules! __parse_args_impl {
    // Parse key.
    ( [] [] [] [] $action:tt [ $key:ident = $($rem:tt)* ] $parsed:tt ) => {
        $crate::__parse_args_impl!(
            [ ::core::stringify!($key) ] [] [] [] $action [ $($rem)* ] $parsed
        )
    };
    ( [] [] [] [] $action:tt [ $key:literal = $($rem:tt)* ] $parsed:tt ) => {
        $crate::__parse_args_impl!( [ $key ] [] [] [] $action [ $($rem)* ] $parsed )
    };
    ( [] [] [] [] $action:tt [ ($key:expr) = $($rem:tt)* ] $parsed:tt ) => {
        $crate::__parse_args_impl!( [ $key ] [] [] [] $action [ $($rem)* ] $parsed )
    };
    ( [] [] [] [] $action:tt [ @$key:ident = $($rem:tt)* ] $parsed:tt ) => {
        $crate::__parse_args_impl!(
            [ ::core::concat!("@", ::core::stringify!($key)) ]
            [] [] [] $action [ $($rem)* ] $parsed
        )
    };
    ( [] [] [] [] $action:tt [ @fn $(, $($rem:tt)* )? ] $parsed:tt ) => {
        $crate::__parse_args_impl!(
            [ "@fn" ] [] [] [] $action
            [ $crate::uninstantiated_fn_name!() $(, $($rem)* )? ] $parsed
        )
    };

    // Parse send modifier.
    ( $key:tt [] [] [] $action:tt [ :a $($rem:tt)* ] $parsed:tt ) => {
        $crate::__parse_args_impl!( $key [ AlreadySendable ] [] [] $action [ $($rem)* ] $parsed )
    };
    ( $key:tt [] [] [] $action:tt [ :t $($rem:tt)* ] $parsed:tt ) => {
        $crate::__parse_args_impl!( $key [ Temporary ] [] [] $action [ $($rem)* ] $parsed )
    };
    ( $key:tt [] [] [] $action:tt [ :s $($rem:tt)* ] $parsed:tt ) => {
        $crate::__parse_args_impl!( $key [ Stringifiable ] [] [] $action [ $($rem)* ] $parsed )
    };

    // Parse format modifier.
    ( $key:tt $send:tt [] [] $action:tt [ % $($rem:tt)* ] $parsed:tt ) => {
        $crate::__parse_args_impl!( $key $send [ ViaDisplay ] [] $action [ $($rem)* ] $parsed )
    };
    ( $key:tt $send:tt [] [] $action:tt [ ? $($rem:tt)* ] $parsed:tt ) => {
        $crate::__parse_args_impl!( $key $send [ ViaDebug ] [] $action [ $($rem)* ] $parsed )
    };
    ( $key:tt $send:tt [] [] $action:tt [ #% $($rem:tt)* ] $parsed:tt ) => {
        $crate::__parse_args_impl!( $key $send [ ViaAltDisplay ] [] $action [ $($rem)* ] $parsed )
    };
    ( $key:tt $send:tt [] [] $action:tt [ #? $($rem:tt)* ] $parsed:tt ) => {
        $crate::__parse_args_impl!( $key $send [ ViaAltDebug ] [] $action [ $($rem)* ] $parsed )
    };

    // Parse `format_args` modifier.
    ( [ $key:expr ] [] [] [] K [ :f ( $($_fmt:tt)* ) $(, $($rem:tt)* )? ] [ $($parsed:tt)* ] ) => {
        $crate::__parse_args_impl!( [] [] [] [] K [ $($($rem)*)? ] [ $($parsed)* [ $key ] ] )
    };
    ( [ $key:expr ] [] [] [] V [ :f ( $($fmt:tt)* ) $(, $($rem:tt)* )? ] [ $($parsed:tt)* ] ) => {
        $crate::__parse_args_impl!(
            [] [] [] [] V [ $($($rem)*)? ] [ $($parsed)* [
                $crate::Arg( $crate::Stringifiable( core::format_args!( $($fmt)* ) ) )
            ] ]
        )
    };

    // Parse raw arguments (with spread or flattening)
    ( [] [] [] [] V [ :r $expr:expr $(, $($rem:tt)* )? ] [ $($parsed:tt)* ] ) => {
        $crate::__parse_args_impl!(
            [] [] [] [] V [ $($($rem)*)? ] [ $($parsed)* [ $expr ] ]
        )
    };

    // Parse expression
    ( [] $send:tt $fmt:tt [] $action:tt [ $expr:expr $(, $($rem:tt)* )? ] $parsed:tt ) => {
        $crate::__parse_args_impl!(
            [ ::core::stringify!($expr) ] $send $fmt [ $expr ] $action [ $($($rem)*)? ] $parsed
        )
    };
    ( $key:tt $send:tt $fmt:tt [] $action:tt [ $expr:expr $(, $($rem:tt)* )? ] $parsed:tt ) => {
        $crate::__parse_args_impl!( $key $send $fmt [ $expr ] $action [ $($($rem)*)? ] $parsed )
    };

    // Add key to parsed keys list and continue.
    ( [ $key:expr ] $_send:tt $_fmt:tt [ $_expr:expr ] K $rem:tt [ $($parsed:tt)* ] ) => {
        $crate::__parse_args_impl!( [] [] [] [] K $rem [ $($parsed)* [ $key ] ] )
    };

    // Apply value modifiers, add to parsed values, and continue.
    ( $key:tt $send:tt [ $fmt:ident ] [ $expr:expr ] V $rem:tt $parsed:tt ) => {
        $crate::__parse_args_impl!( $key $send [] [ $crate::$fmt($expr) ] V $rem $parsed )
    };
    ( $key:tt [ $send:ident ] [] [ $expr:expr ] V $rem:tt $parsed:tt ) => {
        $crate::__parse_args_impl!( $key [] [] [ $crate::$send($expr) ] V $rem $parsed )
    };
    ( $key:tt [] [] [ $expr:expr ] V $rem:tt [ $($parsed:tt)* ] ) => {
        $crate::__parse_args_impl!( [] [] [] [] V $rem [ $($parsed)* [ $crate::Arg($expr) ] ] )
    };

    // Build array of keys.
    ( [] [] [] [] K [] [ $( [ $keys:expr ] )* ] ) => {
        [ $($keys),* ]
    };

    // `MixedArgs` support nested tuple of arguments up to 12 element tuple.
    // Match tuples with more than 12 elements and nest the fist 12 recursively.
    ( [] [] [] [] V [] [
        [ $elem1:expr ] [ $elem2:expr ] [ $elem3:expr ] [ $elem4:expr ]
        [ $elem5:expr ] [ $elem6:expr ] [ $elem7:expr ] [ $elem8:expr ]
        [ $elem9:expr ] [ $elem10:expr ] [ $elem11:expr ] [ $elem12:expr ]
        $( $rem:tt )+
    ] ) => {
        $crate::__parse_args_impl!( [] [] [] [] V [] [
            [ (
                $elem1, $elem2, $elem3, $elem4,
                $elem5, $elem6, $elem7, $elem8,
                $elem9, $elem10, $elem11, $elem12,
            ) ]
            $( $rem )+
        ] )
    };

    // Build nested tuple of arguments with tuples up to 12 elements.
    ( [] [] [] [] V [] [ $( [ $values:expr ] )* ] ) => {
        ( $($values,)* )
    };

    // Handle malformed input.
    ( $($tt:tt)* ) => {
        $crate::__handle_malformed_args!( $($tt)* )
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __handle_malformed_args {
    ([][$_s:ident] $_f:tt $_e:tt $_a:tt[$_k:tt = $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Argument key must precede the send modifier");
    };
    ([][][$_f:ident] $_e:tt $_a:tt[$_k:tt = $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Argument key must precede the format modifier");
    };
    ([] $_s:tt $_f:tt $_e:tt $_a:tt[$key:tt = $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!(::core::concat!(
            "Only ident, literal or (expr) keys are supported, given: ",
            ::core::stringify!($key)
        ));
    };

    ($_k:tt[$_s:ident] $_f:tt $_e:tt $_a:tt[: a $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Only one send modifier is allowed");
    };
    ($_k:tt[$_s:ident] $_f:tt $_e:tt $_a:tt[: t $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Only one send modifier is allowed");
    };
    ($_k:tt[$_s:ident] $_f:tt $_e:tt $_a:tt[: s $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Only one send modifier is allowed");
    };

    ($_k:tt $_s:tt[$_f:ident] $_e:tt $_a:tt[: a $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Send modifier must precede format modifier");
    };
    ($_k:tt $_s:tt[$_f:ident] $_e:tt $_a:tt[: t $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Send modifier must precede format modifier");
    };
    ($_k:tt $_s:tt[$_f:ident] $_e:tt $_a:tt[: s $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Send modifier must precede format modifier");
    };

    ($_k:tt $_s:tt[$_f:ident] $_e:tt $_a:tt[% $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Only one format modifier is allowed");
    };
    ($_k:tt $_s:tt[$_f:ident] $_e:tt $_a:tt[? $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Only one format modifier is allowed");
    };
    ($_k:tt $_s:tt[$_f:ident] $_e:tt $_a:tt[# % $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Only one format modifier is allowed");
    };
    ($_k:tt $_s:tt[$_f:ident] $_e:tt $_a:tt[#? $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Only one format modifier is allowed");
    };

    ($_k:tt[$_s:ident] $_st:tt $_e:tt $_a:tt[: f $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Args formatted with `:f` can not be combined with send modifiers");
    };
    ($_k:tt $_s:tt[$_f:ident] $_e:tt $_a:tt[: f $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!(
            "Args formatted with `:f` can not be combined with format modifiers"
        );
    };

    ($_k:tt $_s:tt $_f:tt[]K[: r $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Args passed with `:r` can only be used with separate descriptor");
    };
    ([][$_s:ident] $_f:tt[]V[: r $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Args passed with `:r` can not be used with send modifier");
    };
    ([] $_s:tt[$_f:ident][]V[: r $($_r:tt)*] $_p:tt) => {
        ::core::compile_error!("Args passed with `:r` can not be used with format modifier");
    };
}

#[cfg(test)]
mod tests {
    use crate::Arg;

    #[test]
    fn test_macro_args_keys() {
        macro_rules! example_macro {
            ($tt1:tt $tt2:tt $tt3:tt) => {
                concat!($tt3, $tt2, $tt1)
            };
        }

        const fn example_fn(v1: &'static str, from: usize, until: usize) -> &'static str {
            v1.split_at(until).0.split_at(from).1
        }

        let baz = "baz_arg_name";

        let args = parse_keys!(
            foo = foo1 * 2,   // Identifier key is stringified.
            "bar" = bar2 * 3, // Literal key is used as is (as expression)
            (baz) = baz3 * 3, // Any expression in parentheses, used as is.
            ("qux") = :s % qux4 * 3,
            (example_macro!("qwe" "asd" "zxc")) = 123 + 234,
            (example_fn("Hello World!", 6, 11)) = 1 * 2 * 3,
            // If no key matches, the stringified expression becomes the key.
            quux,
            :t ? quux + 123,
            "corge",
            "corge" + 123,
            (grault),
            (grault + 123),
            ("garply"),
            :a #% ("garply" + 123),
        );

        assert_eq!(
            args,
            [
                "foo",
                "bar",
                "baz_arg_name",
                "qux",
                "zxcasdqwe",
                "World",
                "quux",
                "quux + 123",
                "\"corge\"",
                "\"corge\" + 123",
                "(grault)",
                "(grault + 123)",
                "(\"garply\")",
                "(\"garply\" + 123)",
            ]
        );
    }

    #[test]
    fn test_macro_parse_args_values() {
        let foo = 100;
        let bar = 10_000;
        let quux = 1_000_000;

        let args = parse_values!(
            foo = foo * 2,
            "bar" = foo * 3,
            (baz) = bar * 4,
            ("qux") = bar * 5,
            (example_macro!("qwe" "asd" "zxc")) = 123 + 234,
            (example_fn("Hello World!", 6, 11)) = 2 * 3 * 7,
            quux,
            &"corge"[1..3],
            foo + bar + quux,
            10,
            "11",
            12,
        );

        assert_eq!(
            args,
            (
                Arg(200),
                Arg(300),
                Arg(40_000),
                Arg(50_000),
                Arg(357),
                Arg(42),
                Arg(1_000_000),
                Arg("or"),
                Arg(1_010_100),
                Arg(10),
                Arg("11"),
                Arg(12),
            )
        );
    }

    #[test]
    fn test_macro_parse_args_values_long() {
        use crate::Arg as A;

        let foo = 100;
        let bar = 10_000;
        let quux = 1_000_000;

        #[rustfmt::skip]
        let args = parse_values!(
            foo = foo * 2,
            "bar" = foo * 3,
            (baz) = bar * 4,
            ("qux") = bar * 5,
            (example_macro!("qwe" "asd" "zxc")) = 123 + 234,
            (example_fn("Hello World!", 6, 11)) = 2 * 3 * 7,
            quux,
            &"corge"[1..3],
            foo + bar + quux,
            10, "11", 12,
            13, "14", 15, "16", 17, "18", 19, "20", 21, "22", 23,
            24, "25", 26, "27", 28, "29", 30, "31", 32, "33", 34,
            35, "36", 37, "38", 39, "40", 41, "42", 43, "44", 45,
        );

        #[rustfmt::skip]
        assert_eq!(
            args,
            (
                (
                    (
                        (
                            A(200), A(300), A(40_000), A(50_000),
                            A(357), A(42), A(1_000_000), A("or"),
                            A(1_010_100), A(10), A("11"), A(12),
                        ), A(13), A("14"), A(15),
                        A("16"), A(17), A("18"), A(19),
                        A("20"), A(21), A("22"), A(23),
                    ), A(24), A("25"), A(26),
                    A("27"), A(28), A("29"), A(30),
                    A("31"), A(32), A("33"), A(34),
                ), A(35), A("36"), A(37),
                A("38"), A(39), A("40"), A(41),
                A("42"), A(43), A("44"), A(45),
            )
        );

        #[rustfmt::skip]
        let args = parse_values!(
            foo = foo * 2,
            "bar" = foo * 3,
            (baz) = bar * 4,
            ("qux") = bar * 5,
            (example_macro!("qwe" "asd" "zxc")) = 123 + 234,
            (example_fn("Hello World!", 6, 11)) = 2 * 3 * 7,
            quux,
            &"corge"[1..3],
            foo + bar + quux,
            10, "11", 12,
            13, "14", 15, "16", 17, "18", 19, "20", 21, "22", 23,
            24, "25", 26, "27", 28, "29", 30, "31", 32, "33", 34,
            35, "36", 37, "38", 39, "40", 41, "42", 43, "44", 45,
            "46",
        );

        #[rustfmt::skip]
        assert_eq!(
            args,
            (
                (
                    (
                        (
                            (
                                A(200), A(300), A(40_000), A(50_000),
                                A(357), A(42), A(1_000_000), A("or"),
                                A(1_010_100), A(10), A("11"), A(12),
                            ), A(13), A("14"), A(15),
                            A("16"), A(17), A("18"), A(19),
                            A("20"), A(21), A("22"), A(23),
                        ), A(24), A("25"), A(26),
                        A("27"), A(28), A("29"), A(30),
                        A("31"), A(32), A("33"), A(34),
                    ), A(35), A("36"), A(37),
                    A("38"), A(39), A("40"), A(41),
                    A("42"), A(43), A("44"), A(45),
                ), A("46")
            )
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn test_macro_parse_args_values_modified() {
        use alloc::string::ToString;

        use crate::{
            AlreadySendable, Arg, Stringifiable, Temporary, ViaAltDebug, ViaAltDisplay, ViaDebug,
            ViaDisplay,
        };

        let foo = 100;
        let bar = 10_000;
        let quux = 1_000_000;

        let args = parse_values!(
            foo = % foo * 2,
            "bar" = ? foo * 3,
            (baz) = #% bar * 4,
            ("qux") = #? bar * 5,
            (example_macro!("qwe" "asd" "zxc")) = :t 123 + 234,
            (example_fn("Hello World!", 6, 11)) = :a 2 * 3 * 7,
            :s quux,
            :a % foo + bar + quux,
            :s ? 10,
        );

        assert_eq!(
            args,
            (
                Arg(ViaDisplay(200)),
                Arg(ViaDebug(300)),
                Arg(ViaAltDisplay(40_000)),
                Arg(ViaAltDebug(50_000)),
                Arg(Temporary(357)),
                Arg(AlreadySendable(42)),
                Arg(Stringifiable(1_000_000)),
                Arg(AlreadySendable(ViaDisplay(1_010_100))),
                Arg(Stringifiable(ViaDebug(10))),
            )
        );

        let args = parse_values!(
            arg1 = 11,
            arg2 = :f("{foo} {} {bar} {} {quux}", &"corge"[1..3], "!"),
            arg3 = 33,
        );
        assert_eq!(args.0, Arg(11));
        let _: Arg<Stringifiable<core::fmt::Arguments<'_>>> = args.1;
        assert_eq!(args.1.0.0.to_string(), "100 or 10000 ! 1000000");
        assert_eq!(args.2, Arg(33));
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn test_macro_parse_args_with_spread() {
        let args = parse_values!(
            arg1 = 11,
            :r(Arg(22), (Arg(33), ()), Arg(44), (Arg(55), (Arg(66),))),
            arg3 = 33,
        );
        assert_eq!(args.0, Arg(11));
        assert_eq!(
            args.1,
            (Arg(22), (Arg(33), ()), Arg(44), (Arg(55), (Arg(66),)))
        );
        assert_eq!(args.2, Arg(33));
    }

    #[test]
    fn test_fn_meta_attribute() {
        let keys = parse_keys!(@fn);
        assert_eq!(keys, ["@fn"]);

        let values = parse_values!(@fn);
        assert_eq!(values, (Arg("test_fn_meta_attribute"),));
    }
}
