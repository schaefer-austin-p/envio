//! `EnvioCore`: the one QObject QML talks to. It owns the `Core` handle and
//! the `Ui` state; events come in through `QtSink` on the Qt thread, `Ui`
//! turns them into model changes and signals, and this file only mirrors
//! those onto the models and Qt signals. The invokables are in `calls.rs`.

// Signal and invokable signatures are the QML API: flat arguments, so the
// repo argument-count gate does not fit this module (the only one it is
// off for).
#![allow(clippy::too_many_arguments)]

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;

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
        #[qproperty(QStringList, allowed_senders, cxx_name = "allowedSenders")]
        #[qproperty(i32, older_row, cxx_name = "olderRow")]
        #[qproperty(i32, sync_days, cxx_name = "syncDays")]
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
            title: &QString,
            fingerprint: &QString,
            hex: &QString,
            text: &QString,
        );
        #[qsignal]
        #[cxx_name = "accountAdded"]
        fn account_added(self: Pin<&mut EnvioCore>, account: &QString, email: &QString);
        #[qsignal]
        #[cxx_name = "confirmRemove"]
        fn confirm_remove(self: Pin<&mut EnvioCore>, account: &QString, text: &QString);
        #[qsignal]
        #[cxx_name = "messageOpened"]
        fn message_opened(self: Pin<&mut EnvioCore>, view: &QMap_QString_QVariant);
        #[qsignal]
        #[cxx_name = "messageUnavailable"]
        fn message_unavailable(self: Pin<&mut EnvioCore>, id: f64, text: &QString);
        #[qsignal]
        #[cxx_name = "attachmentSaved"]
        fn attachment_saved(self: Pin<&mut EnvioCore>, path: &QString);
        #[qsignal]
        #[cxx_name = "attachmentFailed"]
        fn attachment_failed(self: Pin<&mut EnvioCore>, id: f64);
        #[qsignal]
        #[cxx_name = "undoOffer"]
        fn undo_offer(self: Pin<&mut EnvioCore>, undo: f64, text: &QString);
        #[qsignal]
        #[cxx_name = "undoClosed"]
        fn undo_closed(self: Pin<&mut EnvioCore>, undo: f64);
        #[qsignal]
        #[cxx_name = "composeReady"]
        fn compose_ready(
            self: Pin<&mut EnvioCore>,
            compose: f64,
            draft: &QMap_QString_QVariant,
            quoted: &QString,
        );
        #[qsignal]
        #[cxx_name = "draftSaved"]
        fn draft_saved(self: Pin<&mut EnvioCore>, compose: f64);
        #[qsignal]
        #[cxx_name = "composeRefused"]
        fn compose_refused(self: Pin<&mut EnvioCore>, compose: f64, text: &QString);
        #[qsignal]
        fn queued(self: Pin<&mut EnvioCore>, compose: f64, text: &QString);
        #[qsignal]
        fn attached(
            self: Pin<&mut EnvioCore>,
            compose: f64,
            path: &QString,
            name: &QString,
            size: f64,
        );
        #[qsignal]
        #[cxx_name = "attachFailed"]
        fn attach_failed(self: Pin<&mut EnvioCore>, compose: f64, text: &QString);
        #[qsignal]
        #[cxx_name = "searchFailed"]
        fn search_failed(self: Pin<&mut EnvioCore>, text: &QString);
        #[qsignal]
        #[cxx_name = "searchDone"]
        fn search_done(self: Pin<&mut EnvioCore>);
        #[qsignal]
        #[cxx_name = "statusText"]
        fn status_text(self: Pin<&mut EnvioCore>, account: &QString, text: &QString);
        /// `Core::start` failed; `text` is translated.
        #[qsignal]
        #[cxx_name = "startFailed"]
        fn start_failed(self: Pin<&mut EnvioCore>, text: &QString);
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
        #[cxx_name = "loadOlder"]
        fn load_older(self: Pin<&mut EnvioCore>);
        #[qinvokable]
        #[cxx_name = "setSyncWindow"]
        fn set_sync_window(self: Pin<&mut EnvioCore>, days: i32);
        #[qinvokable]
        fn select(self: Pin<&mut EnvioCore>, id: i64, on: bool);
        #[qinvokable]
        #[cxx_name = "clearSelection"]
        fn clear_selection(self: Pin<&mut EnvioCore>);
        #[qinvokable]
        fn act(self: Pin<&mut EnvioCore>, action: &QString, folder: i64) -> bool;
        #[qinvokable]
        fn undo(self: Pin<&mut EnvioCore>, id: i64);
        #[qinvokable]
        fn open(self: Pin<&mut EnvioCore>, id: i64, load_images: bool, width_px: i32);
        #[qinvokable]
        #[cxx_name = "allowImagesFrom"]
        fn allow_images_from(self: Pin<&mut EnvioCore>, address: &QString);
        #[qinvokable]
        #[cxx_name = "disallowImagesFrom"]
        fn disallow_images_from(self: Pin<&mut EnvioCore>, address: &QString);
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
        ) -> f64;
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
        fn edit_outbox(self: Pin<&mut EnvioCore>, item: &QString) -> f64;
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
        #[cxx_name = "linkView"]
        fn link_view(self: &EnvioCore, url: &QString) -> QMap_QString_QVariant;
        #[qinvokable]
        #[cxx_name = "dateBucket"]
        fn date_bucket(self: &EnvioCore, unix_secs: i64) -> QString;
    }

    // The models are created in QML (`RowsModel { kind: "messages" }`;
    // cxx-qt cannot construct a QObject from Rust) and handed over with
    // `attachModel`, which brings them up to date.
    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_name = "attachModel"]
        unsafe fn attach_model(self: Pin<&mut EnvioCore>, model: *mut RowsModel);
    }
}

use std::collections::HashMap;

use cxx_qt::CxxQtThread;
use cxx_qt_lib::{QString, QStringList};
use envio_core::{Core, Settings};
use envio_ui::{Model, Ui};

use crate::models::qobject::RowsModel;

#[derive(Default)]
pub struct EnvioCoreRust {
    pub(crate) ready: bool,
    pub(crate) theme: QString,
    pub(crate) dark_mode: QString,
    pub(crate) images_always: bool,
    pub(crate) default_account: QString,
    pub(crate) allowed_senders: QStringList,
    /// `envio_ui::OlderRow` as 0 hidden, 1 ready, 2 loading, 3 done.
    pub(crate) older_row: i32,
    /// The sync window in days; 0 is all mail.
    pub(crate) sync_days: i32,
    pub(crate) core: Option<Core>,
    pub(crate) ui: Ui,
    pub(crate) models: HashMap<Model, Box<CxxQtThread<RowsModel>>>,
    /// The settings as last received: `saveSettings` changes four fields.
    pub(crate) settings: Settings,
    pub(crate) next_compose: i64,
    pub(crate) opened_inboxes: bool,
}
