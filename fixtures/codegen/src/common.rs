pub struct Span<'a> {
    #[cfg(all(feature = "span", feature = "tracing"))]
    pub _tracing: ::tracing::span::EnteredSpan,

    #[cfg(feature = "slog")]
    pub slog: ::slog::Logger,

    #[cfg(feature = "polog")]
    pub polog: ::polog::BorrowingSpan<'a, crate::backends::polog::PologLogger>,

    #[cfg(not(feature = "polog"))]
    pub _lt: ::core::marker::PhantomData<&'a ()>,
}

pub struct DetachedLogger {
    #[cfg(feature = "slog")]
    pub slog: ::slog::Logger,

    #[cfg(feature = "polog")]
    pub polog: ::polog::DetachedLogger<crate::backends::polog::PologLogger>,
}

impl DetachedLogger {
    #[inline]
    pub fn new() -> Self {
        #[cfg(feature = "slog")]
        let slog;
        #[cfg(feature = "polog")]
        let polog;

        #[cfg(feature = "log")]
        {
            crate::backends::log::LogLogger::new(log::LevelFilter::Info).set_as_global_logger();
        }

        #[cfg(feature = "tracing")]
        {
            crate::backends::tracing::TracingLogger::new(tracing::level_filters::LevelFilter::INFO)
                .set_as_global_logger();
        }

        #[cfg(feature = "slog")]
        {
            let logger = crate::backends::slog::SlogLogger::new(Some(slog::Level::Info));
            slog = slog::Logger::root(logger, slog::o!());
        }

        #[cfg(feature = "polog")]
        {
            let logger = crate::backends::polog::PologLogger::new(polog::LevelFilter::Info);
            polog = polog::LoggerExt::detach(&logger);
        };

        Self {
            #[cfg(feature = "slog")]
            slog,
            #[cfg(feature = "polog")]
            polog,
        }
    }
}

#[macro_export]
macro_rules! log {
    ( $span:expr, $msg:expr $( , $( $value:ident ),* $(,)? )? ) => {
        #[cfg(feature = "log")]
        ::log::info!( $( $( $value = $value ),* ; )? $msg);

        #[cfg(feature = "tracing")]
        ::tracing::info!( $($( $value, )*)? $msg );

        #[cfg(feature = "slog")]
        ::slog::info!( &$span.slog, $msg $(; $( stringify!($value) => $value),*)? );

        #[cfg(feature = "polog")]
        ::polog::info!( &mut $span.polog, $msg $(; $($value),*)? );
    };
}

#[macro_export]
macro_rules! nested_span {
    ( $span:expr, $msg:expr $( , $( $value:ident ),* $(,)? )? ) => {{
        #[cfg(all(feature = "span", feature = "tracing"))]
        let tracing = ::tracing::info_span!( $msg $($( , $value )*)?  ).entered();

        #[cfg(all(feature = "context", feature = "slog"))]
        let slog = $span.slog.new(
            ::slog::o!( "msg" => $msg $($( , stringify!($value) => $value)*)? )
        );
        #[cfg(all(not(feature = "context"), feature = "slog"))]
        let slog = $span.slog.clone();

        #[cfg(all(feature = "span", feature = "polog"))]
        let polog = ::polog::info_span!( &mut $span.polog, $msg $(; $($value),*)? );
        #[cfg(all(not(feature = "span"), feature = "polog"))]
        let polog = ::polog::LoggerExt::transparent_borrowing_span(&mut $span.polog);

        $crate::common::Span {
            #[cfg(all(feature = "span", feature = "tracing"))]
            _tracing: tracing,
            #[cfg(feature = "slog")]
            slog,
            #[cfg(feature = "polog")]
            polog,
            #[cfg(not(feature = "polog"))]
            _lt: core::marker::PhantomData,
        }
    }};
}

#[macro_export]
macro_rules! into_detached {
    ( $span:expr ) => {{
        #[cfg(feature = "slog")]
        let slog = $span.slog.clone();

        #[cfg(feature = "polog")]
        let polog = ::polog::LoggerExt::detach(&$span.polog);

        $crate::common::DetachedLogger {
            #[cfg(feature = "slog")]
            slog,
            #[cfg(feature = "polog")]
            polog,
        }
    }};
}
