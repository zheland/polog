#[cfg(any(feature = "write", feature = "send"))]
use core::cell::RefCell;
#[cfg(feature = "write")]
use core::fmt;
#[cfg(feature = "write")]
use core::fmt::Write as _;
#[cfg(any(feature = "write", feature = "send"))]
use core::hint::black_box;
#[cfg(all(feature = "write-stderr", feature = "write"))]
use std::io::Write as _;
use std::sync::Arc;
#[cfg(feature = "write")]
use std::time::SystemTime;

#[cfg(feature = "write")]
use polog::DisplayableMixedArgs;
use polog::{
    BaseHandler, ContextArgs, Descriptor, DynHandler, Handler, LevelFilter, LogArgs, SharedHandler,
    SpanId, SpanIdGenerator, SpanKind,
};
#[cfg(feature = "send")]
use polog_util::TypeErasedArcLogArgs;

#[cfg(feature = "send")]
use polog_util::TypeErasedArgs;

thread_local! {
    #[cfg(feature = "write")]
    static BUF: RefCell<String> = RefCell::new(String::with_capacity(512));

    #[cfg(feature = "send")]
    static EVENT: RefCell<Event> = const { RefCell::new(Event::None) };
}

#[derive(Clone, Debug)]
pub struct PologLogger {
    max_level: LevelFilter,
    span_id_generator: Arc<SpanIdGenerator>,
}

#[cfg(feature = "send")]
#[expect(dead_code, reason = "read by the `black_box`")]
enum Event {
    None,
    Log {
        descriptor: &'static Descriptor,
        args: TypeErasedArcLogArgs,
        span_id: Option<SpanId>,
    },
    EnterSpan {
        descriptor: &'static Descriptor,
        args: TypeErasedArcLogArgs,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
    },
    ExitSpan {
        descriptor: &'static Descriptor,
        span_id: SpanId,
    },
}

impl PologLogger {
    #[inline]
    #[must_use]
    pub fn new(max_level: LevelFilter) -> Self {
        Self {
            max_level,
            span_id_generator: Arc::new(SpanIdGenerator::new()),
        }
    }

    #[cfg(feature = "write")]
    #[inline]
    fn log_write<'a, Args>(descriptor: &'static Descriptor, args: &Args, span_id: Option<SpanId>)
    where
        Args: DisplayableMixedArgs<'a>,
    {
        BUF.with_borrow_mut(|buf| {
            buf.clear();
            fmt_descriptor(buf, descriptor, "LOG");
            fmt_args(buf, descriptor, args);
            fmt_spans(buf, &[span_id]);
            writeln!(buf).unwrap();

            #[cfg(feature = "write-stderr")]
            let mut stderr = std::io::stderr();
            #[cfg(feature = "write-stderr")]
            stderr.write_all(buf.as_bytes()).unwrap();

            let _ = black_box(buf);
        });
    }

    #[cfg(feature = "write")]
    #[inline]
    fn enter_span_write<'a, Args>(
        descriptor: &'static Descriptor,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
    ) where
        Args: DisplayableMixedArgs<'a>,
    {
        BUF.with_borrow_mut(|buf| {
            fmt_descriptor(buf, descriptor, "ENTER");
            fmt_args(buf, descriptor, args);
            fmt_spans(buf, &[parent_span_id, Some(span_id)]);
            writeln!(buf).unwrap();

            #[cfg(feature = "write-stderr")]
            let mut stderr = std::io::stderr();
            #[cfg(feature = "write-stderr")]
            stderr.write_all(buf.as_bytes()).unwrap();

            let _ = black_box(buf);
        });
    }

    #[cfg(feature = "write")]
    #[inline]
    fn exit_span_write(descriptor: &'static Descriptor, span_id: SpanId) {
        BUF.with_borrow_mut(|buf| {
            fmt_descriptor(buf, descriptor, "EXIT");
            fmt_spans(buf, &[Some(span_id)]);
            writeln!(buf).unwrap();

            #[cfg(feature = "write-stderr")]
            let mut stderr = std::io::stderr();
            #[cfg(feature = "write-stderr")]
            stderr.write_all(buf.as_bytes()).unwrap();

            let _ = black_box(buf);
        });
    }
}

impl BaseHandler for PologLogger {
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        self.span_id_generator.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {}

    #[inline]
    fn flush_on_panic(&mut self) {}
}

impl<Context> DynHandler<Context> for PologLogger
where
    Context: ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        _span_kind: SpanKind,
        _context: Context,
    ) {
        #[cfg(feature = "write")]
        Self::exit_span_write(descriptor, span_id);

        #[cfg(feature = "send")]
        EVENT.with_borrow_mut(|event| {
            *event = Event::ExitSpan {
                descriptor,
                span_id,
            };
            let _ = black_box(event);
        });

        #[cfg(not(any(feature = "write", feature = "send")))]
        let _ = (descriptor, span_id);
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        _descriptor: &'static Descriptor,
        _span_id: SpanId,
        _context: Context,
    ) {
    }
}

impl<Context> Handler<Context> for PologLogger
where
    Context: ContextArgs,
{
    type EnrichedContext = Context;

    #[inline]
    fn try_enable(
        &self,
        descriptor: &'static Descriptor,
        context: Context,
    ) -> Option<Self::EnrichedContext> {
        (descriptor.level <= self.max_level).then_some(context)
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
        #[cfg(feature = "write")]
        Self::log_write(descriptor, &args, span_id);

        #[cfg(feature = "send")]
        EVENT.with_borrow_mut(|event| {
            *event = Event::Log {
                descriptor,
                args: TypeErasedArgs::new(Arc::new(args.into_sendable())),
                span_id,
            };
            let _ = black_box(event);
        });

        #[cfg(not(any(feature = "write", feature = "send")))]
        drop((descriptor, args, span_id));
    }

    #[inline]
    fn enter_span<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        _span_kind: SpanKind,
        _context: Context,
    ) where
        Args: LogArgs<'a>,
    {
        #[cfg(feature = "write")]
        Self::enter_span_write(descriptor, &args, parent_span_id, span_id);

        #[cfg(feature = "send")]
        EVENT.with_borrow_mut(|event| {
            *event = Event::EnterSpan {
                descriptor,
                args: TypeErasedArgs::new(Arc::new(args.into_sendable())),
                parent_span_id,
                span_id,
            };
            let _ = black_box(event);
        });

        #[cfg(not(any(feature = "write", feature = "send")))]
        drop((descriptor, args, parent_span_id, span_id));
    }

    fn log_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        _context: Self::EnrichedContext,
    ) where
        Args: polog::StaticLogArgs,
    {
        #[cfg(feature = "write")]
        Self::log_write(descriptor, &args, span_id);

        #[cfg(feature = "send")]
        EVENT.with_borrow_mut(|event| {
            *event = Event::Log {
                descriptor,
                args: TypeErasedArgs::new(Arc::new(args)),
                span_id,
            };
            let _ = black_box(event);
        });

        #[cfg(not(any(feature = "write", feature = "send")))]
        drop((descriptor, args, span_id));
    }

    fn enter_span_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        _span_kind: SpanKind,
        _context: Self::EnrichedContext,
    ) where
        Args: polog::StaticLogArgs,
    {
        #[cfg(feature = "write")]
        Self::enter_span_write(descriptor, &args, parent_span_id, span_id);

        #[cfg(feature = "send")]
        EVENT.with_borrow_mut(|event| {
            *event = Event::EnterSpan {
                descriptor,
                args: TypeErasedArgs::new(Arc::new(args)),
                parent_span_id,
                span_id,
            };
            let _ = black_box(event);
        });

        #[cfg(not(any(feature = "write", feature = "send")))]
        drop((descriptor, args, parent_span_id, span_id));
    }
}

impl<Context> SharedHandler<Context> for PologLogger where Context: ContextArgs {}

#[cfg(feature = "write")]
#[inline]
pub fn fmt_descriptor(writer: &mut String, descriptor: &Descriptor, kind: &str) {
    let time = humantime::format_rfc3339_micros(SystemTime::now());
    write!(
        writer,
        "P {time} {} {} {kind} {}",
        descriptor.level.as_uppercase_str(),
        descriptor.module_path,
        descriptor.message,
    )
    .unwrap();
}

#[cfg(feature = "write")]
#[inline]
pub fn fmt_args<'a, Args>(writer: &mut String, descriptor: &Descriptor, args: &Args)
where
    Args: DisplayableMixedArgs<'a>,
{
    let num_args = args.num_args().min(descriptor.param_names.len());
    for i in 0..num_args {
        write!(
            writer,
            " {}={}",
            descriptor.param_names[i],
            fmt::from_fn(|f| args.display_fmt_nth(f, i).unwrap())
        )
        .unwrap();
    }
}

#[cfg(feature = "write")]
#[inline]
pub fn fmt_spans(writer: &mut String, span_ids: &[Option<SpanId>]) {
    for span_id in span_ids.iter().flatten() {
        write!(writer, " #{}", span_id.0).unwrap();
    }
}
