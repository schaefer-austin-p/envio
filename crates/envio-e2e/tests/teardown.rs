//! A core that cannot stop fails the test instead of hanging it (#116).
//! Needs no Stalwart.

mod support;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use envio_core::Command;
use log::LevelFilter;
use support::core_run::CoreRun;

#[test]
fn a_stuck_core_does_not_hang_teardown() {
    let (done, finished) = channel();
    std::thread::spawn(move || {
        let run = CoreRun::start(LevelFilter::Off);
        run.core().send(Command::Stall { ms: 120_000 });
        let started = Instant::now();
        // A failing test unwinds through the drop of its run.
        let outcome = catch_unwind(AssertUnwindSafe(move || {
            let _run = run;
            panic!("forced");
        }));
        let _ = done.send((outcome.is_err(), started.elapsed()));
    });
    let (panicked, took) = finished
        .recv_timeout(Duration::from_secs(15))
        .expect("teardown hung: the drop did not return within 15 s");
    assert!(panicked);
    assert!(took < Duration::from_secs(15), "took {took:?}");
}
