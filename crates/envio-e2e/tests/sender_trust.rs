//! Sender trust (#285) end to end: a DKIM-signed message is appended to
//! Stalwart, the sender is allowed through the command API, and the message
//! opens with its images after the second render, once the key has been
//! looked up (a fake resolver holds the test key of the `envio-mime`
//! fixtures). A copy with one body byte changed never loads them.

mod support;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use envio_core::api::view::MessageView;
use envio_core::{Command, Event, FolderRole, ImagesChoice};
use envio_net::dns::{TxtAnswer, TxtLookup};
use log::LevelFilter;
use support::core_run::{CoreRun, NET, password, setup};
use support::stalwart::Stalwart;

const SENDER: &str = "bob@example.com";
const KEY_NAME: &str = "test._domainkey.example.com";
const IMAGE: &str = "images.example.net/a.png";

/// Tests that sign in at once can make Stalwart drop SMTP TLS connections
/// (see `core_stalwart.rs`), so they run one at a time.
static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn fixture_dir() -> String {
    format!(
        "{}/../envio-mime/tests/fixtures/dkim",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// Answers from the fixtures' key table and records every name asked.
struct Fake {
    answers: HashMap<String, String>,
    asked: Mutex<Vec<String>>,
}

impl Fake {
    fn new() -> Arc<Fake> {
        let keys = std::fs::read_to_string(format!("{}/keys.txt", fixture_dir())).unwrap();
        let answers = keys
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(n, r)| (n.to_owned(), r.to_owned()))
            .collect();
        Arc::new(Fake {
            answers,
            asked: Mutex::new(Vec::new()),
        })
    }
}

impl TxtLookup for Fake {
    fn txt<'a>(&'a self, name: &'a str) -> Pin<Box<dyn Future<Output = TxtAnswer> + Send + 'a>> {
        self.asked.lock().unwrap().push(name.to_owned());
        let answer = match self.answers.get(name) {
            Some(record) => TxtAnswer::Records {
                texts: vec![record.clone()],
                ttl: 3600,
            },
            None => TxtAnswer::NoRecord,
        };
        Box::pin(async move { answer })
    }
}

fn loaded(view: &MessageView) -> bool {
    view.blocked_images == 0 && view.html.contains(IMAGE)
}

/// What opening the message showed.
struct Shown {
    /// The first render.
    first: MessageView,
    /// The render that followed the key lookup, if one came within 3 s.
    second: Option<MessageView>,
    /// The key names looked up.
    asked: Vec<String>,
}

/// Appends `raw` for `user`, starts a core with the fake resolver, allows
/// the sender and opens the message. `None` without a Stalwart.
async fn open_after_allow(user: &str, raw: &[u8]) -> Option<Shown> {
    let _turn = TURN.lock().await;
    let server = Stalwart::find().await?;
    let mut session = server
        .imap(&envio_net::Credentials::new(user, &password(user)))
        .await;
    session.append("INBOX", &[], raw).await.unwrap();
    session.logout().await.unwrap();

    let dns = Fake::new();
    let run = CoreRun::start_with_dns(LevelFilter::Info, Some(dns.clone()));
    let account = run.add_account_with(user, &setup(&server, user));
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    let id = run.row_with(inbox, "Hello").id;
    run.core().send(Command::AllowImagesFrom {
        address: SENDER.to_owned(),
    });
    run.core().send(Command::OpenMessage {
        id,
        images: ImagesChoice::Default,
        width_px: 300,
    });
    let pick = |e: &Event| match e {
        Event::MessageOpened { id: i, view } if *i == id => Some(view.clone()),
        _ => None,
    };
    let first = run.wait(NET, pick);
    let end = Instant::now() + Duration::from_secs(3);
    let mut second = None;
    while let Ok(event) = run
        .events
        .recv_timeout(end.saturating_duration_since(Instant::now()))
    {
        if let Some(view) = pick(&event) {
            second = Some(view);
            break;
        }
    }
    let asked = dns.asked.lock().unwrap().clone();
    Some(Shown {
        first,
        second,
        asked,
    })
}

#[tokio::test]
async fn an_allowed_sender_with_a_verifying_signature_gets_images_on_the_second_render() {
    let raw = std::fs::read(format!("{}/html.eml", fixture_dir())).unwrap();
    let Some(shown) = open_after_allow("grace", &raw).await else {
        return;
    };
    assert!(shown.first.header.sender_checkable);
    assert!(!loaded(&shown.first), "the first render waits for the key");
    let second = shown.second.expect("a second render follows the lookup");
    assert!(second.header.sender_verified, "{}", second.html);
    assert!(loaded(&second), "{}", second.html);
    assert_eq!(shown.asked, [KEY_NAME]);
}

#[tokio::test]
async fn a_copy_with_one_body_byte_changed_keeps_images_blocked() {
    let mut raw = std::fs::read(format!("{}/html.eml", fixture_dir())).unwrap();
    let at = raw.windows(4).position(|w| w == b"Look").unwrap();
    raw[at] = b'l';
    let Some(shown) = open_after_allow("heidi", &raw).await else {
        return;
    };
    assert!(shown.first.header.sender_checkable);
    assert!(!loaded(&shown.first));
    assert_eq!(
        shown.asked,
        [KEY_NAME],
        "the lookup ran; the body hash did not fit"
    );
    assert!(shown.second.is_none(), "nothing is shown again");
}
