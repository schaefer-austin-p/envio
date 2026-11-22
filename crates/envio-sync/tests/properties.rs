//! Property tests over random runs (a fixed set of seeds, so failures
//! repeat): unsent mail is never lost or resent, and flags converge.

mod support;

use std::collections::{BTreeMap, BTreeSet, HashSet};

use envio_net::NetError;
use envio_net::imap::{Caps, MessageFlags, Role};
use envio_net::smtp::SendError;
use envio_store::{ActionKind, Flags, NewOutboxItem, Outbox, SendState};
use envio_sync::UNDO_MS;
use envio_sync::convert::store_flags;
use envio_sync::driver::{FileOpts, file_sent, record, send_outbox, sync_account};
use envio_sync::error::{ErrorKind, SyncError};
use envio_sync::server::MailSender;
use support::fake::{FakeSender, FakeServer, NOW_MS, NOW_SECS, eml};
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

/// Net failures come back as `Ok` with `stopped` and the like, or as a net
/// `Err` (a failed SELECT, SEARCH or APPEND); only a local one fails the test.
fn tolerate<T>(result: Result<T, SyncError>, seed: u64) {
    if let Err(error) = result {
        assert_ne!(error.kind(), ErrorKind::Local, "seed {seed}: {error}");
    }
}

/// What the server has in Sent, by Message-ID, with how often.
fn sent_counts(server: &FakeServer) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for id in server.ids("Sent") {
        *counts.entry(id).or_insert(0) += 1;
    }
    counts
}

/// A Gmail-like server files what it accepted in Sent itself.
fn mirror_accepted(server: &mut FakeServer, sender: &Recording, mirrored: &mut usize) {
    for eml in &sender.accepted[*mirrored..] {
        server.deliver("Sent", eml.as_bytes(), NOW_SECS);
    }
    *mirrored = sender.accepted.len();
}

fn outbox_ids(store: &Store) -> BTreeSet<String> {
    store
        .outbox
        .items()
        .unwrap()
        .into_iter()
        .map(|item| item.meta.message_id.trim_matches(['<', '>']).to_owned())
        .collect()
}

#[tokio::test]
async fn unsent_mail_is_never_lost_or_resent() {
    for seed in 1..=SEEDS {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut server = FakeServer::new();
        server.add_folder("Sent", Some(Role::Sent));
        // Odd seeds: the server files accepted mail in Sent itself.
        let gmail = seed % 2 == 1;
        let mut store = Store::new();
        sync_account(
            &mut server,
            &mut store.local(),
            ACCOUNT,
            NOW_MS,
            Some(30),
            &[],
        )
        .await
        .unwrap();
        let mut sender = Recording::default();
        let mut mirrored = 0;
        let mut queued = BTreeSet::new();
        let mut now = NOW_MS;
        for step in 0..STEPS {
            match rng.below(8) {
                0 => {
                    let id = format!("m{step}");
                    let item = NewOutboxItem {
                        send_after_ms: 0,
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
                2 => tolerate(
                    send_outbox(&mut sender, &mut store.local(), ACCOUNT, now).await,
                    seed,
                ),
                3 => {
                    let op = ["select", "search_message_id", "append"][rng.below(3) as usize];
                    server.fail_on(op, NetError::Timeout);
                }
                4 => tolerate(
                    file_sent(
                        &mut server,
                        &mut store.local(),
                        ACCOUNT,
                        FileOpts {
                            files_itself: gmail,
                            now_ms: now,
                            retries: false,
                        },
                    )
                    .await,
                    seed,
                ),
                5 => now += SETTLE_MS,
                // A crash between the APPEND and the Outbox removal: the
                // item is back, accepted, though it is already in Sent.
                6 => {
                    let filed: Vec<String> = sent_counts(&server)
                        .into_keys()
                        .filter(|id| !outbox_ids(&store).contains(id))
                        .collect();
                    if !filed.is_empty() {
                        let id = filed[rng.below(filed.len() as u64) as usize].clone();
                        let short = id.trim_end_matches("@example.org");
                        let item = NewOutboxItem {
                            send_after_ms: 0,
                            account: ACCOUNT.into(),
                            message_id: format!("<{id}>"),
                            queued_at_ms: now,
                            from: "carol@example.org".into(),
                            recipients: vec!["dave@example.org".into()],
                        };
                        let item = store.outbox.enqueue(item, &eml(short)).unwrap();
                        store
                            .outbox
                            .set_state(&item.id, SendState::Accepted)
                            .unwrap();
                    }
                }
                _ => store.outbox = Outbox::open(&store.dir.path().join("outbox")).unwrap(),
            }
            if gmail {
                mirror_accepted(&mut server, &sender, &mut mirrored);
            }
            check(&server, &store, &queued, seed);
        }
        // Settle: no failures left, every backoff outlasted.
        server.clear_failures();
        sender.inner.results.clear();
        for _ in 0..10 {
            now += SETTLE_MS;
            tolerate(
                send_outbox(&mut sender, &mut store.local(), ACCOUNT, now).await,
                seed,
            );
            if gmail {
                mirror_accepted(&mut server, &sender, &mut mirrored);
            }
            tolerate(
                file_sent(
                    &mut server,
                    &mut store.local(),
                    ACCOUNT,
                    FileOpts {
                        files_itself: gmail,
                        now_ms: now,
                        retries: false,
                    },
                )
                .await,
                seed,
            );
            let synced = sync_account(&mut server, &mut store.local(), ACCOUNT, now, Some(30), &[])
                .await
                .unwrap();
            assert!(
                synced.stopped.is_none() && synced.refused.is_empty(),
                "seed {seed}: settle sync did not run"
            );
            check(&server, &store, &queued, seed);
        }
        // Nothing is stuck: what is left is for the user to decide.
        for item in store.outbox.items().unwrap() {
            assert!(
                matches!(
                    item.meta.state,
                    SendState::MayHaveBeenSent | SendState::Failed { .. }
                ),
                "seed {seed}: {} stuck as {:?}",
                item.meta.message_id,
                item.meta.state
            );
        }
        let mut handed_over = HashSet::new();
        for eml in &sender.accepted {
            assert!(handed_over.insert(eml), "seed {seed}: accepted twice");
        }
    }
}

/// Every queued item is in the Outbox or, once out, in Sent exactly once;
/// nothing is in Sent twice.
fn check(server: &FakeServer, store: &Store, queued: &BTreeSet<String>, seed: u64) {
    let in_outbox = outbox_ids(store);
    let sent = sent_counts(server);
    for (id, count) in &sent {
        assert!(*count <= 1, "seed {seed}: {id} is in Sent {count} times");
    }
    for id in queued {
        assert!(
            in_outbox.contains(id) || sent.get(id) == Some(&1),
            "seed {seed}: {id} vanished"
        );
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
        sync_account(
            &mut server,
            &mut store.local(),
            ACCOUNT,
            NOW_MS,
            Some(30),
            &[],
        )
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
                3 => tolerate(
                    sync_account(&mut server, &mut store.local(), ACCOUNT, now, Some(30), &[])
                        .await,
                    seed,
                ),
                _ => now += 1 + rng.below(UNDO_MS as u64 * 2) as i64,
            }
        }
        server.clear_failures();
        for _ in 0..10 {
            now += SETTLE_MS;
            let synced = sync_account(&mut server, &mut store.local(), ACCOUNT, now, Some(30), &[])
                .await
                .unwrap();
            assert!(
                synced.stopped.is_none() && synced.refused.is_empty(),
                "seed {seed}: settle sync did not run"
            );
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
