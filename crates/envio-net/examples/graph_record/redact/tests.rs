use serde_json::json;

use super::*;

const REAL: &str = "user@outlook.com";
const GUID: &str = "12345678-90ab-cdef-1234-567890abcdef";

fn dump(v: &Value) -> String {
    v.to_string()
}

#[test]
fn addresses_are_stable_per_address_and_case_insensitive() {
    let mut r = Redactor::default();
    assert_eq!(r.address(REAL), "alice@example.org");
    assert_eq!(r.address("Other@Hotmail.com"), "bob@example.org");
    assert_eq!(r.address("USER@outlook.com"), "alice@example.org");
    assert_eq!(r.address("third@live.com"), "user3@example.org");
    assert_eq!(r.name_for(REAL), "Alice");
}

#[test]
fn free_text_loses_addresses_guids_and_ips() {
    let mut r = Redactor::default();
    let out = r.text(&format!(
        "mail {REAL}, id {GUID}, from 10.1.2.3 and 2001:db8::1 at 10:20:30"
    ));
    assert!(out.contains("alice@example.org"), "{out}");
    assert!(!out.contains("outlook"), "{out}");
    assert!(!out.contains(GUID), "{out}");
    assert!(
        !out.contains("10.1.2.3") && !out.contains("2001:db8"),
        "{out}"
    );
    assert!(out.contains("10:20:30"), "times stay: {out}");
    assert!(guids(&out).len() == 1 && out.contains("00000000-0000-4000-8000-000000000001"));
}

#[test]
fn only_allow_listed_headers_survive_and_their_values_are_redacted() {
    let mut r = Redactor::default();
    let list = vec![
        json!({"name": "Received", "value": "from mail.outlook.com (10.0.0.1)"}),
        json!({"name": "X-MS-Exchange-Organization-AuthAs", "value": "Internal"}),
        json!({"name": "Authentication-Results", "value": format!("spf=pass smtp.mailfrom={REAL}")}),
        json!({"name": "From", "value": format!("Jane Doe <{REAL}>")}),
        json!({"name": "To", "value": "\"Doe, Bob\" <bob@live.com>, carol@hotmail.com"}),
        json!({"name": "Subject", "value": "Our secret plan"}),
        json!({"name": "Message-ID", "value": "<AM6PR02MB1234@AM6PR02MB.outlook.com>"}),
        json!({"name": "References", "value": "<a@x.example> <b@x.example>"}),
        json!({"name": "Content-Type", "value": "multipart/mixed; boundary=\"_000_AM6PR_outlook.com_\""}),
        json!({"name": "Date", "value": "Sat, 21 Nov 2026 10:20:30 +0000"}),
    ];
    let out = r.headers(&list);
    let names: Vec<&str> = out.iter().map(|h| h["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "From",
            "To",
            "Subject",
            "Message-ID",
            "References",
            "Content-Type",
            "Date"
        ]
    );
    let text = serde_json::to_string(&out).unwrap();
    for leak in [
        "Jane", "Doe", "outlook", "live.com", "hotmail", "secret", "AM6PR",
    ] {
        assert!(!text.contains(leak), "{leak} in {text}");
    }
    assert_eq!(out[6]["value"], "Sat, 21 Nov 2026 00:00:00 +0000");
    assert_eq!(out[0]["value"], "Alice <alice@example.org>");
    assert_eq!(
        out[1]["value"],
        "Bob <bob@example.org>, User 3 <user3@example.org>"
    );
    assert_eq!(out[2]["value"], "Subject 1");
    assert_eq!(out[3]["value"], "<fake-1@example.org>");
    assert_eq!(out[4]["value"], "<fake-2@example.org> <fake-3@example.org>");
    assert!(!text.contains("10:20:30"), "the send time is replaced");
}

#[test]
fn a_message_object_is_redacted_and_bodies_are_dropped() {
    let mut r = Redactor::default();
    let mut v = json!({
        "@odata.etag": "W/\"CQAAABYAAA\"",
        "id": "AAMkReal1",
        "changeKey": "CQAAReal",
        "subject": "Quarterly numbers",
        "bodyPreview": "secret preview",
        "body": {"contentType": "html", "content": "<p>secret</p>"},
        "webLink": "https://outlook.live.com/owa/?ItemID=AAMkReal1",
        "internetMessageId": "<real@mail.outlook.com>",
        "parentFolderId": "AAMkFolder",
        "categories": ["Red category"],
        "from": {"emailAddress": {"name": "Jane Doe", "address": REAL}},
        "toRecipients": [{"emailAddress": {"name": "Bob", "address": "bob@live.com"}}],
        "isRead": false,
        "singleValueExtendedProperties": [{"id": "Integer 0x0E08", "value": "1234"}],
    });
    r.value(&mut v);
    let text = dump(&v);
    for leak in [
        "Real",
        "Quarterly",
        "secret",
        "outlook",
        "live",
        "Jane",
        "Red category",
    ] {
        assert!(!text.contains(leak), "{leak} in {text}");
    }
    assert!(v["id"].as_str().unwrap().starts_with("AAMk-"));
    assert_ne!(v["parentFolderId"], v["id"]);
    assert_eq!(v["from"]["emailAddress"]["address"], "alice@example.org");
    assert_eq!(v["from"]["emailAddress"]["name"], "Alice");
    assert_eq!(v["categories"][0], "Category 1");
    assert_eq!(
        v["singleValueExtendedProperties"][0]["id"],
        "Integer 0x0E08"
    );
    assert_eq!(v["singleValueExtendedProperties"][0]["value"], "1234");
    assert_eq!(v["isRead"], false);
}

#[test]
fn ids_are_consistent_across_values_and_removals_keep_their_shape() {
    let mut r = Redactor::default();
    let mut a = json!({"value": [{"id": "AAMkX", "isRead": true}]});
    let mut b = json!({"value": [{"id": "AAMkX", "@removed": {"reason": "deleted"}}]});
    r.value(&mut a);
    r.value(&mut b);
    assert_eq!(a["value"][0]["id"], b["value"][0]["id"]);
    assert_eq!(b["value"][0]["@removed"]["reason"], "deleted");
}

#[test]
fn folder_names_keep_well_known_ones_only() {
    let mut r = Redactor::default();
    let mut v = json!([
        {"displayName": "Inbox"},
        {"displayName": "Taxes 2024"},
        {"displayName": "Sent Items"},
        {"displayName": "Taxes 2024"}
    ]);
    r.value(&mut v);
    assert_eq!(v[0]["displayName"], "Inbox");
    assert_eq!(v[1]["displayName"], "Folder 1");
    assert_eq!(v[2]["displayName"], "Sent Items");
    assert_eq!(v[3]["displayName"], "Folder 1");
}

#[test]
fn links_keep_their_shape_with_fake_ids_and_tokens() {
    let mut r = Redactor::default();
    let mut v = json!({
        "@odata.context": format!("https://graph.microsoft.com/v1.0/$metadata#users('{GUID}')/mailFolders('AAMkF')/messages"),
        "@odata.nextLink": "https://graph.microsoft.com/v1.0/me/mailFolders('AAMkF')/messages/delta?$skiptoken=REALTOKEN&$select=isRead&evil=1",
        "@odata.deltaLink": "https://graph.microsoft.com/v1.0/me/mailFolders('AAMkF')/messages/delta?$deltatoken=REALDELTA",
        "value": []
    });
    r.value(&mut v);
    assert_eq!(
        v["@odata.nextLink"],
        "https://graph.microsoft.com/v1.0/me/mailFolders('AAMk-2')/messages/delta?$skiptoken=fake-2&$select=isRead"
    );
    assert_eq!(
        v["@odata.deltaLink"],
        "https://graph.microsoft.com/v1.0/me/mailFolders('AAMk-2')/messages/delta?$deltatoken=fake-1"
    );
    let text = dump(&v);
    assert!(!text.contains(GUID) && !text.contains("REAL"), "{text}");
}

#[test]
fn a_batch_keeps_request_ids_and_redacts_the_replies() {
    let mut r = Redactor::default();
    let mut v = json!({"responses": [
        {"id": "2", "status": 404, "body": {"error": {"code": "ErrorItemNotFound", "message": format!("not found for {REAL}")}}},
        {"id": "1", "status": 200, "body": {"id": "AAMkOne", "subject": "S"}}
    ]});
    r.value(&mut v);
    assert_eq!(v["responses"][0]["id"], "2");
    assert_eq!(v["responses"][1]["id"], "1");
    assert_eq!(v["responses"][1]["body"]["id"], "AAMk-1");
    assert!(!dump(&v).contains("outlook"));
}
