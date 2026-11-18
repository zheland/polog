#![expect(clippy::unwrap_used, reason = "okay in tests")]

use core::{fmt, mem};
use std::sync::{Arc, Mutex};

use polog::{
    BaseHandler, BorrowingHandler, ContextArgs, Descriptor, DisplayableMixedArgs, DynHandler,
    Handler, LevelFilter, LogArgs, SharedHandler, SpanId, SpanIdGenerator, SpanKind, StaticLogArgs,
};
use polog_util::{
    BaseExternalHandler, ExternalArgs, ExternalArgsVisitor, ExternalDescriptor, ExternalHandler,
};

#[doc(hidden)]
#[derive(Clone, Default, Debug)]
pub struct Tracker(pub Arc<Mutex<TrackerInner>>);

#[doc(hidden)]
#[derive(Default, Debug)]
pub struct TrackerInner {
    span_id_generator: SpanIdGenerator,
    events: Vec<&'static str>,
    details: Vec<String>,
    details_enabled: bool,
    should_panic_on_bug: Option<bool>,
}

#[allow(
    clippy::allow_attributes,
    dead_code,
    reason = "included from multiple modules"
)]
impl Tracker {
    #[inline]
    #[must_use]
    pub fn take_events(&self) -> Vec<&'static str> {
        mem::take(&mut self.0.lock().unwrap().events)
    }

    #[inline]
    #[must_use]
    pub fn take_details(&self) -> Vec<String> {
        mem::take(&mut self.0.lock().unwrap().details)
    }

    #[inline]
    pub fn set_details_enabled(&self, value: bool) {
        self.0.lock().unwrap().details_enabled = value;
    }

    #[inline]
    pub fn set_should_panic_on_bug(&self, value: Option<bool>) {
        self.0.lock().unwrap().should_panic_on_bug = value;
    }
}

impl BaseHandler for Tracker {
    #[inline]
    fn generate_span_id(&self) -> SpanId {
        let mut this = self.0.lock().unwrap();
        this.events.push("generate_span_id");
        this.span_id_generator.generate()
    }

    #[inline]
    fn flush_blocking(&mut self) {
        let mut this = self.0.lock().unwrap();
        this.events.push("flush_blocking");
    }

    #[inline]
    fn flush_on_panic(&mut self) {
        let mut this = self.0.lock().unwrap();
        this.events.push("flush_on_panic");
    }

    #[inline]
    fn should_panic_on_bug(
        &self,
        descriptor: &'static Descriptor,
        span_id: Option<SpanId>,
    ) -> bool {
        let mut this = self.0.lock().unwrap();
        this.events.push("should_panic_on_bug");
        if this.details_enabled {
            this.details.push(format!(
                "should_panic_on_bug({:?}, {span_id:?})",
                descriptor.message,
            ));
        }
        this.should_panic_on_bug.unwrap_or(cfg!(debug_assertions))
    }

    #[inline]
    #[track_caller]
    fn panic(&mut self, descriptor: &'static Descriptor, span_id: Option<SpanId>) -> ! {
        let mut this = self.0.lock().unwrap();
        this.events.push("panic");
        drop(this);
        polog::default_log_related_panic(descriptor, span_id);
    }
}

impl<Context> DynHandler<Context> for Tracker
where
    Context: ContextArgs,
{
    #[inline]
    fn exit_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    ) {
        let mut this = self.0.lock().unwrap();
        this.events.push(match span_kind {
            SpanKind::Nested => "exit_span(nested)",
            SpanKind::Detached => "exit_span(detached)",
        });
        if this.details_enabled {
            this.details.push(format!(
                "exit_span({:?}, {span_id:?}, {span_kind:?}, {})",
                descriptor.message,
                fmt_context(&context)
            ));
        }
    }

    #[inline]
    fn enter_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        let mut this = self.0.lock().unwrap();
        this.events.push("enter_poll_span");
        if this.details_enabled {
            this.details.push(format!(
                "enter_poll_span({:?}, {span_id:?}, {})",
                descriptor.message,
                fmt_context(&context)
            ));
        }
    }

    #[inline]
    fn pending_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        let mut this = self.0.lock().unwrap();
        this.events.push("pending_poll_span");
        if this.details_enabled {
            this.details.push(format!(
                "pending_poll_span({:?}, {span_id:?}, {})",
                descriptor.message,
                fmt_context(&context)
            ));
        }
    }

    #[inline]
    fn ready_poll_span(
        &mut self,
        descriptor: &'static Descriptor,
        span_id: SpanId,
        context: Context,
    ) {
        let mut this = self.0.lock().unwrap();
        this.events.push("ready_poll_span");
        if this.details_enabled {
            this.details.push(format!(
                "ready_poll_span({:?}, {span_id:?}, {})",
                descriptor.message,
                fmt_context(&context)
            ));
        }
    }
}

impl<Context> Handler<Context> for Tracker
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
        let mut this = self.0.lock().unwrap();
        this.events.push("try_enable");
        if this.details_enabled {
            this.details.push(format!(
                "try_enable({:?}, {})",
                descriptor.message,
                fmt_context(&context)
            ));
        }
        Some(context)
    }

    #[inline]
    fn log<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        context: Context,
    ) where
        Args: LogArgs<'a>,
    {
        let mut this = self.0.lock().unwrap();
        this.events.push("log");
        if this.details_enabled {
            this.details.push(format!(
                "log({:?}, {}, {span_id:?}, {})",
                descriptor.message,
                fmt_args(&args),
                fmt_context(&context)
            ));
        }
    }

    #[inline]
    fn enter_span<'a, Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Context,
    ) where
        Args: LogArgs<'a>,
    {
        let mut this = self.0.lock().unwrap();
        this.events.push(match span_kind {
            SpanKind::Nested => "enter_span(nested)",
            SpanKind::Detached => "enter_span(detached)",
        });
        if this.details_enabled {
            this.details.push(format!(
                "enter_span({:?}, {}, {parent_span_id:?}, {span_id:?}, {span_kind:?}, {})",
                descriptor.message,
                fmt_args(&args),
                fmt_context(&context)
            ));
        }
    }

    #[inline]
    fn log_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        span_id: Option<SpanId>,
        context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        let mut this = self.0.lock().unwrap();
        this.events.push("log_static");
        if this.details_enabled {
            this.details.push(format!(
                "log_static({:?}, {}, {span_id:?}, {})",
                descriptor.message,
                fmt_args(&args),
                fmt_context(&context)
            ));
        }
    }

    #[inline]
    fn enter_span_static<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: Self::EnrichedContext,
    ) where
        Args: StaticLogArgs,
    {
        let mut this = self.0.lock().unwrap();
        this.events.push(match span_kind {
            SpanKind::Nested => "enter_span_static(nested)",
            SpanKind::Detached => "enter_span_static(detached)",
        });
        if this.details_enabled {
            this.details.push(format!(
                "enter_span_static({:?}, {}, {parent_span_id:?}, {span_id:?}, {span_kind:?}, {})",
                descriptor.message,
                fmt_args(&args),
                fmt_context(&context)
            ));
        }
    }
}

impl<Context> BorrowingHandler<Context> for Tracker
where
    Context: ContextArgs,
{
    #[inline]
    fn log_borrowed<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        span_id: Option<SpanId>,
        context: &Context,
    ) where
        Args: StaticLogArgs,
    {
        let mut this = self.0.lock().unwrap();
        this.events.push("log_borrowed");
        if this.details_enabled {
            this.details.push(format!(
                "log_borrowed({:?}, {}, {span_id:?}, {})",
                descriptor.message,
                fmt_args(&args),
                fmt_context(context)
            ));
        }
    }

    #[inline]
    fn enter_span_borrowed<Args>(
        &mut self,
        descriptor: &'static Descriptor,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        span_kind: SpanKind,
        context: &Context,
    ) where
        Args: StaticLogArgs,
    {
        let mut this = self.0.lock().unwrap();
        this.events.push(match span_kind {
            SpanKind::Nested => "enter_span_borrowed(nested)",
            SpanKind::Detached => "enter_span_borrowed(detached)",
        });
        if this.details_enabled {
            this.details.push(format!(
                "enter_span_borrowed({:?}, {}, {parent_span_id:?}, {span_id:?}, {span_kind:?}, {})",
                descriptor.message,
                fmt_args(&args),
                fmt_context(context)
            ));
        }
    }
}

impl<Context> SharedHandler<Context> for Tracker where Context: ContextArgs {}

impl BaseExternalHandler for Tracker {
    #[inline]
    fn max_level_external(&self) -> LevelFilter {
        let mut this = self.0.lock().unwrap();
        this.events.push("max_level_external");
        if this.details_enabled {
            this.details.push("max_level_external".to_owned());
        }
        LevelFilter::ALL
    }

    #[inline]
    fn is_enabled_external<Desc>(&self, descriptor: &Desc) -> bool
    where
        Desc: ExternalDescriptor,
    {
        let mut this = self.0.lock().unwrap();
        this.events.push("is_enabled_external");
        if this.details_enabled {
            this.details
                .push(format!("is_enabled_external({:?})", descriptor.message()));
        }
        true
    }
}

impl<Context> ExternalHandler<Context> for Tracker
where
    Context: ContextArgs,
{
    #[inline]
    fn log_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        span_id: Option<SpanId>,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let mut this = self.0.lock().unwrap();
        this.events.push("log_external");
        if this.details_enabled {
            this.details.push(format!(
                "log_external({:?}, {}, {span_id:?}, {})",
                descriptor.message(),
                fmt_external_args(&args),
                fmt_context(&context)
            ));
        }
    }

    #[inline]
    fn new_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let mut this = self.0.lock().unwrap();
        this.events.push("new_span_external");
        if this.details_enabled {
            this.details.push(format!(
                "new_span_external({:?}, {}, {parent_span_id:?}, {span_id:?}, {})",
                descriptor.message(),
                fmt_external_args(&args),
                fmt_context(&context)
            ));
        }
    }

    #[inline]
    fn enter_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        parent_span_id: Option<SpanId>,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let mut this = self.0.lock().unwrap();
        this.events.push("enter_span_external");
        if this.details_enabled {
            this.details.push(format!(
                "enter_span_external({:?}, {}, {parent_span_id:?}, {span_id:?}, {})",
                descriptor.message(),
                fmt_external_args(&args),
                fmt_context(&context)
            ));
        }
    }

    #[inline]
    fn exit_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        let mut this = self.0.lock().unwrap();
        this.events.push("exit_span_external");
        if this.details_enabled {
            this.details.push(format!(
                "exit_span_external({:?}, {span_id:?}, {})",
                descriptor.message(),
                fmt_context(&context)
            ));
        }
    }

    #[inline]
    fn close_span_external<Desc>(&mut self, descriptor: &Desc, span_id: SpanId, context: Context)
    where
        Desc: ExternalDescriptor,
    {
        let mut this = self.0.lock().unwrap();
        this.events.push("close_span_external");
        if this.details_enabled {
            this.details.push(format!(
                "close_span_external({:?}, {span_id:?}, {})",
                descriptor.message(),
                fmt_context(&context)
            ));
        }
    }

    #[inline]
    fn record_span_external<Desc, Args>(
        &mut self,
        descriptor: &Desc,
        args: &Args,
        span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
        Args: ExternalArgs,
    {
        let mut this = self.0.lock().unwrap();
        this.events.push("record_span_external");
        if this.details_enabled {
            this.details.push(format!(
                "record_span_external({:?}, {}, {span_id:?}, {})",
                descriptor.message(),
                fmt_external_args(&args),
                fmt_context(&context)
            ));
        }
    }

    #[expect(clippy::similar_names, reason = "better than alternatives")]
    #[inline]
    fn link_span_external<Desc>(
        &mut self,
        descriptor: &Desc,
        followed_span_id: SpanId,
        follower_span_id: SpanId,
        context: Context,
    ) where
        Desc: ExternalDescriptor,
    {
        let mut this = self.0.lock().unwrap();
        this.events.push("link_span_external");
        if this.details_enabled {
            this.details.push(format!(
                "link_span_external({:?}, {followed_span_id:?}, {follower_span_id:?}, {})",
                descriptor.message(),
                fmt_context(&context)
            ));
        }
    }
}

#[inline]
fn fmt_context<Context>(context: &Context) -> impl fmt::Display
where
    Context: ContextArgs,
{
    fmt::from_fn(|f| {
        let num_args = context.num_args();
        write!(f, "(")?;
        if num_args > 0 {
            context.debug_fmt_nth(f, 0).unwrap()?;
            for i in 1..num_args {
                write!(f, ", ")?;
                context.debug_fmt_nth(f, i).unwrap()?;
            }
        }
        write!(f, ")")?;
        Ok(())
    })
}

#[inline]
fn fmt_args<'a, Args>(args: &Args) -> impl fmt::Display
where
    Args: DisplayableMixedArgs<'a>,
{
    fmt::from_fn(|f| {
        let num_args = args.num_args();
        write!(f, "(")?;
        if num_args > 0 {
            args.display_fmt_nth(f, 0).unwrap()?;
            for i in 1..num_args {
                write!(f, ", ")?;
                args.display_fmt_nth(f, i).unwrap()?;
            }
        }
        write!(f, ")")?;
        Ok(())
    })
}

#[inline]
fn fmt_external_args<Args>(args: &Args) -> impl fmt::Display
where
    Args: ExternalArgs,
{
    struct Visitor<'a, 'b>(&'b mut fmt::Formatter<'a>, bool);

    impl ExternalArgsVisitor for Visitor<'_, '_> {
        fn visit_message(&mut self, value: polog_util::ExternalArgValue<'_>) {
            if self.1 {
                let _ = write!(self.0, ", ");
            }
            self.1 = true;
            let _ = write!(self.0, "{value}");
        }

        fn visit_arg(
            &mut self,
            key: polog_util::util::MaybeStaticStr<'_>,
            value: polog_util::ExternalArgValue<'_>,
        ) {
            if self.1 {
                let _ = write!(self.0, ", ");
            }
            self.1 = true;
            let _ = write!(self.0, "{key} = {value}");
        }
    }

    fmt::from_fn(|f| {
        write!(f, "(")?;
        args.visit(&mut Visitor(f, false));
        write!(f, ")")?;
        Ok(())
    })
}
