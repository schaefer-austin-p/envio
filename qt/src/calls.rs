//! The invokables of `EnvioCore`: each is one call into `envio_ui::Ui` or
//! `Core::send`, after turning QML's strings into core types.

use std::path::PathBuf;
use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};
use envio_core::{
    Answer, Command, ConnectRequest, ImagesChoice, MessageRowId, UndoId, app_password_setup,
    count_text, ispdb_note,
};
use secrecy::SecretString;
use zeroize::Zeroize;

use crate::convert as cv;
use crate::core_object::qobject;
use crate::lifecycle::clock;
use crate::variant::map_of;

/// A message id from a JS number in a QVariantList.
#[allow(clippy::cast_possible_truncation)]
fn variant_id(v: &QVariant) -> Option<i64> {
    v.value::<f64>().map(|n| n as i64)
}

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

    /// Asks for a refresh and returns its ticket: `refreshed` carries it
    /// back when that refresh is over.
    pub fn refresh(mut self: Pin<&mut Self>, account: &QString) -> i64 {
        let account = Some(s(account)).filter(|a| !a.is_empty());
        let ticket = {
            let mut state = self.as_mut().rust_mut();
            state.next_refresh += 1;
            state.next_refresh
        };
        self.send_command(Command::Refresh {
            account,
            ticket: ticket as u64,
        });
        ticket
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

    /// `account` is empty for a new account, else the key to sign in again.
    /// `kind` is "graph" or "imap" (the wizard's pick), or empty: the
    /// address's own provider, or the account's.
    pub fn start_oauth(self: Pin<&mut Self>, email: &QString, account: &QString, kind: &QString) {
        let account = s(account);
        let kind = envio_ui::parse_kind(&s(kind));
        self.send_command(Command::StartOAuth {
            email: s(email),
            account: (!account.is_empty()).then_some(account),
            kind,
        });
    }

    /// The setup of an app-password provider ("icloud", "gmail", "yahoo")
    /// for `email`, as `connect` takes it; empty for any other id.
    pub fn app_password_setup(&self, provider: &QString, email: &QString) -> QString {
        let json = app_password_setup(&s(provider), &s(email))
            .map(|setup| crate::json::setup_to_json(&setup))
            .unwrap_or_default();
        QString::from(json)
    }

    /// The link the browser handed back; it is not logged.
    pub fn oauth_redirect(self: Pin<&mut Self>, url: &QString) {
        self.send_command(Command::OAuthRedirect {
            url: SecretString::from(s(url)),
        });
    }

    pub fn confirm_oauth(self: Pin<&mut Self>, accept: bool) {
        self.send_command(Command::ConfirmOAuth { accept });
    }

    pub fn cancel_oauth(self: Pin<&mut Self>) {
        self.send_command(Command::CancelOAuth);
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

    /// "Switch anyway" on a stuck move to Graph.
    pub fn switch_anyway(self: Pin<&mut Self>, account: &QString) {
        self.send_command(Command::SwitchAnyway {
            account: s(account),
        });
    }

    pub fn remove_account(self: Pin<&mut Self>, account: &QString, confirmed: bool) {
        self.send_command(Command::RemoveAccount {
            account: s(account),
            confirmed,
        });
    }

    pub fn set_account_label(self: Pin<&mut Self>, account: &QString, label: &QString) {
        self.send_command(Command::SetAccountLabel {
            account: s(account),
            label: s(label),
        });
    }

    pub fn list_folders(self: Pin<&mut Self>, account: &QString) {
        self.send_command(Command::ListFolders {
            account: s(account),
        });
    }

    pub fn drawer_account(mut self: Pin<&mut Self>, account: &QString) {
        let account = s(account);
        let account = (!account.is_empty()).then_some(account);
        self.as_mut().rust_mut().ui.set_drawer_account(account);
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

    /// Mark `id` as the open message (0 clears).
    pub fn set_open_message(mut self: Pin<&mut Self>, id: i64) {
        let outs = self
            .as_mut()
            .rust_mut()
            .ui
            .set_open((id != 0).then_some(id));
        self.run(outs);
    }

    /// Move the keyboard-focus row `delta` rows (Up/Down).
    pub fn step_focus(mut self: Pin<&mut Self>, delta: i32) {
        let outs = self.as_mut().rust_mut().ui.step_focus(delta);
        self.run(outs);
    }

    /// The message Enter opens: the focused row, else the open message; 0
    /// for neither.
    pub fn open_focused(&self) -> i64 {
        self.rust().ui.open_focused().unwrap_or(0)
    }

    /// The local draft behind a shown row (Enter opens it in Compose); empty
    /// for a server message.
    pub fn draft_id_of(&self, id: i64) -> QString {
        QString::from(self.rust().ui.draft_id_of(id).unwrap_or_default())
    }

    /// The position of a shown row in the list, -1 when it is not shown.
    pub fn message_index(&self, id: i64) -> i64 {
        self.rust().ui.row_index(id)
    }

    pub fn clear_selection(mut self: Pin<&mut Self>) {
        let outs = self.as_mut().rust_mut().ui.clear_selection();
        self.run(outs);
    }

    /// Act on the selection; false when there was nothing to act on (the
    /// selected rows are gone from their lists).
    pub fn act(mut self: Pin<&mut Self>, action: &QString, folder: i64) -> bool {
        let Some(action) = cv::user_action(&s(action), folder) else {
            return false;
        };
        let Some((command, outs)) = self.as_mut().rust_mut().ui.act_on_selection(action) else {
            return false;
        };
        self.as_mut().run(outs);
        self.send_command(command);
        true
    }

    /// Act on exactly these messages (ids as JS numbers), leaving the
    /// selection alone; false when none of them is shown any more.
    pub fn act_on(
        mut self: Pin<&mut Self>,
        ids: &QList<QVariant>,
        action: &QString,
        folder: i64,
    ) -> bool {
        let Some(action) = cv::user_action(&s(action), folder) else {
            return false;
        };
        let ids: Vec<i64> = ids.iter().filter_map(variant_id).collect();
        let Some((command, outs)) = self.as_mut().rust_mut().ui.act_on(ids, action) else {
            return false;
        };
        self.as_mut().run(outs);
        self.send_command(command);
        true
    }

    pub fn undo(self: Pin<&mut Self>, id: i64) {
        let undo = UndoId(u64::try_from(id).unwrap_or(0));
        self.send_command(Command::Undo { undo });
    }

    /// `answer` is `accepted`, `tentative` or `declined`; anything else is
    /// ignored.
    pub fn answer_invite(self: Pin<&mut Self>, id: i64, answer: &QString, note: &QString) {
        let Some(answer) = Answer::parse(&s(answer)) else {
            return;
        };
        self.send_command(Command::AnswerInvite {
            id: MessageRowId(id),
            answer,
            note: s(note),
        });
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

    /// The reader's width changed: show message `id` again for `width_px`.
    pub fn refit(self: Pin<&mut Self>, id: i64, width_px: i32) {
        self.send_command(Command::RefitMessage {
            id: MessageRowId(id),
            width_px: u32::try_from(width_px).unwrap_or(0),
        });
    }

    /// The natural sizes of the remote images the reader loaded for
    /// message `id`, as `{"url": [width, height]}`.
    pub fn report_image_sizes(self: Pin<&mut Self>, id: i64, sizes_json: &QString) {
        match crate::json::image_sizes_from_json(&s(sizes_json)) {
            Ok(sizes) => self.send_command(Command::ImageSizes {
                id: MessageRowId(id),
                sizes,
            }),
            Err(error) => log::warn!("ignored malformed image sizes: {error}"),
        }
    }

    pub fn allow_images_from(self: Pin<&mut Self>, address: &QString) {
        self.send_command(Command::AllowImagesFrom {
            address: s(address),
        });
    }

    pub fn disallow_images_from(self: Pin<&mut Self>, address: &QString) {
        self.send_command(Command::DisallowImagesFrom {
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

    pub fn export_invite(self: Pin<&mut Self>, id: i64, dir: &QString) {
        self.send_command(Command::ExportInvite {
            id: MessageRowId(id),
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
    ) -> f64 {
        let Some((from, draft_id, mailto)) = cv::start_from(&s(kind), id, &s(account), &s(mailto))
        else {
            return 0.0;
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
        crate::variant::js_num(compose)
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
    pub fn edit_outbox(mut self: Pin<&mut Self>, item: &QString) -> f64 {
        let compose = {
            let mut state = self.as_mut().rust_mut();
            state.next_compose += 1;
            state.next_compose
        };
        self.send_command(Command::EditOutbox {
            item: s(item),
            compose: cv::compose_id(compose),
        });
        crate::variant::js_num(compose)
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

    /// The "Load older messages" row was tapped.
    pub fn load_older(mut self: Pin<&mut Self>) {
        let Some((command, outs)) = self.as_mut().rust_mut().ui.load_older() else {
            return;
        };
        self.as_mut().run(outs);
        self.send_command(command);
    }

    /// The list was scrolled to its end: the same load, once until rows
    /// arrive and not again after a failure.
    pub fn load_older_near_end(mut self: Pin<&mut Self>) {
        let Some((command, outs)) = self.as_mut().rust_mut().ui.load_older_near_end() else {
            return;
        };
        self.as_mut().run(outs);
        self.send_command(command);
    }

    /// `kind` is "selected", "results", "waiting" or "newmail".
    pub fn count_text(&self, kind: &QString, n: i32) -> QString {
        let text = cv::count_kind(&s(kind))
            .map(|kind| count_text(kind, u64::try_from(n).unwrap_or(0)))
            .unwrap_or_default();
        QString::from(text)
    }

    /// A link's parts for the link sheet (`envio_ui::link_view`).
    pub fn link_view(&self, url: &QString) -> QMap<QMapPair_QString_QVariant> {
        let v = envio_ui::link_view(&s(url));
        map_of(&serde_json::json!({
            "beforeHost": v.before_host,
            "userinfo": v.userinfo,
            "host": v.host,
            "unicodeHost": v.unicode_host.unwrap_or_default(),
            "unicodeNote": v.unicode_note,
            "afterHost": v.after_host,
            "warning": v.warning,
            "openable": v.openable,
        }))
    }

    /// "time", "weekday", "daymonth", "full" or "none"; QML formats it.
    pub fn date_bucket(&self, unix_secs: i64) -> QString {
        let (now, offset) = clock();
        QString::from(envio_ui::bucket(now, Some(unix_secs), offset).as_str())
    }
}
