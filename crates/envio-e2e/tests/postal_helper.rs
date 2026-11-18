//! `Alert::to_postal_json` through the push helper's card builder: what
//! Postal does with a local `Post`. The helper is a bin crate with no
//! library, so its pure `card.rs` is compiled in here (it needs only
//! serde_json); core and the helper never depend on each other.

#[path = "../../envio-push-helper/src/card.rs"]
mod card;

use envio_core::Alert;
use serde_json::{Value, json};

fn generic() -> card::Text {
    card::Text {
        summary: "Envio".into(),
        new_mail: "New mail".into(),
        counted: |n| format!("{n} new"),
    }
}

fn through_helper(alert: &Alert) -> Value {
    let out = card::convert(alert.to_postal_json().as_bytes(), &generic());
    serde_json::from_str(&out).unwrap()
}

fn alert(summary: &str, body: &str) -> Alert {
    Alert {
        tag: "new-1700".into(),
        summary: summary.into(),
        body: body.into(),
        account: "1700".into(),
    }
}

#[test]
fn the_helper_keeps_the_text_of_a_local_alert() {
    let out = through_helper(&alert("Ana", "Lunch?"));
    let n = &out["notification"];
    assert_eq!(n["card"]["summary"], "Ana");
    assert_eq!(n["card"]["body"], "Lunch?");
    assert_eq!(n["card"]["popup"], true);
    assert_eq!(n["card"]["persist"], true);
    assert_eq!(
        n["card"]["actions"],
        json!(["appid://envio.aps/envio/current-user-version"])
    );
    assert_eq!(n["tag"], "new-1700");
    assert!(out.get("message").is_some());
}

#[test]
fn the_helper_keeps_long_text_to_its_limit_without_falling_back() {
    let long = "x".repeat(500);
    let out = through_helper(&alert(&long, &long));
    let card = &out["notification"]["card"];
    assert_eq!(card["summary"].as_str().unwrap().len(), 200);
    assert_ne!(card["summary"], "Envio");
}
