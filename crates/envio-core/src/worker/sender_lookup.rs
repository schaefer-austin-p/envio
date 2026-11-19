//! The driver of DKIM key lookups for the open message (sender trust, #285).
//! The policy is `sender_check::check`; this runs what it asks for on the
//! worker's runtime and takes the answers back. The worker's turn never
//! waits on DNS: the first render is immediate and the verified one follows.

use envio_net::dns::TxtAnswer;
use envio_store::MessageRowId;

use super::{Input, Worker};
use crate::clock::now_ms;

impl Worker<'_> {
    /// Starts the lookup of `names` for message `id` when it is the open
    /// one and no lookup for it runs (opening A, B, A again leaves A's first
    /// task running: its answer serves the new open). `names` are already filtered by
    /// `check` (policy, at most 3, none of the failed ones).
    pub(super) fn request_keys(&mut self, id: MessageRowId, names: Vec<String>) {
        if names.is_empty()
            || self.store.open_message != Some(id)
            || self.store.dkim_in_flight.contains(&id)
        {
            return;
        }
        // No runtime means no worker loop (a unit test): nothing to run on.
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        self.store.dkim_in_flight.insert(id);
        let dns = std::sync::Arc::clone(&self.dns);
        let inputs = self.inputs.clone();
        runtime.spawn(async move {
            let lookups = names.into_iter().map(|name| {
                let dns = std::sync::Arc::clone(&dns);
                async move {
                    let answer = dns.txt(&name).await;
                    (name, answer)
                }
            });
            let answers = futures::future::join_all(lookups).await;
            // The worker is gone: nobody waits for the answer.
            let _ = inputs.send(Input::DkimKeys { id, answers });
        });
    }

    /// The answers of a lookup task. Records and "no such record" go into
    /// the cache whether or not `id` is still open; a name that came back
    /// `Unavailable` is remembered for the open message and not asked for
    /// again until it is opened again. The message is shown again only if a
    /// key was stored: an all-`Unavailable` reply changes nothing.
    pub(super) fn dkim_keys(&mut self, id: MessageRowId, answers: Vec<(String, TxtAnswer)>) {
        // Before anything else, so no branch below can leave it set.
        self.store.dkim_in_flight.remove(&id);
        let now = now_ms();
        let mut stored = false;
        for (name, answer) in answers {
            if answer == TxtAnswer::Unavailable {
                if self.store.open_message == Some(id)
                    && let Some(shown) = self.store.shown.as_mut().filter(|s| s.id == id)
                {
                    shown.failed.insert(name);
                }
            } else {
                self.store.keys.put(name, answer, now);
                stored = true;
            }
        }
        if stored && self.store.open_message == Some(id) {
            self.show_again(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::command::ImagesChoice;
    use crate::api::event::Event;
    use crate::worker::testing::{Sink, open, seed};
    use envio_store::BodyState;

    const NAME: &str = "test._domainkey.example.com";

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../envio-mime/tests/fixtures/dkim/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn key_answer() -> TxtAnswer {
        let keys = String::from_utf8(fixture("keys.txt")).unwrap();
        let record = keys
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{NAME}\t")))
            .unwrap();
        TxtAnswer::Records {
            texts: vec![record.to_owned()],
            ttl: 3600,
        }
    }

    /// A worker with the signed HTML message cached and bob allowed.
    fn worker_with_signed<'s>(dir: &std::path::Path, sink: &'s Sink) -> (Worker<'s>, MessageRowId) {
        let (config, seeded) = seed(dir);
        let mut worker = open(&config, sink);
        let message = worker.store.db.message(seeded.message).unwrap();
        worker
            .store
            .bodies
            .write(message.folder, message.uid, &fixture("html.eml"))
            .unwrap();
        worker
            .store
            .db
            .set_body_state(seeded.message, BodyState::Downloaded)
            .unwrap();
        worker
            .store
            .settings
            .images_from
            .insert("bob@example.com".into());
        (worker, seeded.message)
    }

    fn opened(sink: &Sink) -> usize {
        sink.events()
            .iter()
            .filter(|e| matches!(e, Event::MessageOpened { .. }))
            .count()
    }

    fn lookups_of(worker: &Worker<'_>, id: MessageRowId) -> Vec<String> {
        let message = worker.store.db.message(id).unwrap();
        let raw = worker
            .store
            .bodies
            .read(message.folder, message.uid)
            .unwrap();
        worker.view(&message, &raw, (ImagesChoice::Default, 300)).2
    }

    #[test]
    fn failed_names_survive_every_render_of_the_same_message_until_it_is_opened_again() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, id) = worker_with_signed(dir.path(), &sink);
        worker.open_message(id, ImagesChoice::Default, 300);
        assert_eq!(lookups_of(&worker, id), [NAME]);
        assert_eq!(opened(&sink), 1);

        // The lookup came back Unavailable: nothing is shown again.
        worker.store.dkim_in_flight.insert(id);
        worker.dkim_keys(id, vec![(NAME.into(), TxtAnswer::Unavailable)]);
        assert!(worker.store.dkim_in_flight.is_empty());
        assert_eq!(opened(&sink), 1, "an all-Unavailable reply renders nothing");
        assert!(worker.failed_for(id).contains(NAME));
        assert!(lookups_of(&worker, id).is_empty());

        // Each show() builds a new Shown; the failed name is carried over.
        for _ in 0..3 {
            let message = worker.store.db.message(id).unwrap();
            let view = worker.view(&message, &fixture("html.eml"), (ImagesChoice::Default, 300));
            assert!(view.2.is_empty());
            worker.show(id, (ImagesChoice::Default, 300), view);
            assert!(worker.failed_for(id).contains(NAME));
        }

        // Opening this one again is the retry.
        worker.open_message(id, ImagesChoice::Default, 300);
        assert!(worker.failed_for(id).is_empty());
        assert_eq!(lookups_of(&worker, id), [NAME]);
    }

    #[test]
    fn a_key_that_arrives_shows_the_open_message_again_verified() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, id) = worker_with_signed(dir.path(), &sink);
        worker.open_message(id, ImagesChoice::Default, 300);
        worker.dkim_keys(id, vec![(NAME.into(), key_answer())]);
        let views: Vec<_> = sink
            .events()
            .into_iter()
            .filter_map(|e| match e {
                Event::MessageOpened { view, .. } => Some(view),
                _ => None,
            })
            .collect();
        assert_eq!(views.len(), 2);
        assert!(!views[0].header.sender_verified);
        assert!(views[1].header.sender_verified);
    }
}
