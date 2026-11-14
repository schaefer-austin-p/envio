//! Inline (`cid:`) images: each image part a message embeds is written to
//! the cache, so the reader can draw it from a `file://` URL. They are the
//! message's own parts, not remote, so they show whatever the remote-image
//! choice is (the bytes are still the sender's).
//!
//! Only the open message's folder exists: opening another message drops the
//! rest, and the whole directory goes at start, on a rebuild and when an
//! account is removed.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use envio_mime::AttachmentInfo;

/// Largest image written, in bytes.
const MAX_BYTES: usize = 10 * 1024 * 1024;
/// Most images written for one message.
const MAX_IMAGES: usize = 50;

/// The file extension for the image types Qt's rich text draws. SVG is left
/// out: it can pull in other files.
fn extension(content_type: &str) -> Option<&'static str> {
    match content_type {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" | "image/pjpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/bmp" => Some("bmp"),
        _ => None,
    }
}

/// Removes every inline image. Missing is fine.
pub(super) fn clear_all(root: &Path) {
    if let Err(e) = fs::remove_dir_all(root)
        && e.kind() != io::ErrorKind::NotFound
    {
        log::warn!("clearing the inline images: {e}");
    }
}

/// Removes every message folder under `root` but `keep`.
pub(super) fn clear_except(root: &Path, keep: Option<&str>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if keep.is_none_or(|k| entry.file_name() != k)
            && let Err(e) = fs::remove_dir_all(entry.path())
        {
            log::warn!("dropping an inline image folder: {e}");
        }
    }
}

/// The inline images of one message, by Content-ID.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Inlined {
    /// `file://` URL of each image written.
    pub urls: HashMap<String, String>,
    /// Natural pixel size (width, height), for the images whose header
    /// gave one.
    pub sizes: HashMap<String, (u32, u32)>,
}

/// Writes the message's inline images into `dir` (dropping other messages'
/// folders) and returns Content-ID to `file://` URL, with each image's
/// natural size read from its bytes. A part that can't be written is left
/// out (shown as a missing image), as is any part whose Content-ID another
/// part already has. A file already there with the part's size is kept: the
/// reader may be loading it.
pub(super) fn write(dir: &Path, raw: &[u8], attachments: &[AttachmentInfo]) -> Inlined {
    if let (Some(root), Some(keep)) = (dir.parent(), dir.file_name().and_then(|n| n.to_str())) {
        clear_except(root, Some(keep));
    }
    let mut out = Inlined::default();
    // Index, Content-ID, path and whether the file is already there.
    let mut parts = Vec::new();
    let wanted = attachments
        .iter()
        .filter_map(|a| Some((a, a.content_id.as_ref()?, extension(&a.content_type)?)))
        .filter(|(a, _, _)| a.size <= MAX_BYTES)
        .take(MAX_IMAGES);
    for (info, cid, ext) in wanted {
        if out.urls.contains_key(cid) {
            continue;
        }
        let path = dir.join(format!("{}.{ext}", info.index));
        out.urls.insert(cid.clone(), file_url(&path));
        let have = fs::metadata(&path).is_ok_and(|m| m.len() == info.size as u64);
        parts.push((info.index, cid, path, have));
    }
    if parts.is_empty() {
        return out;
    }
    if parts.iter().any(|(_, _, _, have)| !have)
        && let Err(e) = fs::create_dir_all(dir)
    {
        // Only reachable if the folder vanishes meanwhile: files already
        // there keep their URLs, the rest show as missing.
        log::warn!("creating the inline image folder: {e}");
        for (_, cid, _, have) in &parts {
            if !have {
                out.urls.remove(*cid);
            }
        }
        parts.retain(|(_, _, _, have)| *have);
    }
    // One parse of the message lends every part's bytes: the sizes come
    // from them even for a file kept from an earlier open, and nothing is
    // copied in memory (a part still to be written goes straight to disk).
    let indexes: Vec<usize> = parts.iter().map(|(i, _, _, _)| *i).collect();
    envio_mime::with_attachments(raw, &indexes, |n, bytes| {
        let (_, cid, path, have) = &parts[n];
        if let Some(size) = bytes.and_then(envio_mime::image_size) {
            out.sizes.insert((*cid).clone(), size);
        }
        if *have {
            return;
        }
        let written = bytes
            .ok_or_else(|| io::Error::other("part gone"))
            .and_then(|b| {
                let mut name = path.clone().into_os_string();
                name.push(".tmp");
                let temp = Path::new(&name);
                fs::File::create(temp)
                    .and_then(|mut f| f.write_all(b))
                    .and_then(|()| fs::rename(temp, path))
                    .inspect_err(|_| drop(fs::remove_file(temp)))
            });
        if let Err(e) = written {
            log::warn!("writing an inline image: {e}");
            out.urls.remove(*cid);
            out.sizes.remove(*cid);
        }
    });
    out
}

/// `file://` plus the path, with bytes outside the unreserved set encoded.
fn file_url(path: &Path) -> String {
    use std::fmt::Write;
    let mut url = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~') {
            url.push(char::from(b));
        } else {
            let _ = write!(url, "%{b:02X}");
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIL: &[u8] = b"From: a@example.org\r\nSubject: s\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/related; boundary=XX\r\n\r\n\
--XX\r\nContent-Type: text/html\r\n\r\n<img src=\"cid:logo@example.org\">\r\n\
--XX\r\nContent-Type: image/png\r\nContent-ID: <logo@example.org>\r\n\
Content-Transfer-Encoding: base64\r\n\r\naGVsbG8=\r\n\
--XX\r\nContent-Type: image/svg+xml\r\nContent-ID: <vec@example.org>\r\n\r\n<svg/>\r\n\
--XX--\r\n";

    fn infos() -> Vec<AttachmentInfo> {
        match envio_mime::parse(MAIL) {
            envio_mime::Message::Parsed { attachments, .. } => attachments,
            envio_mime::Message::Raw(_) => panic!("parses"),
        }
    }

    fn path_of(url: &str) -> &str {
        url.strip_prefix("file://").unwrap()
    }

    #[test]
    fn images_are_written_and_named_by_content_id() {
        let dir = tempfile::tempdir().unwrap();
        let urls = write(&dir.path().join("1"), MAIL, &infos()).urls;
        assert_eq!(urls.len(), 1, "{urls:?}");
        let url = &urls["logo@example.org"];
        assert!(url.ends_with(".png"), "{url}");
        assert_eq!(fs::read(path_of(url)).unwrap(), b"hello");
    }

    #[test]
    fn urls_encode_unsafe_path_bytes() {
        assert_eq!(
            file_url(Path::new("/a b/c#d.png")),
            "file:///a%20b/c%23d.png"
        );
    }

    #[test]
    fn a_file_of_the_right_size_is_not_written_again() {
        let dir = tempfile::tempdir().unwrap();
        let one = dir.path().join("1");
        let urls = write(&one, MAIL, &infos()).urls;
        let path = path_of(&urls["logo@example.org"]).to_owned();
        // Same size, other bytes: only a skipped write leaves this.
        fs::write(&path, b"HELLO").unwrap();
        let again = write(&one, MAIL, &infos()).urls;
        assert_eq!(again, urls);
        assert_eq!(fs::read(&path).unwrap(), b"HELLO");
        // A wrong size is replaced, and no temp file stays.
        fs::write(&path, b"x").unwrap();
        write(&one, MAIL, &infos());
        assert_eq!(fs::read(&path).unwrap(), b"hello");
        let names: Vec<_> = fs::read_dir(&one).unwrap().flatten().collect();
        assert_eq!(names.len(), 1);
    }

    #[test]
    fn opening_another_message_drops_the_images_of_the_others() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("1"), MAIL, &infos());
        assert!(dir.path().join("1").exists());
        write(&dir.path().join("2"), MAIL, &infos());
        assert!(!dir.path().join("1").exists());
        assert!(dir.path().join("2").exists());
    }

    /// A related message with one PNG part per `(Content-ID, base64 body)`.
    fn mail_with(parts: &[(String, &str)]) -> Vec<u8> {
        let mut m = String::from(
            "From: a@example.org\r\nSubject: s\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/related; boundary=XX\r\n\r\n\
--XX\r\nContent-Type: text/html\r\n\r\n<p>x</p>\r\n",
        );
        for (cid, body) in parts {
            m.push_str(&format!(
                "--XX\r\nContent-Type: image/png\r\nContent-ID: <{cid}>\r\n\
Content-Transfer-Encoding: base64\r\n\r\n{body}\r\n"
            ));
        }
        m.push_str("--XX--\r\n");
        m.into_bytes()
    }

    fn infos_of(raw: &[u8]) -> Vec<AttachmentInfo> {
        match envio_mime::parse(raw) {
            envio_mime::Message::Parsed { attachments, .. } => attachments,
            envio_mime::Message::Raw(_) => panic!("parses"),
        }
    }

    #[test]
    fn a_part_over_the_size_limit_is_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let one = dir.path().join("1");
        let mut parts = infos();
        // The size comes from the parse; claiming more than the limit
        // avoids building a 10 MB message.
        parts[0].size = MAX_BYTES + 1;
        let out = write(&one, MAIL, &parts);
        assert!(out.urls.is_empty(), "{:?}", out.urls);
        assert!(!one.exists());
        parts[0].size = MAX_BYTES;
        assert_eq!(write(&one, MAIL, &parts).urls.len(), 1);
    }

    #[test]
    fn only_the_first_images_up_to_the_limit_are_written() {
        let parts: Vec<_> = (0..MAX_IMAGES + 3)
            .map(|i| (format!("img{i}@example.org"), "aGVsbG8="))
            .collect();
        let raw = mail_with(&parts);
        let dir = tempfile::tempdir().unwrap();
        let one = dir.path().join("1");
        let out = write(&one, &raw, &infos_of(&raw));
        assert_eq!(out.urls.len(), MAX_IMAGES);
        assert!(out.urls.contains_key("img0@example.org"));
        assert!(
            out.urls
                .contains_key(&format!("img{}@example.org", MAX_IMAGES - 1))
        );
        assert!(
            !out.urls
                .contains_key(&format!("img{MAX_IMAGES}@example.org"))
        );
        assert_eq!(fs::read_dir(&one).unwrap().count(), MAX_IMAGES);
    }

    #[test]
    fn a_duplicate_content_id_is_skipped_and_the_first_wins() {
        let raw = mail_with(&[
            ("dup@example.org".to_owned(), "aGVsbG8="),
            ("dup@example.org".to_owned(), "d29ybGQ="),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let one = dir.path().join("1");
        let out = write(&one, &raw, &infos_of(&raw));
        assert_eq!(out.urls.len(), 1, "{:?}", out.urls);
        let url = &out.urls["dup@example.org"];
        assert_eq!(fs::read(path_of(url)).unwrap(), b"hello");
        assert_eq!(fs::read_dir(&one).unwrap().count(), 1);
    }

    #[test]
    fn the_natural_size_comes_from_the_bytes_even_for_a_kept_file() {
        // A PNG header, width 2000.
        let body = "iVBORw0KGgoAAAANSUhEUgAAB9AAAAAHCAYAAAA=";
        let raw = mail_with(&[("p@example.org".to_owned(), body)]);
        let dir = tempfile::tempdir().unwrap();
        let one = dir.path().join("1");
        for _ in 0..2 {
            let out = write(&one, &raw, &infos_of(&raw));
            assert_eq!(out.sizes["p@example.org"], (2000, 7));
        }
    }

    #[test]
    fn clear_all_removes_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("inline");
        write(&root.join("1"), MAIL, &infos());
        clear_all(&root);
        assert!(!root.exists());
        clear_all(&root);
    }
}
