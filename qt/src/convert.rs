//! Plain conversions between what QML passes (strings, JSON text) and
//! envio-core's types, and from core's views to JSON values for the QML
//! side. No Qt types except in `variant`; no decisions.

use std::path::PathBuf;

use envio_core::{
    AccountSetup, ComposeId, CountKind, DarkMode, DraftInput, FolderId, ImagesDefault, ListRef,
    MessageRowId, MessageView, OutboxDecision, SearchScope, Security, ServerAddr, Service,
    Settings, SetupSource, StartFrom, ThemeName, UserAction,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The draft as QML holds it (Sketch §3). `quoted` and `keepQuoted` are
/// QML's own and are ignored here.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
struct DraftJson {
    account: String,
    draft_id: Option<String>,
    to: Vec<String>,
    cc: Vec<String>,
    bcc: Vec<String>,
    subject: String,
    body: String,
    in_reply_to: Option<String>,
    references: Vec<String>,
    attachments: Vec<PathBuf>,
}

pub fn draft_from_json(text: &str) -> Result<DraftInput, serde_json::Error> {
    let d: DraftJson = serde_json::from_str(text)?;
    Ok(DraftInput {
        account: d.account,
        draft_id: d.draft_id,
        to: d.to,
        cc: d.cc,
        bcc: d.bcc,
        subject: d.subject,
        body: d.body,
        in_reply_to: d.in_reply_to,
        references: d.references,
        attachments: d.attachments,
    })
}

/// The draft with the keys QML sends back in `draftChanged`.
pub fn draft_value(draft: &DraftInput) -> Value {
    let d = DraftJson {
        account: draft.account.clone(),
        draft_id: draft.draft_id.clone(),
        to: draft.to.clone(),
        cc: draft.cc.clone(),
        bcc: draft.bcc.clone(),
        subject: draft.subject.clone(),
        body: draft.body.clone(),
        in_reply_to: draft.in_reply_to.clone(),
        references: draft.references.clone(),
        attachments: draft.attachments.clone(),
    };
    serde_json::to_value(d).unwrap_or(Value::Null)
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SetupJson {
    name: String,
    email: String,
    username: String,
    imap_host: String,
    imap_port: u16,
    imap_security: String,
    smtp_host: String,
    smtp_port: u16,
    smtp_security: String,
    source: Option<String>,
    needs_confirm: bool,
}

const SOURCES: [(SetupSource, &str); 5] = [
    (SetupSource::Ispdb, "ispdb"),
    (SetupSource::Provider, "provider"),
    (SetupSource::WellKnown, "wellknown"),
    (SetupSource::Srv, "srv"),
    (SetupSource::Guess, "guess"),
];

fn security_word(s: Security) -> &'static str {
    match s {
        Security::Tls => "tls",
        Security::StartTls => "starttls",
    }
}

fn security_of(word: &str) -> Option<Security> {
    match word {
        "tls" => Some(Security::Tls),
        "starttls" => Some(Security::StartTls),
        _ => None,
    }
}

pub fn setup_to_json(setup: &AccountSetup) -> String {
    let source = SOURCES
        .iter()
        .find(|(s, _)| Some(*s) == setup.source)
        .map(|(_, w)| (*w).to_owned());
    let json = SetupJson {
        name: setup.name.clone(),
        email: setup.email.clone(),
        username: setup.username.clone(),
        imap_host: setup.imap.host.clone(),
        imap_port: setup.imap.port,
        imap_security: security_word(setup.imap.security).to_owned(),
        smtp_host: setup.smtp.host.clone(),
        smtp_port: setup.smtp.port,
        smtp_security: security_word(setup.smtp.security).to_owned(),
        source,
        needs_confirm: setup.needs_confirm,
    };
    serde_json::to_string(&json).unwrap_or_default()
}

pub fn setup_from_json(text: &str) -> Result<AccountSetup, String> {
    let j: SetupJson = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let security = |w: &str| security_of(w).ok_or_else(|| format!("unknown security {w:?}"));
    Ok(AccountSetup {
        name: j.name,
        email: j.email,
        username: j.username,
        imap: ServerAddr {
            host: j.imap_host,
            port: j.imap_port,
            security: security(&j.imap_security)?,
        },
        smtp: ServerAddr {
            host: j.smtp_host,
            port: j.smtp_port,
            security: security(&j.smtp_security)?,
        },
        source: j
            .source
            .and_then(|w| SOURCES.iter().find(|(_, n)| *n == w).map(|(s, _)| *s)),
        needs_confirm: j.needs_confirm,
    })
}

/// The reader's message (Sketch §3: `messageOpened`).
pub fn message_view_value(view: &MessageView) -> Value {
    let h = &view.header;
    let attachments: Vec<Value> = view
        .attachments
        .iter()
        .map(|a| {
            json!({
                "index": a.index,
                "name": a.name,
                "mime": a.content_type,
                "size": a.size,
            })
        })
        .collect();
    json!({
        "html": view.html,
        "blockedImages": view.blocked_images,
        "from": h.from,
        "fromAddress": h.from_address,
        "to": h.to,
        "cc": h.cc,
        "subject": h.subject,
        "date": h.date.unwrap_or(0),
        "account": h.account,
        "senderKnown": h.sender_known,
        "attachments": attachments,
    })
}

pub fn service_word(service: Service) -> &'static str {
    match service {
        Service::Imap => "imap",
        Service::Smtp => "smtp",
    }
}

pub fn service_of(word: &str) -> Option<Service> {
    match word {
        "imap" => Some(Service::Imap),
        "smtp" => Some(Service::Smtp),
        _ => None,
    }
}

/// `act` takes "archive", "delete", "move", "read", "unread", "flag",
/// "unflag"; anything else is dropped with a log line.
pub fn user_action(word: &str, folder: i64) -> Option<UserAction> {
    let action = match word {
        "archive" => UserAction::Archive,
        "delete" => UserAction::Delete,
        "move" => UserAction::Move {
            to: FolderId(folder),
        },
        "read" => UserAction::MarkRead(true),
        "unread" => UserAction::MarkRead(false),
        "flag" => UserAction::Flag(true),
        "unflag" => UserAction::Flag(false),
        _ => {
            log::warn!("ignored unknown action {word:?}");
            return None;
        }
    };
    Some(action)
}

/// "inboxes" or "folder" (with its id).
pub fn list_ref(kind: &str, id: i64) -> Option<ListRef> {
    match kind {
        "inboxes" => Some(ListRef::Inboxes),
        "folder" => Some(ListRef::Folder(FolderId(id))),
        _ => {
            log::warn!("ignored unknown list kind {kind:?}");
            None
        }
    }
}

/// "all", "account:<key>" or "folder:<id>".
pub fn search_scope(text: &str) -> Option<SearchScope> {
    if text == "all" {
        return Some(SearchScope::All);
    }
    if let Some(key) = text.strip_prefix("account:") {
        return Some(SearchScope::Account(key.to_owned()));
    }
    if let Some(id) = text.strip_prefix("folder:").and_then(|i| i.parse().ok()) {
        return Some(SearchScope::Folder(FolderId(id)));
    }
    log::warn!("ignored unknown search scope {text:?}");
    None
}

/// "new" (account), "reply", "replyall", "forward" (message id), "draft"
/// (the stored draft id in `extra`) or "mailto" (the URL in `extra`).
/// Returns the start and the command's `draft_id` and `mailto`.
pub fn start_from(
    kind: &str,
    id: i64,
    account: &str,
    extra: &str,
) -> Option<(StartFrom, Option<String>, Option<String>)> {
    let from = match kind {
        "new" => StartFrom::New {
            account: account.to_owned(),
        },
        "reply" => StartFrom::Reply {
            id: MessageRowId(id),
            all: false,
        },
        "replyall" => StartFrom::Reply {
            id: MessageRowId(id),
            all: true,
        },
        "forward" => StartFrom::Forward {
            id: MessageRowId(id),
        },
        "draft" => return Some((StartFrom::Draft, Some(extra.to_owned()), None)),
        "mailto" => return Some((StartFrom::Mailto, None, Some(extra.to_owned()))),
        _ => {
            log::warn!("ignored unknown compose kind {kind:?}");
            return None;
        }
    };
    Some((from, None, None))
}

pub fn decision(word: &str) -> Option<OutboxDecision> {
    match word {
        "sendagain" => Some(OutboxDecision::SendAgain),
        "movetosent" => Some(OutboxDecision::MoveToSent),
        "retry" => Some(OutboxDecision::Retry),
        "delete" => Some(OutboxDecision::Delete),
        _ => {
            log::warn!("ignored unknown outbox decision {word:?}");
            None
        }
    }
}

/// "selected", "results", "waiting" or "newmail".
pub fn count_kind(word: &str) -> Option<CountKind> {
    match word {
        "selected" => Some(CountKind::Selected),
        "results" => Some(CountKind::Results),
        "waiting" => Some(CountKind::Waiting),
        "newmail" => Some(CountKind::NewMail),
        _ => None,
    }
}

pub fn compose_id(id: i64) -> ComposeId {
    ComposeId(u64::try_from(id).unwrap_or(0))
}

pub fn theme_word(t: ThemeName) -> &'static str {
    match t {
        ThemeName::Carbon => "carbon",
        ThemeName::Airmail => "airmail",
    }
}

pub fn dark_word(d: DarkMode) -> &'static str {
    match d {
        DarkMode::System => "system",
        DarkMode::Light => "light",
        DarkMode::Dark => "dark",
    }
}

/// The settings with the four values the Settings page edits replaced;
/// unknown words keep what was there.
pub fn settings_with(
    current: &Settings,
    theme: &str,
    dark: &str,
    images_always: bool,
    default_account: &str,
) -> Settings {
    Settings {
        theme: match theme {
            "airmail" => ThemeName::Airmail,
            "carbon" => ThemeName::Carbon,
            _ => current.theme,
        },
        dark: match dark {
            "light" => DarkMode::Light,
            "dark" => DarkMode::Dark,
            "system" => DarkMode::System,
            _ => current.dark,
        },
        images: if images_always {
            ImagesDefault::Always
        } else {
            ImagesDefault::Ask
        },
        default_account: (!default_account.is_empty()).then(|| default_account.to_owned()),
        ..current.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_json_round_trip() {
        let text = r#"{"account":"a","to":["x@y.z"],"cc":[],"bcc":["b@y.z"],
            "subject":"Hi","body":"text","quoted":"> old","keepQuoted":true,
            "attachments":["/tmp/f.txt"],"inReplyTo":"id@host"}"#;
        let draft = draft_from_json(text).unwrap();
        assert_eq!(draft.account, "a");
        assert_eq!(draft.bcc, ["b@y.z"]);
        assert_eq!(draft.in_reply_to.as_deref(), Some("id@host"));
        assert_eq!(draft.attachments, [PathBuf::from("/tmp/f.txt")]);
        let back = draft_from_json(&draft_value(&draft).to_string()).unwrap();
        assert_eq!(back, draft);
        assert!(draft_from_json("not json").is_err());
    }

    #[test]
    fn setup_json_round_trip() {
        let setup = AccountSetup {
            name: "Ann".into(),
            email: "ann@example.org".into(),
            username: "ann".into(),
            imap: ServerAddr {
                host: "imap.example.org".into(),
                port: 993,
                security: Security::Tls,
            },
            smtp: ServerAddr {
                host: "smtp.example.org".into(),
                port: 587,
                security: Security::StartTls,
            },
            source: Some(SetupSource::Srv),
            needs_confirm: true,
        };
        let text = setup_to_json(&setup);
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["imapHost"], "imap.example.org");
        assert_eq!(v["smtpPort"], 587);
        assert_eq!(v["username"], "ann");
        assert_eq!(setup_from_json(&text).unwrap(), setup);
        assert!(setup_from_json(&text.replace("starttls", "plain")).is_err());
    }

    #[test]
    fn unknown_action_is_ignored_with_a_log() {
        assert_eq!(user_action("archive", 0), Some(UserAction::Archive));
        assert_eq!(
            user_action("move", 7),
            Some(UserAction::Move { to: FolderId(7) })
        );
        assert_eq!(user_action("read", 0), Some(UserAction::MarkRead(true)));
        assert_eq!(user_action("unflag", 0), Some(UserAction::Flag(false)));
        assert_eq!(user_action("explode", 0), None);
        assert_eq!(list_ref("sideways", 1), None);
        assert_eq!(decision("shrug"), None);
        assert_eq!(search_scope("folder:x"), None);
    }
}
