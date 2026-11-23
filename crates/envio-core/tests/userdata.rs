//! What belongs to the user: autocomplete, settings, allowed image senders.

mod support;

use envio_core::{
    Command, ComposeId, Contact, DarkMode, DraftInput, Event, FolderRole, Settings, Suggestion,
};
use envio_store::{Db, load_settings, save_contacts, save_settings};
use support::Harness;

const ME: &str = "me@example.org";

/// A started core with one account; `prepare` runs first, with the
/// folders in place, to seed what the core loads.
fn started(prepare: impl FnOnce(&Harness)) -> (Harness, String) {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[]);
    std::fs::create_dir_all(h.data_dir()).unwrap();
    prepare(&h);
    h.start_core();
    (h, key)
}

fn remember(h: &Harness, found: &[(&str, &str)]) {
    let db = Db::open(&h.paths().db()).unwrap();
    let found: Vec<(String, String)> = found
        .iter()
        .map(|(n, a)| ((*n).to_owned(), (*a).to_owned()))
        .collect();
    db.remember_addresses(&found, 1_000).unwrap();
}

fn suggestions(h: &Harness, query: &str) -> Vec<Suggestion> {
    h.send(Command::Autocomplete {
        query: query.to_owned(),
    });
    h.wait(|e| match e {
        Event::Suggestions { query: q, items } if q == query => Some(items.clone()),
        _ => None,
    })
}

fn addresses(items: &[Suggestion]) -> Vec<&str> {
    items.iter().map(|s| s.address.as_str()).collect()
}

fn settings_event(h: &Harness) -> Settings {
    h.wait(|e| match e {
        Event::Settings(s) => Some(s.clone()),
        _ => None,
    })
}

#[test]
fn autocomplete_puts_contacts_first_without_duplicates() {
    let (h, _) = started(|h| {
        let contacts = [Contact {
            name: "Ada Lovelace".into(),
            address: "ada@example.org".into(),
        }];
        save_contacts(&h.paths().contacts(), &contacts).unwrap();
        // Ada is also a frequent correspondent; Adam only that.
        remember(h, &[("Ada", "ada@example.org"), ("Ada", "ada@example.org")]);
        remember(h, &[("Adam", "adam@example.org")]);
    });
    let items = suggestions(&h, "ad");
    assert_eq!(addresses(&items), ["ada@example.org", "adam@example.org"]);
    assert!(items[0].from_contacts);
    assert_eq!(items[0].name, "Ada Lovelace");
    assert!(!items[1].from_contacts);
    assert_eq!(items[1].last_seen, Some(1_000));
    assert!(suggestions(&h, "").is_empty());
    assert!(suggestions(&h, "   ").is_empty());
}

#[test]
fn autocomplete_answers_at_most_eight() {
    let (h, _) = started(|h| {
        let found: Vec<(String, String)> = (0..12)
            .map(|n| (String::new(), format!("m{n}@example.org")))
            .collect();
        let found: Vec<(&str, &str)> = found
            .iter()
            .map(|(n, a)| (n.as_str(), a.as_str()))
            .collect();
        remember(h, &found);
    });
    assert_eq!(suggestions(&h, "m").len(), 8);
}

// What a sync stores is the `addresses` table (envio-sync's
// `a_sync_remembers_who_wrote_and_who_was_written_to` covers the storing);
// this is the reading of it: a sender the cache has seen is offered, not from the contacts.
#[test]
fn synced_senders_become_suggestions() {
    let (h, _) = started(|h| remember(h, &[("Grace Hopper", "Grace@Navy.example")]));
    let items = suggestions(&h, "hop");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].address, "grace@navy.example");
    assert_eq!(items[0].name, "Grace Hopper");
    assert!(!items[0].from_contacts);
}

#[test]
fn sent_recipients_become_suggestions() {
    let (h, key) = started(|_| ());
    let compose = ComposeId(1);
    h.send(Command::Send {
        compose,
        draft: DraftInput {
            account: key,
            to: vec!["Bob Builder <bob@example.org>".into()],
            cc: vec!["carol@example.org".into()],
            subject: "Plans".into(),
            body: "See you".into(),
            ..DraftInput::default()
        },
    });
    h.wait(|e| matches!(e, Event::Queued { .. }).then_some(()));
    let items = suggestions(&h, "bob");
    assert_eq!(addresses(&items), ["bob@example.org"]);
    assert_eq!(items[0].name, "Bob Builder");
    assert_eq!(addresses(&suggestions(&h, "carol")), ["carol@example.org"]);
}

#[test]
fn settings_are_sent_after_start_and_after_save() {
    let (h, _) = started(|_| ());
    assert_eq!(settings_event(&h), Settings::default());
    let changed = Settings {
        dark: DarkMode::Dark,
        default_account: Some("a1".into()),
        ..Settings::default()
    };
    h.send(Command::SaveSettings(changed.clone()));
    assert_eq!(settings_event(&h), changed);
    let on_disk = load_settings(&h.paths().settings()).unwrap();
    assert_eq!(on_disk.dark, DarkMode::Dark);
    assert_eq!(on_disk.default_account.as_deref(), Some("a1"));
    // A restart reads them back.
    let mut h = h;
    h.restart();
    assert_eq!(settings_event(&h).dark, DarkMode::Dark);
}

#[test]
fn a_failed_save_keeps_the_old_settings() {
    let (h, _) = started(|_| ());
    assert_eq!(settings_event(&h), Settings::default());
    // A folder with a file in it where the settings go: the atomic write
    // cannot replace that.
    let blocker = h.paths().settings();
    std::fs::create_dir_all(&blocker).unwrap();
    std::fs::write(blocker.join("x"), b"x").unwrap();
    h.send(Command::SaveSettings(Settings {
        dark: DarkMode::Dark,
        ..Settings::default()
    }));
    let notice = h.wait(|e| match e {
        Event::Notice { text } => Some(text.clone()),
        _ => None,
    });
    assert!(!notice.is_empty());
    assert_eq!(settings_event(&h), Settings::default());
}

#[test]
fn allow_images_from_is_saved() {
    let (h, _) = started(|_| ());
    settings_event(&h);
    h.send(Command::AllowImagesFrom {
        address: " Ada@Example.org ".into(),
    });
    let settings = settings_event(&h);
    assert!(settings.images_from.contains("ada@example.org"));
    let on_disk = load_settings(&h.paths().settings()).unwrap();
    assert!(on_disk.images_from.contains("ada@example.org"));
}

#[test]
fn disallow_images_from_is_saved() {
    let (h, _) = started(|h| {
        let settings = Settings {
            images_from: ["ada@example.org".to_owned(), "eve@example.org".to_owned()].into(),
            ..Settings::default()
        };
        save_settings(&h.paths().settings(), &settings).unwrap();
    });
    settings_event(&h);
    h.send(Command::DisallowImagesFrom {
        address: " Ada@Example.org ".into(),
    });
    let settings = settings_event(&h);
    assert!(!settings.images_from.contains("ada@example.org"));
    assert!(settings.images_from.contains("eve@example.org"));
    let on_disk = load_settings(&h.paths().settings()).unwrap();
    assert!(!on_disk.images_from.contains("ada@example.org"));
    assert!(on_disk.images_from.contains("eve@example.org"));
}

// The app builds a save from the settings it last heard; a save made before
// a removal's echo arrived must not bring the sender back.
#[test]
fn a_stale_save_keeps_the_current_allowed_senders() {
    let (h, _) = started(|h| {
        let settings = Settings {
            images_from: ["ada@example.org".to_owned(), "eve@example.org".to_owned()].into(),
            ..Settings::default()
        };
        save_settings(&h.paths().settings(), &settings).unwrap();
    });
    let stale = settings_event(&h);
    h.send(Command::DisallowImagesFrom {
        address: "ada@example.org".into(),
    });
    settings_event(&h);
    h.send(Command::SaveSettings(Settings {
        dark: DarkMode::Dark,
        ..stale
    }));
    let settings = settings_event(&h);
    assert_eq!(settings.dark, DarkMode::Dark);
    assert!(!settings.images_from.contains("ada@example.org"));
    assert!(settings.images_from.contains("eve@example.org"));
    let on_disk = load_settings(&h.paths().settings()).unwrap();
    assert!(!on_disk.images_from.contains("ada@example.org"));
}

#[test]
fn two_set_notify_for_different_accounts_both_survive() {
    let (h, _) = started(|_| ());
    settings_event(&h);
    // Both sent before either echo: a snapshot-based save would lose one.
    h.send(Command::SetNotify {
        account: "a".into(),
        on: false,
    });
    h.send(Command::SetNotify {
        account: "b".into(),
        on: false,
    });
    settings_event(&h);
    let settings = settings_event(&h);
    assert_eq!(settings.notify_off.len(), 2);
    let on_disk = load_settings(&h.paths().settings()).unwrap();
    assert_eq!(on_disk.notify_off, settings.notify_off);
    h.send(Command::SetNotify {
        account: "a".into(),
        on: true,
    });
    assert_eq!(
        settings_event(&h).notify_off,
        ["b".to_owned()].into_iter().collect()
    );
}

#[test]
fn set_notify_then_set_notify_text_both_survive() {
    let (h, _) = started(|_| ());
    settings_event(&h);
    h.send(Command::SetNotify {
        account: "a".into(),
        on: false,
    });
    h.send(Command::SetNotifyText { on: true });
    settings_event(&h);
    let settings = settings_event(&h);
    assert!(settings.notify_off.contains("a"));
    assert!(settings.notify_show_text);
}

// An app save built from a snapshot older than a switch must not undo it.
#[test]
fn a_stale_save_keeps_the_notification_switches() {
    let (h, _) = started(|_| ());
    settings_event(&h);
    h.send(Command::SetNotify {
        account: "a".into(),
        on: false,
    });
    h.send(Command::SetNotifyText { on: true });
    h.send(Command::SaveSettings(Settings {
        dark: DarkMode::Dark,
        ..Settings::default()
    }));
    settings_event(&h);
    settings_event(&h);
    let settings = settings_event(&h);
    assert_eq!(settings.dark, DarkMode::Dark);
    assert!(settings.notify_off.contains("a"));
    assert!(settings.notify_show_text);
}

#[test]
fn the_list_pane_width_is_clamped_saved_and_kept_by_a_stale_save() {
    let (h, _) = started(|_| ());
    settings_event(&h);
    h.send(Command::SetListPane { gu: 61 });
    assert_eq!(settings_event(&h).list_pane_gu, 61);
    assert_eq!(
        load_settings(&h.paths().settings()).unwrap().list_pane_gu,
        61
    );
    h.send(Command::SetListPane { gu: 500 });
    assert_eq!(settings_event(&h).list_pane_gu, 70);
    h.send(Command::SetListPane { gu: 1 });
    assert_eq!(settings_event(&h).list_pane_gu, 36);
    // An app save built from an older snapshot must not undo the width.
    h.send(Command::SaveSettings(Settings::default()));
    assert_eq!(settings_event(&h).list_pane_gu, 36);
}
