//! The worker: one thread, one current-thread runtime, one command loop.

mod dispatch;
mod store;

use std::collections::VecDeque;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;

use crate::api::command::Command;
use crate::api::event::Event;
use crate::api::view::{AccountInfo, CacheState};
use crate::clock::now_ms;
use crate::core::{Config, EventSink};
use crate::health::AccountStatus;
use crate::text;
use dispatch::{Flow, handle};
use envio_net::Roots;
use envio_sync::schedule::backoff_ms;
use store::Store;

pub(crate) struct Worker<'s> {
    #[allow(dead_code)] // used by the handlers of later tasks
    store: Store,
    sink: &'s dyn EventSink,
    #[allow(dead_code)]
    roots: Roots,
    #[allow(dead_code)]
    foreground: bool,
}

/// Returns on `Shutdown` or when the handle is gone.
pub(crate) fn run(
    config: &Config,
    commands: &mut UnboundedReceiver<Command>,
    sink: &dyn EventSink,
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async {
        let Some(mut worker) = Worker::open(config, commands, sink).await else {
            return;
        };
        while worker.turn(commands).await == Flow::Continue {}
    });
}

impl<'s> Worker<'s> {
    /// Opens the store, retrying with backoff while it can't; `None` only
    /// when told to shut down meanwhile.
    async fn open(
        config: &Config,
        commands: &mut UnboundedReceiver<Command>,
        sink: &'s dyn EventSink,
    ) -> Option<Worker<'s>> {
        let mut failures: u32 = 0;
        let mut queued: VecDeque<Command> = VecDeque::new();
        let (store, cache) = loop {
            match Store::open(config, now_ms()) {
                Ok(opened) => break opened,
                Err(error) => {
                    failures = failures.saturating_add(1);
                    log::error!("opening the store: {error}");
                }
            }
            let wait = backoff_ms(failures, u64::from(failures));
            let wait = Duration::from_millis(u64::try_from(wait).unwrap_or(0));
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                command = commands.recv() => match command {
                    None => return None,
                    Some(command) => {
                        if matches!(command, Command::Shutdown) {
                            if !queued.is_empty() {
                                let names: Vec<&str> = queued.iter().map(Command::name).collect();
                                log::warn!(
                                    "shutdown with {} unhandled commands: {}",
                                    names.len(),
                                    names.join(", ")
                                );
                            }
                            return None;
                        }
                        queued.push_back(command);
                    }
                },
            }
        };
        let mut worker = Worker {
            store,
            sink,
            roots: config.roots.clone(),
            foreground: config.foreground,
        };
        worker.announce(cache);
        // What was sent while the store could not open, in order.
        for command in queued {
            if handle(&mut worker, command).await == Flow::Stop {
                return None;
            }
        }
        Some(worker)
    }

    fn announce(&self, cache: CacheState) {
        if cache == CacheState::Rebuilt {
            self.sink.emit(Event::Notice {
                text: text::notice::cache_rebuilt(),
            });
        }
        let accounts = self
            .store
            .accounts
            .list()
            .iter()
            .map(|a| AccountInfo {
                key: a.key.clone(),
                name: a.settings.name.clone(),
                email: a.settings.email.clone(),
                status: AccountStatus::Ready,
            })
            .collect();
        self.sink.emit(Event::Started { accounts, cache });
    }

    /// One loop iteration: wait for a command, handle it and any queued.
    async fn turn(&mut self, commands: &mut UnboundedReceiver<Command>) -> Flow {
        match commands.recv().await {
            Some(command) => match handle(self, command).await {
                Flow::Continue => self.drain(commands).await,
                Flow::Stop => Flow::Stop,
            },
            None => Flow::Stop,
        }
    }

    /// Handles the commands already queued.
    async fn drain(&mut self, commands: &mut UnboundedReceiver<Command>) -> Flow {
        while let Ok(command) = commands.try_recv() {
            if handle(self, command).await == Flow::Stop {
                return Flow::Stop;
            }
        }
        Flow::Continue
    }

    /// Saves open drafts; filled in with the draft commands (Task 12).
    fn flush_drafts(&mut self) {}
}
