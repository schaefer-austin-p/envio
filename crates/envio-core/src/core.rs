//! The handle the app holds: commands go in, events come out through the sink.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::api::command::Command;
use crate::api::event::Event;
use crate::supervisor::supervise;

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
    commands: UnboundedSender<Command>,
    thread: Option<JoinHandle<()>>,
}

// The handle is shared across the app's threads (#39); nothing else is.
const _: fn() = || {
    fn check<T: Send + Sync>() {}
    check::<Core>();
};

impl Core {
    pub fn start(config: Config, sink: Arc<dyn EventSink>) -> io::Result<Core> {
        crate::logging::init(&config.logs_dir(), config.log_level)?;
        let (commands, receiver) = unbounded_channel();
        let thread = thread::Builder::new()
            .name("envio-worker".into())
            .spawn(move || supervise(config, receiver, sink))?;
        Ok(Core {
            commands,
            thread: Some(thread),
        })
    }

    /// Never blocks. After the worker ended the command is dropped.
    pub fn send(&self, command: Command) {
        if let Err(error) = self.commands.send(command) {
            log::warn!("worker has stopped; dropped {:?}", error.0);
        }
    }

    /// Stops the worker and waits for it (what `Drop` does).
    pub fn shutdown(self) {}
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
