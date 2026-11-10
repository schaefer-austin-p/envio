//! Property tests over random runs (a fixed set of seeds, so failures
//! repeat): unsent mail is never lost or resent, and flags converge.

mod support;

use std::collections::{BTreeSet, HashSet};

use envio_net::NetError;
use envio_net::imap::{Caps, MessageFlags, Role};
use envio_net::smtp::SendError;
use envio_store::{ActionKind, Flags, NewOutboxItem, Outbox};
use envio_sync::UNDO_MS;
use envio_sync::convert::store_flags;
use envio_sync::driver::{file_sent, record, send_outbox, sync_account};
use envio_sync::server::MailSender;
use support::fake::{FakeSender, FakeServer, NOW_MS, eml};
use support::local::{ACCOUNT, Store};

const SEEDS: u64 = 64;
const STEPS: usize = 40;
/// Longer than the longest backoff plus the Undo window.
const SETTLE_MS: i64 = 400_000;

/// xorshift64: small, and enough to shuffle steps.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }

    fn coin(&mut self) -> bool {
        self.below(2) == 0
    }
}

/// Fails the test if a message the server took (or may have taken) is
/// handed over again; keeps the messages the server accepted.
#[derive(Default)]
struct Recording {
    inner: FakeSender,
    taken: HashSet<Vec<u8>>,
    accepted: Vec<String>,
}

impl MailSender for Recording {
    async fn send(&mut self, from: &str, to: &[String], eml: &[u8]) -> Result<(), SendError> {
        assert!(!self.taken.contains(eml), "resent a message");
        let result = self.inner.send(from, to, eml).await;
        match &result {
            Ok(()) => self
                .accepted
                .push(String::from_utf8_lossy(eml).into_owned()),
            Err(error) if !error.unconfirmed => return result,
            Err(_) => {}
        }
        self.taken.insert(eml.to_vec());
        result
    }
}

fn send_result(rng: &mut Rng) -> Result<(), SendError> {
    let (error, unconfirmed) = match rng.below(5) {
        0 => return Ok(()),
        1 => (NetError::Timeout, true),
        2 => (NetError::Busy("421 later".into()), false),
        3 => (NetError::Refused("550 no".into()), false),
        _ => (NetError::SignIn("535 no".into()), false),
    };
    Err(SendError { error, unconfirmed })
}

#[tokio::test]
async fn unsent_mail_is_never_lost_or_resent() {
    for seed in 1..=SEEDS {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut server = FakeServer::new();
        server.add_folder("Sent", Some(Role::Sent));
        let mut store = Store::new();
        sync_account(&mut server, &mut store.local(), ACCOUNT, NOW_MS)
            .await
            .unwrap();
        let mut sender = Recording::default();
        let mut queued = BTreeSet::new();
        let mut now = NOW_MS;
        for step in 0..STEPS {
            match rng.below(7) {
                0 => {
                    let id = format!("m{step}");
                    let item = NewOutboxItem {
                        account: ACCOUNT.into(),
                        message_id: format!("<{id}@example.org>"),
                        queued_at_ms: now,
                        from: "carol@example.org".into(),
                        recipients: vec!["dave@example.org".into()],
                    };
                    store.outbox.enqueue(item, &eml(&id)).unwrap();
                    queued.insert(format!("{id}@example.org"));
                }
                1 => sender.inner.results.push(send_result(&mut rng)),
                2 => {
                    let _ = send_outbox(&mut sender, &mut store.local(), ACCOUNT, now).await;
                }
                3 => {
                    let op = ["select", "search_message_id", "append"][rng.below(3) as usize];
                    server.fail_on(op, NetError::Timeout);
                }
                4 => {
                    let gmail = rng.coin();
                    let _ = file_sent(&mut server, &mut store.local(), ACCOUNT, gmail).await;
                }
                5 => now += SETTLE_MS,
                _ => store.outbox = Outbox::open(&store.dir.path().join("outbox")).unwrap(),
            }
            let in_outbox: BTreeSet<String> = store
                .outbox
                .items()
                .unwrap()
                .into_iter()
                .map(|item| item.meta.message_id.trim_matches(['<', '>']).to_owned())
                .collect();
            let sent: BTreeSet<String> = server.ids("Sent").into_iter().collect();
            for id in &queued {
                // In Sent, or accepted (and Gmail files it): nothing else
                // may take an item out of the Outbox.
                let wanted = format!("<{id}>");
                let accepted = sender.accepted.iter().any(|eml| eml.contains(&wanted));
                let left_safely = sent.contains(id) || accepted;
                assert!(
                    in_outbox.contains(id) || left_safely,
                    "seed {seed}: {id} vanished"
                );
            }
        }
    }
}

fn random_flags(rng: &mut Rng) -> Flags {
    let mut flags = Flags::empty();
    for flag in [Flags::SEEN, Flags::FLAGGED, Flags::ANSWERED] {
        if rng.coin() {
            flags = flags.with(flag);
        }
    }
    flags
}

fn server_flags(flags: Flags) -> MessageFlags {
    MessageFlags {
        seen: flags.contains(Flags::SEEN),
        flagged: flags.contains(Flags::FLAGGED),
        answered: flags.contains(Flags::ANSWERED),
        ..MessageFlags::default()
    }
}

#[tokio::test]
async fn flags_converge_on_the_servers() {
    for seed in 1..=SEEDS {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut server = FakeServer::new();
        if seed % 2 == 0 {
            server.caps = Caps::default();
        }
        for n in 0..8 {
            server.deliver_id("INBOX", &format!("m{n}"));
        }
        let mut store = Store::new();
        sync_account(&mut server, &mut store.local(), ACCOUNT, NOW_MS)
            .await
            .unwrap();
        let inbox = store.folder("INBOX").id;
        let mut now = NOW_MS;
        for _ in 0..STEPS {
            match rng.below(5) {
                0 => {
                    let uid = 1 + rng.below(8) as u32;
                    if let Some(message) = store.db.message_at(inbox, uid).unwrap() {
                        let kind = ActionKind::SetFlags {
                            add: random_flags(&mut rng),
                            remove: random_flags(&mut rng),
                        };
                        record(&mut store.local(), message.id, kind, now).unwrap();
                    }
                }
                1 => {
                    let uid = 1 + rng.below(8) as u32;
                    let flags = server_flags(random_flags(&mut rng));
                    server.set_flags("INBOX", uid, flags);
                }
                2 => {
                    let op = ["select", "store_flags", "fetch_flags", "search_since"]
                        [rng.below(4) as usize];
                    server.fail_on(op, NetError::Timeout);
                }
                3 => {
                    let _ = sync_account(&mut server, &mut store.local(), ACCOUNT, now).await;
                }
                _ => now += 1 + rng.below(UNDO_MS as u64 * 2) as i64,
            }
        }
        server.clear_failures();
        for _ in 0..10 {
            now += SETTLE_MS;
            sync_account(&mut server, &mut store.local(), ACCOUNT, now)
                .await
                .unwrap();
        }
        assert_eq!(store.journal.pending().count(), 0, "seed {seed}");
        let on_server: Vec<(u32, Flags)> = server.folders["INBOX"]
            .messages
            .iter()
            .map(|(uid, message)| (*uid, store_flags(&message.flags)))
            .collect();
        assert_eq!(
            store.db.flags_by_uid(inbox).unwrap(),
            on_server,
            "seed {seed}"
        );
    }
}
