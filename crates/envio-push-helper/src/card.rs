//! The helper's one decision: which card the input becomes. Pure.
//!
//! Anyone holding Envio's push token can send the helper any JSON, so a
//! remote message never reaches the card except as a count. The card is a
//! fixed template; the one input with words of its own is a local `Post`,
//! marked by Envio, and even that keeps only whitelisted fields.
//!
//! Residual risk: nothing in the helper's input is known to be unforgeable
//! by a remote sender. The push service wraps a remote `data` object as
//! `message`, so a message carrying `message` never takes the local path;
//! a sender who can shape the top level of the input beyond that could
//! still show up to 200 characters of summary and body on an Envio card
//! (never an action, never other fields). The spec's 9b "Threats this
//! design accepts" lists this; 9b decides whether to close it.

use serde_json::{Map, Value, json};

/// What a tap on the card opens: Envio, in front. Never taken from input.
const OPEN_ENVIO: &str = "appid://envio.aps/envio/current-user-version";
/// Replaces the last generic card instead of stacking another.
const GENERIC_TAG: &str = "new-mail";
/// The field Envio sets on its own notifications.
const LOCAL_MARK: &str = "envio_local";
const MAX_TEXT: usize = 200;

/// The words of the generic card, translated by the caller.
pub struct Text {
    pub summary: String,
    /// The body when the push carries no count.
    pub new_mail: String,
    /// The body for `n` unread messages.
    pub counted: fn(u64) -> String,
}

/// The output for `input`. `message` is always present (the push service
/// may drop output without one) and empty: nothing remote is passed on.
/// Input that is not a JSON object gives an empty message only.
pub fn convert(input: &[u8], text: &Text) -> String {
    let Ok(Value::Object(input)) = serde_json::from_slice::<Value>(input) else {
        return json!({ "message": {} }).to_string();
    };
    // A remote push always arrives wrapped as `message`; a local Post has none.
    let marked =
        input.get(LOCAL_MARK) == Some(&Value::Bool(true)) && !input.contains_key("message");
    let notification = marked
        .then(|| local(&input))
        .flatten()
        .unwrap_or_else(|| generic(&input, text));
    json!({ "message": {}, "notification": notification }).to_string()
}

fn card(summary: &str, body: &str) -> Value {
    json!({
        "summary": summary,
        "body": body,
        "popup": true,
        "persist": true,
        "actions": [OPEN_ENVIO],
    })
}

fn badge(count: u64) -> Value {
    let count = count.min(i32::MAX as u64);
    json!({ "count": count, "visible": count > 0 })
}

fn short(value: Option<&Value>) -> Option<String> {
    Some(value?.as_str()?.chars().take(MAX_TEXT).collect())
}

/// A marked local notification: summary, body, tag and count only.
fn local(input: &Map<String, Value>) -> Option<Value> {
    let notification = input.get("notification")?;
    let card_in = notification.get("card")?;
    let summary = short(card_in.get("summary"))?;
    let body = short(card_in.get("body"))?;
    let mut out = json!({ "card": card(&summary, &body) });
    if let Some(tag) = short(notification.get("tag")) {
        out["tag"] = tag.into();
    }
    let count = notification
        .get("emblem-counter")
        .and_then(|counter| counter.get("count"))
        .and_then(Value::as_u64);
    if let Some(count) = count {
        out["emblem-counter"] = badge(count);
    }
    Some(out)
}

fn generic(input: &Map<String, Value>, text: &Text) -> Value {
    let unread = input
        .get("message")
        .and_then(|message| message.get("u"))
        .and_then(Value::as_u64);
    let body = match unread {
        Some(n) if n > 0 => (text.counted)(n),
        _ => text.new_mail.clone(),
    };
    let mut out = json!({ "card": card(&text.summary, &body), "tag": GENERIC_TAG });
    if let Some(n) = unread {
        out["emblem-counter"] = badge(n);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text() -> Text {
        Text {
            summary: "Envio".into(),
            new_mail: "New mail".into(),
            counted: |n| format!("{n} new"),
        }
    }

    fn run(input: &str) -> Value {
        serde_json::from_str(&convert(input.as_bytes(), &text())).unwrap()
    }

    #[test]
    fn a_remote_message_gets_the_fixed_card_with_the_count() {
        let out = run(r#"{"message":{"u":3,"x":"y"}}"#);
        let n = &out["notification"];
        assert_eq!(n["card"]["summary"], "Envio");
        assert_eq!(n["card"]["body"], "3 new");
        assert_eq!(n["card"]["popup"], true);
        assert_eq!(n["card"]["actions"], json!([OPEN_ENVIO]));
        assert_eq!(n["tag"], "new-mail");
        assert_eq!(n["emblem-counter"], json!({"count": 3, "visible": true}));
        assert_eq!(out["message"], json!({}));
    }

    #[test]
    fn hostile_remote_input_never_reaches_the_card() {
        let out = run(
            r#"{"message":{"u":"x","summary":"Pay now"},"notification":{"card":{"summary":"Bank","body":"Pay now","actions":["https://evil.example"]},"tag":"t"}}"#,
        );
        let n = &out["notification"];
        assert_eq!(n["card"]["summary"], "Envio");
        assert_eq!(n["card"]["body"], "New mail");
        assert_eq!(n["card"]["actions"], json!([OPEN_ENVIO]));
        assert_eq!(n["tag"], "new-mail");
        assert!(n.get("emblem-counter").is_none());
        assert_eq!(out["message"], json!({}));
        assert!(!out.to_string().contains("evil"));
    }

    #[test]
    fn the_count_is_clamped_and_zero_hides_the_badge() {
        let big = run(r#"{"message":{"u":99999999999}}"#);
        assert_eq!(big["notification"]["emblem-counter"]["count"], i32::MAX);
        let zero = run(r#"{"message":{"u":0}}"#);
        assert_eq!(zero["notification"]["emblem-counter"]["visible"], false);
        assert_eq!(zero["notification"]["card"]["body"], "New mail");
    }

    #[test]
    fn a_marked_local_notification_keeps_its_text_but_not_its_action() {
        let out = run(
            r#"{"envio_local":true,"notification":{"tag":"new-1","card":{"summary":"grace","body":"2 new messages","actions":["https://evil.example"],"icon":"x"},"emblem-counter":{"count":2,"visible":true}}}"#,
        );
        let n = &out["notification"];
        assert_eq!(n["card"]["summary"], "grace");
        assert_eq!(n["card"]["body"], "2 new messages");
        assert_eq!(n["card"]["actions"], json!([OPEN_ENVIO]));
        assert!(n["card"].get("icon").is_none());
        assert_eq!(n["tag"], "new-1");
        assert_eq!(n["emblem-counter"]["count"], 2);
        assert_eq!(out["message"], json!({}));
    }

    #[test]
    fn a_forged_mark_on_a_remote_message_is_ignored() {
        let out = run(
            r#"{"envio_local":true,"message":{"u":2},"notification":{"tag":"t","card":{"summary":"Bank","body":"Pay now"}}}"#,
        );
        let n = &out["notification"];
        assert_eq!(n["card"]["summary"], "Envio");
        assert_eq!(n["card"]["body"], "2 new");
        assert_eq!(n["tag"], "new-mail");
        assert!(!out.to_string().contains("Pay now"));
    }

    #[test]
    fn an_unmarked_notification_is_ignored() {
        let out = run(r#"{"notification":{"card":{"summary":"x","body":"y"}}}"#);
        assert_eq!(out["notification"]["card"]["summary"], "Envio");
    }

    #[test]
    fn bad_input_writes_an_empty_message() {
        for input in [&b""[..], b"not json", b"[1,2]", b"\"text\"", b"null"] {
            assert_eq!(convert(input, &text()), r#"{"message":{}}"#);
        }
    }
}
