//! What `Ui` produced, mirrored onto the models and Qt signals. Model
//! changes, signals and the property sets are all queued on the Qt thread
//! in `Ui`'s order, so QML sees a signal only after the rows it describes.

use std::path::Path;
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use envio_core::{Command, ComposeId, Event, OAuthFailure, OAuthOutcome, Settings};
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
    pub(crate) fn run(mut self: Pin<&mut Self>, outs: Vec<Output>) {
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
        self.as_mut().sync_open_message();
        self.as_mut().sync_focus_message();
        self.sync_list_flags();
    }

    /// `listIsDrafts` and `selectionHasLocal` follow `Ui`, queued like
    /// `sync_open_message`.
    fn sync_list_flags(self: Pin<&mut Self>) {
        let (drafts, local) = (
            self.rust().ui.list_is_drafts(),
            self.rust().ui.selection_has_local(),
        );
        let queued = self.qt_thread().queue(move |mut obj| {
            if obj.rust().list_is_drafts != drafts {
                obj.as_mut().rust_mut().list_is_drafts = drafts;
                obj.as_mut().list_is_drafts_changed();
            }
            if obj.rust().selection_has_local != local {
                obj.as_mut().rust_mut().selection_has_local = local;
                obj.as_mut().selection_has_local_changed();
            }
        });
        if queued.is_err() {
            log::info!("EnvioCore is gone; dropped the list flags");
        }
    }

    /// The open message can clear by itself (its row left, the list
    /// changed): the property follows `Ui`.
    ///
    /// Queued like the signals, behind the row changes of the same `run`,
    /// so a handler of `openMessageChanged` sees the rows it describes.
    fn sync_open_message(self: Pin<&mut Self>) {
        let open = self.rust().ui.open_message().unwrap_or(0);
        let queued = self.qt_thread().queue(move |mut obj| {
            if obj.rust().open_message != open {
                obj.as_mut().rust_mut().open_message = open;
                obj.as_mut().open_message_changed();
            }
        });
        if queued.is_err() {
            log::info!("EnvioCore is gone; dropped the open message");
        }
    }

    /// `focusMessage` follows `Ui` like `openMessage`.
    fn sync_focus_message(self: Pin<&mut Self>) {
        let focus = self.rust().ui.focus_message().unwrap_or(0);
        let queued = self.qt_thread().queue(move |mut obj| {
            if obj.rust().focus_message != focus {
                obj.as_mut().rust_mut().focus_message = focus;
                obj.as_mut().focus_message_changed();
            }
        });
        if queued.is_err() {
            log::info!("EnvioCore is gone; dropped the focus message");
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
            Signal::ListSwitched { folder } => self.list_switched(crate::variant::js_num(folder)),
            Signal::WorkerRestarted(t) => self.worker_restarted(&q(&t)),
            Signal::Detected {
                email,
                text,
                needs_confirm,
                setup,
                servers,
                oauth,
            } => self.detected(
                &q(&email),
                &q(&text),
                needs_confirm,
                &q(&json::setup_to_json(&setup)),
                &q(&servers),
                &q(oauth.as_ref().map_or("", |o| o.provider.as_str())),
            ),
            Signal::SetupProblem { email, text } => self.setup_problem(&q(&email), &q(&text)),
            Signal::SetupRefused {
                email,
                title,
                text,
                manual_ok,
            } => {
                self.setup_refused(&q(&email), &q(&title), &q(&text), manual_ok)
            }
            Signal::MailOnMicrosoft { email, title, text } => {
                self.mail_on_microsoft(&q(&email), &q(&title), &q(&text))
            }
            Signal::OAuthBrowser { email, url, .. } => self.oauth_browser(&q(&email), &q(&url)),
            Signal::OAuthConfirm {
                email,
                signed_in_as,
                text,
            } => self.oauth_confirm(&q(&email), &q(&signed_in_as.unwrap_or_default()), &q(&text)),
            Signal::OAuthMismatch { email, title, text } => {
                self.oauth_mismatch(&q(&email), &q(&title), &q(&text));
            }
            Signal::OAuthDone { email, outcome } => {
                let (name, text) = match outcome {
                    OAuthOutcome::Added => ("added", String::new()),
                    OAuthOutcome::Resumed => ("resumed", String::new()),
                    OAuthOutcome::Cancelled => ("cancelled", String::new()),
                    OAuthOutcome::Failed(why @ OAuthFailure::Admin) => ("admin", why.text()),
                    OAuthOutcome::Failed(
                        why @ (OAuthFailure::Mismatch | OAuthFailure::NoTenant),
                    ) => ("mismatch", why.text()),
                    OAuthOutcome::Failed(why) => ("failed", why.text()),
                };
                self.oauth_done(&q(&email), &q(name), &q(&text));
            }
            Signal::TrustPrompt {
                email,
                service,
                host,
                title,
                fingerprint,
                hex,
                text,
            } => self.trust_prompt(
                &q(&email),
                &q(convert::service_word(service)),
                &q(&host),
                &q(&title),
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
            Signal::InviteExported { id, path } => {
                self.invite_exported(crate::variant::js_num(id), &q(&lossy(&path)));
            }
            Signal::InviteExportFailed { id } => {
                self.invite_export_failed(crate::variant::js_num(id));
            }
            Signal::UndoOffer { undo, text, scope } => {
                let scope = match scope {
                    envio_core::UndoScope::List => "list",
                    envio_core::UndoScope::Reader => "reader",
                };
                self.undo_offer(
                    crate::variant::js_num(i64::try_from(undo.0).unwrap_or(0)),
                    &q(&text),
                    &q(scope),
                );
            }
            Signal::UndoClosed { undo } => {
                self.undo_closed(crate::variant::js_num(i64::try_from(undo.0).unwrap_or(0)));
            }
            Signal::ComposeReady {
                compose,
                draft,
                quoted,
                note,
            } => {
                let map = map_of(&json::draft_value(&draft));
                self.compose_ready(compose_num(compose), &map, &q(&quoted), &q(&note));
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
            .set_notify_show_text(settings.notify_show_text);
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
