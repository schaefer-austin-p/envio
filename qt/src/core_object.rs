//! `EnvioCore`: the one QObject QML talks to. It owns the `Core` handle and
//! the `Ui` state; events come in through `QtSink` on the Qt thread, `Ui`
//! turns them into model changes and signals, and this file only mirrors
//! those onto the models and Qt signals. The invokables are in `calls.rs`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    unsafe extern "C++" {
        include!("envio_qt/src/models.cxxqt.h");
        type RowsModel = crate::models::qobject::RowsModel;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, ready)]
        #[qproperty(QString, theme)]
        #[qproperty(QString, dark_mode, cxx_name = "darkMode")]
        #[qproperty(bool, images_always, cxx_name = "imagesAlways")]
        #[qproperty(QString, default_account, cxx_name = "defaultAccount")]
        type EnvioCore = super::EnvioCoreRust;
    }

    impl cxx_qt::Threading for EnvioCore {}

    // One signal per envio_ui::Signal variant, with flat arguments.
    extern "RustQt" {
        #[qsignal]
        fn notice(self: Pin<&mut EnvioCore>, text: &QString);
        #[qsignal]
        #[cxx_name = "storeProblem"]
        fn store_problem(self: Pin<&mut EnvioCore>, text: &QString);
        #[qsignal]
        #[cxx_name = "workerRestarted"]
        fn worker_restarted(self: Pin<&mut EnvioCore>, text: &QString);
        #[qsignal]
        fn detected(
            self: Pin<&mut EnvioCore>,
            email: &QString,
            text: &QString,
            needs_confirm: bool,
            setup_json: &QString,
        );
        #[qsignal]
        #[cxx_name = "setupProblem"]
        fn setup_problem(self: Pin<&mut EnvioCore>, email: &QString, text: &QString);
        #[qsignal]
        #[cxx_name = "trustPrompt"]
        fn trust_prompt(
            self: Pin<&mut EnvioCore>,
            email: &QString,
            service: &QString,
            host: &QString,
            fingerprint: &QString,
            hex: &QString,
            text: &QString,
        );
        #[qsignal]
        #[cxx_name = "accountAdded"]
        fn account_added(self: Pin<&mut EnvioCore>, account: &QString);
        #[qsignal]
        #[cxx_name = "confirmRemove"]
        fn confirm_remove(self: Pin<&mut EnvioCore>, account: &QString, text: &QString);
        #[qsignal]
        #[cxx_name = "messageOpened"]
        fn message_opened(self: Pin<&mut EnvioCore>, view: &QMap_QString_QVariant);
        #[qsignal]
        #[cxx_name = "messageUnavailable"]
        fn message_unavailable(self: Pin<&mut EnvioCore>, id: i64, text: &QString);
        #[qsignal]
        #[cxx_name = "attachmentSaved"]
        fn attachment_saved(self: Pin<&mut EnvioCore>, path: &QString);
        #[qsignal]
        #[cxx_name = "undoOffer"]
        fn undo_offer(self: Pin<&mut EnvioCore>, undo: i64, text: &QString);
        #[qsignal]
        #[cxx_name = "undoClosed"]
        fn undo_closed(self: Pin<&mut EnvioCore>);
        #[qsignal]
        #[cxx_name = "composeReady"]
        fn compose_ready(
            self: Pin<&mut EnvioCore>,
            compose: i64,
            draft: &QMap_QString_QVariant,
            quoted: &QString,
        );
        #[qsignal]
        #[cxx_name = "draftSaved"]
        fn draft_saved(self: Pin<&mut EnvioCore>, compose: i64);
        #[qsignal]
        #[cxx_name = "composeRefused"]
        fn compose_refused(self: Pin<&mut EnvioCore>, compose: i64, text: &QString);
        #[qsignal]
        fn queued(self: Pin<&mut EnvioCore>, compose: i64);
        #[qsignal]
        fn attached(
            self: Pin<&mut EnvioCore>,
            compose: i64,
            path: &QString,
            name: &QString,
            size: i64,
        );
        #[qsignal]
        #[cxx_name = "attachFailed"]
        fn attach_failed(self: Pin<&mut EnvioCore>, compose: i64, text: &QString);
        #[qsignal]
        #[cxx_name = "searchFailed"]
        fn search_failed(self: Pin<&mut EnvioCore>, text: &QString);
        #[qsignal]
        #[cxx_name = "statusText"]
        fn status_text(self: Pin<&mut EnvioCore>, account: &QString, text: &QString);
    }

    // Invokables (bodies in calls.rs).
    extern "RustQt" {
        #[qinvokable]
        fn start(self: Pin<&mut EnvioCore>);
        #[qinvokable]
        fn stop(self: Pin<&mut EnvioCore>);
        #[qinvokable]
        #[cxx_name = "setActive"]
        fn set_active(self: Pin<&mut EnvioCore>, active: bool);
        #[qinvokable]
        fn refresh(self: Pin<&mut EnvioCore>, account: &QString);
        #[qinvokable]
        #[cxx_name = "ispdbNote"]
        fn ispdb_note(self: &EnvioCore) -> QString;
        #[qinvokable]
        fn detect(self: Pin<&mut EnvioCore>, email: &QString);
        #[qinvokable]
        fn connect(
            self: Pin<&mut EnvioCore>,
            setup_json: &QString,
            password: &QString,
            servers_confirmed: bool,
        );
        #[qinvokable]
        #[cxx_name = "acceptSetupCertificate"]
        fn accept_setup_certificate(self: Pin<&mut EnvioCore>, service: &QString, hex: &QString);
        #[qinvokable]
        #[cxx_name = "acceptCertificate"]
        fn accept_certificate(
            self: Pin<&mut EnvioCore>,
            account: &QString,
            service: &QString,
            hex: &QString,
        );
        #[qinvokable]
        #[cxx_name = "updatePassword"]
        fn update_password(self: Pin<&mut EnvioCore>, account: &QString, password: &QString);
        #[qinvokable]
        #[cxx_name = "removeAccount"]
        fn remove_account(self: Pin<&mut EnvioCore>, account: &QString, confirmed: bool);
        #[qinvokable]
        #[cxx_name = "listFolders"]
        fn list_folders(self: Pin<&mut EnvioCore>, account: &QString);
        #[qinvokable]
        #[cxx_name = "openList"]
        fn open_list(self: Pin<&mut EnvioCore>, kind: &QString, id: i64);
        #[qinvokable]
        fn more(self: Pin<&mut EnvioCore>);
        #[qinvokable]
        fn select(self: Pin<&mut EnvioCore>, id: i64, on: bool);
        #[qinvokable]
        #[cxx_name = "clearSelection"]
        fn clear_selection(self: Pin<&mut EnvioCore>);
        #[qinvokable]
        fn act(self: Pin<&mut EnvioCore>, action: &QString, folder: i64);
        #[qinvokable]
        fn undo(self: Pin<&mut EnvioCore>, id: i64);
        #[qinvokable]
        fn open(self: Pin<&mut EnvioCore>, id: i64, load_images: bool, width_px: i32);
        #[qinvokable]
        #[cxx_name = "allowImagesFrom"]
        fn allow_images_from(self: Pin<&mut EnvioCore>, address: &QString);
        #[qinvokable]
        #[cxx_name = "saveAttachment"]
        fn save_attachment(self: Pin<&mut EnvioCore>, id: i64, index: i32, dir: &QString);
        #[qinvokable]
        #[cxx_name = "startCompose"]
        fn start_compose(
            self: Pin<&mut EnvioCore>,
            kind: &QString,
            id: i64,
            account: &QString,
            mailto: &QString,
        ) -> i64;
        #[qinvokable]
        #[cxx_name = "draftChanged"]
        fn draft_changed(self: Pin<&mut EnvioCore>, compose: i64, draft_json: &QString);
        #[qinvokable]
        #[cxx_name = "closeCompose"]
        fn close_compose(self: Pin<&mut EnvioCore>, compose: i64, discard: bool);
        #[qinvokable]
        fn send(self: Pin<&mut EnvioCore>, compose: i64, draft_json: &QString);
        #[qinvokable]
        fn attach(self: Pin<&mut EnvioCore>, compose: i64, path: &QString);
        #[qinvokable]
        #[cxx_name = "listOutbox"]
        fn list_outbox(self: Pin<&mut EnvioCore>);
        #[qinvokable]
        #[cxx_name = "decideOutbox"]
        fn decide_outbox(self: Pin<&mut EnvioCore>, item: &QString, decision: &QString);
        #[qinvokable]
        #[cxx_name = "editOutbox"]
        fn edit_outbox(self: Pin<&mut EnvioCore>, item: &QString) -> i64;
        #[qinvokable]
        fn search(self: Pin<&mut EnvioCore>, scope: &QString, query: &QString, server: bool);
        #[qinvokable]
        fn autocomplete(self: Pin<&mut EnvioCore>, query: &QString);
        #[qinvokable]
        #[cxx_name = "addContact"]
        fn add_contact(self: Pin<&mut EnvioCore>, name: &QString, address: &QString);
        #[qinvokable]
        #[cxx_name = "saveSettings"]
        fn save_settings(
            self: Pin<&mut EnvioCore>,
            theme: &QString,
            dark_mode: &QString,
            images_always: bool,
            default_account: &QString,
        );
        #[qinvokable]
        #[cxx_name = "countText"]
        fn count_text(self: &EnvioCore, kind: &QString, n: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "dateBucket"]
        fn date_bucket(self: &EnvioCore, unix_secs: i64) -> QString;
    }

    // The models are created in QML (`RowsModel { kind: "messages" }`) and
    // handed over once; see the report for why.
    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_name = "attachModel"]
        unsafe fn attach_model(self: Pin<&mut EnvioCore>, model: *mut RowsModel);
    }
}

use std::collections::HashMap;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::{QDateTime, QString, QTimeZone};
use envio_core::{Command, Config, Core, Event, Roots, RootsSource, Settings, paths};
use envio_ui::{Model, ModelChange, Output, Signal, Ui};

use crate::convert;
use crate::models::qobject::RowsModel;
use crate::sink::QtSink;
use crate::variant::map_of;

#[derive(Default)]
pub struct EnvioCoreRust {
    ready: bool,
    theme: QString,
    dark_mode: QString,
    images_always: bool,
    default_account: QString,
    pub(crate) core: Option<Core>,
    pub(crate) ui: Ui,
    models: HashMap<Model, Box<CxxQtThread<RowsModel>>>,
    /// The settings as last received: `saveSettings` changes four fields.
    pub(crate) settings: Settings,
    /// Certificates accepted for the account being set up; the next
    /// `connect` carries them.
    pub(crate) accepted: Vec<(envio_core::Service, String)>,
    pub(crate) next_compose: i64,
    opened_inboxes: bool,
}

/// The wall clock and the local UTC offset, for `Ui` (which has no clock).
pub(crate) fn clock() -> (i64, i32) {
    let now = QDateTime::current_secs_since_epoch();
    let zone = QTimeZone::system_time_zone();
    let offset = zone.as_ref().map_or(0, |z| {
        z.offset_from_utc(&QDateTime::from_secs_since_epoch(now, z))
    });
    (now, offset)
}

fn log_roots(roots: &Roots) {
    let source = match roots.source() {
        RootsSource::System => "system",
        RootsSource::Bundled => "bundled",
        RootsSource::Empty => "empty",
    };
    log::warn!("CA roots: source {source}, {} certificates", roots.len());
    for error in roots.load_errors() {
        log::warn!("CA roots: {error}");
    }
}

impl qobject::EnvioCore {
    /// Start the core: folders, roots, worker. `Started` arrives later.
    pub fn start(mut self: Pin<&mut Self>) {
        if self.rust().core.is_some() {
            return;
        }
        let env = |name: &str| std::env::var(name).ok();
        let (cache_dir, data_dir) = paths::app_dirs(&env);
        let roots = Roots::system_or_bundled();
        let config = Config {
            cache_dir,
            data_dir,
            roots: roots.clone(),
            log_level: log::LevelFilter::Info,
            foreground: true,
        };
        let sink = Arc::new(QtSink::new(self.qt_thread()));
        match Core::start(config, sink) {
            Ok(core) => {
                // After the start: the file logger is up only now (#37).
                log_roots(&roots);
                self.as_mut().rust_mut().core = Some(core);
            }
            Err(error) => {
                eprintln!("envio: the core did not start: {error}");
                log::error!("the core did not start: {error}");
            }
        }
    }

    /// Flush unsent work and stop the worker (also done when QML drops
    /// the object).
    pub fn stop(mut self: Pin<&mut Self>) {
        if let Some(core) = self.as_mut().rust_mut().core.take() {
            core.shutdown();
        }
    }

    /// # Safety
    ///
    /// `model` is null or points to a live `RowsModel`.
    pub unsafe fn attach_model(mut self: Pin<&mut Self>, model: *mut RowsModel) {
        // SAFETY: by the contract above; QML owns the model.
        let Some(model) = (unsafe { model.as_mut() }) else {
            return;
        };
        // SAFETY: the model stays where Qt put it.
        let mut model = unsafe { Pin::new_unchecked(model) };
        let kind = String::from(model.as_ref().kind());
        let Some(which) = crate::models::model_of(&kind) else {
            log::warn!("a RowsModel with unknown kind {kind:?} was not attached");
            return;
        };
        self.as_mut()
            .rust_mut()
            .models
            .insert(which, Box::new(model.as_mut().qt_thread()));
    }

    /// An event from the core, on the Qt thread.
    pub(crate) fn apply(mut self: Pin<&mut Self>, event: Event) {
        let started = matches!(event, Event::Started { .. });
        let (now, offset) = clock();
        let outs = self.as_mut().rust_mut().ui.apply(event, now, offset);
        self.as_mut().run(outs);
        if started {
            self.as_mut().started();
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

    /// Mirror what `Ui` produced: model changes, signals, commands.
    pub(crate) fn run(mut self: Pin<&mut Self>, outs: Vec<Output>) {
        for out in outs {
            match out {
                Output::Change(change) => self.as_ref().change_model(change),
                Output::Signal(signal) => self.as_mut().emit_signal(signal),
                Output::Send(command) => self.as_mut().send_command(command),
            }
        }
    }

    pub(crate) fn send_command(self: Pin<&mut Self>, command: Command) {
        match &self.rust().core {
            Some(core) => core.send(command),
            None => log::warn!("the core is not running; dropped {}", command.name()),
        }
    }

    fn change_model(self: Pin<&Self>, change: ModelChange) {
        let ModelChange { model, op } = change;
        let Some(thread) = self.rust().models.get(&model) else {
            log::warn!("no {model:?} model attached; dropped a change");
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
                &q(&convert::setup_to_json(&setup)),
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
            Signal::AccountAdded(a) => self.account_added(&q(&a)),
            Signal::ConfirmRemove { account, text } => self.confirm_remove(&q(&account), &q(&text)),
            Signal::MessageOpened(view) => {
                let map = map_of(&convert::message_view_value(&view));
                self.message_opened(&map);
            }
            Signal::MessageUnavailable { id, text } => self.message_unavailable(id, &q(&text)),
            Signal::AttachmentSaved { path } => self.attachment_saved(&q(&lossy(&path))),
            Signal::UndoOffer { undo, text } => {
                self.undo_offer(i64::try_from(undo.0).unwrap_or(0), &q(&text));
            }
            Signal::UndoClosed => self.undo_closed(),
            Signal::ComposeReady {
                compose,
                draft,
                quoted,
            } => {
                let map = map_of(&convert::draft_value(&draft));
                self.compose_ready(compose_num(compose), &map, &q(&quoted));
            }
            Signal::DraftSaved { compose } => self.draft_saved(compose_num(compose)),
            Signal::ComposeRefused { compose, text } => {
                self.compose_refused(compose_num(compose), &q(&text));
            }
            Signal::Queued { compose } => self.queued(compose_num(compose)),
            Signal::Attached {
                compose,
                path,
                name,
                size,
            } => self.attached(
                compose_num(compose),
                &q(&lossy(&path)),
                &q(&name),
                i64::try_from(size).unwrap_or(i64::MAX),
            ),
            Signal::AttachFailed { compose, text } => {
                self.attach_failed(compose_num(compose), &q(&text));
            }
            Signal::Settings(settings) => self.as_mut().settings_arrived(settings),
            Signal::SearchFailed(t) => self.search_failed(&q(&t)),
            Signal::Status { account, text } => self.status_text(&q(&account), &q(&text)),
        }
    }

    fn settings_arrived(mut self: Pin<&mut Self>, settings: Settings) {
        self.as_mut()
            .set_theme(QString::from(convert::theme_word(settings.theme)));
        self.as_mut()
            .set_dark_mode(QString::from(convert::dark_word(settings.dark)));
        self.as_mut()
            .set_images_always(settings.images == envio_core::ImagesDefault::Always);
        self.as_mut().set_default_account(QString::from(
            settings.default_account.as_deref().unwrap_or(""),
        ));
        self.as_mut().rust_mut().settings = settings;
    }
}

fn lossy(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn compose_num(id: envio_core::ComposeId) -> i64 {
    i64::try_from(id.0).unwrap_or(0)
}
