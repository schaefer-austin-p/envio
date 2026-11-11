//! The invokables of `EnvioCore`: each is one call into `envio_ui::Ui` or
//! `Core::send`, after turning QML's strings into core types.

use std::path::PathBuf;
use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use envio_core::{
    Command, ConnectRequest, ImagesChoice, MessageRowId, UndoId, count_text, ispdb_note,
};
use secrecy::SecretString;
use zeroize::Zeroize;

use crate::convert as cv;
use crate::core_object::qobject;
use crate::lifecycle::clock;

fn s(q: &QString) -> String {
    String::from(q)
}

/// A password never lives as anything but a `SecretString` on the Rust side.
/// The secret is copied at its exact size (no reallocation that would leave
/// an unzeroed buffer behind) and the intermediate `String` is zeroized.
fn secret(q: &QString) -> SecretString {
    let mut text = String::from(q);
    let secret = SecretString::from(text.as_str());
    text.zeroize();
    secret
}

impl qobject::EnvioCore {
    pub fn set_active(self: Pin<&mut Self>, active: bool) {
        self.send_command(Command::AppActive(active));
    }

    pub fn refresh(self: Pin<&mut Self>, account: &QString) {
        let account = Some(s(account)).filter(|a| !a.is_empty());
        self.send_command(Command::Refresh { account });
    }

    pub fn ispdb_note(&self) -> QString {
        QString::from(ispdb_note())
    }

    pub fn detect(mut self: Pin<&mut Self>, email: &QString) {
        {
            let ui = &mut self.as_mut().rust_mut().ui;
            ui.clear_accepted();
            ui.begin_setup(&s(email));
        }
        self.send_command(Command::Detect { email: s(email) });
    }

    /// `setup_json` is what `detected` carried; the password is not logged.
    pub fn connect(
        mut self: Pin<&mut Self>,
        setup_json: &QString,
        password: &QString,
        servers_confirmed: bool,
    ) {
        let password = secret(password);
        let setup = match crate::json::setup_from_json(&s(setup_json)) {
            Ok(setup) => setup,
            Err(error) => {
                log::warn!("ignored a malformed account setup: {error}");
                return;
            }
        };
        self.as_mut().rust_mut().ui.begin_setup(&setup.email);
        let accepted = self.rust().ui.accepted().to_vec();
        let request = ConnectRequest {
            setup,
            password,
            servers_confirmed,
            accepted,
        };
        self.as_mut()
            .send_command(Command::Connect(Box::new(request)));
    }

    /// Remember a certificate the user accepted while setting up; the next
    /// `connect` carries it.
    pub fn accept_setup_certificate(mut self: Pin<&mut Self>, service: &QString, hex: &QString) {
        match cv::service_of(&s(service)) {
            Some(service) => {
                self.as_mut()
                    .rust_mut()
                    .ui
                    .accept_setup_certificate(service, s(hex));
            }
            None => log::warn!("ignored a certificate for an unknown service"),
        }
    }

    pub fn accept_certificate(
        self: Pin<&mut Self>,
        account: &QString,
        service: &QString,
        hex: &QString,
    ) {
        let Some(service) = cv::service_of(&s(service)) else {
            log::warn!("ignored a certificate for an unknown service");
            return;
        };
        self.send_command(Command::AcceptCertificate {
            account: s(account),
            service,
            fingerprint: s(hex),
        });
    }

    pub fn update_password(self: Pin<&mut Self>, account: &QString, password: &QString) {
        let password = secret(password);
        self.send_command(Command::UpdatePassword {
            account: s(account),
            password,
        });
    }

    pub fn remove_account(self: Pin<&mut Self>, account: &QString, confirmed: bool) {
        self.send_command(Command::RemoveAccount {
            account: s(account),
            confirmed,
        });
    }

    pub fn list_folders(self: Pin<&mut Self>, account: &QString) {
        self.send_command(Command::ListFolders {
            account: s(account),
        });
    }

    pub fn open_list(mut self: Pin<&mut Self>, kind: &QString, id: i64) {
        let Some(list) = cv::list_ref(&s(kind), id) else {
            return;
        };
        let (command, outs) = self.as_mut().rust_mut().ui.open_list(list);
        self.as_mut().run(outs);
        self.send_command(command);
    }

    pub fn more(mut self: Pin<&mut Self>) {
        if let Some(command) = self.as_mut().rust_mut().ui.more() {
            self.send_command(command);
        }
    }

    pub fn select(mut self: Pin<&mut Self>, id: i64, on: bool) {
        let outs = self.as_mut().rust_mut().ui.select(id, on);
        self.run(outs);
    }

    pub fn clear_selection(mut self: Pin<&mut Self>) {
        let outs = self.as_mut().rust_mut().ui.clear_selection();
        self.run(outs);
    }

    pub fn act(mut self: Pin<&mut Self>, action: &QString, folder: i64) {
        let Some(action) = cv::user_action(&s(action), folder) else {
            return;
        };
        if let Some((command, outs)) = self.as_mut().rust_mut().ui.act_on_selection(action) {
            self.as_mut().run(outs);
            self.send_command(command);
        }
    }

    pub fn undo(self: Pin<&mut Self>, id: i64) {
        let undo = UndoId(u64::try_from(id).unwrap_or(0));
        self.send_command(Command::Undo { undo });
    }

    pub fn open(self: Pin<&mut Self>, id: i64, load_images: bool, width_px: i32) {
        self.send_command(Command::OpenMessage {
            id: MessageRowId(id),
            images: if load_images {
                ImagesChoice::Load
            } else {
                ImagesChoice::Default
            },
            width_px: u32::try_from(width_px).unwrap_or(0),
        });
    }

    pub fn allow_images_from(self: Pin<&mut Self>, address: &QString) {
        self.send_command(Command::AllowImagesFrom {
            address: s(address),
        });
    }

    pub fn save_attachment(self: Pin<&mut Self>, id: i64, index: i32, dir: &QString) {
        self.send_command(Command::SaveAttachment {
            id: MessageRowId(id),
            index: usize::try_from(index).unwrap_or(usize::MAX),
            dir: PathBuf::from(s(dir)),
        });
    }

    /// `mailto` is the mailto URL for kind "mailto" and the stored draft id
    /// for kind "draft". Returns the compose id (0 when the kind is unknown).
    #[allow(clippy::too_many_arguments)] // the QML signature
    pub fn start_compose(
        mut self: Pin<&mut Self>,
        kind: &QString,
        id: i64,
        account: &QString,
        mailto: &QString,
    ) -> i64 {
        let Some((from, draft_id, mailto)) = cv::start_from(&s(kind), id, &s(account), &s(mailto))
        else {
            return 0;
        };
        let compose = {
            let mut state = self.as_mut().rust_mut();
            state.next_compose += 1;
            state.next_compose
        };
        self.send_command(Command::StartCompose {
            compose: cv::compose_id(compose),
            from,
            draft_id,
            mailto,
        });
        compose
    }

    pub fn draft_changed(self: Pin<&mut Self>, compose: i64, draft_json: &QString) {
        match crate::json::draft_from_json(&s(draft_json)) {
            Ok(draft) => self.send_command(Command::DraftChanged {
                compose: cv::compose_id(compose),
                draft,
            }),
            Err(error) => log::warn!("ignored a malformed draft: {error}"),
        }
    }

    pub fn close_compose(self: Pin<&mut Self>, compose: i64, discard: bool) {
        self.send_command(Command::CloseCompose {
            compose: cv::compose_id(compose),
            discard,
        });
    }

    pub fn send(self: Pin<&mut Self>, compose: i64, draft_json: &QString) {
        match crate::json::draft_from_json(&s(draft_json)) {
            Ok(draft) => self.send_command(Command::Send {
                compose: cv::compose_id(compose),
                draft,
            }),
            Err(error) => log::warn!("ignored a malformed draft: {error}"),
        }
    }

    pub fn attach(self: Pin<&mut Self>, compose: i64, path: &QString) {
        self.send_command(Command::Attach {
            compose: cv::compose_id(compose),
            path: PathBuf::from(s(path)),
        });
    }

    pub fn list_outbox(self: Pin<&mut Self>) {
        self.send_command(Command::ListOutbox);
    }

    pub fn decide_outbox(self: Pin<&mut Self>, item: &QString, decision: &QString) {
        if let Some(decision) = cv::decision(&s(decision)) {
            self.send_command(Command::DecideOutbox {
                item: s(item),
                decision,
            });
        }
    }

    /// Returns the compose id the refused message opens in.
    pub fn edit_outbox(mut self: Pin<&mut Self>, item: &QString) -> i64 {
        let compose = {
            let mut state = self.as_mut().rust_mut();
            state.next_compose += 1;
            state.next_compose
        };
        self.send_command(Command::EditOutbox {
            item: s(item),
            compose: cv::compose_id(compose),
        });
        compose
    }

    /// `scope` is "all", "account:<key>" or "folder:<id>".
    pub fn search(mut self: Pin<&mut Self>, scope: &QString, query: &QString, server: bool) {
        let Some(scope) = cv::search_scope(&s(scope)) else {
            return;
        };
        let (command, outs) = self.as_mut().rust_mut().ui.search(scope, s(query), server);
        self.as_mut().run(outs);
        self.send_command(command);
    }

    pub fn autocomplete(self: Pin<&mut Self>, query: &QString) {
        self.send_command(Command::Autocomplete { query: s(query) });
    }

    pub fn add_contact(self: Pin<&mut Self>, name: &QString, address: &QString) {
        self.send_command(Command::AddContact {
            name: s(name),
            address: s(address),
        });
    }

    #[allow(clippy::too_many_arguments)] // the QML signature
    pub fn save_settings(
        self: Pin<&mut Self>,
        theme: &QString,
        dark_mode: &QString,
        images_always: bool,
        default_account: &QString,
    ) {
        let edit = cv::SettingsEdit {
            theme: &s(theme),
            dark: &s(dark_mode),
            images_always,
            default_account: &s(default_account),
        };
        let settings = cv::settings_with(&self.rust().settings, &edit);
        self.send_command(Command::SaveSettings(settings));
    }

    /// `kind` is "selected", "results", "waiting" or "newmail".
    pub fn count_text(&self, kind: &QString, n: i32) -> QString {
        let text = cv::count_kind(&s(kind))
            .map(|kind| count_text(kind, u64::try_from(n).unwrap_or(0)))
            .unwrap_or_default();
        QString::from(text)
    }

    /// "time", "weekday", "daymonth", "full" or "none"; QML formats it.
    pub fn date_bucket(&self, unix_secs: i64) -> QString {
        let (now, offset) = clock();
        QString::from(envio_ui::bucket(now, Some(unix_secs), offset).as_str())
    }
}
