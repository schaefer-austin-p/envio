//! What `Ui` produced, mirrored onto the models and Qt signals. Model
//! changes, signals and the property sets are all queued on the Qt thread
//! in `Ui`'s order, so QML sees a signal only after the rows it describes.

use std::path::Path;
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use envio_core::{Command, ComposeId, Event, Settings};
use envio_ui::{ModelChange, Output, Signal};

use crate::convert;
use crate::core_object::qobject;
use crate::json;
use crate::lifecycle::clock;
use crate::variant::map_of;

impl qobject::EnvioCore {
    /// An event from the core, on the Qt thread.
    pub(crate) fn apply(mut self: Pin<&mut Self>, event: Event) {
        let started = matches!(event, Event::Started { .. });
        let (now, offset) = clock();
        let outs = self.as_mut().rust_mut().ui.apply(event, now, offset);
        self.as_mut().run(outs);
        if started {
            // After the accounts' rows, in the same queue.
            let _ = self.qt_thread().queue(|obj| obj.started());
        }
    }

    /// `Ready`, and what the app asks for at start: the Outbox, and the
    /// unified inbox once.
    fn started(mut self: Pin<&mut Self>) {
        self.as_mut().set_ready(true);
        self.as_mut().send_command(Command::ListOutbox);
        if !self.rust().opened_inboxes {
            self.as_mut().rust_mut().opened_inboxes = true;
            self.as_mut().open_list(&QString::from("inboxes"), 0);
        }
    }

    /// Mirror what `Ui` produced, in its order: model changes and signals
    /// are queued (one FIFO on the Qt thread), commands go to the core.
    pub(crate) fn run(self: Pin<&mut Self>, outs: Vec<Output>) {
        for out in outs {
            match out {
                Output::Change(change) => self.as_ref().change_model(change),
                Output::Signal(signal) => {
                    let queued = self.qt_thread().queue(move |obj| obj.emit_signal(signal));
                    if queued.is_err() {
                        log::info!("EnvioCore is gone; dropped a signal");
                    }
                }
                Output::Send(command) => self.as_ref().send_command_ref(command),
            }
        }
    }

    pub(crate) fn send_command(self: Pin<&mut Self>, command: Command) {
        self.as_ref().send_command_ref(command);
    }

    fn send_command_ref(self: Pin<&Self>, command: Command) {
        match &self.rust().core {
            Some(core) => core.send(command),
            None => log::warn!("the core is not running; dropped {}", command.name()),
        }
    }

    fn change_model(self: Pin<&Self>, change: ModelChange) {
        let ModelChange { model, op } = change;
        let Some(thread) = self.rust().models.get(&model) else {
            // `attachModel` resets a model from `Ui`'s rows, so nothing is
            // lost.
            log::info!("no {model:?} model attached yet; the change waits in Ui");
            return;
        };
        if thread.queue(move |m| m.apply(op)).is_err() {
            log::warn!("the {model:?} model is gone; dropped a change");
        }
    }

    fn emit_signal(mut self: Pin<&mut Self>, signal: Signal) {
        let q = |s: &str| QString::from(s);
        match signal {
            Signal::Notice(t) => self.notice(&q(&t)),
            Signal::StoreProblem(t) => self.store_problem(&q(&t)),
            Signal::WorkerRestarted(t) => self.worker_restarted(&q(&t)),
            Signal::Detected {
                email,
                text,
                needs_confirm,
                setup,
            } => self.detected(
                &q(&email),
                &q(&text),
                needs_confirm,
                &q(&json::setup_to_json(&setup)),
            ),
            Signal::SetupProblem { email, text } => self.setup_problem(&q(&email), &q(&text)),
            Signal::TrustPrompt {
                email,
                service,
                host,
                fingerprint,
                hex,
                text,
            } => self.trust_prompt(
                &q(&email),
                &q(convert::service_word(service)),
                &q(&host),
                &q(&fingerprint),
                &q(&hex),
                &q(&text),
            ),
            Signal::AccountAdded { account, email } => self.account_added(&q(&account), &q(&email)),
            Signal::ConfirmRemove { account, text } => self.confirm_remove(&q(&account), &q(&text)),
            Signal::MessageOpened { id, view } => {
                let map = map_of(&json::message_view_value(id, &view));
                self.message_opened(&map);
            }
            Signal::MessageUnavailable { id, text } => {
                self.message_unavailable(crate::variant::js_num(id), &q(&text));
            }
            Signal::AttachmentSaved { path } => self.attachment_saved(&q(&lossy(&path))),
            Signal::AttachmentFailed { id } => self.attachment_failed(crate::variant::js_num(id)),
            Signal::UndoOffer { undo, text } => {
                self.undo_offer(
                    crate::variant::js_num(i64::try_from(undo.0).unwrap_or(0)),
                    &q(&text),
                );
            }
            Signal::UndoClosed { undo } => {
                self.undo_closed(crate::variant::js_num(i64::try_from(undo.0).unwrap_or(0)));
            }
            Signal::ComposeReady {
                compose,
                draft,
                quoted,
            } => {
                let map = map_of(&json::draft_value(&draft));
                self.compose_ready(compose_num(compose), &map, &q(&quoted));
            }
            Signal::DraftSaved { compose } => self.draft_saved(compose_num(compose)),
            Signal::ComposeRefused { compose, text } => {
                self.compose_refused(compose_num(compose), &q(&text));
            }
            Signal::Queued { compose, text } => self.queued(compose_num(compose), &q(&text)),
            Signal::Attached {
                compose,
                path,
                name,
                size,
            } => self.attached(
                compose_num(compose),
                &q(&lossy(&path)),
                &q(&name),
                crate::variant::js_num(i64::try_from(size).unwrap_or(i64::MAX)),
            ),
            Signal::AttachFailed { compose, text } => {
                self.attach_failed(compose_num(compose), &q(&text));
            }
            Signal::Settings(settings) => self.as_mut().settings_arrived(settings),
            Signal::OlderRow(row) => self.set_older_row(convert::older_row_code(row)),
            Signal::SearchFailed(t) => self.search_failed(&q(&t)),
            Signal::SearchDone => self.search_done(),
            Signal::Status { account, text } => self.status_text(&q(&account), &q(&text)),
        }
    }

    fn settings_arrived(mut self: Pin<&mut Self>, settings: Settings) {
        self.as_mut()
            .set_theme(QString::from(convert::theme_word(settings.theme)));
        self.as_mut()
            .set_dark_mode(QString::from(convert::dark_word(settings.dark)));
        self.as_mut()
            .set_sync_days(i32::try_from(settings.sync_window_days).unwrap_or(30));
        self.as_mut()
            .set_images_always(settings.images == envio_core::ImagesDefault::Always);
        self.as_mut().set_default_account(QString::from(
            settings.default_account.as_deref().unwrap_or(""),
        ));
        let mut senders = QStringList::default();
        for address in &settings.images_from {
            senders.append(QString::from(address.as_str()));
        }
        self.as_mut().set_allowed_senders(senders);
        self.as_mut().rust_mut().settings = settings;
    }
}

fn lossy(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn compose_num(id: ComposeId) -> f64 {
    crate::variant::js_num(i64::try_from(id.0).unwrap_or(0))
}
