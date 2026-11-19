//! Sender trust (#285): DKIM keys are looked up only for a sender the user
//! allowed, only while the message is open, and a verified message is shown
//! again with its images. The resolver is a fake that counts calls.

mod support;

use std::collections::HashMap;
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
}

impl Fake {
    fn new(answers: HashMap<String, TxtAnswer>, fallback: TxtAnswer) -> Arc<Fake> {
        Arc::new(Fake {
            answers,
            fallback,
            calls: Mutex::new(Vec::new()),
        })
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
        Box::pin(async move { answer })
    }
}

struct Open {
    h: Harness,
    id: MessageRowId,
    dns: Arc<Fake>,
}

struct Setup<'a> {
    raw: &'a [u8],
    host: &'a str,
    allowed: bool,
    images_always: bool,
}

impl<'a> Setup<'a> {
    fn new(raw: &'a [u8]) -> Self {
        Setup {
            raw,
            host: "outlook.office365.com",
            allowed: true,
            images_always: false,
        }
    }

    fn start(self, dns: Arc<Fake>) -> Open {
        let mut h = Harness::unstarted();
        let key = h.seed_account_on(ME, Some("pw"), (self.host, 1));
        let folder = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, self.raw)]);
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
        let id = h.wait(|e| match e {
            Event::Messages { rows, .. } => Some(rows[0].id),
            _ => None,
        });
        Open { h, id, dns }
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
        self.h.send(Command::OpenMessage {
            id: self.id,
            images,
            width_px: 300,
        });
        self.opened()
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
