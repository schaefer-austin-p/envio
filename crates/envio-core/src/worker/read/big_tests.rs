//! The whole message of a big one (a forward's) makes the reader ask again
//! for its partial body, except where bodies always come whole (Graph).

use envio_store::{Envelope, Flags, MessageRowId, NewMessage};

use super::*;
use crate::worker::testing::{Sink, into_graph, open, seed};

const MIB: u32 = 1024 * 1024;
const RAW: &[u8] = b"From: a@example.org\r\nSubject: s\r\n\r\nbody\r\n";

/// A message of `size` waits for its body; the whole message arrives.
/// True when the reader is asked to open it again (for the partial body).
fn asks_again(graph: bool, size: u32) -> bool {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    if graph {
        into_graph(&mut worker, &seeded.account);
    }
    let new = NewMessage {
        uid: 9,
        size,
        envelope: Envelope::default(),
        flags: Flags::default(),
        preview: String::new(),
    };
    let id: MessageRowId = worker.store.db.upsert_message(seeded.folder, &new).unwrap();
    worker.opening.insert(id, (ImagesChoice::Default, 300));
    worker.body_fetched(id, Some(RAW), true);
    worker.open_again.contains(&id)
}

#[test]
fn an_imap_message_over_10_mib_is_opened_again_for_its_partial_body() {
    assert!(asks_again(false, 11 * MIB));
}

#[test]
fn a_small_imap_message_is_shown_from_its_whole_body() {
    assert!(!asks_again(false, 100 * 1024));
}

#[test]
fn a_graph_message_over_10_mib_is_shown_from_its_whole_body() {
    assert!(!asks_again(true, 11 * MIB));
}
