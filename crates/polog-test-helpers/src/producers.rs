use core::hint::black_box;
use std::sync::{Arc, Barrier};

use polog::{Logger, LoggerExt};

pub const FILE: &str = file!();
pub const PRODUCE_LOGS_BASE_LINE: u32 = 25;

#[inline]
pub fn produce_logs<L>(logger: &mut L, custom_level: polog::Level)
where
    L: Logger,
{
    #[derive(Debug)]
    struct Custom<T>(T, T, T, T, T, T);

    const STATIC_VALUE: &str = "static";

    let temp_string1 = black_box(String::from("l1"));
    let temp_str1 = temp_string1.as_str();
    let temp_string2 = black_box(String::from("l2"));
    let temp_str2 = temp_string2.as_str();
    let number = 123;

    assert_eq!(PRODUCE_LOGS_BASE_LINE, line!());
    polog::error!(logger, "Oh no!"; not = "again", STATIC_VALUE);
    polog::warn!(logger, "Sigh!"; temp = :t temp_str1);
    polog::info!(logger, "Ok!"; number = 123, ?STATIC_VALUE);
    polog::debug!(logger, "I see!"; display = % 123, debug = #? &[1, 2, 3]);
    polog::trace!(
        logger,
        target: "custom_target",
        "Too much!";
        custom1 = :a ? Custom(1, 2, 3, 4, 5, 6),
        custom2 = :s ? Custom(
            temp_str1, temp_str2, temp_str1, temp_str2, temp_str1, temp_str2
        ),
        number = 123.123,
        complex = :f("{}-{number:04x}-{:E}-{:04}-{temp_str1}", 234, 345.456, 567),
        arg5 = 55,
        arg6 = 66,
        arg7 = 77,
        arg8 = 88,
        arg9 = 99,
        arg10 = 1010,
        arg11 = 1111,
        arg12 = 1212,
        arg13 = 1313,
        arg14 = 1414,
        arg15 = 1515,
        arg16 = 1616,
    );

    polog::log!(logger, :dyn custom_level, "Custom level."; arg1 = 123);
    polog::log!(
        logger,
        :dyn custom_level,
        target: "custom_target",
        "Custom level.";
        arg1 = 123
    );
    polog::log!(
        logger,
        descriptor_ref: polog::log_descriptor_ref!(
            polog::Level::Debug,
            target: "another_target",
            "Custom description.",
            keys: &["arg2", "arg3"],
        );
        234,
        345,
    );
}

#[expect(clippy::unwrap_used, reason = "okay in tests")]
#[inline]
pub fn produce_spanned_logs<L>(logger: &mut L)
where
    L: Logger,
{
    polog::error!(logger, "Some error!"; 2 * 3);
    let mut logger = polog::info_span!(logger, "Info span."; qwe = "asd");
    polog::warn!(&mut logger, "Some warn!"; 3 * 4);

    let barrier1 = Arc::new(Barrier::new(2));
    let barrier2 = Arc::new(Barrier::new(2));
    let barrier3 = Arc::new(Barrier::new(2));

    let handle1 = std::thread::spawn({
        let mut logger = logger.detach();
        let barrier1 = Arc::clone(&barrier1);
        let barrier2 = Arc::clone(&barrier2);
        move || {
            let mut logger =
                polog::info_span!(&mut logger, "First detached span."; a = "b", c = "d");
            let _ = barrier1.wait();
            let _ = barrier2.wait(); // Wait for the second thread to finish.
            let temp_string1 = black_box(String::from("temp"));
            let temp_str1 = temp_string1.as_str();
            let mut logger = polog::info_span!(
                &mut logger,
                "First in-thread span.";
                seven = 8 - 1,
                :t temp_str1
            );
            polog::trace!(&mut logger, "Some trace!"; 'a', 'b', 'c', :t temp_str1);
        }
    });

    let _ = barrier1.wait(); // Wait for the first thread to start.

    let handle2 = std::thread::spawn({
        let mut logger = logger.detach();
        let barrier3 = Arc::clone(&barrier3);
        move || {
            let mut logger =
                polog::info_span!(&mut logger, "Second detached span."; e = "f", g = "h");
            let mut logger =
                polog::debug_span!(&mut logger, "Second in-thread span."; nine = 8 + 1);
            polog::trace!(&mut logger, "Another trace!"; 'd', 'e', 'f');
            drop(logger);
            let _ = barrier2.wait();
            let _ = barrier3.wait();
        }
    });

    handle1.join().unwrap();
    let _ = barrier3.wait();
    handle2.join().unwrap();

    std::thread::scope(|scope| {
        let handle1 = scope.spawn({
            let mut logger = polog::info_span!(&mut logger, "First scoped span."; a = "b", c = "d");
            move || {
                let temp_string1 = black_box(String::from("temp"));
                let temp_str1 = temp_string1.as_str();
                let mut logger = polog::debug_span!(
                    &mut logger,
                    "Third in-thread span.";
                    seven = 8 - 1,
                    :t temp_str1
                );
                polog::trace!(&mut logger, "Different trace!"; 'a', 'b', 'c', :t temp_str1);
            }
        });

        handle1.join().unwrap();
    });
}
