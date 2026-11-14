#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]
#![expect(clippy::unwrap_used, reason = "okay in tests")]

use std::sync::{Arc, Mutex};
use std::vec::Vec;

use polog::{
    ContextArgs, Descriptor, DisplayableMixedArgs, DynHandler, Handler, Level, LogArgs,
    SharedHandler, SpanId, SpanIdGenerator, SpanKind, log_descriptor_ref,
};
use polog_test_helpers::panic::catch_unwind_on_thread;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
struct Event {
    descriptor: &'static Descriptor,
    span_id: Option<SpanId>,
    formatted_args: Vec<String>,
}

#[derive(Clone, Debug)]
struct PanicLogger {
    events: Arc<Mutex<Vec<Event>>>,
    span_id_generator: Arc<SpanIdGenerator>,
    panic_on_bug: bool,
}

impl PanicLogger {
    #[inline]
    #[must_use]
    fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            span_id_generator: Arc::new(SpanIdGenerator::new()),
            panic_on_bug: false,
        }
    }

    #[inline]
    fn log_impl<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        span_id: Option<SpanId>,
    ) where
        Args: DisplayableMixedArgs<'a>,
    {
        let formatted_args = (0..args.num_args())
            .map(|n| {
                let mut string = String::new();
                args.display_fmt_nth(&mut string, n).unwrap().unwrap();
                string
            })
            .collect();
        self.events.lock().unwrap().push(Event {
            descriptor,
            span_id,
            formatted_args,
        });
    }
}

impl polog::BaseHandler for PanicLogger {
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        self.span_id_generator.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {}

    #[inline]
    fn flush_on_panic(&mut self) {}

    #[inline]
    fn should_panic_on_bug(
        &self,
        _descriptor: &'static Descriptor,
        _span_id: Option<SpanId>,
    ) -> bool {
        self.panic_on_bug
    }
}

#[expect(clippy::unimplemented, reason = "okay in tests")]
impl<Context> DynHandler<Context> for PanicLogger
where
    Context: ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _span_kind: SpanKind,
        _context: Context,
    ) {
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
        unimplemented!()
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
        unimplemented!()
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
        unimplemented!()
    }
}

impl<Context> Handler<Context> for PanicLogger
where
    Context: ContextArgs,
{
    type EnrichedContext = Context;

    #[inline]
    fn try_enable(
        &self,
        _descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        Some(context)
    }

    #[inline]
    fn log<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        _context: Context,
    ) where
        Args: LogArgs<'a>,
    {
        self.log_impl(descriptor, &args, span_id);
    }

    #[inline]
    fn enter_span<'a, Args>(
        &mut self,
        _descriptor: &'static Descriptor,
        _args: Args,
        _parent_span_id: Option<SpanId>,
        _span_id: SpanId,
        _span_kind: SpanKind,
        _context: Context,
    ) where
        Args: LogArgs<'a>,
    {
    }

    #[inline]
    fn log_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        _context: Self::EnrichedContext,
    ) where
        Args: polog::StaticLogArgs,
    {
        self.log_impl(descriptor, &args, span_id);
    }

    #[inline]
    fn enter_span_static<Args>(
        &mut self,
        _descriptor: &'static Descriptor,
        _args: Args,
        _parent_span_id: Option<SpanId>,
        _span_id: SpanId,
        _span_kind: SpanKind,
        _context: Self::EnrichedContext,
    ) where
        Args: polog::StaticLogArgs,
    {
    }
}

impl<Context> SharedHandler<Context> for PanicLogger where Context: ContextArgs {}

#[test]
fn test_panic_and_bug() {
    let logger = PanicLogger::new();

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        polog::fatal!(&mut logger, "fatal example"; f1 = 1, f2 = 2);
    });
    let err = result.unwrap_err();
    assert!(err.starts_with("FATAL at root span in panic at crates/polog/tests/panic.rs:"));
    assert!(err.ends_with(": fatal example"));

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        polog::bug!(&mut logger, "bug example"; b1 = 3, b2 = 4);
    });
    assert_eq!(result, Ok(()));

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        logger.panic_on_bug = true;
        polog::bug!(&mut logger, "bug example"; b1 = 3, b2 = 4);
    });
    let err = result.unwrap_err();
    assert!(err.starts_with("BUG   at root span in panic at crates/polog/tests/panic.rs:"));
    assert!(err.ends_with(": bug example"));
}

#[expect(clippy::too_many_lines, reason = "better to keep together")]
#[test]
fn test_panic_and_bug_assert() {
    let logger = PanicLogger::new();

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        let mut logger = polog::info_span!(&mut logger, "unnamed span");
        polog::bug_assert!(2 - 1 == 1, &mut logger);
        polog::bug_assert!("abc".len() == 2, &mut logger);
        polog::fatal_assert!(2 - 1 == 1, &mut logger);
        polog::fatal_assert!("abc".len() == 2, &mut logger);
    });
    let err = result.unwrap_err();
    assert!(err.starts_with("FATAL at span 1 in panic at crates/polog/tests/panic.rs:"));
    assert!(err.ends_with(": Assertion `\"abc\".len() == 2` failed"));

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        let mut logger = polog::info_span!(&mut logger, "unnamed span");
        polog::bug_assert!(2 - 1 == 1, &mut logger, "first assert"; a1 = 1, a2 = 2);
        polog::bug_assert!("abc".len() == 2, &mut logger, "second assert"; a3 = 3, a4 = 4);
        polog::fatal_assert!(2 - 1 == 1, &mut logger, "third assert"; a1 = 1, a2 = 2);
        polog::fatal_assert!("abc".len() == 2, &mut logger, "fourth assert"; a3 = 3, a4 = 4);
    });
    let err = result.unwrap_err();
    assert!(err.starts_with("FATAL at span 2 in panic at crates/polog/tests/panic.rs:"));
    assert!(err.ends_with(": fourth assert"));

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        let mut logger = polog::info_span!(&mut logger, "unnamed span");
        polog::bug_assert!(2 - 1 == 1, &mut logger, "first assert"; a1 = 1, a2 = 2);
        polog::bug_assert!("abc".len() == 2, &mut logger, "second assert"; a3 = 3, a4 = 4);
    });
    assert_eq!(result, Ok(()));

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        logger.panic_on_bug = true;
        let mut logger = polog::info_span!(&mut logger, "unnamed span");
        polog::bug_assert!(2 - 1 == 1, &mut logger, "first assert"; a1 = 1, a2 = 2);
        polog::bug_assert!("abc".len() == 2, &mut logger, "second assert"; a3 = 3, a4 = 4);
    });
    let err = result.unwrap_err();
    assert!(err.starts_with("BUG   at span 4 in panic at crates/polog/tests/panic.rs:"));
    assert!(err.ends_with(": second assert"));

    assert!(matches!(
        logger.events.lock().unwrap().as_slice(),
        &[
            Event {
                descriptor: Descriptor {
                    module_path: "panic",
                    target: None,
                    message: "Assertion `\"abc\".len() == 2` failed",
                    param_names: [],
                    level: Level::Bug,
                    ..
                },
                span_id: span_id_11,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "Assertion `\"abc\".len() == 2` failed",
                    param_names: [],
                    level: Level::Fatal,
                    ..
                },
                span_id: span_id_12,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "second assert",
                    param_names: ["a3", "a4"],
                    level: Level::Bug,
                    ..
                },
                span_id: span_id_21,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "fourth assert",
                    param_names: ["a3", "a4"],
                    level: Level::Fatal,
                    ..
                },
                span_id: span_id_22,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "second assert",
                    param_names: ["a3", "a4"],
                    level: Level::Bug,
                    ..
                },
                span_id: span_id_31,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "second assert",
                    param_names: ["a3", "a4"],
                    level: Level::Bug,
                    ..
                },
                span_id: span_id_41,
                ..
            }
        ] if span_id_11.unwrap().0.get() == 1
            && span_id_12.unwrap().0.get() == 1
            && span_id_21.unwrap().0.get() == 2
            && span_id_22.unwrap().0.get() == 2
            && span_id_31.unwrap().0.get() == 3
            && span_id_41.unwrap().0.get() == 4
    ));
}

#[expect(clippy::too_many_lines, reason = "better to keep together")]
#[test]
fn test_panic_and_bug_assert_eq() {
    let logger = PanicLogger::new();

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        let mut logger = polog::info_span!(&mut logger, "unnamed span");
        polog::bug_assert_eq!(2 - 1, 1, &mut logger);
        polog::bug_assert_eq!("abc".len(), 2, &mut logger);
        polog::fatal_assert_eq!(2 - 1, 1, &mut logger);
        polog::fatal_assert_eq!("abc".len(), 2, &mut logger);
    });
    let err = result.unwrap_err();
    assert!(err.starts_with("FATAL at span 1 in panic at crates/polog/tests/panic.rs:"));
    assert!(err.ends_with(": Assertion `\"abc\".len() == 2` failed"));

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        let mut logger = polog::info_span!(&mut logger, "unnamed span");
        polog::bug_assert_eq!(2 - 1, 1, &mut logger, "first assert"; a1 = 1, a2 = 2);
        polog::bug_assert_eq!("abc".len(), 2, &mut logger, "second assert"; a3 = 3, a4 = 4);
        polog::fatal_assert_eq!(2 - 1, 1, &mut logger, "third assert"; a1 = 1, a2 = 2);
        polog::fatal_assert_eq!("abc".len(), 2, &mut logger, "fourth assert"; a3 = 3, a4 = 4);
    });
    let err = result.unwrap_err();
    assert!(err.starts_with("FATAL at span 2 in panic at crates/polog/tests/panic.rs:"));
    assert!(err.ends_with(": fourth assert"));

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        let mut logger = polog::info_span!(&mut logger, "unnamed span");
        polog::bug_assert_eq!(2 - 1, 1, &mut logger, "first assert"; a1 = 1, a2 = 2);
        polog::bug_assert_eq!("abc".len(), 2, &mut logger, "second assert"; a3 = 3, a4 = 4);
    });
    assert_eq!(result, Ok(()));

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        logger.panic_on_bug = true;
        let mut logger = polog::info_span!(&mut logger, "unnamed span");
        polog::bug_assert_eq!(2 - 1, 1, &mut logger, "first assert"; a1 = 1, a2 = 2);
        polog::bug_assert_eq!("abc".len(), 2, &mut logger, "second assert"; a3 = 3, a4 = 4);
    });
    let err = result.unwrap_err();
    assert!(err.starts_with("BUG   at span 4 in panic at crates/polog/tests/panic.rs:"));
    assert!(err.ends_with(": second assert"));

    assert!(matches!(
        logger.events.lock().unwrap().as_slice(),
        &[
            Event {
                descriptor: Descriptor {
                    module_path: "panic",
                    target: None,
                    message: "Assertion `\"abc\".len() == 2` failed",
                    param_names: ["\"abc\".len()", "2"],
                    level: Level::Bug,
                    ..
                },
                span_id: span_id_11,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "Assertion `\"abc\".len() == 2` failed",
                    param_names: ["\"abc\".len()", "2"],
                    level: Level::Fatal,
                    ..
                },
                span_id: span_id_12,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "second assert",
                    param_names: ["a3", "a4"],
                    level: Level::Bug,
                    ..
                },
                span_id: span_id_21,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "fourth assert",
                    param_names: ["a3", "a4"],
                    level: Level::Fatal,
                    ..
                },
                span_id: span_id_22,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "second assert",
                    param_names: ["a3", "a4"],
                    level: Level::Bug,
                    ..
                },
                span_id: span_id_31,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "second assert",
                    param_names: ["a3", "a4"],
                    level: Level::Bug,
                    ..
                },
                span_id: span_id_41,
                ..
            }
        ] if span_id_11.unwrap().0.get() == 1
            && span_id_12.unwrap().0.get() == 1
            && span_id_21.unwrap().0.get() == 2
            && span_id_22.unwrap().0.get() == 2
            && span_id_31.unwrap().0.get() == 3
            && span_id_41.unwrap().0.get() == 4
    ));
}

#[expect(clippy::too_many_lines, reason = "better to keep together")]
#[test]
fn test_panic_and_bug_assert_ne() {
    let logger = PanicLogger::new();

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        let mut logger = polog::info_span!(&mut logger, "unnamed span");
        polog::bug_assert_ne!(3 - 1, 1, &mut logger);
        polog::bug_assert_ne!("abc".len(), 3, &mut logger);
        polog::fatal_assert_ne!(3 - 1, 1, &mut logger);
        polog::fatal_assert_ne!("abc".len(), 3, &mut logger);
    });
    let err = result.unwrap_err();
    assert!(err.starts_with("FATAL at span 1 in panic at crates/polog/tests/panic.rs:"));
    assert!(err.ends_with(": Assertion `\"abc\".len() != 3` failed"));

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        let mut logger = polog::info_span!(&mut logger, "unnamed span");
        polog::bug_assert_ne!(3 - 1, 1, &mut logger, "first assert"; a1 = 1, a2 = 2);
        polog::bug_assert_ne!("abc".len(), 3, &mut logger, "second assert"; a3 = 3, a4 = 4);
        polog::fatal_assert_ne!(3 - 1, 1, &mut logger, "third assert"; a1 = 1, a2 = 2);
        polog::fatal_assert_ne!("abc".len(), 3, &mut logger, "fourth assert"; a3 = 3, a4 = 4);
    });
    let err = result.unwrap_err();
    assert!(err.starts_with("FATAL at span 2 in panic at crates/polog/tests/panic.rs:"));
    assert!(err.ends_with(": fourth assert"));

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        let mut logger = polog::info_span!(&mut logger, "unnamed span");
        polog::bug_assert_ne!(3 - 1, 1, &mut logger, "first assert"; a1 = 1, a2 = 2);
        polog::bug_assert_ne!("abc".len(), 3, &mut logger, "second assert"; a3 = 3, a4 = 4);
    });
    assert_eq!(result, Ok(()));

    let result = catch_unwind_on_thread(&logger, |mut logger| {
        logger.panic_on_bug = true;
        let mut logger = polog::info_span!(&mut logger, "unnamed span");
        polog::bug_assert_ne!(3 - 1, 1, &mut logger, "first assert"; a1 = 1, a2 = 2);
        polog::bug_assert_ne!("abc".len(), 3, &mut logger, "second assert"; a3 = 3, a4 = 4);
    });
    let err = result.unwrap_err();
    assert!(err.starts_with("BUG   at span 4 in panic at crates/polog/tests/panic.rs:"));
    assert!(err.ends_with(": second assert"));

    assert!(matches!(
        logger.events.lock().unwrap().as_slice(),
        &[
            Event {
                descriptor: Descriptor {
                    module_path: "panic",
                    target: None,
                    message: "Assertion `\"abc\".len() != 3` failed",
                    param_names: ["\"abc\".len()", "3"],
                    level: Level::Bug,
                    ..
                },
                span_id: span_id_11,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "Assertion `\"abc\".len() != 3` failed",
                    param_names: ["\"abc\".len()", "3"],
                    level: Level::Fatal,
                    ..
                },
                span_id: span_id_12,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "second assert",
                    param_names: ["a3", "a4"],
                    level: Level::Bug,
                    ..
                },
                span_id: span_id_21,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "fourth assert",
                    param_names: ["a3", "a4"],
                    level: Level::Fatal,
                    ..
                },
                span_id: span_id_22,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "second assert",
                    param_names: ["a3", "a4"],
                    level: Level::Bug,
                    ..
                },
                span_id: span_id_31,
                ..
            },
            Event {
                descriptor: Descriptor {
                    message: "second assert",
                    param_names: ["a3", "a4"],
                    level: Level::Bug,
                    ..
                },
                span_id: span_id_41,
                ..
            }
        ] if span_id_11.unwrap().0.get() == 1
            && span_id_12.unwrap().0.get() == 1
            && span_id_21.unwrap().0.get() == 2
            && span_id_22.unwrap().0.get() == 2
            && span_id_31.unwrap().0.get() == 3
            && span_id_41.unwrap().0.get() == 4
    ));
}

#[test]
fn test_panic_levels() {
    const FATAL_DESC: &Descriptor = log_descriptor_ref!(Level::Fatal, "Message");
    const BUG_DESC: &Descriptor = log_descriptor_ref!(Level::Bug, "Message");
    const ERROR_DESC: &Descriptor = log_descriptor_ref!(Level::Error, "Message");
    const WARN_DESC: &Descriptor = log_descriptor_ref!(Level::Warn, "Message");
    const INFO_DESC: &Descriptor = log_descriptor_ref!(Level::Info, "Message");
    const DEBUG_DESC: &Descriptor = log_descriptor_ref!(Level::Debug, "Message");
    const TRACE_DESC: &Descriptor = log_descriptor_ref!(Level::Trace, "Message");

    let logger = PanicLogger::new();

    for (desc, prefix) in [
        (FATAL_DESC, "FATAL at root span in panic at"),
        (BUG_DESC, "BUG   at root span in panic at"),
        (ERROR_DESC, "ERROR at root span in panic at"),
        (WARN_DESC, "WARN  at root span in panic at"),
        (INFO_DESC, "INFO  at root span in panic at"),
        (DEBUG_DESC, "DEBUG at root span in panic at"),
        (TRACE_DESC, "TRACE at root span in panic at"),
    ] {
        let result = catch_unwind_on_thread(&logger, |_logger| {
            polog::default_log_related_panic(desc, None)
        });
        assert!(result.err().unwrap().starts_with(prefix));
    }
}
