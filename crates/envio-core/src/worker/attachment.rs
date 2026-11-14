//! Saving an attachment (the reader's Save and Open, a forward's copies). From
//! the cached message, or, for a big message stored without its attachments
//! (`BodyState::Partial`), from the server: the part is fetched alone, decoded
//! and written, and never kept in the cache.

use std::io;
use std::path::{Path, PathBuf};

use envio_mime::Message as Mime;
use envio_store::{BodyState, MessageRowId};

use super::Worker;
use super::plan::PartWant;
use super::read::{safe_file_name, write_new};
use crate::api::event::Event;
use crate::clock::now_ms;
use crate::text;

impl Worker<'_> {
    pub(super) fn save_attachment(&mut self, id: MessageRowId, index: usize, dir: &Path) {
        if let Some(want) = self.stub_want(id, index, dir) {
            return self.queue_part(want);
        }
        match self.write_attachment(id, index, dir) {
            Ok(path) => self.emit(Event::AttachmentSaved { id, index, path }),
            Err(e) => {
                log::warn!("saving an attachment: {e}");
                self.attachment_failed(id, index);
            }
        }
    }

    pub(super) fn write_attachment(
        &self,
        id: MessageRowId,
        index: usize,
        dir: &Path,
    ) -> io::Result<PathBuf> {
        let message = self.store.db.message(id).map_err(io::Error::other)?;
        let raw = self
            .store
            .bodies
            .read(message.folder, message.uid)
            .map_err(io::Error::other)?;
        let bytes = envio_mime::attachment_bytes(&raw, index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such attachment"))?;
        let filename = match envio_mime::parse(&raw) {
            Mime::Parsed { attachments, .. } => attachments
                .into_iter()
                .find(|a| a.index == index)
                .and_then(|a| a.filename),
            Mime::Raw(_) => None,
        };
        let name = safe_file_name(filename.as_deref().unwrap_or(""), index);
        std::fs::create_dir_all(dir)?;
        write_new(dir, &name, &bytes)
    }

    /// The server fetch that saves attachment `index` of `id`, when the
    /// cached message holds only a stub of it.
    fn stub_want(&self, id: MessageRowId, index: usize, dir: &Path) -> Option<PartWant> {
        let message = self.store.db.message(id).ok()?;
        if message.body != BodyState::Partial {
            return None;
        }
        let raw = self.store.bodies.read(message.folder, message.uid).ok()?;
        let Mime::Parsed { attachments, .. } = envio_mime::parse(&raw) else {
            return None;
        };
        let info = attachments.into_iter().find(|a| a.index == index)?;
        Some(PartWant {
            id,
            index,
            section: info.section?,
            size: info.size,
            name: safe_file_name(info.filename.as_deref().unwrap_or(""), index),
            dir: dir.to_path_buf(),
        })
    }

    fn queue_part(&mut self, want: PartWant) {
        let now = now_ms();
        let live = self
            .account_of(want.id)
            .and_then(|account| self.live.get_mut(&account));
        let Some(live) = live.filter(|l| !l.health.is_paused() && l.health.can_run(now)) else {
            return self.attachment_failed(want.id, want.index);
        };
        let queued = live
            .wants
            .parts
            .iter()
            .any(|p| p.id == want.id && p.index == want.index);
        if !queued {
            live.wants.parts.push(want);
        }
    }

    /// The part step ended: write what it fetched. `None` is a part gone
    /// from the server.
    pub(super) fn part_fetched(&mut self, want: &PartWant, raw: Option<&[u8]>) {
        let written = raw
            .and_then(envio_mime::decode_part)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "part gone"))
            .and_then(|bytes| {
                std::fs::create_dir_all(&want.dir)?;
                write_new(&want.dir, &want.name, &bytes)
            });
        match written {
            Ok(path) => self.emit(Event::AttachmentSaved {
                id: want.id,
                index: want.index,
                path,
            }),
            Err(e) => {
                log::warn!("saving a fetched attachment: {e}");
                self.attachment_failed(want.id, want.index);
            }
        }
    }

    /// Steps failed: nobody waiting for an attachment of `account` (any
    /// account for `None`) will get it soon.
    pub(super) fn parts_failed(&mut self, account: Option<&str>) {
        let mut failed = Vec::new();
        for (key, live) in &mut self.live {
            if account.is_none_or(|a| a == key) {
                failed.extend(live.wants.parts.drain(..).map(|p| (p.id, p.index)));
            }
        }
        for (id, index) in failed {
            self.attachment_failed(id, index);
        }
    }

    fn attachment_failed(&self, id: MessageRowId, index: usize) {
        self.emit(Event::AttachmentFailed { id, index });
        self.emit(Event::Notice {
            text: text::read::attachment_not_saved(),
        });
    }
}
