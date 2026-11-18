/// Constructs a span at the specified level.
///
/// # Syntax
///
/// Accepted forms include:
/// - `span!( $logger:expr, $(:dyn)? $level:expr, $message:expr $(, $arg )*
///   $(,)? )`,
/// - `span!( $logger:expr, descriptor: $descriptor:expr $(,)? )`,
/// - `span!( $logger:expr, descriptor_ref: $descriptor_ref:expr $(,)? )`.
///
/// This macro syntax is similar to the `log!` macro.
/// For the detailed syntax documentation, please see the [`log!`] macro
/// documentation.
///
/// # Examples
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let logger = polog::span!(&mut logger, polog::SpanLevel::Info, "Message on Info level");
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
/// let logger = polog::span!(
///     &mut logger, polog::SpanLevel::Debug, "Message";
///     arg1 = "value",
///     arg2 = :t non_static_str_ref,
///     arg3 = ? (123, true, 'a')
/// );
/// ```
///
/// # Argument evaluation
///
/// The argument list is constructed only when
/// [`LoggerExt::try_enable`] returns `Some` for this span.
/// When the span is disabled, a transparent borrowing span is returned that
/// propagates the parent [`SpanId`] to events and child spans without
/// evaluating the arguments or emitting any enter/exit events.
///
/// [`log!`]: crate::log!
/// [`LoggerExt::try_enable`]: crate::LoggerExt::try_enable
/// [`SpanId`]: crate::SpanId
#[macro_export]
macro_rules! span {
    ( $logger:expr $(, $($args:tt)* )? ) => {{
        let descriptor: &'static $crate::Descriptor =
            $crate::span_descriptor_ref!( $($($args)*)? );
        let logger = &mut *$logger;
        #[allow(clippy::allow_attributes, unreachable_code, reason = "macro-generated")]
        if let Some(context) = $crate::LoggerExt::try_enable(logger, descriptor) {
            $crate::LoggerExt::borrowing_span(
                logger,
                descriptor,
                $crate::locate_and_parse_values!( $($($args)*)? ),
                context,
            )
        } else {
            $crate::LoggerExt::transparent_borrowing_span(logger)
        }
    }};
}

/// Constructs a span that owns a cloned handler at the specified level.
///
/// The macro creates an [`OwningSpan`] that emits [`DynHandler::exit_span`] on
/// drop.
/// Use it when the span must be stored in a struct or moved into another thread
/// or task.
///
/// The span is always reported as
/// [`SpanKind::Detached`], because it owns a cloned handler and the original
/// logger remains available to the caller. Use [`span!`] for a
/// [`SpanKind::Nested`] span whose lifetime the borrow checker ties to the
/// parent.
///
/// # Syntax
///
/// Accepted forms include:
/// - `owning_span!( $logger:expr, $(:dyn)? $level:expr, $message:expr $(, $arg
///   )* $(,)? )`,
/// - `owning_span!( $logger:expr, descriptor: $descriptor:expr $(,)? )`,
/// - `owning_span!( $logger:expr, descriptor_ref: $descriptor_ref:expr $(,)? )`
///
/// This macro syntax is similar to the [`log!`] macro.
/// For the detailed syntax documentation, please see the [`log!`] macro
/// documentation.
///
/// # Examples
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let logger = polog::owning_span!(&logger, polog::SpanLevel::Info, "Message on Info level");
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
/// let logger = polog::owning_span!(
///     &logger, polog::SpanLevel::Debug, "Message";
///     arg1 = "value",
///     arg2 = :t non_static_str_ref,
///     arg3 = ? (123, true, 'a')
/// );
/// ```
///
/// # Argument evaluation
///
/// The argument list is constructed only when
/// [`LoggerExt::try_enable`] returns `Some` for this span.
/// When the span is disabled, a transparent owning span is returned that
/// propagates the parent [`SpanId`] to events and child spans without
/// evaluating the arguments or emitting any enter/exit events.
///
/// [`log!`]: crate::log!
/// [`span!`]: crate::span!
/// [`SpanId`]: crate::SpanId
/// [`OwningSpan`]: crate::OwningSpan
/// [`SpanKind::Nested`]: crate::SpanKind::Nested
/// [`SpanKind::Detached`]: crate::SpanKind::Detached
/// [`DynHandler::exit_span`]: crate::DynHandler::exit_span
/// [`LoggerExt::try_enable`]: crate::LoggerExt::try_enable
#[macro_export]
macro_rules! owning_span {
    ( $logger:expr $(, $($args:tt)* )? ) => {{
        let descriptor: &'static $crate::Descriptor =
            $crate::span_descriptor_ref!( $($($args)*)? );
        let logger = &*$logger;
        #[allow(clippy::allow_attributes, unreachable_code, reason = "macro-generated")]
        if let Some(context) = $crate::LoggerExt::try_enable(logger, descriptor) {
            $crate::LoggerExt::owning_span(
                logger,
                descriptor,
                $crate::locate_and_parse_values!( $($($args)*)? ),
                context,
            )
        } else {
            $crate::LoggerExt::transparent_owning_span(logger)
        }
    }};
}

/// Constructs a deferred span at the specified level.
///
/// The macro creates a [`DeferredSpan`] that emits no events on creation.
/// The span is materialized by calling
/// [`DeferredSpan::enter`] or by passing the deferred span to
/// [`PollInstrument::poll_instrument`](crate::PollInstrument::poll_instrument).
///
/// # Syntax
///
/// Accepted forms include:
/// - `deferred_span!( $logger:expr, $(:dyn)? $level:expr, $message:expr $(,
///   $arg )* $(,)? )`,
/// - `deferred_span!( $logger:expr, descriptor: $descriptor:expr $(,)? )`,
/// - `deferred_span!( $logger:expr, descriptor_ref: $descriptor_ref:expr $(,)?
///   )`.
///
/// This macro syntax is similar to the `log!` macro.
/// For the detailed syntax documentation, please see the [`log!`] macro
/// documentation.
///
/// # Examples
///
/// ```rust
/// # use polog_test_helpers::accept_all::AcceptAll;
/// # let mut logger = AcceptAll; // Enables args construction.
/// # {
/// let mut logger = polog::Discard;
/// # }
/// let logger = polog::deferred_span!(&logger, polog::SpanLevel::Info, "Message on Info level");
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
/// let logger = polog::deferred_span!(
///     &logger, polog::SpanLevel::Debug, "Message";
///     arg1 = "value",
///     arg2 = :t non_static_str_ref,
///     arg3 = ? (123, true, 'a')
/// );
/// ```
///
/// # Argument capture
///
/// The macro expands to a `move` closure that owns the argument expressions.
/// Local variables referenced by the arguments are moved into the closure at
/// macro expansion.
/// The argument expressions themselves — function calls, method calls,
/// arithmetic — are evaluated lazily when the span is entered.
///
/// The enablement check ([`Handler::try_enable`]) is performed at
/// [`DeferredSpan::enter`] time, not at macro expansion.
/// Argument expressions are likewise evaluated only during `enter`, and only
/// when `try_enable` returns `Some`: a disabled deferred span never evaluates
/// its arguments.
/// The `move` capture described above happens at macro expansion regardless,
/// so a non-`Copy` local referenced by the argument list is moved into the
/// closure even if the span is later found disabled.
///
/// # Span kind
///
/// The materialized span is always reported as
/// [`SpanKind::Detached`], because the [`DeferredSpan`] owns a cloned handler
/// and cannot be proven to be contained within its parent's lifetime.
/// Use [`LoggerExt::borrowing_span`] for a
/// [`SpanKind::Nested`](crate::SpanKind::Nested) span.
///
/// [`log!`]: crate::log!
/// [`DeferredSpan`]: crate::DeferredSpan
/// [`LoggerExt::borrowing_span`]: crate::LoggerExt::borrowing_span
/// [`DeferredSpan::enter`]: crate::DeferredSpan::enter
/// [`Handler::try_enable`]: crate::Handler::try_enable
/// [`SpanKind::Detached`]: crate::SpanKind::Detached
#[macro_export]
macro_rules! deferred_span {
    ( $logger:expr $(, $($args:tt)* )? ) => {{
        let descriptor: &'static $crate::Descriptor =
            $crate::span_descriptor_ref!( $($($args)*)? );
        let logger = &*$logger;
        $crate::LoggerExt::deferred_span(
            logger,
            descriptor,
            move || $crate::locate_and_parse_values!( $($($args)*)? ),
        )
    }};
}
