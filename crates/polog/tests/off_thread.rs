#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]
#![expect(clippy::unwrap_used, reason = "okay in tests")]

use core::any::type_name;
use std::sync::Arc;

use polog::{ContextArgs, DynDisplayableMixedArgs, Logger, PollInstrument, SharedHandler};
use polog_test_helpers::producers;

enum RawEvent {
    Log {
        info: RawEventInfo,
        span_id: Option<polog::SpanId>,
    },
    Enter {
        info: RawEventInfo,
        parent_span_id: Option<polog::SpanId>,
        span_id: polog::SpanId,
        span_kind: polog::SpanKind,
    },
    Exit {
        span_id: polog::SpanId,
    },
    EnterPoll {
        span_id: polog::SpanId,
    },
    PendingPoll {
        span_id: polog::SpanId,
    },
    ReadyPoll {
        span_id: polog::SpanId,
    },
}

struct RawEventInfo {
    pub descriptor: &'static polog::Descriptor,
    pub raw_args_type: String,
    pub raw_args_sendable_type: String,
    pub args: Box<dyn 'static + Send + DynDisplayableMixedArgs<'static>>,
}

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
    EnterPoll {
        span_id: polog::SpanId,
    },
    PendingPoll {
        span_id: polog::SpanId,
    },
    ReadyPoll {
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
    pub raw_args_sendable_type: String,
    pub formatted_args: Vec<String>,
}

impl From<RawEventInfo> for EventInfo {
    fn from(
        RawEventInfo {
            descriptor,
            raw_args_type,
            raw_args_sendable_type,
            args,
        }: RawEventInfo,
    ) -> Self {
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
                args.dyn_display_fmt_nth(&mut string, n).unwrap().unwrap();
                string
            })
            .collect();
        Self {
            location,
            module_path,
            target: *target,
            level: *level,
            message,
            param_names,
            raw_args_type,
            raw_args_sendable_type,
            formatted_args,
        }
    }
}

impl From<RawEvent> for Event {
    #[inline]
    fn from(data: RawEvent) -> Self {
        match data {
            RawEvent::Log { info, span_id } => Self::Log {
                info: info.into(),
                span_id,
            },
            RawEvent::Enter {
                info,
                parent_span_id,
                span_id,
                span_kind,
            } => Self::Enter {
                info: info.into(),
                parent_span_id,
                span_id,
                span_kind,
            },
            RawEvent::Exit { span_id } => Self::Exit { span_id },
            RawEvent::EnterPoll { span_id } => Self::EnterPoll { span_id },
            RawEvent::PendingPoll { span_id } => Self::PendingPoll { span_id },
            RawEvent::ReadyPoll { span_id } => Self::ReadyPoll { span_id },
        }
    }
}

#[derive(Clone, Debug)]
struct OffThreadLogger(
    tokio::sync::mpsc::UnboundedSender<RawEvent>,
    Arc<polog::SpanIdGenerator>,
);

impl OffThreadLogger {
    #[inline]
    #[must_use]
    fn new(tx: tokio::sync::mpsc::UnboundedSender<RawEvent>) -> Self {
        Self(tx, Arc::new(polog::SpanIdGenerator::new()))
    }
}

impl polog::BaseHandler for OffThreadLogger {
    #[inline]
    fn generate_span_id(&self) -> polog::SpanId {
        self.1.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {}

    #[inline]
    fn flush_on_panic(&mut self) {}
}

impl<Context> polog::DynHandler<Context> for OffThreadLogger
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
        self.0.send(RawEvent::Exit { span_id }).unwrap();
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        _descriptor: &'static polog::Descriptor,
        span_id: polog::SpanId,
        _context: Context,
    ) {
        self.0.send(RawEvent::EnterPoll { span_id }).unwrap();
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        _descriptor: &'static polog::Descriptor,
        span_id: polog::SpanId,
        _context: Context,
    ) {
        self.0.send(RawEvent::PendingPoll { span_id }).unwrap();
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        _descriptor: &'static polog::Descriptor,
        span_id: polog::SpanId,
        _context: Context,
    ) {
        self.0.send(RawEvent::ReadyPoll { span_id }).unwrap();
    }
}

impl<Context> polog::Handler<Context> for OffThreadLogger
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
        self.0
            .send(RawEvent::Log {
                info: RawEventInfo {
                    descriptor,
                    raw_args_type: type_name::<Args>().replace("polog::args::Arg", "@"),
                    raw_args_sendable_type: type_name::<Args::Sendable>()
                        .replace("polog::args::Arg", "@"),
                    args: Box::new(args.into_sendable()),
                },
                span_id,
            })
            .unwrap();
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
        self.0
            .send(RawEvent::Enter {
                info: RawEventInfo {
                    descriptor,
                    raw_args_type: type_name::<Args>().replace("polog::args::Arg", "@"),
                    raw_args_sendable_type: type_name::<Args::Sendable>()
                        .replace("polog::args::Arg", "@"),
                    args: Box::new(args.into_sendable()),
                },
                parent_span_id,
                span_id,
                span_kind,
            })
            .unwrap();
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
        self.0
            .send(RawEvent::Log {
                info: RawEventInfo {
                    descriptor,
                    raw_args_type: type_name::<Args>().replace("polog::args::Arg", "@"),
                    raw_args_sendable_type: type_name::<Args>().replace("polog::args::Arg", "@"),
                    args: Box::new(args),
                },
                span_id,
            })
            .unwrap();
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
        self.0
            .send(RawEvent::Enter {
                info: RawEventInfo {
                    descriptor,
                    raw_args_type: type_name::<Args>().replace("polog::args::Arg", "@"),
                    raw_args_sendable_type: type_name::<Args>().replace("polog::args::Arg", "@"),
                    args: Box::new(args),
                },
                parent_span_id,
                span_id,
                span_kind,
            })
            .unwrap();
    }
}

impl<Context> SharedHandler<Context> for OffThreadLogger where Context: ContextArgs {}

#[expect(
    clippy::too_many_lines,
    reason = "better to keep within a single function"
)]
#[test]
fn test_async_logger_log() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut logger = OffThreadLogger::new(tx);
    let file = producers::FILE;
    let base_line = producers::PRODUCE_LOGS_BASE_LINE;
    let handle = std::thread::spawn(move || {
        producers::produce_logs(&mut logger, polog::Level::Error);
    });

    assert_eq!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Log {
            info: EventInfo {
                location: (file, base_line + 1, 5),
                module_path: "polog_test_helpers::producers",
                target: None,
                level: polog::Level::Error,
                message: "Oh no!",
                param_names: &["not", "STATIC_VALUE"],
                raw_args_type: "(@<&str>, @<&str>)".to_owned(),
                raw_args_sendable_type: "(@<&str>, @<&str>)".to_owned(),
                formatted_args: vec!["again".to_owned(), "static".to_owned()]
            },
            span_id: None
        }
    );

    assert_eq!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Log {
            info: EventInfo {
                location: (file, base_line + 2, 5),
                module_path: "polog_test_helpers::producers",
                target: None,
                level: polog::Level::Warn,
                message: "Sigh!",
                param_names: &["temp"],
                raw_args_type: "(@<polog::send::wrappers::Temporary<&str>>,)".to_owned(),
                raw_args_sendable_type:
                    "(@<polog::send::wrappers::AlreadySendable<alloc::string::String>>,)".to_owned(),
                formatted_args: vec!["l1".to_owned()]
            },
            span_id: None
        }
    );

    assert_eq!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Log {
            info: EventInfo {
                location: (file, base_line + 3, 5),
                module_path: "polog_test_helpers::producers",
                target: None,
                level: polog::Level::Info,
                message: "Ok!",
                param_names: &["number", "STATIC_VALUE"],
                raw_args_type: "(@<i32>, @<polog::fmt::wrappers::ViaDebug<&str>>)".to_owned(),
                raw_args_sendable_type: "(@<i32>, @<polog::fmt::wrappers::ViaDebug<&str>>)"
                    .to_owned(),
                formatted_args: vec!["123".to_owned(), "\"static\"".to_owned()]
            },
            span_id: None
        }
    );

    assert_eq!(
        Event::from(rx.blocking_recv().unwrap()),
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
                raw_args_sendable_type: "(@<polog::fmt::wrappers::ViaDisplay<i32>>, \
                                         @<polog::fmt::wrappers::ViaAltDebug<[i32; 3]>>)"
                    .to_owned(),
                formatted_args: vec!["123".to_owned(), "[\n    1,\n    2,\n    3,\n]".to_owned()]
            },
            span_id: None
        }
    );

    assert_eq!(
        Event::from(rx.blocking_recv().unwrap()),
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
                raw_args_sendable_type: "(\
                    (\
                        @<polog::send::wrappers::AlreadySendable<polog::fmt::wrappers::ViaDebug<\
                            polog_test_helpers::producers::produce_logs::Custom<i32>\
                        >>>, \
                        @<alloc::string::String>, \
                        @<f64>, \
                        @<alloc::string::String>, \
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
            span_id: None
        }
    );

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
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
        Event::from(rx.blocking_recv().unwrap()),
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
        Event::from(rx.blocking_recv().unwrap()),
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

    handle.join().unwrap();
    assert!(rx.blocking_recv().is_none());
}

#[expect(
    clippy::too_many_lines,
    reason = "better to keep within a single function"
)]
#[cfg(feature = "alloc")]
#[test]
fn test_async_logger_span() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut logger = OffThreadLogger::new(tx);
    let handle = std::thread::spawn(move || producers::produce_spanned_logs(&mut logger));

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
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
        Event::from(rx.blocking_recv().unwrap()),
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
        Event::from(rx.blocking_recv().unwrap()),
        Event::Log {
            info: EventInfo {
                level: polog::Level::Warn,
                message: "Some warn!",
                ..
            },
            span_id,
        } if span_id.unwrap().0.get() == 1
    ));

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Enter {
            info: EventInfo {
                level: polog::Level::Info,
                message: "First detached span.",
                ..
            },
            parent_span_id,
            span_id,
            span_kind: polog::SpanKind::Detached,
        } if parent_span_id.unwrap().0.get() == 1 && span_id.0.get() == 2
    ));

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Enter {
            info: EventInfo {
                level: polog::Level::Info,
                message: "Second detached span.",
                ..
            },
            parent_span_id,
            span_id,
            span_kind: polog::SpanKind::Detached,
        } if parent_span_id.unwrap().0.get() == 1 && span_id.0.get() == 3
    ));

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Enter {
            info: EventInfo {
                level: polog::Level::Debug,
                message: "Second in-thread span.",
                ..
            },
            parent_span_id,
            span_id,
            span_kind: polog::SpanKind::Nested,
        } if parent_span_id.unwrap().0.get() == 3 && span_id.0.get() == 4
    ));

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Log {
            info: EventInfo {
                level: polog::Level::Trace,
                message: "Another trace!",
                ..
            },
            span_id,
        } if span_id.unwrap().0.get() == 4
    ));

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Exit {
            span_id,
        } if span_id.0.get() == 4
    ));

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Enter {
            info: EventInfo {
                level: polog::Level::Info,
                message: "First in-thread span.",
                ..
            },
            parent_span_id,
            span_id,
            span_kind: polog::SpanKind::Nested,
        } if parent_span_id.unwrap().0.get() == 2 && span_id.0.get() == 5
    ));

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Log {
            info: EventInfo {
                level: polog::Level::Trace,
                message: "Some trace!",
                ..
            },
            span_id,
        } if span_id.unwrap().0.get() == 5
    ));

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Exit {
            span_id,
        } if span_id.0.get() == 5
    ));

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Exit {
            span_id,
        } if span_id.0.get() == 2
    ));

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Exit {
            span_id,
        } if span_id.0.get() == 3
    ));

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
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
        Event::from(rx.blocking_recv().unwrap()),
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
        Event::from(rx.blocking_recv().unwrap()),
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
        Event::from(rx.blocking_recv().unwrap()),
        Event::Exit {
            span_id,
        } if span_id.0.get() == 7
    ));

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Exit {
            span_id,
        } if span_id.0.get() == 6
    ));

    assert!(matches!(
        Event::from(rx.blocking_recv().unwrap()),
        Event::Exit {
            span_id,
        } if span_id.0.get() == 1
    ));

    handle.join().unwrap();
    assert!(rx.blocking_recv().is_none());
}

#[expect(
    clippy::too_many_lines,
    reason = "better to keep within a single function"
)]
#[tokio::test]
async fn test_async_logger_async() {
    async fn func1<L>(mut logger: L)
    where
        L: Logger,
    {
        let mut logger = polog::info_span!(&mut logger, "Fn func1."; qwe = "asd");
        polog::error!(&mut logger, "Some error!"; 2 * 3);
        let deferred = polog::deferred_info_span!(&mut logger, "Polled");
        let task1 = tokio::task::spawn(func2.poll_instrument(deferred));
        task1.await.unwrap();
        polog::info!(&mut logger, "Some info!"; 2 * 3);
    }

    async fn func2<L>(mut logger: L)
    where
        L: Logger,
    {
        polog::warn!(&mut logger, "Some warn!"; 3 * 4);
        tokio::task::yield_now().await;
        polog::warn!(&mut logger, "Another warn!"; 4 * 5);
    }

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let logger = OffThreadLogger::new(tx);
    let _handle = tokio::task::spawn(func1(logger));

    assert!(matches!(
        Event::from(rx.recv().await.unwrap()),
        Event::Enter {
            info: EventInfo {
                level: polog::Level::Info,
                message: "Fn func1.",
                ..
            },
            parent_span_id,
            span_id,
            span_kind: polog::SpanKind::Nested,
        } if parent_span_id.is_none() && span_id.0.get() == 1
    ));

    assert!(matches!(
        Event::from(rx.recv().await.unwrap()),
        Event::Log {
            info: EventInfo {
                level: polog::Level::Error,
                message: "Some error!",
                ..
            },
            span_id,
        } if span_id.unwrap().0.get() == 1
    ));

    assert!(matches!(
        Event::from(rx.recv().await.unwrap()),
        Event::Enter {
            info: EventInfo {
                level: polog::Level::Info,
                message: "Polled",
                ..
            },
            parent_span_id,
            span_id,
            span_kind: polog::SpanKind::Detached,
        } if parent_span_id.unwrap().0.get() == 1 && span_id.0.get() == 2
    ));

    assert!(matches!(
        Event::from(rx.recv().await.unwrap()),
        Event::EnterPoll {
            span_id,
        } if span_id.0.get() == 2
    ));

    assert!(matches!(
        Event::from(rx.recv().await.unwrap()),
        Event::Log {
            info: EventInfo {
                level: polog::Level::Warn,
                message: "Some warn!",
                ..
            },
            span_id,
        } if span_id.unwrap().0.get() == 2
    ));

    assert!(matches!(
        Event::from(rx.recv().await.unwrap()),
        Event::PendingPoll {
            span_id,
        } if span_id.0.get() == 2
    ));

    assert!(matches!(
        Event::from(rx.recv().await.unwrap()),
        Event::EnterPoll {
            span_id,
        } if span_id.0.get() == 2
    ));

    assert!(matches!(
        Event::from(rx.recv().await.unwrap()),
        Event::Log {
            info: EventInfo {
                level: polog::Level::Warn,
                message: "Another warn!",
                ..
            },
            span_id,
        } if span_id.unwrap().0.get() == 2
    ));

    assert!(matches!(
        Event::from(rx.recv().await.unwrap()),
        Event::ReadyPoll {
            span_id,
        } if span_id.0.get() == 2
    ));

    assert!(matches!(
        Event::from(rx.recv().await.unwrap()),
        Event::Exit {
            span_id,
        } if span_id.0.get() == 2
    ));

    assert!(matches!(
        Event::from(rx.recv().await.unwrap()),
        Event::Log {
            info: EventInfo {
                level: polog::Level::Info,
                message: "Some info!",
                ..
            },
            span_id,
        } if span_id.unwrap().0.get() == 1
    ));

    assert!(matches!(
        Event::from(rx.recv().await.unwrap()),
        Event::Exit {
            span_id,
        } if span_id.0.get() == 1
    ));

    assert!(rx.recv().await.is_none());
}
