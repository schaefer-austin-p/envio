//! Edit links a server draft to its local copy only after the server,
//! asked now, says the original has no keywords (#324 T4, Decision 7). The
//! cache is only a pre-filter: unknown or non-empty keywords detach with no
//! network; a cache that says "none" is confirmed by one FLAGS fetch before
//! the draft may replace the original. Every failure detaches.

use envio_store::{Flags, Folder, Message, MessageRowId};

use super::Worker;
use crate::api::command::ComposeId;
use crate::clock::now_ms;
use crate::drafts::import::{Source, draft_from_raw};
use crate::text;

/// The account, the original's Message-ID and whether its system flags are
/// \Seen and \Draft at most.
type Original<'a> = (&'a str, Option<&'a str>, bool);

impl Worker<'_> {
    /// The original's bytes are at hand and no local file matches. `live`
    /// is the keyword check's answer when it was made (`Some(None)`: the
    /// server could not say).
    pub(super) fn import_or_check(
        &mut self,
        compose: ComposeId,
        (message, folder): (&Message, &Folder),
        (raw, live): (&[u8], Option<Option<&[String]>>),
    ) {
        let id = message.id;
        let message_id = message.envelope.message_id.as_deref();
        let plain_flags = message.flags.without(Flags::SEEN | Flags::DRAFT) == Flags::empty();
        let original = (&folder.account[..], message_id, plain_flags);
        if let Some(fresh) = live {
            return self.import_draft(compose, original, (fresh, raw));
        }
        let cached = match self.store.db.keywords_of(folder.id, message.uid) {
            Ok(cached) => cached,
            Err(e) => {
                log::warn!("reading the cached keywords: {e}");
                None
            }
        };
        // No network for an Edit that detaches anyway.
        let may_link =
            plain_flags && matches!(cached.as_deref(), Some([])) && self.would_link(original, raw);
        if !may_link || !self.ask_server(id) {
            return self.import_draft(compose, original, (cached.as_deref(), raw));
        }
        self.awaiting_keywords
            .entry(id)
            .or_default()
            .push((compose, raw.to_vec()));
    }

    /// Whether the draft would be linked if the original had no keywords.
    fn would_link(&self, (account, message_id, plain_flags): Original<'_>, raw: &[u8]) -> bool {
        let Some(settings) = self.store.accounts.get(account).map(|a| &a.settings) else {
            return false;
        };
        let source = Source {
            account,
            identity: &settings.email,
            name: &settings.name,
            message_id,
            plain_flags,
            keywords: Some(&[]),
            safe_name: super::read::safe_file_name,
        };
        draft_from_raw(raw, &source, 0).is_some_and(|imported| imported.link.is_some())
    }

    /// Queues the keyword check of `id`; `false` when the account cannot
    /// run it now (the Edit then detaches).
    fn ask_server(&mut self, id: MessageRowId) -> bool {
        let now = now_ms();
        let full = self.disk_full(now);
        let live = self
            .account_of(id)
            .and_then(|account| self.live.get_mut(&account));
        let Some(live) = live else {
            return false;
        };
        if full || live.health.is_paused() || !live.health.can_run(now) {
            return false;
        }
        if !live.wants.keywords.contains(&id) {
            live.wants.keywords.push(id);
        }
        true
    }

    /// The keyword check ended: the parked Edits go on with what the server
    /// reported (`None`: it could not say, so they detach).
    pub(super) fn keywords_checked(&mut self, id: MessageRowId, fresh: Option<Vec<String>>) {
        for live in self.live.values_mut() {
            live.wants.keywords.retain(|k| *k != id);
        }
        for (compose, raw) in self.awaiting_keywords.remove(&id).unwrap_or_default() {
            self.edit_checked(compose, id, (Some(&raw), Some(fresh.as_deref())));
        }
    }

    /// A step failed or could not run: nobody waiting for a check of
    /// `account` (any account for `None`) will get it soon. Their Edits
    /// detach; a detached copy is always safe.
    pub(super) fn keywords_failed(&mut self, account: Option<&str>) {
        let waiting: Vec<MessageRowId> = self
            .awaiting_keywords
            .keys()
            .copied()
            .filter(|id| account.is_none_or(|a| self.account_of(*id).is_none_or(|b| a == b)))
            .collect();
        for id in waiting {
            for live in self.live.values_mut() {
                live.wants.keywords.retain(|k| *k != id);
            }
            for (compose, raw) in self.awaiting_keywords.remove(&id).unwrap_or_default() {
                self.edit_checked(compose, id, (Some(&raw), Some(None)));
            }
        }
    }

    /// The window closed: its Edit no longer waits for the check.
    pub(super) fn forget_keyword_wait(&mut self, compose: ComposeId) {
        self.awaiting_keywords.retain(|_, waiting| {
            waiting.retain(|(c, _)| *c != compose);
            !waiting.is_empty()
        });
        let waiting = &self.awaiting_keywords;
        for live in self.live.values_mut() {
            live.wants.keywords.retain(|id| waiting.contains_key(id));
        }
    }

    /// Makes a local draft of the server draft's bytes and opens it. The
    /// store does it all or nothing and links the copy to the original
    /// only after its files are in (see `Drafts::import`); a failure
    /// refuses, with the original untouched. `keywords` is what the server
    /// just reported: only an empty list allows the link.
    fn import_draft(
        &mut self,
        compose: ComposeId,
        (account, message_id, plain_flags): Original<'_>,
        (keywords, raw): (Option<&[String]>, &[u8]),
    ) {
        let Some((identity, name)) = self
            .store
            .accounts
            .get(account)
            .map(|a| (a.settings.email.clone(), a.settings.name.clone()))
        else {
            return self.compose_refused(compose, text::compose::no_account());
        };
        let source = Source {
            account,
            identity: &identity,
            name: &name,
            message_id,
            plain_flags,
            keywords,
            safe_name: super::read::safe_file_name,
        };
        let Some(imported) = draft_from_raw(raw, &source, now_ms()) else {
            return self.compose_refused(compose, text::read::message_unavailable());
        };
        let made = self.store.drafts.import(
            &imported.draft,
            imported.link.as_deref(),
            |drafts, id, draft| {
                for a in &imported.attachments {
                    let name =
                        super::read::safe_file_name(a.filename.as_deref().unwrap_or(""), a.index);
                    draft
                        .attachments
                        .push(drafts.keep_bytes(id, &name, &a.bytes)?);
                }
                Ok(())
            },
        );
        let (id, linked) = match made {
            Ok(made) => made,
            Err(e) => {
                log::warn!("importing a draft: {e}");
                return self.compose_refused(compose, text::compose::draft_unavailable());
            }
        };
        let note = if linked {
            String::new()
        } else {
            text::compose::editing_a_copy()
        };
        self.start_from_draft(compose, Some(&id), note);
    }
}
