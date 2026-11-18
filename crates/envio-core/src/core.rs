//! The handle the app holds: commands go in, events come out through the sink.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use envio_store::StorePaths;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::alert::{Poster, PosterSlot};
use crate::api::command::Command;
use crate::api::event::Event;
use crate::clock::now_ms;
use crate::drafts::{DraftBook, DraftKeeper};
use crate::setup_thread::SetupThread;
use crate::supervisor::supervise;
use crate::worker::Input;

/// Where events go. Called on the worker thread, so it must not block.
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: Event);
}

#[derive(Clone)]
pub struct Config {
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
    pub roots: envio_net::Roots,
    pub log_level: log::LevelFilter,
    /// Whether the app is in the foreground at start.
    pub foreground: bool,
}

impl Config {
    pub fn logs_dir(&self) -> PathBuf {
        self.cache_dir.join("logs")
    }
}

pub struct Core {
    commands: UnboundedSender<Input>,
    thread: Option<JoinHandle<()>>,
    /// Saves drafts on its own thread, whatever the worker is doing. Dropped
    /// after the worker has stopped (field order), which flushes it again.
    keeper: DraftKeeper,
    poster: Arc<PosterSlot>,
}

// The handle is shared across the app's threads (#39); nothing else is.
const _: fn() = || {
    fn check<T: Send + Sync>() {}
    check::<Core>();
};

impl Core {
    pub fn start(config: Config, sink: Arc<dyn EventSink>) -> io::Result<Core> {
        crate::logging::init(&config.logs_dir(), config.log_level)?;
        let paths = StorePaths::new(&config.cache_dir, &config.data_dir);
        let book = Arc::new(DraftBook::new(&paths.drafts()));
        // The first open of the folder (here when it works, else when the
        // store opens) also sweeps orphaned attachment copies, once per
        // process: never in a worker that restarts after a panic.
        if let Err(error) = book.drafts() {
            log::warn!("opening the drafts folder: {error}");
        }
        let keeper = DraftKeeper::start(Arc::clone(&book), Arc::clone(&sink))?;
        let poster = Arc::new(PosterSlot::default());
        let (commands, receiver) = unbounded_channel();
        // Outlives the worker's restarts: a probe that was running when the
        // worker panicked ends in the same channel, and is dropped as stale.
        let setup = Arc::new(SetupThread::start(config.roots.clone(), commands.clone())?);
        let shown = Arc::clone(&poster);
        let thread = thread::Builder::new()
            .name("envio-worker".into())
            .spawn(move || supervise(config, receiver, sink, (book, setup, shown)))?;
        Ok(Core {
            commands,
            thread: Some(thread),
            keeper,
            poster,
        })
    }

    /// Never blocks for long (only `AppActive(false)` waits, at most 500 ms,
    /// for the drafts to be saved). After the worker ended the command is
    /// dropped.
    pub fn send(&self, command: Command) {
        // Stamped here, on the caller's thread: the worker may be busy in a
        // step when it gets to the command, and a time limit (Undo) counts
        // from when the user pressed.
        let at = now_ms();
        // Drafts never go through the worker: it may be stuck in a network
        // step, or restarting.
        let command = match command {
            Command::DraftChanged { compose, draft } => {
                return self.keeper.changed(compose, draft, at);
            }
            Command::Attach { compose, path } => return self.keeper.attach(compose, path),
            Command::CloseCompose { compose, discard } => {
                self.keeper.close(compose, discard);
                // The worker still forgets a reply waiting for its body.
                Command::CloseCompose { compose, discard }
            }
            Command::AppActive(false) => {
                self.keeper.flush();
                Command::AppActive(false)
            }
            other => other,
        };
        if let Err(error) = self.commands.send(Input::Command((command, at)))
            && let Input::Command((command, _)) = error.0
        {
            log::warn!("worker has stopped; dropped {}", command.name());
        }
    }

    /// Where new-mail alerts and the launcher badge go (the platform's
    /// part). Until one is set they go nowhere; a later call replaces it.
    /// The worker tells the new poster the current badge, then each change.
    pub fn set_poster(&self, poster: Box<dyn Poster>) {
        self.poster.set(poster);
        let _ = self.commands.send(Input::PosterSet);
    }

    /// Stops the worker and waits for it (what `Drop` does).
    pub fn shutdown(self) {}
}

impl Drop for Core {
    fn drop(&mut self) {
        // Saved before the worker is stopped, so even a worker stuck in a
        // step costs no draft.
        self.keeper.flush();
        let _ = self
            .commands
            .send(Input::Command((Command::Shutdown, now_ms())));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
