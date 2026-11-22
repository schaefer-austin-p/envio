use super::*;

const FOLDERS: &[u8] = include_bytes!("../../../tests/fixtures/graph/folders.json");
const CHILDREN: &[u8] = include_bytes!("../../../tests/fixtures/graph/folders_children.json");
const DELTA_FIRST: &[u8] = include_bytes!("../../../tests/fixtures/graph/delta_first.json");
const DELTA_FINAL: &[u8] = include_bytes!("../../../tests/fixtures/graph/delta_final.json");
const DELTA_READ: &[u8] = include_bytes!("../../../tests/fixtures/graph/delta_after_read.json");
const DELTA_GONE: &[u8] = include_bytes!("../../../tests/fixtures/graph/delta_removed.json");
const BATCH: &[u8] = include_bytes!("../../../tests/fixtures/graph/batch.json");
const SEARCH1: &[u8] = include_bytes!("../../../tests/fixtures/graph/search_page1.json");
const SEARCH2: &[u8] = include_bytes!("../../../tests/fixtures/graph/search_page2.json");
const FIND: &[u8] = include_bytes!("../../../tests/fixtures/graph/find_message_id_inbox.json");
const ERR400: &[u8] = include_bytes!("../../../tests/fixtures/graph/error_400.json");

fn list_of(body: &[u8]) -> Vec<GMeta> {
    search(body).unwrap().0
}

fn page(r: Result<DeltaPage>) -> (Vec<GChange>, Vec<String>, DeltaNext) {
    match r.unwrap() {
        DeltaPage::Page {
            changed,
            removed,
            next,
        } => (changed, removed, next),
        DeltaPage::Expired => panic!("expired"),
    }
}

#[test]
fn folders_come_through_with_well_known_names_and_counts() {
    let (list, next) = folders(FOLDERS).unwrap();
    assert_eq!(next, None);
    assert!(list.len() >= 5);
    let inbox = list
        .iter()
        .find(|f| f.well_known.as_deref() == Some("inbox"))
        .unwrap();
    assert_eq!(inbox.display_name, "Inbox");
    assert!(!inbox.hidden);
    assert!(inbox.parent_id.is_some());
    let kids = list.iter().find(|f| f.child_count > 0).unwrap();
    assert_eq!(kids.well_known.as_deref(), Some("conversationhistory"));
    assert_eq!(folders(CHILDREN).unwrap().0, vec![]);
}

#[test]
fn a_folder_page_with_a_next_link_gives_it() {
    let body = br#"{"value":[{"id":"f1","displayName":"A","isHidden":true,"childFolderCount":2,
        "wellKnownName":"archive","folderClass":"IPF.Note"}],"@odata.nextLink":"https://graph.microsoft.com/v1.0/x?$skip=1"}"#;
    let (list, next) = folders(body).unwrap();
    assert_eq!(
        next.as_deref(),
        Some("https://graph.microsoft.com/v1.0/x?$skip=1")
    );
    assert!(list[0].hidden && list[0].child_count == 2);
    assert_eq!(list[0].class.as_deref(), Some("IPF.Note"));
}

#[test]
fn delta_pages_say_more_then_done() {
    let (changed, removed, next) = page(delta(200, DELTA_FIRST));
    assert_eq!(changed.len(), 3);
    assert!(removed.is_empty());
    assert!(matches!(next, DeltaNext::More(l) if l.contains("$skiptoken")));
    let m = &changed[0];
    assert_eq!(m.is_read, Some(false));
    assert_eq!(m.flagged, Some(false));
    assert_eq!(m.is_draft, Some(false));
    assert_eq!(m.categories, Some(vec![]));
    assert_eq!(m.received_ms, Some(1_795_300_784_000));
    let (changed, _, next) = page(delta(200, DELTA_FINAL));
    assert_eq!(changed.len(), 1);
    assert!(matches!(next, DeltaNext::Done(l) if l.contains("$deltatoken")));
}

#[test]
fn a_removal_lands_in_removed_not_changed() {
    let (changed, removed, next) = page(delta(200, DELTA_GONE));
    assert!(changed.is_empty());
    assert_eq!(removed.len(), 1);
    assert!(matches!(next, DeltaNext::Done(_)));
}

#[test]
fn a_changed_item_with_only_some_properties_leaves_the_rest_unchanged() {
    let (changed, removed, next) = page(delta(200, DELTA_READ));
    assert!(removed.is_empty() && matches!(next, DeltaNext::Done(_)));
    let c = &changed[0];
    assert_eq!(c.is_read, Some(true));
    assert_eq!(
        (c.received_ms, c.flagged, c.categories.clone(), c.is_draft),
        (None, None, None, None),
        "absent means unchanged, never false"
    );
}

#[test]
fn flags_and_categories_are_read() {
    let body = br#"{"@odata.deltaLink":"d","value":[
        {"id":"a","receivedDateTime":"2026-01-02T03:04:05Z","isRead":true,
         "flag":{"flagStatus":"flagged"},"categories":["Red","Blue"],"isDraft":true}]}"#;
    let (changed, _, _) = page(delta(200, body));
    let m = &changed[0];
    assert_eq!(m.is_read, Some(true));
    assert_eq!((m.flagged, m.is_draft), (Some(true), Some(true)));
    assert_eq!(
        m.categories,
        Some(vec!["Red".to_owned(), "Blue".to_owned()])
    );
}

#[test]
fn an_expired_delta_is_expired_by_status_or_by_code() {
    assert_eq!(delta(410, b"").unwrap(), DeltaPage::Expired);
    let body = br#"{"error":{"code":"syncStateNotFound","message":"x"}}"#;
    assert_eq!(delta(200, body).unwrap(), DeltaPage::Expired);
    assert_eq!(delta(400, body).unwrap(), DeltaPage::Expired);
}

#[test]
fn a_delta_page_without_links_or_values_is_a_protocol_error() {
    assert!(matches!(
        delta(200, br#"{"value":[]}"#),
        Err(NetError::Protocol(_))
    ));
    assert!(matches!(
        delta(200, br#"{"@odata.deltaLink":"d"}"#),
        Err(NetError::Protocol(_))
    ));
    assert!(matches!(delta(200, b"<html>"), Err(NetError::Protocol(_))));
}

#[test]
fn search_and_filter_replies_parse_with_paging() {
    let (list, next) = search(SEARCH1).unwrap();
    assert_eq!(list.len(), 2);
    assert!(next.is_some());
    let (list, next) = search(SEARCH2).unwrap();
    assert!(list.is_empty());
    assert_eq!(next, None);
    assert_eq!(list_of(SEARCH1)[0].received_ms, 1_795_300_784_000);
    assert_eq!(search(FIND).unwrap().0.len(), 1);
}

#[test]
fn batch_summaries_carry_size_headers_and_time() {
    let subs = batch(BATCH).unwrap();
    assert_eq!(subs.len(), 3);
    assert!(
        subs.iter()
            .all(|s| s.status == 200 && s.retry_after_s.is_none())
    );
    let ids: Vec<&str> = subs.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["3", "2", "1"], "in Graph's order, not the request's");
    let with_headers = subs
        .iter()
        .map(|s| summary(&s.body).unwrap())
        .find(|s| !s.headers.is_empty())
        .unwrap();
    assert!(with_headers.size > 0);
    assert!(with_headers.received_ms > 0);
    assert!(
        with_headers
            .headers
            .iter()
            .any(|(n, v)| n == "Message-ID" && v.ends_with("@example.org>"))
    );
}

#[test]
fn a_summary_without_the_size_property_has_size_zero() {
    let body = br#"{"id":"m","receivedDateTime":"2026-01-02T03:04:05Z"}"#;
    let s = summary(body).unwrap();
    assert_eq!((s.size, s.headers.len()), (0, 0));
}

#[test]
fn the_size_property_is_found_by_any_spelling_of_its_id() {
    for id in ["Integer 0x0E08", "Integer 0xe08", "integer 0x00E08"] {
        let body = format!(
            r#"{{"id":"m","receivedDateTime":"2026-01-02T03:04:05Z",
            "singleValueExtendedProperties":[{{"id":"Integer 0x1","value":"9"}},{{"id":"{id}","value":"1234"}}]}}"#
        );
        assert_eq!(summary(body.as_bytes()).unwrap().size, 1234, "{id}");
    }
}

#[test]
fn batch_replies_are_read_by_id_with_status_and_retry_after() {
    let body = br#"{"responses":[
        {"id":"2","status":404,"body":{"error":{"code":"ErrorItemNotFound"}}},
        {"id":"1","status":200,"body":{"id":"m"}},
        {"id":"3","status":429,"headers":{"retry-after":"7"},"body":{"error":{"code":"x"}}},
        {"id":"4","status":503,"headers":{"Retry-After":"Wed, 21 Oct 2026 07:28:00 GMT"},"body":null}]}"#;
    let subs = batch(body).unwrap();
    let by = |id: &str| subs.iter().find(|s| s.id == id).unwrap();
    assert_eq!(by("2").status, 404);
    assert_eq!(by("1").status, 200);
    assert_eq!(by("3").retry_after_s, Some(7));
    assert_eq!(by("4").retry_after_s, None);
    let err = summary(&by("1").body).unwrap_err().to_string();
    assert!(err.contains("receivedDateTime"), "{err}");
}

#[test]
fn a_batch_without_responses_is_a_protocol_error() {
    assert!(matches!(batch(br#"{"x":1}"#), Err(NetError::Protocol(_))));
    assert!(matches!(
        batch(br#"{"responses":[{"id":"1"}]}"#),
        Err(NetError::Protocol(_))
    ));
}

#[test]
fn an_error_keeps_status_and_a_plain_code_only() {
    let e = error(400, ERR400, None);
    assert_eq!(
        (e.status, e.code.as_str()),
        (400, "RequestBroker--ParseUri")
    );
    let body = br#"{"error":{"code":"Bad code with spaces <alice@example.org>"}}"#;
    assert_eq!(error(500, body, None).code, "");
    assert_eq!(
        error(503, b"not json", Some(" 12 ")).retry_after_s,
        Some(12)
    );
    assert_eq!(error(503, b"", Some("soon")).retry_after_s, None);
}

#[test]
fn parse_errors_never_quote_the_body() {
    let secret = br#"{"value":[{"subject":"secret plan alice@example.org"}]}"#;
    let err = folders(secret).unwrap_err().to_string();
    assert!(!err.contains("secret") && !err.contains("alice"), "{err}");
}
