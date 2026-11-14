#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]
#![expect(clippy::unimplemented, reason = "okay in tests")]
#![expect(clippy::unwrap_used, reason = "okay in tests")]

use core::any::type_name;
use std::collections::VecDeque;
use std::sync::Arc;

use polog::{ContextArgs, SharedHandler};
use polog_test_helpers::producers;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
enum Event {
    Log {
        info: EventInfo,
        span_id: Option<polog::SpanId>,
    },
    Enter {
        info: EventInfo,
        parent_span_id: Option<polog::SpanId>,
        span_id: polog::SpanId,
        span_kind: polog::SpanKind,
    },
    Exit {
        span_id: polog::SpanId,
    },
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
struct EventInfo {
    pub location: (&'static str, u32, u32),
    pub module_path: &'static str,
    pub target: Option<&'static str>,
    pub level: polog::Level,
    pub message: &'static str,
    pub param_names: &'static [&'static str],
    pub raw_args_type: String,
    pub formatted_args: Vec<String>,
}

#[derive(Clone, Debug)]
struct SameThreadLogger(VecDeque<Event>, Arc<polog::SpanIdGenerator>);

impl SameThreadLogger {
    #[inline]
    #[must_use]
    fn new() -> Self {
        Self(VecDeque::new(), Arc::new(polog::SpanIdGenerator::new()))
    }
}

impl Default for SameThreadLogger {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl polog::BaseHandler for SameThreadLogger {
    #[inline]
    fn generate_span_id(&self) -> polog::SpanId {
        self.1.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {}

    #[inline]
    fn flush_on_panic(&mut self) {}
}

impl<Context> polog::DynHandler<Context> for SameThreadLogger
where
    Context: ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        _descriptor: &'static polog::Descriptor,
        span_id: polog::SpanId,
        _span_kind: polog::SpanKind,
        _context: Context,
    ) {
        self.0.push_back(Event::Exit { span_id });
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        _descriptor: &'static polog::Descriptor,
        _span_id: polog::SpanId,
        _context: Context,
    ) {
        unimplemented!()
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        _descriptor: &'static polog::Descriptor,
        _span_id: polog::SpanId,
        _context: Context,
    ) {
        unimplemented!()
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        _descriptor: &'static polog::Descriptor,
        _span_id: polog::SpanId,
        _context: Context,
    ) {
        unimplemented!()
    }
}

impl<Context> polog::Handler<Context> for SameThreadLogger
where
    Context: ContextArgs,
{
    type EnrichedContext = Context;

    #[inline]
    fn try_enable(
        &self,
        _descriptor: &'static polog::Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        Some(context)
    }

    #[inline]
    fn log<'a, Args>(
        &mut self,
        descriptor: &'static polog::Descriptor,
        args: Args,
        span_id: Option<polog::SpanId>,
        _context: Context,
    ) where
        Args: polog::LogArgs<'a>,
    {
        let info = convert_args_to_event_into(descriptor, &args);
        self.0.push_back(Event::Log { info, span_id });
    }

    #[inline]
    fn enter_span<'a, Args>(
        &mut self,
        descriptor: &'static polog::Descriptor,
        args: Args,
        parent_span_id: Option<polog::SpanId>,
        span_id: polog::SpanId,
        span_kind: polog::SpanKind,
        _context: Context,
    ) where
        Args: polog::LogArgs<'a>,
    {
        let info = convert_args_to_event_into(descriptor, &args);
        self.0.push_back(Event::Enter {
            info,
            parent_span_id,
            span_id,
            span_kind,
        });
    }

    #[inline]
    fn log_static<Args>(
        &mut self,
        descriptor: &'static polog::Descriptor,
        args: Args,
        span_id: Option<polog::SpanId>,
        _context: Self::EnrichedContext,
    ) where
        Args: polog::StaticLogArgs,
    {
        let info = convert_args_to_event_into(descriptor, &args);
        self.0.push_back(Event::Log { info, span_id });
    }

    #[inline]
    fn enter_span_static<Args>(
        &mut self,
        descriptor: &'static polog::Descriptor,
        args: Args,
        parent_span_id: Option<polog::SpanId>,
        span_id: polog::SpanId,
        span_kind: polog::SpanKind,
        _context: Self::EnrichedContext,
    ) where
        Args: polog::StaticLogArgs,
    {
        let info = convert_args_to_event_into(descriptor, &args);
        self.0.push_back(Event::Enter {
            info,
            parent_span_id,
            span_id,
            span_kind,
        });
    }
}

impl<Context> SharedHandler<Context> for SameThreadLogger where Context: ContextArgs {}

fn convert_args_to_event_into<'a, Args>(
    descriptor: &'static polog::Descriptor,
    args: &Args,
) -> EventInfo
where
    Args: polog::DisplayableMixedArgs<'a>,
{
    let polog::Descriptor {
        kind: _,
        location,
        module_path,
        target,
        level,
        message,
        param_names,
    } = descriptor;
    let location = (location.file(), location.line(), location.column());
    let formatted_args = (0..args.num_args())
        .map(|n| {
            let mut string = String::new();
            args.display_fmt_nth(&mut string, n).unwrap().unwrap();
            string
        })
        .collect();
    let raw_args_type = type_name::<Args>().replace("polog::args::Arg", "@");
    EventInfo {
        location,
        module_path,
        target: *target,
        level: *level,
        message,
        param_names,
        raw_args_type,
        formatted_args,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "better to keep within a single function"
)]
#[test]
fn test_same_thread_log() {
    let mut logger = SameThreadLogger::new();
    let file = producers::FILE;
    let base_line = producers::PRODUCE_LOGS_BASE_LINE;
    producers::produce_logs(&mut logger, polog::Level::Error);
    let mut entries = logger.0.into_iter();

    assert_eq!(
        entries.next().unwrap(),
        Event::Log {
            info: EventInfo {
                location: (file, base_line + 1, 5),
                module_path: "polog_test_helpers::producers",
                target: None,
                level: polog::Level::Error,
                message: "Oh no!",
                param_names: &["not", "STATIC_VALUE"],
                raw_args_type: "(@<&str>, @<&str>)".to_owned(),
                formatted_args: vec!["again".to_owned(), "static".to_owned()],
            },
            span_id: None,
        }
    );

    assert_eq!(
        entries.next().unwrap(),
        Event::Log {
            info: EventInfo {
                location: (file, base_line + 2, 5),
                module_path: "polog_test_helpers::producers",
                target: None,
                level: polog::Level::Warn,
                message: "Sigh!",
                param_names: &["temp"],
                raw_args_type: "(@<polog::send::wrappers::Temporary<&str>>,)".to_owned(),
                formatted_args: vec!["l1".to_owned()],
            },
            span_id: None,
        }
    );

    assert_eq!(
        entries.next().unwrap(),
        Event::Log {
            info: EventInfo {
                location: (file, base_line + 3, 5),
                module_path: "polog_test_helpers::producers",
                target: None,
                level: polog::Level::Info,
                message: "Ok!",
                param_names: &["number", "STATIC_VALUE"],
                raw_args_type: "(@<i32>, @<polog::fmt::wrappers::ViaDebug<&str>>)".to_owned(),
                formatted_args: vec!["123".to_owned(), "\"static\"".to_owned()],
            },
            span_id: None,
        }
    );

    assert_eq!(
        entries.next().unwrap(),
        Event::Log {
            info: EventInfo {
                location: (file, base_line + 4, 5),
                module_path: "polog_test_helpers::producers",
                target: None,
                level: polog::Level::Debug,
                message: "I see!",
                param_names: &["display", "debug"],
                raw_args_type: "(@<polog::fmt::wrappers::ViaDisplay<i32>>, \
                                @<polog::fmt::wrappers::ViaAltDebug<&[i32; 3]>>)"
                    .to_owned(),
                formatted_args: vec!["123".to_owned(), "[\n    1,\n    2,\n    3,\n]".to_owned()],
            },
            span_id: None,
        }
    );

    assert_eq!(
        entries.next().unwrap(),
        Event::Log {
            info: EventInfo {
                location: (file, base_line + 5, 5),
                module_path: "polog_test_helpers::producers",
                target: Some("custom_target"),
                level: polog::Level::Trace,
                message: "Too much!",
                param_names: &[
                    "custom1", "custom2", "number", "complex", "arg5", "arg6", "arg7", "arg8",
                    "arg9", "arg10", "arg11", "arg12", "arg13", "arg14", "arg15", "arg16",
                ],
                raw_args_type: "(\
                    (\
                        @<polog::send::wrappers::AlreadySendable<polog::fmt::wrappers::ViaDebug<\
                            polog_test_helpers::producers::produce_logs::Custom<i32>\
                        >>>, \
                        @<polog::send::wrappers::Stringifiable<polog::fmt::wrappers::ViaDebug<\
                            polog_test_helpers::producers::produce_logs::Custom<&str>\
                        >>>, \
                        @<f64>, \
                        @<polog::send::wrappers::Stringifiable<core::fmt::Arguments<'_>>>, \
                        @<i32>, @<i32>, @<i32>, @<i32>, @<i32>, @<i32>, @<i32>, @<i32>\
                    ), \
                    @<i32>, @<i32>, @<i32>, @<i32>\
                )"
                .to_owned(),
                formatted_args: vec![
                    "Custom(1, 2, 3, 4, 5, 6)".to_owned(),
                    "Custom(\"l1\", \"l2\", \"l1\", \"l2\", \"l1\", \"l2\")".to_owned(),
                    "123.123".to_owned(),
                    "234-007b-3.45456E2-0567-l1".to_owned(),
                    "55".to_owned(),
                    "66".to_owned(),
                    "77".to_owned(),
                    "88".to_owned(),
                    "99".to_owned(),
                    "1010".to_owned(),
                    "1111".to_owned(),
                    "1212".to_owned(),
                    "1313".to_owned(),
                    "1414".to_owned(),
                    "1515".to_owned(),
                    "1616".to_owned(),
                ],
            },
            span_id: None,
        }
    );

    assert!(matches!(
        entries.next().unwrap(),
        Event::Log {
            info: EventInfo {
                level: polog::Level::Error,
                message: "Custom level.",
                param_names: &["arg1"],
                ..
            },
            span_id: None
        }
    ));

    assert!(matches!(
        entries.next().unwrap(),
        Event::Log {
            info: EventInfo {
                level: polog::Level::Error,
                target: Some("custom_target"),
                message: "Custom level.",
                param_names: &["arg1"],
                ..
            },
            span_id: None
        }
    ));

    assert!(matches!(
        entries.next().unwrap(),
        Event::Log {
            info: EventInfo {
                level: polog::Level::Debug,
                target: Some("another_target"),
                message: "Custom description.",
                param_names: &["arg2", "arg3"],
                ..
            },
            span_id: None
        }
    ));

    assert_eq!(entries.next(), None);
}

#[cfg(feature = "alloc")]
#[test]
fn test_same_thread_span() {
    let mut logger = SameThreadLogger::new();

    producers::produce_spanned_logs(&mut logger);
    let mut entries = logger.0.into_iter();

    assert!(matches!(
        entries.next().unwrap(),
        Event::Log {
            info: EventInfo {
                level: polog::Level::Error,
                message: "Some error!",
                ..
            },
            span_id: None
        }
    ));

    assert!(matches!(
        entries.next().unwrap(),
        Event::Enter {
            info: EventInfo {
                level: polog::Level::Info,
                message: "Info span.",
                ..
            },
            parent_span_id: None,
            span_id,
            span_kind: polog::SpanKind::Nested,
        } if span_id.0.get() == 1
    ));

    assert!(matches!(
        entries.next().unwrap(),
        Event::Log {
            info: EventInfo {
                level: polog::Level::Warn,
                message: "Some warn!",
                ..
            },
            span_id,
        } if span_id.unwrap().0.get() == 1
    ));

    // Note that the detached spans are recorded into a cloned logger internal
    // `VecDeque` and are forgotten on clones drop.
    // Detached span logs are checked in `same_thread` tests.

    assert!(matches!(
        entries.next().unwrap(),
        Event::Enter {
            info: EventInfo {
                level: polog::Level::Info,
                message: "First scoped span.",
                ..
            },
            parent_span_id,
            span_id,
            span_kind: polog::SpanKind::Nested,
        } if parent_span_id.unwrap().0.get() == 1 && span_id.0.get() == 6
    ));

    assert!(matches!(
        entries.next().unwrap(),
        Event::Enter {
            info: EventInfo {
                level: polog::Level::Debug,
                message: "Third in-thread span.",
                ..
            },
            parent_span_id,
            span_id,
            span_kind: polog::SpanKind::Nested,
        } if parent_span_id.unwrap().0.get() == 6 && span_id.0.get() == 7
    ));

    assert!(matches!(
        entries.next().unwrap(),
        Event::Log {
            info: EventInfo {
                level: polog::Level::Trace,
                message: "Different trace!",
                ..
            },
            span_id,
        } if span_id.unwrap().0.get() == 7
    ));

    assert!(matches!(
        entries.next().unwrap(),
        Event::Exit {
            span_id,
        } if span_id.0.get() == 7
    ));

    assert!(matches!(
        entries.next().unwrap(),
        Event::Exit {
            span_id,
        } if span_id.0.get() == 6
    ));

    assert!(matches!(
        entries.next().unwrap(),
        Event::Exit {
            span_id,
        } if span_id.0.get() == 1
    ));

    assert_eq!(entries.next(), None);
}
