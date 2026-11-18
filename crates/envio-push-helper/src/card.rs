//! The helper's one decision: which card the input becomes. Pure.

use serde_json::{Map, Value, json};

/// What a tap on the card opens: Envio, in front.
const OPEN_ENVIO: &str = "appid://envio.aps/envio/current-user-version";

/// The words of the generic card, translated by the caller.
pub struct Text {
    pub summary: String,
    pub body: String,
}

/// The output for `input`: a local `Post` already carries its
/// `notification`, which goes through unchanged; anything else gets the
/// generic card, with the unread count from `message.u` as the launcher
/// badge. Input that is not a JSON object gives `{}`.
pub fn convert(input: &[u8], text: &Text) -> String {
    let Ok(Value::Object(input)) = serde_json::from_slice::<Value>(input) else {
        return "{}".into();
    };
    let mut out = Map::new();
    if let Some(message) = input.get("message") {
        out.insert("message".into(), message.clone());
    }
    let notification = match input.get("notification") {
        Some(notification) if notification.is_object() => notification.clone(),
        _ => generic(&input, text),
    };
    out.insert("notification".into(), notification);
    Value::Object(out).to_string()
}

fn generic(input: &Map<String, Value>, text: &Text) -> Value {
    let mut notification = json!({
        "card": {
            "summary": text.summary,
            "body": text.body,
            "popup": true,
            "persist": true,
            "actions": [OPEN_ENVIO],
        }
    });
    let unread = input
        .get("message")
        .and_then(|message| message.get("u"))
        .and_then(Value::as_u64);
    if let Some(count) = unread {
        notification["emblem-counter"] = json!({ "count": count, "visible": true });
    }
    notification
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text() -> Text {
        Text {
            summary: "Envio".into(),
            body: "New mail".into(),
        }
    }

    fn parsed(output: &str) -> Value {
        serde_json::from_str(output).unwrap()
    }

    #[test]
    fn passes_a_local_notification_through() {
        let input = r#"{"notification":{"card":{"summary":"grace","body":"2 new messages"},"tag":"new-1"}}"#;
        let out = parsed(&convert(input.as_bytes(), &text()));
        assert_eq!(out["notification"], parsed(input)["notification"]);
        assert_eq!(out.as_object().unwrap().len(), 1);
    }

    #[test]
    fn builds_the_generic_card_from_a_message() {
        let input = r#"{"message":{"u":3,"x":"y"}}"#;
        let out = parsed(&convert(input.as_bytes(), &text()));
        let card = &out["notification"]["card"];
        assert_eq!(card["summary"], "Envio");
        assert_eq!(card["body"], "New mail");
        assert_eq!(card["popup"], true);
        assert_eq!(card["actions"][0], OPEN_ENVIO);
        assert_eq!(out["notification"]["emblem-counter"]["count"], 3);
        assert_eq!(out["notification"]["emblem-counter"]["visible"], true);
        assert_eq!(out["message"]["u"], 3);
    }

    #[test]
    fn a_message_without_a_count_sets_no_badge() {
        let out = parsed(&convert(br#"{"message":{}}"#, &text()));
        assert!(out["notification"].get("emblem-counter").is_none());
    }

    #[test]
    fn bad_input_writes_an_empty_object() {
        for input in [&b""[..], b"not json", b"[1,2]", b"\"text\"", b"null"] {
            assert_eq!(convert(input, &text()), "{}");
        }
    }

    #[test]
    fn a_notification_that_is_not_an_object_gets_the_generic_card() {
        let out = parsed(&convert(br#"{"notification":"x"}"#, &text()));
        assert_eq!(out["notification"]["card"]["body"], "New mail");
    }
}
