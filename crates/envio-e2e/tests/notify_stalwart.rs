//! New-mail alerts against Stalwart from crates/envio-net/tests/stalwart/up.sh,
//! through a recording `Poster`. Skips without it, unless
//! ENVIO_REQUIRE_STALWART=1.
//!
//! The core has no background IDLE yet, so these run with the app in the
//! foreground and `hooks::set_unfocused`, which makes the worker treat it
//! as not focused (what a background app is). Plan 9a Task 4 adds the
//! variant with a real `AppActive(false)`.

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use envio_core::hooks::{self, Point};
use envio_core::{Alert, Event, FolderRole, Poster};
use log::LevelFilter;
use support::core_run::{CoreRun, NET, password};
use support::stalwart::{Stalwart, message, message_id};

#[derive(Default, Clone)]
struct Recorder(Arc<Mutex<Recorded>>);

#[derive(Default)]
struct Recorded {
    alerts: Vec<Alert>,
    badges: Vec<u32>,
}

impl Poster for Recorder {
    fn post(&self, alert: &Alert) {
        self.0.lock().unwrap().alerts.push(alert.clone());
    }
    fn badge(&self, total_unread: u32) {
        self.0.lock().unwrap().badges.push(total_unread);
    }
}

impl Recorder {
    fn alerts(&self) -> Vec<Alert> {
        self.0.lock().unwrap().alerts.clone()
    }
}

/// Puts the core back to normal when the test ends, however it ends.
struct Unfocused;

impl Unfocused {
    fn on() -> Self {
        hooks::set_unfocused(true);
        Unfocused
    }
}

impl Drop for Unfocused {
    fn drop(&mut self) {
        hooks::set_unfocused(false);
    }
}

fn idle_entered(account: &str) -> std::sync::mpsc::Receiver<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = Mutex::new(tx);
    hooks::before_next(Point::IdleEntered, account, move || {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(());
        }
    });
    rx
}

fn creds(user: &str) -> envio_net::Credentials {
    envio_net::Credentials::new(user, &password(user))
}

/// Appends the messages to the user's INBOX, one after the other.
async fn deliver(server: &Stalwart, user: &str, raw: &[Vec<u8>]) {
    let mut session = server.imap(&creds(user)).await;
    for eml in raw {
        session.append("INBOX", &[], eml).await.unwrap();
    }
    session.logout().await.unwrap();
}

fn from_alice(subject: &str) -> Vec<u8> {
    message(&message_id(), subject, 500)
}

/// A message the user sent to themselves.
fn from_self(user: &str, subject: &str) -> Vec<u8> {
    format!(
        "From: {user}@envio.test\r\nTo: {user}@envio.test\r\nSubject: {subject}\r\n\
         Message-ID: {}\r\nDate: Sun, 08 Nov 2026 10:00:00 +0000\r\n\
         MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nnote\r\n",
        message_id()
    )
    .into_bytes()
}

/// One test at a time: the focus hook is process-wide.
static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn three_messages_in_a_second_give_alerts_that_count_to_three_without_text() {
    let _turn = TURN.lock().await;
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let run = CoreRun::start(LevelFilter::Info);
    let recorder = Recorder::default();
    run.core().set_poster(Box::new(recorder.clone()));
    let account = run.add_account(&server, "grace");
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    let idle = idle_entered(&account);
    idle.recv_timeout(NET).expect("IDLE was never entered");
    // The first sync of an account never notifies.
    assert!(recorder.alerts().is_empty());

    let _unfocused = Unfocused::on();
    let subjects = ["Alpha 1", "Alpha 2", "Alpha 3"];
    let raw: Vec<_> = subjects.iter().map(|s| from_alice(s)).collect();
    deliver(&server, "grace", &raw).await;

    // One NewMail per wake; their counts add up to the three messages.
    let mut counted = 0;
    let mut wakes = 0;
    while counted < 3 {
        counted += run.wait(NET, |event| match event {
            Event::NewMail(mail) if mail.account == account => {
                assert_eq!(mail.first.is_some(), mail.count == 1);
                Some(mail.count)
            }
            _ => None,
        });
        wakes += 1;
    }
    assert_eq!(counted, 3, "more than the three that were delivered");
    for subject in subjects {
        run.row_with(inbox, subject);
    }

    let alerts = recorder.alerts();
    assert_eq!(alerts.len(), wakes, "one alert per wake");
    for alert in &alerts {
        assert_eq!(alert.tag, format!("new-{account}"));
        assert_eq!(alert.account, account);
        assert_eq!(alert.summary, "grace");
        // Text is off: the count, never a sender or subject.
        assert!(alert.body.ends_with("new message") || alert.body.ends_with("new messages"));
        assert!(!alert.body.contains("Alpha") && !alert.body.contains("Alice"));
    }
    let shown: u32 = alerts
        .iter()
        .map(|a| a.body.split(' ').next().unwrap().parse::<u32>().unwrap())
        .sum();
    assert_eq!(shown, 3);
    // The badge followed the Inbox unread count up.
    let badges = recorder.0.lock().unwrap().badges.clone();
    assert!(badges.last().is_some_and(|total| *total >= 3), "{badges:?}");
}

#[tokio::test]
async fn mail_the_account_sent_to_itself_does_not_notify() {
    let _turn = TURN.lock().await;
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let run = CoreRun::start(LevelFilter::Info);
    let recorder = Recorder::default();
    run.core().set_poster(Box::new(recorder.clone()));
    let account = run.add_account(&server, "heidi");
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    let idle = idle_entered(&account);
    idle.recv_timeout(NET).expect("IDLE was never entered");

    let _unfocused = Unfocused::on();
    // The next IDLE after the wake the note causes.
    let idle = idle_entered(&account);
    deliver(&server, "heidi", &[from_self("heidi", "Note to self")]).await;
    run.row_with(inbox, "Note to self");
    // A mail from someone else right after is the control: it notifies.
    idle.recv_timeout(NET).expect("IDLE did not come back");
    deliver(&server, "heidi", &[from_alice("Control")]).await;
    // Waiting for the rows would swallow the events, so wait for the alert.
    let end = std::time::Instant::now() + NET;
    while recorder.alerts().is_empty() {
        assert!(
            std::time::Instant::now() < end,
            "the control never notified"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    run.row_with(inbox, "Control");
    // Only the control notified: the note to self did not.
    let alerts = recorder.alerts();
    assert_eq!(alerts.len(), 1, "{alerts:?}");
    assert_eq!(alerts[0].body, "1 new message");
}
