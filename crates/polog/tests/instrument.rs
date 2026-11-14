#![expect(missing_docs, reason = "okay in tests")]
#![allow(unused_crate_dependencies, reason = "okay in tests")]
#![expect(clippy::unwrap_used, reason = "okay in tests")]

use polog::PollInstrument;
use polog_test_helpers::tracker::Tracker;

async fn example<L>(mut logger: L)
where
    L: polog::Logger,
{
    polog::info!(&mut logger, "Some warn!"; 3 * 4);
    tokio::task::yield_now().await;
    polog::info!(&mut logger, "Another warn!"; 4 * 5);
}

#[tokio::test]
async fn test_instrumented() {
    let mut tracker = Tracker::default();
    let deferred = polog::deferred_info_span!(&mut tracker, "Polled");
    let task = tokio::task::spawn(example.poll_instrument(deferred));
    task.await.unwrap();
    assert_eq!(
        tracker.take_events(),
        &[
            "try_enable",
            "generate_span_id",
            "enter_span(detached)",
            "enter_poll_span",
            "try_enable",
            "log",
            "pending_poll_span",
            "enter_poll_span",
            "try_enable",
            "log",
            "ready_poll_span",
            "exit_span(detached)"
        ]
    );

    let mut tracker = Tracker::default();
    let mut deferred = polog::deferred_info_span!(&mut tracker, "Polled");
    deferred.forget();
    let task = tokio::task::spawn(example.poll_instrument(deferred));
    task.await.unwrap();
    assert_eq!(
        tracker.take_events(),
        &["try_enable", "log", "try_enable", "log"]
    );
}
