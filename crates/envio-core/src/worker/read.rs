//! Reading from the cache: lists, search, opening a message, attachments.
//! Nothing here writes the journal; the only network use is the body step
//! an `OpenMessage` asks the account's runtime for.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};

use envio_mime::{Message as Mime, RenderOptions};
use envio_store::{BodyState, FolderId, Message, MessageRowId, Page};

use super::Worker;
use crate::api::event::Event;
use crate::api::view::{AttachmentRow, FolderRow, MessageRow, MessageView};
use crate::clock::now_ms;
use crate::text;

pub(crate) const SEARCH_LIMIT: usize = 200;

/// Longest file name written, in bytes (file systems stop at 255).
const MAX_NAME_BYTES: usize = 200;

/// Renders raw mail for the reader. Sanitising stays in `envio_mime`;
/// a message that is not valid mail comes back as escaped text.
pub(crate) fn render_view(raw: &[u8], images: bool, width_px: u32) -> MessageView {
    let message = envio_mime::parse(raw);
    let options = RenderOptions {
        allow_remote_images: images,
        cid_urls: HashMap::new(),
        blocked_label: text::read::blocked_images(),
        max_image_width: width_px,
    };
    let rendered = envio_mime::render(&message, &options);
    let (raw, attachments) = match &message {
        Mime::Raw(_) => (true, Vec::new()),
        Mime::Parsed { attachments, .. } => (
            false,
            attachments
                .iter()
                .map(|a| AttachmentRow {
                    index: a.index,
                    name: safe_file_name(a.filename.as_deref().unwrap_or(""), a.index),
                    content_type: a.content_type.clone(),
                    size: a.size,
                })
                .collect(),
        ),
    };
    MessageView {
        html: rendered.html,
        raw,
        blocked_images: rendered.blocked_images,
        attachments,
    }
}

/// A name from a sender, made fit to be a file in one directory: its last
/// path segment, no control characters, no leading dots, never empty.
pub(crate) fn safe_file_name(name: &str, index: usize) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last.chars().filter(|c| !c.is_control()).collect();
    let mut out = String::new();
    for c in cleaned.trim().trim_start_matches('.').trim().chars() {
        if out.len() + c.len_utf8() > MAX_NAME_BYTES {
            break;
        }
        out.push(c);
    }
    if out.is_empty() {
        format!("attachment-{}", index + 1)
    } else {
        out
    }
}

/// The name for the `n`th try: `a.txt`, `a (2).txt`, `a (3).txt`.
fn numbered(name: &str, n: u32) -> String {
    if n <= 1 {
        return name.to_owned();
    }
    match name.rfind('.').filter(|dot| *dot > 0) {
        Some(dot) => format!("{} ({n}){}", &name[..dot], &name[dot..]),
        None => format!("{name} ({n})"),
    }
}

/// Writes `bytes` to a new file in `dir`, never over an existing one.
fn write_new(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    use io::Write;
    for n in 1..=1000 {
        let path = dir.join(numbered(name, n));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(bytes)?;
                return Ok(path);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "too many files with that name",
    ))
}

fn folder_name(path: &str) -> String {
    match path.rsplit(['/', '.']).next() {
        Some(last) if !last.is_empty() => last.to_owned(),
        _ => path.to_owned(),
    }
}

fn row(message: Message) -> MessageRow {
    MessageRow {
        id: message.id,
        folder: message.folder,
        from: message.envelope.from,
        subject: message.envelope.subject,
        preview: message.preview,
        date: (message.envelope.date != 0).then_some(message.envelope.date),
        flags: message.flags,
    }
}

impl Worker<'_> {
    fn read_failed(&self, error: &envio_store::StoreError) {
        log::warn!("reading the cache: {error}");
        self.emit(Event::Notice {
            text: text::read::could_not_read(),
        });
    }

    pub(super) fn list_folders(&self, account: &str) {
        match self.store.db.folders(account) {
            Ok(folders) => {
                let folders = folders
                    .into_iter()
                    .map(|f| FolderRow {
                        name: folder_name(&f.path),
                        id: f.id,
                        path: f.path,
                        role: f.role,
                        unread: 0,
                    })
                    .collect();
                self.emit(Event::Folders {
                    account: account.to_owned(),
                    folders,
                });
            }
            Err(e) => self.read_failed(&e),
        }
    }

    pub(super) fn list_messages(&self, folder: FolderId, page: Page) {
        match self.store.db.messages(folder, page) {
            Ok(messages) => self.emit(Event::Messages {
                folder,
                page,
                rows: messages.into_iter().map(row).collect(),
            }),
            Err(e) => self.read_failed(&e),
        }
    }

    pub(super) fn search(&self, account: &str, query: String) {
        let limit = u32::try_from(SEARCH_LIMIT).unwrap_or(u32::MAX);
        match self.store.db.search(account, &query, limit) {
            Ok(messages) => self.emit(Event::SearchResults {
                account: account.to_owned(),
                query,
                rows: messages.into_iter().map(row).collect(),
            }),
            Err(e) => self.read_failed(&e),
        }
    }

    fn unavailable(&self, id: MessageRowId) {
        self.emit(Event::MessageUnavailable {
            id,
            text: text::read::message_unavailable(),
        });
    }

    /// The account a cached message belongs to.
    fn account_of(&self, id: MessageRowId) -> Option<String> {
        let message = self.store.db.message(id).ok()?;
        Some(self.store.db.folder(message.folder).ok()?.account)
    }

    pub(super) fn open_message(&mut self, id: MessageRowId, images: bool, width_px: u32) {
        let Ok(message) = self.store.db.message(id) else {
            return self.unavailable(id);
        };
        if message.body == BodyState::Downloaded {
            match self.store.bodies.read(message.folder, message.uid) {
                Ok(raw) => {
                    let view = render_view(&raw, images, width_px);
                    return self.emit(Event::MessageOpened { id, view });
                }
                // The file is gone: fetch it again.
                Err(e) => log::warn!("reading a cached body: {e}"),
            }
        }
        let now = now_ms();
        let full = self.disk_full(now);
        let Some(account) = self.account_of(id) else {
            return self.unavailable(id);
        };
        let Some(live) = self.live.get_mut(&account) else {
            return self.unavailable(id);
        };
        if full || live.health.is_paused() || !live.health.can_run(now) {
            return self.unavailable(id);
        }
        if !live.wants.bodies.contains(&id) {
            live.wants.bodies.push(id);
        }
        self.opening.insert(id, (images, width_px));
    }

    /// A body step ended: show the message that waited for it. `None` is a
    /// body gone from the server.
    pub(super) fn body_fetched(&mut self, id: MessageRowId, raw: Option<&[u8]>) {
        let Some((images, width_px)) = self.opening.remove(&id) else {
            return;
        };
        match raw {
            Some(raw) => {
                let view = render_view(raw, images, width_px);
                self.emit(Event::MessageOpened { id, view });
            }
            None => self.unavailable(id),
        }
    }

    /// A step failed: nobody waiting for a body of `account` (any account
    /// for `None`) will get it soon.
    pub(super) fn open_failed(&mut self, account: Option<&str>) {
        let waiting: Vec<MessageRowId> = self
            .opening
            .keys()
            .copied()
            .filter(|id| account.is_none_or(|a| self.account_of(*id).is_none_or(|b| a == b)))
            .collect();
        for id in waiting {
            self.opening.remove(&id);
            for live in self.live.values_mut() {
                live.wants.bodies.retain(|b| *b != id);
            }
            self.unavailable(id);
        }
    }

    pub(super) fn save_attachment(&self, id: MessageRowId, index: usize, dir: &Path) {
        match self.write_attachment(id, index, dir) {
            Ok(path) => self.emit(Event::AttachmentSaved { id, index, path }),
            Err(e) => {
                log::warn!("saving an attachment: {e}");
                self.emit(Event::Notice {
                    text: text::read::attachment_not_saved(),
                });
            }
        }
    }

    fn write_attachment(&self, id: MessageRowId, index: usize, dir: &Path) -> io::Result<PathBuf> {
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
        write_new(dir, &name, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_file_name_strips_paths_and_controls() {
        assert_eq!(safe_file_name("../../etc/passwd", 0), "passwd");
        assert_eq!(safe_file_name("C:\\dir\\a.txt", 0), "a.txt");
        assert_eq!(safe_file_name("a\u{0}b\n.txt", 0), "ab.txt");
        assert_eq!(safe_file_name("..", 0), "attachment-1");
        assert_eq!(safe_file_name(".hidden", 0), "hidden");
        assert_eq!(safe_file_name("", 2), "attachment-3");
        assert_eq!(safe_file_name("dir/", 1), "attachment-2");
        assert!(safe_file_name(&"x".repeat(1000), 0).len() <= MAX_NAME_BYTES);
    }

    #[test]
    fn a_taken_name_gets_a_number_before_the_extension() {
        assert_eq!(numbered("a.txt", 1), "a.txt");
        assert_eq!(numbered("a.txt", 2), "a (2).txt");
        assert_eq!(numbered("archive.tar.gz", 3), "archive.tar (3).gz");
        assert_eq!(numbered("noext", 2), "noext (2)");
    }
}
