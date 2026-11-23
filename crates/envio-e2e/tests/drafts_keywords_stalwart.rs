//! Edit never replaces a server draft that carries IMAP keywords (#324),
//! checked against Stalwart from crates/envio-net/tests/stalwart/up.sh.
//! Skips without it, unless ENVIO_REQUIRE_STALWART=1.
//!
//! Stalwart sets `$HasNoAttachment` on every plain message itself; the
//! rule ignores that one, so only `$Label1` (set by STORE) decides here.

mod support;

use std::time::{Duration, Instant};

use envio_core::{Command, FolderId, FolderRole};
use envio_store::Db;
use log::LevelFilter;
use support::core_run::{CoreRun, NET};
use support::drafts::{
    USER, drafts_now, edit_and_save, edit_shown_and_save, put_draft, settles_at,
};
use support::labels::{label, uid_of};
use support::stalwart::{Stalwart, message_id};

/// Waits until the cache holds `$Label1` for the message (`present`) or
/// not, so the next Edit starts from a known cache.
fn cache_has_label(run: &CoreRun, folder: FolderId, uid: u32, present: bool) {
    let db = Db::open(&run.paths().db()).unwrap();
    let end = Instant::now() + NET;
    loop {
        let known = db.keywords_of(folder, uid).unwrap();
        if let Some(words) = &known
            && words.iter().any(|w| w.eq_ignore_ascii_case("$Label1")) == present
        {
            return;
        }
        assert!(
            Instant::now() < end,
            "cache keywords {known:?}, wanted $Label1 present: {present}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[tokio::test]
async fn a_draft_with_a_keyword_is_copied_and_one_without_is_replaced() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let run = CoreRun::start(LevelFilter::Info);
    let account = run.add_account(&server, USER);
    let (folder, drafts) = run.folder(&account, FolderRole::Drafts);
    let tag = message_id();
    let tag = tag.trim_matches(['<', '>']).to_owned();
    let refresh = || {
        run.core().send(Command::Refresh {
            account: Some(account.clone()),
            ticket: 0,
        })
    };

    // The server holds `$Label1`, and the cache knows it: a copy.
    let original = format!("Labelled original {tag}");
    let copy = format!("Labelled copy {tag}");
    put_draft(&server, &drafts, (&original, ""), "Original text.").await;
    let uid = uid_of(&server, &drafts, &original).await;
    label(&server, &drafts, uid, true).await;
    refresh();
    run.row_with(folder, &original);
    cache_has_label(&run, folder, uid, true);
    edit_and_save(&run, (&account, folder), &original, &copy);
    settles_at(&server, &drafts, &copy, 1).await;
    // Give a wrongly queued replacement time to show.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let kept = drafts_now(&server, &drafts, &original).await;
    assert_eq!(kept.len(), 1, "the original stays: {kept:?}");
    assert!(kept[0].1.contains("Original text."), "{kept:?}");
    let all = drafts_now(&server, &drafts, &tag).await;
    assert_eq!(all.len(), 2, "two drafts in the account: {all:?}");

    // A fresh draft (the first one is linked to its copy now) had the
    // keyword and lost it on the server; a flag check shows that, so Edit
    // replaces this one.
    let cleared = format!("Cleared original {tag}");
    let replaced = format!("Cleared replaced {tag}");
    put_draft(&server, &drafts, (&cleared, ""), "Original text.").await;
    let uid = uid_of(&server, &drafts, &cleared).await;
    label(&server, &drafts, uid, true).await;
    refresh();
    run.row_with(folder, &cleared);
    cache_has_label(&run, folder, uid, true);
    label(&server, &drafts, uid, false).await;
    refresh();
    cache_has_label(&run, folder, uid, false);
    edit_and_save(&run, (&account, folder), &cleared, &replaced);
    settles_at(&server, &drafts, &replaced, 1).await;
    settles_at(&server, &drafts, &cleared, 0).await;
    assert_eq!(drafts_now(&server, &drafts, &copy).await.len(), 1);

    // A stale cache: it says "no keywords" but another client added one
    // after our last sync. Edit asks the server and copies.
    let stale = format!("Stale original {tag}");
    let stale_copy = format!("Stale copy {tag}");
    put_draft(&server, &drafts, (&stale, ""), "Original text.").await;
    let uid = uid_of(&server, &drafts, &stale).await;
    refresh();
    run.row_with(folder, &stale);
    let db = Db::open(&run.paths().db()).unwrap();
    let end = Instant::now() + NET;
    while db.keywords_of(folder, uid).unwrap().is_none() {
        assert!(Instant::now() < end, "the cache never learned the keywords");
        std::thread::sleep(Duration::from_millis(200));
    }
    label(&server, &drafts, uid, true).await;
    // A push (IDLE) could have refreshed the cache already; then Edit would
    // copy on the cached keyword alone and prove nothing about the live
    // check. Fail loudly instead of passing without it.
    let cached = db.keywords_of(folder, uid).unwrap();
    assert!(
        cached
            .as_ref()
            .is_some_and(|w| !w.iter().any(|k| k.eq_ignore_ascii_case("$Label1"))),
        "the cache must still be stale before Edit: {cached:?}"
    );
    edit_shown_and_save(&run, (&account, folder), &stale, &stale_copy);
    settles_at(&server, &drafts, &stale_copy, 1).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let kept = drafts_now(&server, &drafts, &stale).await;
    assert_eq!(kept.len(), 1, "the stale original stays: {kept:?}");
}
