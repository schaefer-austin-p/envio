//! Sender trust (#285): DKIM keys are looked up only for a sender the user
//! allowed, only while the message is open, and a verified message is shown
//! again with its images. The resolver is a fake that counts calls.

mod support;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use envio_core::api::view::MessageView;
use envio_core::{Command, Event, FolderRole, ImagesChoice, ListRef, MessageRowId};
use envio_net::dns::{TxtAnswer, TxtLookup};
use futures::future::BoxFuture;
use support::Harness;

const ME: &str = "me@example.org";
const FROM: &str = "bob@example.com";
const NAME: &str = "test._domainkey.example.com";
const ED_NAME: &str = "ed._domainkey.example.com";
const IMAGE: &str = "images.example.net/a.png";
const IMAGE_URL: &str = "http://images.example.net/a.png";

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../envio-mime/tests/fixtures/dkim/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// The key records of the DKIM fixtures, by name.
fn records() -> HashMap<String, TxtAnswer> {
    let text = String::from_utf8(fixture("keys.txt")).unwrap();
    text.lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, record)| {
            let answer = TxtAnswer::Records {
                texts: vec![record.to_owned()],
                ttl: 3600,
            };
            (name.to_owned(), answer)
        })
        .collect()
}

/// A resolver that answers from a table and counts every call.
struct Fake {
    answers: HashMap<String, TxtAnswer>,
    fallback: TxtAnswer,
    calls: Mutex<Vec<String>>,
    /// While `false`, an answer waits (see `hold`).
    gate: AtomicBool,
}

impl Fake {
    fn new(answers: HashMap<String, TxtAnswer>, fallback: TxtAnswer) -> Arc<Fake> {
        Arc::new(Fake {
            answers,
            fallback,
            calls: Mutex::new(Vec::new()),
            gate: AtomicBool::new(true),
        })
    }

    /// From now on, answers wait until `release`.
    fn hold(&self) {
        self.gate.store(false, Ordering::SeqCst);
    }

    fn release(&self) {
        self.gate.store(true, Ordering::SeqCst);
    }

    fn calls(&self) -> Vec<String> {
        let mut calls = self.calls.lock().unwrap().clone();
        calls.sort();
        calls
    }
}

impl TxtLookup for Fake {
    fn txt<'a>(&'a self, name: &'a str) -> BoxFuture<'a, TxtAnswer> {
        self.calls.lock().unwrap().push(name.to_owned());
        let answer = self.answers.get(name).unwrap_or(&self.fallback).clone();
        Box::pin(async move {
            while !self.gate.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            answer
        })
    }
}

struct Open {
    h: Harness,
    /// The first message.
    id: MessageRowId,
    /// Every message, by uid order.
    ids: Vec<MessageRowId>,
    dns: Arc<Fake>,
}

struct Setup<'a> {
    raw: &'a [u8],
    host: &'a str,
    allowed: bool,
    images_always: bool,
    /// More messages after `raw`.
    others: Vec<&'a [u8]>,
    /// The first message is cached as a partial body.
    partial: bool,
}

impl<'a> Setup<'a> {
    fn new(raw: &'a [u8]) -> Self {
        Setup {
            raw,
            host: "outlook.office365.com",
            allowed: true,
            images_always: false,
            others: Vec::new(),
            partial: false,
        }
    }

    fn start(self, dns: Arc<Fake>) -> Open {
        let mut h = Harness::unstarted();
        let key = h.seed_account_on(ME, Some("pw"), (self.host, 1));
        let mut messages = vec![(1, self.raw)];
        for (n, raw) in self.others.iter().enumerate() {
            messages.push((u32::try_from(n).unwrap() + 2, *raw));
        }
        let folder = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &messages);
        if self.partial {
            let db = envio_store::Db::open(&h.paths().db()).unwrap();
            db.set_body_state(MessageRowId(1), envio_store::BodyState::Partial)
                .unwrap();
        }
        std::fs::create_dir_all(h.data_dir()).unwrap();
        let mut settings = envio_core::Settings::default();
        if self.allowed {
            settings.images_from = [FROM.to_owned()].into();
        }
        if self.images_always {
            settings.images = envio_core::ImagesDefault::Always;
        }
        envio_store::save_settings(&h.paths().settings(), &settings).unwrap();
        h.dns = Some(dns.clone());
        h.start_core();
        h.send(Command::ListMessages {
            list: ListRef::Folder(folder),
            after: None,
        });
        let rows = h.wait(|e| match e {
            Event::Messages { rows, .. } => Some(rows.clone()),
            _ => None,
        });
        let ids: Vec<MessageRowId> = (1..=messages.len())
            .map(|n| {
                let subject = format!("message {n}");
                rows.iter().find(|r| r.subject == subject).unwrap().id
            })
            .collect();
        Open {
            h,
            id: ids[0],
            ids,
            dns,
        }
    }
}

impl Open {
    fn opened(&self) -> MessageView {
        let id = self.id;
        self.h.wait(|e| match e {
            Event::MessageOpened { id: i, view } if *i == id => Some(view.clone()),
            _ => None,
        })
    }

    fn open(&self, images: ImagesChoice) -> MessageView {
        self.open_id(self.id, images)
    }

    fn open_id(&self, id: MessageRowId, images: ImagesChoice) -> MessageView {
        self.h.send(Command::OpenMessage {
            id,
            images,
            width_px: 300,
        });
        self.opened_id(id)
    }

    fn opened_id(&self, id: MessageRowId) -> MessageView {
        self.h.wait(|e| match e {
            Event::MessageOpened { id: i, view } if *i == id => Some(view.clone()),
            _ => None,
        })
    }

    /// Whether a `MessageOpened` came after a pause long enough for any
    /// lookup to have ended, and before the answer to a later command.
    fn shown_again(&self) -> bool {
        std::thread::sleep(Duration::from_millis(200));
        self.h.send(Command::ListFolders {
            account: String::new(),
        });
        self.h.wait(|e| match e {
            Event::MessageOpened { .. } => Some(true),
            Event::Folders { .. } | Event::Notice { .. } => Some(false),
            _ => None,
        })
    }
}

fn loaded(view: &MessageView) -> bool {
    view.blocked_images == 0 && view.html.contains(IMAGE)
}

fn with_header(header: &str, raw: &[u8]) -> Vec<u8> {
    let mut out = header.as_bytes().to_vec();
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(raw);
    out
}

fn ok_dns() -> Arc<Fake> {
    Fake::new(records(), TxtAnswer::NoRecord)
}

#[test]
fn an_outlook_style_account_is_verified_by_dkim_once_the_key_arrives() {
    let raw = with_header(
        "Authentication-Results: dkim=pass header.d=example.com",
        &fixture("html.eml"),
    );
    let s = Setup::new(&raw).start(ok_dns());
    let first = s.open(ImagesChoice::Default);
    assert!(!first.header.sender_verified);
    assert!(first.header.sender_checkable);
    assert!(!loaded(&first), "{}", first.html);
    let second = s.opened();
    assert!(second.header.sender_verified);
    assert!(second.header.sender_known);
    assert!(loaded(&second), "{}", second.html);
    assert_eq!(s.dns.calls(), [NAME]);
}

#[test]
fn a_reopened_message_is_verified_at_once_from_the_cache() {
    let s = Setup::new(&fixture("html.eml")).start(ok_dns());
    s.open(ImagesChoice::Default);
    assert!(s.opened().header.sender_verified);
    let again = s.open(ImagesChoice::Default);
    assert!(again.header.sender_verified && loaded(&again));
    assert!(!s.shown_again(), "nothing follows a render that is final");
    assert_eq!(s.dns.calls(), [NAME], "no second lookup within the TTL");
}

#[test]
fn a_sender_who_is_not_allowed_is_never_looked_up() {
    let raw = fixture("html.eml");
    let mut setup = Setup::new(&raw);
    setup.allowed = false;
    let s = setup.start(ok_dns());
    let view = s.open(ImagesChoice::Default);
    assert!(view.header.sender_checkable);
    assert!(!view.header.sender_verified);
    assert!(!loaded(&view));
    assert!(!s.shown_again());
    assert!(s.dns.calls().is_empty());
}

#[test]
fn unsigned_mail_is_not_looked_up_and_not_checkable() {
    let raw = b"From: Bob <bob@example.com>\r\nSubject: s\r\nMIME-Version: 1.0\r\n\
Content-Type: text/html; charset=utf-8\r\n\r\n\
<p>x</p><img src=\"http://images.example.net/a.png\" alt=\"a\">";
    let s = Setup::new(raw).start(ok_dns());
    let view = s.open(ImagesChoice::Default);
    assert!(!view.header.sender_checkable && !view.header.sender_verified);
    assert!(!s.shown_again());
    assert!(s.dns.calls().is_empty());
}

#[test]
fn a_copy_with_a_changed_body_never_verifies() {
    let mut raw = fixture("html.eml");
    let at = raw.windows(4).position(|w| w == b"Look").unwrap();
    raw[at] = b'L' ^ 0x20;
    let s = Setup::new(&raw).start(ok_dns());
    let view = s.open(ImagesChoice::Default);
    assert!(view.header.sender_checkable && !view.header.sender_verified);
    assert!(!s.shown_again());
    assert!(!loaded(&view));
}

#[test]
fn a_key_that_does_not_fit_the_signature_keeps_images_blocked() {
    // The record under the signature's name is an Ed25519 key; the
    // signature is RSA.
    let mut answers = records();
    answers.insert(NAME.to_owned(), answers[ED_NAME].clone());
    let s = Setup::new(&fixture("html.eml")).start(Fake::new(answers, TxtAnswer::NoRecord));
    let first = s.open(ImagesChoice::Default);
    assert!(!loaded(&first));
    // The lookup ran; its answer changed nothing on screen.
    assert!(!s.shown_again());
    assert_eq!(s.dns.calls(), [NAME]);
}

#[test]
fn images_that_always_load_or_were_loaded_by_hand_need_no_lookup() {
    let raw = fixture("html.eml");
    let mut setup = Setup::new(&raw);
    setup.images_always = true;
    let s = setup.start(ok_dns());
    let view = s.open(ImagesChoice::Default);
    assert!(loaded(&view));
    assert!(!s.shown_again());
    assert!(s.dns.calls().is_empty());

    let s = Setup::new(&fixture("html.eml")).start(ok_dns());
    let view = s.open(ImagesChoice::Load);
    assert!(loaded(&view));
    assert!(!s.shown_again());
    assert!(s.dns.calls().is_empty());
}

#[test]
fn an_unavailable_resolver_costs_one_lookup_per_name_per_open() {
    let s =
        Setup::new(&fixture("html.eml")).start(Fake::new(HashMap::new(), TxtAnswer::Unavailable));
    let first = s.open(ImagesChoice::Default);
    assert!(first.header.sender_checkable && !loaded(&first));
    // The reply changes nothing and renders nothing.
    assert!(!s.shown_again());
    assert_eq!(s.dns.calls(), [NAME]);
    // Opening it again is the retry.
    s.open(ImagesChoice::Default);
    assert!(!s.shown_again());
    assert_eq!(s.dns.calls(), [NAME, NAME]);
}

/// The message signed twice: RSA (name `test`) and Ed25519 (name `ed`).
fn signed_twice() -> Vec<u8> {
    let ed = fixture("ed.eml");
    let end = ed.windows(7).position(|w| w == b"\nFrom: ").unwrap() + 1;
    let mut raw = ed[..end].to_vec();
    raw.extend(fixture("relaxed.eml"));
    raw
}

#[test]
fn one_failed_name_does_not_stop_the_other_from_verifying() {
    // `signed_twice` is the plain-text fixture; checking the verdict is
    // enough here.
    let mut answers = records();
    answers.insert(ED_NAME.to_owned(), TxtAnswer::Unavailable);
    let s = Setup::new(&signed_twice()).start(Fake::new(answers, TxtAnswer::NoRecord));
    let first = s.open(ImagesChoice::Default);
    assert!(!first.header.sender_verified);
    let second = s.opened();
    assert!(second.header.sender_verified);
    assert_eq!(s.dns.calls(), [ED_NAME, NAME]);
}

#[test]
#[ignore = "needs T2: the provider table replaces the registrable-domain rule (#285)"]
fn a_forged_header_on_a_self_hosted_server_with_no_signature_is_not_verified() {
    let raw = b"Authentication-Results: mx.example.org; dkim=pass header.d=example.com\r\n\
From: Bob <bob@example.com>\r\nSubject: s\r\nMIME-Version: 1.0\r\n\
Content-Type: text/html; charset=utf-8\r\n\r\n\
<p>x</p><img src=\"http://images.example.net/a.png\" alt=\"a\">";
    let mut setup = Setup::new(raw);
    setup.host = "imap.example.org";
    let s = setup.start(ok_dns());
    let view = s.open(ImagesChoice::Default);
    assert!(!view.header.sender_verified && !loaded(&view));
    assert!(!s.shown_again());
    assert!(s.dns.calls().is_empty());
}

#[test]
fn an_expired_key_is_looked_up_again_on_the_next_open() {
    let s = Setup::new(&fixture("html.eml")).start(ok_dns());
    s.open(ImagesChoice::Default);
    assert!(s.opened().header.sender_verified);
    // The key lives at most an hour.
    s.h.send(Command::AdvanceClock {
        ms: 2 * 3600 * 1000,
    });
    let first = s.open(ImagesChoice::Default);
    assert!(!first.header.sender_verified, "the cached key had expired");
    assert!(s.opened().header.sender_verified);
    assert_eq!(s.dns.calls(), [NAME, NAME]);
}

#[test]
fn a_message_verified_while_open_stays_verified_when_its_key_expires() {
    let s = Setup::new(&fixture("html.eml")).start(ok_dns());
    s.open(ImagesChoice::Default);
    let verified = s.opened();
    assert!(verified.header.sender_verified && loaded(&verified));
    s.h.send(Command::AdvanceClock {
        ms: 2 * 3600 * 1000,
    });
    // The reader reports the size of the image it loaded; the message is
    // drawn again, and it must not lose its images for a new lookup.
    s.h.send(Command::ImageSizes {
        id: s.id,
        sizes: [(IMAGE_URL.to_owned(), (1000, 10))].into(),
    });
    let again = s.opened();
    assert!(
        again.header.sender_verified && loaded(&again),
        "{}",
        again.html
    );
    assert!(!s.shown_again(), "no flicker, no second verification");
    assert_eq!(s.dns.calls(), [NAME]);
}

#[test]
fn a_partial_body_is_never_looked_up() {
    let raw = fixture("html.eml");
    let mut setup = Setup::new(&raw);
    setup.partial = true;
    let s = setup.start(ok_dns());
    let view = s.open(ImagesChoice::Default);
    assert!(!view.header.sender_verified);
    assert!(!s.shown_again());
    assert!(s.dns.calls().is_empty(), "a partial body can never verify");
}

#[test]
fn an_answer_for_a_message_that_is_no_longer_open_fills_the_cache_and_changes_nothing() {
    let plain = b"From: Bob <bob@example.com>\r\nSubject: s\r\n\r\nhi\r\n".to_vec();
    let raw = fixture("html.eml");
    let mut setup = Setup::new(&raw);
    setup.others = vec![&plain];
    let s = setup.start(ok_dns());
    let (a, b) = (s.ids[0], s.ids[1]);
    s.dns.hold();
    s.open_id(a, ImagesChoice::Default);
    s.open_id(b, ImagesChoice::Default);
    s.dns.release();
    assert!(!s.shown_again(), "the reader no longer shows A");
    // A's key is in the cache: A is verified at once, with no new lookup.
    let again = s.open_id(a, ImagesChoice::Default);
    assert!(again.header.sender_verified && loaded(&again));
    assert_eq!(s.dns.calls(), [NAME]);
}

#[test]
fn one_lookup_task_runs_per_message_however_often_it_is_opened() {
    let raw = fixture("html.eml");
    let mut setup = Setup::new(&raw);
    setup.others = vec![&raw];
    let s = setup.start(ok_dns());
    let (a, b) = (s.ids[0], s.ids[1]);
    s.dns.hold();
    s.open_id(a, ImagesChoice::Default);
    s.open_id(b, ImagesChoice::Default);
    s.open_id(a, ImagesChoice::Default);
    s.dns.release();
    let verified = s.opened_id(a);
    assert!(verified.header.sender_verified);
    assert!(!s.shown_again());
    // One for A, one for B: reopening A did not start a second one.
    assert_eq!(s.dns.calls(), [NAME, NAME]);
}

#[test]
fn an_image_size_report_after_a_failed_lookup_neither_renders_nor_looks_up_again() {
    let s =
        Setup::new(&fixture("html.eml")).start(Fake::new(HashMap::new(), TxtAnswer::Unavailable));
    s.open(ImagesChoice::Default);
    assert!(!s.shown_again());
    s.h.send(Command::ImageSizes {
        id: s.id,
        sizes: [(IMAGE_URL.to_owned(), (1000, 10))].into(),
    });
    assert!(!s.shown_again());
    assert_eq!(s.dns.calls(), [NAME]);
}
