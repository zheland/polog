/// Logs a message at the specified level.
///
/// # Syntax
///
/// Accepted forms include:
/// - `log!( $logger:expr, $(:dyn)? $level:expr $(, target: $target:expr )?, $(,
///   $message:expr )? $(, keys: $keys:expr )? $(; $($arg),*)? )`,
/// - `log!( $logger:expr, descriptor: $descriptor:expr $(,)? )`,
/// - `log!( $logger:expr, descriptor_ref: $descriptor_ref:expr $(,)? )`.
///
/// # Examples
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// polog::log!(&mut logger, polog::Level::Info, "Message on Info level");
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
/// polog::log!(
///     &mut logger, polog::Level::Debug, "Message";
///     arg1 = "value",
///     arg2 = :t non_static_str_ref,
///     arg3 = ? (123, true, 'a')
/// );
/// ```
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let is_error = true;
/// let level = if is_error { polog::Level::Error } else { polog::Level::Warn };
/// polog::log!(&mut logger, :dyn level, "Message"; arg = "abc");
/// ```
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// const DESCRIPTOR_REF: &polog::Descriptor =
///     polog::log_descriptor_ref!(polog::Level::Debug, "Raw args"; arg1, arg2, arg3);
/// let args_as_single_tuple = (polog::Arg(123), polog::Arg(234), polog::Arg(345));
/// polog::log!(&mut logger, descriptor_ref: DESCRIPTOR_REF; :r(args_as_single_tuple));
/// ```
///
/// # Argument evaluation
///
/// The argument list is constructed only when
/// [`LoggerExt::try_enable`] returns `Some` for this event.
/// Disabled events therefore do not evaluate their argument expressions and do
/// not pay for building the argument tuple.
/// This means the macro itself is the enablement guard: there is no separate
/// `log_enabled!`-style check, and a level-only pre-check would be *less*
/// precise than the descriptor-aware filtering the macro already performs.
///
/// # Syntax details
///
/// Syntax parts include:
/// - `$logger:expr`: An expression implementing [`Logger`] trait.
/// - `$(:dyn)?`: A optional modifier that enables use of non-constant log
///   level. Usage of non-constant log levels may result in less optimized code.
/// - `$level:expr`: A log level expression of type [`Level`]. If `dyn` modifier
///   is absent the expression should be const expression.
/// - `$message:expr`: An event message const expression.
/// - `$descriptor:expr` uses an existing descriptor instead of building one
///   from the level and message.
/// - `$(, $arg )*`: A comma-separated list of arguments.
/// - `$(,)?`: An optional trailing comma.
///
/// The `$arg` is an argument passed to a logger.
/// The `$arg` can be one of:
/// - `$( $key = )? $( $send_modifier )? $( $fmt_modifier )? $expr:expr`
/// - `$key = :f( $fmt_str:expr $(, $fmt_args )* $(,)? )`,
/// - `@fn` (which expands to `@fn = polog::uninstantiated_fn_name!()`),
///
/// where:
/// - `$( $key = )?`: An optional explicit argument key,
/// - `$key`: A key of the key-value pair. It is either `$key:ident`,
///   `$key:literal`, `($key:expr)`, or `@$key:ident`.
/// - `$key:ident`: An identifier which stringified to `&'static str` key.
/// - `$key:literal`: A literal `&'static str` key.
/// - `($key:expr)`: A const expression enclosed in parentheses that resolves to
///   `&'static str` key.
/// - `@$key:ident`: An identifier which stringified to `&'static str` meta
///   argument key.
/// - `$( $send_modifier )?`: An optional send modifier. It is either ` `
///   (absent), `:a`, `:t`, or `:s`.
/// - `$( $fmt_modifier )?`: An optional send modifier. It is either ` `
///   (absent), `%`, `?`, `#%`, or `#?`.
/// - `$fmt_str:expr $(, $fmt_args )* $(,)?`: a list of arguments passed to
///   `format_args!` stdlib macro. Similar arguments used in stdlib macros like:
///   `print!`, `write!`, `format!`. If the same-thread logger, the value will
///   be formatted without intermediate stringification. If the off-thread
///   logger is used, the value will be stringified on logging thread using
///   specified formatting.
///
/// Send modifiers are used to explicitly specify how the argument must be sent
/// to off-thread loggers.
/// Most common argument values without lifetimes will work will without any
/// send modifier specified.
/// Using some uncommon argument values and argument values with lifetimes may
/// fail to compile if proper send modifier is not specified.
/// All send modifier are no-op if the synchronous same-thread logger is used.
/// Valid send modifiers are:
/// - ` ` (absent): The value will be passed to the logger in a zero‑cost way
///   where possible using the **optimistic** trait bounds assumptions. The
///   value requires to implement [`IntoSendable`] trait.
/// - `:a`: The value is declared as already sendable. Wraps the value into
///   [`AlreadySendable`] and requires the value to implement `'static + Send`.
///   Use this if the value is `'static + Send` but the log facade failed to
///   recognize this.
/// - `:t`: The value is temporary and will be converted to a `'static + Send`
///   form using **pessimistic** trait-bound assumptions. Wraps the value into
///   [`Temporary`] and requires it to implement [`TemporaryIntoSendable`]. If
///   the value doesn't implement [`TemporaryIntoSendable`] you can use
///   [`SendableWith`] to explicitly specify the conversion.
/// - `:s`: The value will be stringified before sending if necessary. Wraps the
///   value into `Stringifiable`.
/// - `:r(..args)` passes multiple log arguments as a single `MixedArgs` tuple.
///   `:r` cannot be combined with send or format modifiers and can be used only
///   with a separately specified descriptor.
///
/// Format modifiers are used to specify the used value formatting.
/// Valid format modifiers are:
/// - ` ` (absent): The value is formatted with [`Display`] formatting, and
///   requires value to implement [`Display`] (subject to change in future
///   releases).
/// - `%`: wraps value expression into [`ViaDisplay`]. Requires value to
///   implement [`Display`].
/// - `#%`: wraps value expression into [`ViaAltDisplay`]. Requires value to
///   implement [`Display`].
/// - `?`: wraps value expression into [`ViaDebug`]. Requires value to implement
///   [`Debug`].
/// - `#?`: wraps value expression into [`ViaAltDebug`]. Requires value to
///   implement [`Debug`].
///
/// By default, the descriptor's key names are derived from the argument list:
/// `foo = expr` produces the key `"foo"`, a bare `expr` produces
/// `stringify!(expr)`, and `; arg1, arg2` produces `["arg1", "arg2"]`.
///
/// The `keys: &[...]` clause overrides this derivation and sets
/// [`Descriptor::param_names`] verbatim.
///
/// [`AlreadySendable`]: crate::AlreadySendable
/// [`Descriptor::param_names`]: crate::Descriptor::param_names
/// [`Display`]: ::core::fmt::Display
/// [`IntoSendable`]: crate::IntoSendable
/// [`Level`]: crate::Level
/// [`Logger`]: crate::Logger
/// [`LoggerExt::try_enable`]: crate::LoggerExt::try_enable
/// [`SendableWith`]: crate::SendableWith
/// [`Temporary`]: crate::Temporary
/// [`TemporaryIntoSendable`]: crate::TemporaryIntoSendable
/// [`ViaAltDebug`]: crate::ViaAltDebug
/// [`ViaAltDisplay`]: crate::ViaAltDisplay
/// [`ViaDebug`]: crate::ViaDebug
/// [`ViaDisplay`]: crate::ViaDisplay
#[macro_export]
macro_rules! log {
    ( $logger:expr $(, $($args:tt)* )? ) => {{
        let descriptor: &'static $crate::Descriptor =
            $crate::log_descriptor_ref!( $($($args)*)? );
        let logger = &mut *$logger;
        #[allow(clippy::allow_attributes, unreachable_code, reason = "macro-generated")]
        if let Some(context) = $crate::LoggerExt::try_enable(logger, descriptor) {
            $crate::LoggerExt::log(
                logger,
                descriptor,
                $crate::locate_and_parse_values!( $($($args)*)? ),
                context,
            );
        }
        $crate::LoggerExt::panic_if_critical(logger, descriptor);
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __log_fatal_impl {
    ( $logger:expr $(, $($args:tt)* )? ) => {{
        const DESCRIPTOR: &'static $crate::Descriptor =
            $crate::log_descriptor_ref!( $($($args)*)? );
        const { assert!(DESCRIPTOR.level.into_u8() == $crate::Level::Fatal.into_u8()) };
        let logger = &mut *$logger;
        #[allow(clippy::allow_attributes, unreachable_code, reason = "macro-generated")]
        if let Some(context) = $crate::LoggerExt::try_enable(logger, DESCRIPTOR) {
            $crate::LoggerExt::log(
                logger,
                DESCRIPTOR,
                $crate::locate_and_parse_values!( $($($args)*)? ),
                context,
            );
        }
        $crate::LoggerExt::flush_and_panic(logger, DESCRIPTOR);
    }};
}
