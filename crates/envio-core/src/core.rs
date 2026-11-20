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
    /// The OAuth providers an account can sign in with (the app passes
    /// `Provider::microsoft()` when it has a client ID). An OAuth account
    /// whose provider is not here cannot connect.
    pub oauth: Vec<envio_net::oauth::Provider>,
    /// Where DKIM key lookups go (sender trust, #285); `None` is the system
    /// resolver. Tests and the end-to-end suite hand in a fake.
    pub dns: Option<Arc<dyn envio_net::dns::TxtLookup>>,
}

/// Whether the app offers Microsoft sign-in. Off (#381): Microsoft's IMAP
/// refuses every OAuth session ("User is authenticated but not connected"),
/// so a Microsoft address is refused at the email step. The OAuth code
/// stays, behind this one switch, for the Graph backend (#382); turn it on
/// there.
const MICROSOFT_SIGN_IN: bool = false;

/// Whether this build offers Microsoft sign-in at all (gate on and a client
/// ID set). The screens use it to tell an existing Microsoft account that
/// cannot sign in again from one that can.
pub fn microsoft_sign_in_offered() -> bool {
    !default_oauth_providers().is_empty()
}

/// The OAuth providers the app offers: Microsoft when `MICROSOFT_SIGN_IN` is
/// on and its client ID is set (`client_ids::MICROSOFT`), else none. The
/// app's `Config` takes this.
pub fn default_oauth_providers() -> Vec<envio_net::oauth::Provider> {
    if !MICROSOFT_SIGN_IN {
        return Vec::new();
    }
    providers_from(envio_net::oauth::Provider::microsoft())
}

fn providers_from(
    microsoft: Option<envio_net::oauth::Provider>,
) -> Vec<envio_net::oauth::Provider> {
    microsoft.into_iter().collect()
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
        // The roots first, private, before anything else makes them (the
        // log folder, the drafts folder); then whatever an older install
        // left too open is narrowed, once (a marker file records it; later
        // starts skip the walk). The database and its files follow in the
        // worker, private as they are made.
        // Not fatal: a root that cannot be made is the store's to report, and
        // the worker retries it.
        let roots: Vec<_> = [&config.cache_dir, &config.data_dir]
            .into_iter()
            .filter_map(|root| {
                envio_store::create_private_dir(root)
                    .and_then(|()| envio_store::tighten_once(root))
                    .err()
                    .map(|error| (root, error))
            })
            .collect();
        crate::logging::init(&config.logs_dir(), config.log_level)?;
        for (root, error) in roots {
            log::warn!("making {} private: {error}", root.display());
        }
        let paths = StorePaths::new(&config.cache_dir, &config.data_dir);
        let book = Arc::new(DraftBook::new(&paths.drafts()));
        // The first open of the folder (here when it works, else when the
        // store opens) also sweeps orphaned attachment copies, once per
        // process: never in a worker that restarts after a panic.
        if let Err(error) = book.drafts() {
            log::warn!("opening the drafts folder: {error}");
        }
        let poster = Arc::new(PosterSlot::default());
        let (commands, receiver) = unbounded_channel();
        // A close whose save finished after `send` stopped waiting for it
        // still reaches the worker, which then uploads the text.
        let saved = commands.clone();
        let keeper = DraftKeeper::start(
            Arc::clone(&book),
            Arc::clone(&sink),
            Box::new(move || {
                let _ = saved.send(Input::DraftsSaved);
            }),
        )?;
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
                // The worker uploads the draft the close saved: wait for
                // the save (a file write, at most 500 ms).
                self.keeper.flush();
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

#[cfg(test)]
mod provider_tests {
    use super::*;
    use envio_net::oauth::Provider;

    #[test]
    fn the_app_offers_no_provider_while_microsoft_sign_in_is_off() {
        // #381 until #382: no provider, so a Microsoft address is refused.
        assert!(default_oauth_providers().is_empty());
    }

    #[test]
    fn the_registered_client_id_is_kept_for_the_graph_backend() {
        let microsoft = envio_net::oauth::Provider::microsoft().expect("a client ID");
        assert_eq!(microsoft.kind, envio_net::oauth::ProviderKind::Microsoft);
        assert!(!microsoft.client_id.is_empty());
    }

    #[test]
    fn a_client_id_offers_microsoft() {
        let with_id = Provider::microsoft_template().with_client_id("test-client-id");
        let list = providers_from(Some(with_id));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].client_id, "test-client-id");
    }
}
