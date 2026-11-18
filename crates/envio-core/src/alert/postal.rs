//! The JSON Lomiri's Postal takes for an alert. Postal runs a local `Post`
//! through the push helper, which keeps only the fields set here and only
//! when `envio_local` is true (crates/envio-push-helper/src/card.rs).

use serde_json::{Value, json};

use super::Alert;

/// What a tap on the card opens: Envio, in front, where the user left it.
const OPEN_ENVIO: &str = "appid://envio.aps/envio/current-user-version";

impl Alert {
    /// The argument of Postal's `Post`. The helper keeps `summary`, `body`
    /// and `tag` (200 characters each) and forces the action itself; the
    /// action is set here too so the card is right without the helper.
    pub fn to_postal_json(&self) -> String {
        let limit = |text: &str| text.chars().take(200).collect::<String>();
        let json: Value = json!({
            "envio_local": true,
            "notification": {
                "tag": limit(&self.tag),
                "card": {
                    "summary": limit(&self.summary),
                    "body": limit(&self.body),
                    "popup": true,
                    "persist": true,
                    "actions": [OPEN_ENVIO],
                },
            },
        });
        json.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(summary: &str, body: &str) -> Alert {
        Alert {
            tag: "new-1700".into(),
            summary: summary.into(),
            body: body.into(),
            account: "1700".into(),
        }
    }

    fn parse(alert: &Alert) -> Value {
        serde_json::from_str(&alert.to_postal_json()).unwrap()
    }

    #[test]
    fn the_postal_json_holds_the_card_and_marks_it_local() {
        let json = parse(&alert("Ana", "Lunch?"));
        assert_eq!(json["envio_local"], true);
        let n = &json["notification"];
        assert_eq!(n["tag"], "new-1700");
        assert_eq!(n["card"]["summary"], "Ana");
        assert_eq!(n["card"]["body"], "Lunch?");
        assert_eq!(n["card"]["popup"], true);
        assert_eq!(n["card"]["persist"], true);
        assert_eq!(n["card"]["actions"], json!([OPEN_ENVIO]));
    }

    #[test]
    fn the_postal_json_keeps_text_within_the_helpers_limit() {
        let long = "é".repeat(500);
        let json = parse(&alert(&long, &long));
        let card = &json["notification"]["card"];
        assert_eq!(card["summary"].as_str().unwrap().chars().count(), 200);
        assert_eq!(card["body"].as_str().unwrap().chars().count(), 200);
    }

    #[test]
    fn the_postal_json_escapes_quotes_and_newlines() {
        let json = parse(&alert("a \"b\"", "x\ny"));
        assert_eq!(json["notification"]["card"]["summary"], "a \"b\"");
        assert_eq!(json["notification"]["card"]["body"], "x\ny");
    }
}
