//! Reading from the cache: lists, search, opening a message, attachments.
//! Nothing here writes the journal; the only network use is the body step
//! an `OpenMessage` asks the account's runtime for.

use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};

use envio_mime::{Address, Message as Mime, RenderOptions};
use envio_store::{BodyState, Cursor, ListRef, Message, MessageRowId, PAGE};

use super::Worker;
use super::folder_order::drawer_order;
use super::rows::{rows, sender_label};
use super::userdata::{is_known, loads_images};
use crate::api::command::ImagesChoice;
use crate::api::event::Event;
use crate::api::view::{AttachmentRow, FolderRow, MessageHeader, MessageView};
use crate::clock::now_ms;
use crate::text;

/// Longest file name written, in bytes (file systems stop at 255).
const MAX_NAME_BYTES: usize = 200;

/// What `render_view` needs to know about the user and the message's place.
pub(crate) struct ViewContext<'a> {
    pub account: &'a str,
    pub settings: &'a envio_store::Settings,
    pub contacts: &'a [envio_store::Contact],
    /// The cached envelope, for mail that can not be parsed.
    pub stored: &'a envio_store::Envelope,
    /// Where this message's inline images are written; `None` shows them as
    /// missing.
    pub inline_dir: Option<&'a Path>,
    /// Natural sizes the reader reported for its remote images.
    pub image_sizes: &'a super::shown::Sizes,
}

fn names(list: &[Address]) -> Vec<String> {
    list.iter().map(Address::display).collect()
}

/// The header lines of a message that could not be parsed come from the
/// cached envelope (`Name <address>` strings).
pub(super) fn stored_header(stored: &envio_store::Envelope, account: &str) -> MessageHeader {
    let from = Address::parse(&stored.from);
    MessageHeader {
        from: from.as_ref().map_or_else(
            || {
                envio_mime::strip_bidi_controls(&stored.from)
                    .trim()
                    .to_owned()
            },
            |a| sender_label(a.name.as_deref(), &a.email),
        ),
        from_address: from.map(|a| a.email).unwrap_or_default(),
        to: Vec::new(),
        cc: Vec::new(),
        subject: envio_mime::strip_bidi_controls(&stored.subject),
        date: (stored.date != 0).then_some(stored.date),
        account: account.to_owned(),
        sender_known: false,
    }
}

/// Renders raw mail for the reader. Sanitising stays in `envio_mime`;
/// a message that is not valid mail comes back as escaped text. Remote
/// images load only as `images` and the sender allow.
pub(crate) fn render_view(
    raw: &[u8],
    images: ImagesChoice,
    width_px: u32,
    ctx: &ViewContext<'_>,
) -> (MessageView, Vec<envio_mime::Unsized>) {
    let message = envio_mime::parse(raw);
    let mut header = stored_header(ctx.stored, ctx.account);
    if let Mime::Parsed { envelope, .. } = &message {
        let sender = envelope.from.first();
        header = MessageHeader {
            from: sender.map_or_else(String::new, |a| sender_label(a.name.as_deref(), &a.email)),
            from_address: sender.map_or_else(String::new, |a| a.email.clone()),
            to: names(&envelope.to),
            cc: names(&envelope.cc),
            subject: envelope.subject.clone(),
            date: envelope.date,
            ..header
        };
    }
    header.sender_known = is_known(ctx.settings, ctx.contacts, &header.from_address);
    let load = loads_images(ctx.settings, images, &header.from_address);
    let inlined = match (&message, ctx.inline_dir) {
        (Mime::Parsed { attachments, .. }, Some(dir)) => {
            super::inline::write(dir, raw, attachments)
        }
        (Mime::Raw(_), Some(dir)) => super::inline::write(dir, raw, &[]),
        _ => super::inline::Inlined::default(),
    };
    let options = RenderOptions {
        allow_remote_images: load,
        cid_urls: inlined.urls,
        cid_sizes: inlined.sizes,
        image_sizes: ctx.image_sizes.clone(),
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
    let view = MessageView {
        header,
        html: rendered.html,
        raw,
        blocked_images: rendered.blocked_images,
        remote_images: rendered.remote_images,
        own_colours: rendered.own_colours,
        attachments,
    };
    (view, rendered.unsized_images)
}

/// Unicode format characters (category Cf): invisible, and some reverse
/// the text direction, which can disguise an extension.
fn is_format(c: char) -> bool {
    matches!(u32::from(c),
        0xAD | 0x600..=0x605 | 0x61C | 0x6DD | 0x70F | 0x180E
        | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064
        | 0x2066..=0x206F | 0xFEFF | 0xFFF9..=0xFFFB
        | 0xE0001 | 0xE0020..=0xE007F)
}

/// A name from a sender, made fit to be a file in one directory: its last
/// path segment, no control characters, no leading dots, never empty.
pub(crate) fn safe_file_name(name: &str, index: usize) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last
        .chars()
        .filter(|c| !c.is_control() && !is_format(*c))
        .collect();
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
                if let Err(e) = file.write_all(bytes) {
                    drop(file);
                    if let Err(e) = std::fs::remove_file(&path) {
                        log::warn!("removing a partial file: {e}");
                    }
                    return Err(e);
                }
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

/// Messages above this many bytes (RFC822.SIZE) are not fetched or parsed
/// until the user asks: the whole body is held in memory, and the HTML is
/// converted on top of it, against a budget of a few hundred MiB.
const OPEN_CAP_BYTES: u32 = 20 * 1024 * 1024;

fn over_cap(size: u32) -> bool {
    size > OPEN_CAP_BYTES
}

/// `size` in whole megabytes, rounded to the nearest.
fn megabytes(size: u32) -> u32 {
    size.saturating_add(500_000) / 1_000_000
}

impl Worker<'_> {
    pub(super) fn read_failed(&self, error: &envio_store::StoreError) {
        log::warn!("reading the cache: {error}");
        self.emit(Event::Notice {
            text: text::read::could_not_read(),
        });
    }

    pub(super) fn list_folders(&self, account: &str) {
        match self.store.db.folders(account) {
            Ok(folders) => {
                let folders = drawer_order(folders);
                let unread = self.store.db.unread_counts(account).unwrap_or_else(|e| {
                    log::warn!("counting unread messages: {e}");
                    Default::default()
                });
                let folders = folders
                    .into_iter()
                    .map(|f| FolderRow {
                        name: folder_name(&f.path),
                        unread: unread.get(&f.id).copied().unwrap_or(0),
                        id: f.id,
                        path: f.path,
                        role: f.role,
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

    pub(super) fn list_messages(&self, list: ListRef, after: Option<Cursor>) {
        match self.store.db.list(list, after, PAGE) {
            Ok(messages) => {
                let end = messages.len() < PAGE as usize;
                self.emit(Event::Messages {
                    list,
                    after,
                    rows: rows(&self.store.db, messages),
                    end,
                });
            }
            Err(e) => {
                self.read_failed(&e);
                self.emit(Event::MessagesFailed { list, after });
            }
        }
    }

    fn unavailable(&self, id: MessageRowId) {
        self.emit(Event::MessageUnavailable {
            id,
            text: text::read::message_unavailable(),
        });
    }

    /// The account a cached message belongs to.
    pub(super) fn account_of(&self, id: MessageRowId) -> Option<String> {
        let message = self.store.db.message(id).ok()?;
        Some(self.store.db.folder(message.folder).ok()?.account)
    }

    /// The reader's view of a cached message's raw bytes.
    pub(super) fn view(
        &self,
        message: &Message,
        raw: &[u8],
        (images, width_px): (ImagesChoice, u32),
    ) -> (MessageView, Vec<envio_mime::Unsized>) {
        let account = self.account_of(message.id).unwrap_or_default();
        // A late view (its body arrived after the reader moved on) must not
        // touch the folder of the message the reader shows now.
        let inline_dir = (self.store.open_message == Some(message.id))
            .then(|| self.store.paths.inline().join(message.id.0.to_string()));
        let image_sizes = self.sizes_for(message.id);
        let ctx = ViewContext {
            account: &account,
            settings: &self.store.settings,
            contacts: &self.store.contacts,
            stored: &message.envelope,
            inline_dir: inline_dir.as_deref(),
            image_sizes: &image_sizes,
        };
        render_view(raw, images, width_px, &ctx)
    }

    /// A body cached before the list had the attachment mark (migration
    /// 0004 starts every row at 0) is never fetched again, so opening it from
    /// the cache sets the exact value and tells the list, as a body download
    /// does.
    fn correct_attachment_mark(&self, message: &Message, raw: &[u8]) {
        // A body that does not parse says nothing: keep the stored guess.
        let Some(exact) = envio_mime::envelope(raw).map(|e| e.has_attachments) else {
            return;
        };
        if exact == message.envelope.has_attachments {
            return;
        }
        match self.store.db.set_has_attachments(message.id, exact) {
            Ok(()) => self.emit(Event::MessagesChanged {
                folders: vec![message.folder],
            }),
            Err(e) => log::warn!("storing the attachment mark: {e}"),
        }
    }

    pub(super) fn open_message(&mut self, id: MessageRowId, images: ImagesChoice, width_px: u32) {
        self.open_message_with(id, (images, width_px), false);
    }

    /// `anyway` lifts the size cap: the user asked for the big message.
    pub(super) fn open_message_with(
        &mut self,
        id: MessageRowId,
        (images, width_px): (ImagesChoice, u32),
        anyway: bool,
    ) {
        self.store.open_message = Some(id);
        self.forget_other_shown(id);
        let Ok(message) = self.store.db.message(id) else {
            return self.unavailable(id);
        };
        if !anyway && over_cap(message.size) {
            return self.emit(Event::MessageTooLarge {
                id,
                text: text::read::message_too_large(megabytes(message.size)),
            });
        }
        if message.body == BodyState::Downloaded {
            match self.store.bodies.read(message.folder, message.uid) {
                Ok(raw) => {
                    let shown = self.view(&message, &raw, (images, width_px));
                    self.show(id, (images, width_px), shown);
                    self.correct_attachment_mark(&message, &raw);
                    return self.mark_opened_read(id);
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
        self.compose_body_fetched(id, raw);
        let Some((images, width_px)) = self.opening.remove(&id) else {
            return;
        };
        match (raw, self.store.db.message(id)) {
            (Some(raw), Ok(message)) => {
                let shown = self.view(&message, raw, (images, width_px));
                self.show(id, (images, width_px), shown);
                self.mark_opened_read(id);
            }
            _ => self.unavailable(id),
        }
    }

    /// A step failed: nobody waiting for a body of `account` (any account
    /// for `None`) will get it soon.
    pub(super) fn open_failed(&mut self, account: Option<&str>) {
        self.compose_failed(account);
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
                self.emit(Event::AttachmentFailed { id, index });
                self.emit(Event::Notice {
                    text: text::read::attachment_not_saved(),
                });
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::testing::{Sink, open, seed};
    use std::fs;

    #[test]
    fn an_idle_error_answers_a_pending_open() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        worker.open_message(seeded.message, ImagesChoice::Default, 300);
        assert!(worker.opening.contains_key(&seeded.message));
        // What `begin_idle` and `idle_done` do with an error.
        worker.open_failed(Some(&seeded.account));
        assert!(worker.opening.is_empty());
        assert!(worker.live[&seeded.account].wants.bodies.is_empty());
        let answered = sink
            .events()
            .into_iter()
            .any(|e| matches!(e, Event::MessageUnavailable { id, .. } if id == seeded.message));
        assert!(answered);
    }

    const WITH_IMAGE: &[u8] = b"From: a@example.org\r\nSubject: s\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/related; boundary=XX\r\n\r\n\
--XX\r\nContent-Type: text/html\r\n\r\n<img src=\"cid:l@example.org\">\r\n\
--XX\r\nContent-Type: image/png\r\nContent-ID: <l@example.org>\r\n\
Content-Transfer-Encoding: base64\r\n\r\naGVsbG8=\r\n--XX--\r\n";

    #[test]
    fn a_late_body_does_not_delete_the_open_messages_inline_images() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        // A (the seeded message) has no body yet: it waits.
        worker.open_message(seeded.message, ImagesChoice::Default, 300);
        assert!(worker.opening.contains_key(&seeded.message));
        // The reader moves on to cached message B, whose images are on disk.
        let b = MessageRowId(seeded.message.0 + 100);
        worker.store.open_message = Some(b);
        let b_dir = worker.store.paths.inline().join(b.0.to_string());
        fs::create_dir_all(&b_dir).unwrap();
        fs::write(b_dir.join("0.png"), b"hello").unwrap();
        // A's body arrives.
        worker.body_fetched(seeded.message, Some(WITH_IMAGE));
        assert!(b_dir.join("0.png").exists());
        let a_dir = worker
            .store
            .paths
            .inline()
            .join(seeded.message.0.to_string());
        assert!(!a_dir.exists(), "a late view writes no files");
    }

    #[test]
    fn removing_an_account_keeps_the_shown_messages_images() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let shown = MessageRowId(41);
        worker.store.open_message = Some(shown);
        let root = worker.store.paths.inline();
        for id in ["41", "42"] {
            fs::create_dir_all(root.join(id)).unwrap();
        }
        super::super::remove::remove_for_test(&mut worker.store, &seeded.account);
        assert!(root.join("41").exists());
        assert!(!root.join("42").exists());
    }

    #[test]
    fn safe_file_name_strips_paths_and_controls() {
        assert_eq!(safe_file_name("../../etc/passwd", 0), "passwd");
        assert_eq!(safe_file_name("C:\\dir\\a.txt", 0), "a.txt");
        assert_eq!(safe_file_name("a\u{0}b\n.txt", 0), "ab.txt");
        assert_eq!(safe_file_name("..", 0), "attachment-1");
        assert_eq!(safe_file_name("a\u{202E}txt.exe", 0), "atxt.exe");
        assert_eq!(safe_file_name("\u{200B}\u{FEFF}", 0), "attachment-1");
        assert_eq!(safe_file_name(".hidden", 0), "hidden");
        assert_eq!(safe_file_name("", 2), "attachment-3");
        assert_eq!(safe_file_name("dir/", 1), "attachment-2");
        assert!(safe_file_name(&"x".repeat(1000), 0).len() <= MAX_NAME_BYTES);
    }

    #[test]
    fn the_stored_from_fallback_has_no_bidi_controls() {
        let stored = envio_store::Envelope {
            from: "\"PayPal\" <service@\u{202E}lapyap.evil>".into(),
            subject: "Invoice \u{202E}fdp.exe".into(),
            ..Default::default()
        };
        let header = stored_header(&stored, "me");
        assert!(!header.from.contains('\u{202E}'), "{}", header.from);
        assert_eq!(header.subject, "Invoice fdp.exe");
        assert_eq!(header.from_address, "");
    }

    #[test]
    fn the_reader_header_uses_the_same_label() {
        let stored = envio_store::Envelope {
            from: "\"billing@paypal.com\" <x@evil.example>".into(),
            ..Default::default()
        };
        assert_eq!(stored_header(&stored, "me").from, "x@evil.example");
        let raw = b"From: \"billing@paypal.com\" <x@evil.example>\r\nSubject: s\r\n\r\nbody\r\n";
        let settings = envio_store::Settings::default();
        let ctx = ViewContext {
            account: "me",
            settings: &settings,
            contacts: &[],
            stored: &stored,
            inline_dir: None,
            image_sizes: &Default::default(),
        };
        let view = render_view(raw, ImagesChoice::Default, 300, &ctx).0;
        assert_eq!(view.header.from, "x@evil.example");
        assert_eq!(view.header.from_address, "x@evil.example");
    }

    #[test]
    fn the_view_says_when_the_mail_sets_colours() {
        let stored = envio_store::Envelope::default();
        let settings = envio_store::Settings::default();
        let ctx = ViewContext {
            account: "me",
            settings: &settings,
            contacts: &[],
            stored: &stored,
            inline_dir: None,
            image_sizes: &Default::default(),
        };
        let html = b"From: a@example.org\r\nSubject: s\r\nContent-Type: text/html\r\n\r\n\
            <table><tr><td style=\"color:#1a1f36\">Hi</td></tr></table>\r\n";
        assert!(
            render_view(html, ImagesChoice::Default, 300, &ctx)
                .0
                .own_colours
        );
        let text = b"From: a@example.org\r\nSubject: s\r\nContent-Type: text/plain\r\n\r\nHi\r\n";
        assert!(
            !render_view(text, ImagesChoice::Default, 300, &ctx)
                .0
                .own_colours
        );
    }

    #[test]
    fn the_view_lists_the_remote_images_it_loads() {
        let stored = envio_store::Envelope::default();
        let settings = envio_store::Settings::default();
        let ctx = ViewContext {
            account: "me",
            settings: &settings,
            contacts: &[],
            stored: &stored,
            inline_dir: None,
            image_sizes: &Default::default(),
        };
        let html = b"From: a@example.org\r\nSubject: s\r\nContent-Type: text/html\r\n\r\n\
            <p><img src=\"https://example.org/a.png\"></p>\r\n";
        let blocked = render_view(html, ImagesChoice::Default, 300, &ctx).0;
        assert!(blocked.remote_images.is_empty());
        let loaded = render_view(html, ImagesChoice::Load, 300, &ctx).0;
        assert_eq!(loaded.remote_images, ["https://example.org/a.png"]);
    }

    #[test]
    fn a_taken_name_gets_a_number_before_the_extension() {
        assert_eq!(numbered("a.txt", 1), "a.txt");
        assert_eq!(numbered("a.txt", 2), "a (2).txt");
        assert_eq!(numbered("archive.tar.gz", 3), "archive.tar (3).gz");
        assert_eq!(numbered("noext", 2), "noext (2)");
    }
}
