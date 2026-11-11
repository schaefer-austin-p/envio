//! Runs the worker and brings it back after a panic.

use std::backtrace::Backtrace;
use std::cell::RefCell;
use std::fs;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Once};
use std::thread;
use std::time::{Duration, Instant};

use envio_sync::schedule::backoff_ms;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::api::event::Event;
use crate::clock::now_ms;
use crate::core::{Config, EventSink};
use crate::worker::Stamped;
use crate::{text, worker};

/// The longest wait before a restart, so `Core::drop` never joins for long.
const RESTART_MAX_MS: i64 = 5_000;
/// Panics this far apart are not counted together.
const FORGIVE_AFTER: Duration = Duration::from_secs(600);

thread_local! {
    static LAST_PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Keeps the worker's panic text (and a backtrace) for the crash log; other
/// threads panic as before.
pub(crate) fn install_panic_hook() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if thread::current().name() == Some("envio-worker") {
                let text = format!("{info}\n\n{}", Backtrace::force_capture());
                LAST_PANIC.with(|p| *p.borrow_mut() = Some(text));
            } else {
                previous(info);
            }
        }));
    });
}

pub(crate) fn supervise(
    config: Config,
    mut commands: UnboundedReceiver<Stamped>,
    sink: Arc<dyn EventSink>,
) {
    install_panic_hook();
    let mut panics: u32 = 0;
    let mut last_panic = Instant::now();
    loop {
        let run = panic::catch_unwind(AssertUnwindSafe(|| {
            worker::run(&config, &mut commands, &*sink);
        }));
        if run.is_ok() {
            return; // Shutdown, or the handle was dropped
        }
        let text = LAST_PANIC
            .with(|p| p.borrow_mut().take())
            .unwrap_or_else(|| "panic".into());
        if let Err(e) = write_crash_log(&config.logs_dir(), &text) {
            log::error!("crash log: {e}");
        }
        log::error!("worker panicked; restarting");
        if last_panic.elapsed() > FORGIVE_AFTER {
            panics = 0;
        }
        panics = panics.saturating_add(1);
        last_panic = Instant::now();
        sink.emit(Event::WorkerRestarted {
            text: text::notice::worker_restarted(),
        });
        let wait = backoff_ms(panics, u64::from(panics)).min(RESTART_MAX_MS);
        thread::sleep(Duration::from_millis(u64::try_from(wait).unwrap_or(0)));
    }
}

pub(crate) fn write_crash_log(dir: &Path, message: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("crash-{}.txt", now_ms()));
    crate::fsutil::write_atomic(&path, message.as_bytes(), 0o600)?;
    Ok(path)
}
