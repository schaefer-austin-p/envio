//! The JSON QML exchanges with the glue: drafts, account setups, and the
//! opened message. Pure conversions; no Qt types.

use std::path::PathBuf;

use envio_core::{AccountSetup, DraftInput, MessageView, Security, ServerAddr, SetupSource};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Why QML's JSON was refused, in words that quote nothing from it: a
/// serde_json message repeats the rejected value, which can be an address
/// or a subject, and logs must not hold those.
#[derive(Debug, PartialEq)]
pub struct Rejected(String);

impl std::fmt::Display for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn rejected(e: &serde_json::Error) -> Rejected {
    Rejected(format!(
        "{:?} at line {} column {}",
        e.classify(),
        e.line(),
        e.column()
    ))
}

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

pub fn draft_from_json(text: &str) -> Result<DraftInput, Rejected> {
    let d: DraftJson = serde_json::from_str(text).map_err(|e| rejected(&e))?;
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

pub fn setup_from_json(text: &str) -> Result<AccountSetup, Rejected> {
    let j: SetupJson = serde_json::from_str(text).map_err(|e| rejected(&e))?;
    let security =
        |w: &str| security_of(w).ok_or_else(|| Rejected("an unknown security word".into()));
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
pub fn message_view_value(id: i64, view: &MessageView) -> Value {
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
        "id": id,
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
        // The refusal quotes nothing QML sent.
        let bad = draft_from_json(r#"{"to":"bob@example.org"}"#).unwrap_err();
        assert!(!bad.to_string().contains("bob"), "{bad}");
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
    fn the_opened_message_map_names_its_id() {
        use envio_core::MessageHeader;
        let view = MessageView {
            header: MessageHeader {
                from: "f".into(),
                from_address: "f@example.org".into(),
                to: vec![],
                cc: vec![],
                subject: "s".into(),
                date: None,
                account: "a".into(),
                sender_known: false,
            },
            html: String::new(),
            raw: false,
            blocked_images: 0,
            attachments: vec![],
        };
        assert_eq!(message_view_value(41, &view)["id"], 41);
    }
}
