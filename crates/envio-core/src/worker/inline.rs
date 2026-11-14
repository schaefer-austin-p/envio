//! Inline (`cid:`) images: each image part a message embeds is written to
//! the cache, so the reader can draw it from a `file://` URL. They are the
//! message's own parts, not remote, so they show whatever the remote-image
//! choice is.

use std::collections::HashMap;
use std::fs;
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

/// Writes the message's inline images into `dir` and returns Content-ID to
/// `file://` URL. A part that can't be written is left out (shown as a
/// missing image), as is any part whose Content-ID another part already has.
pub(super) fn write(
    dir: &Path,
    raw: &[u8],
    attachments: &[AttachmentInfo],
) -> HashMap<String, String> {
    let mut urls = HashMap::new();
    let parts = attachments
        .iter()
        .filter_map(|a| Some((a, a.content_id.as_ref()?, extension(&a.content_type)?)))
        .filter(|(a, _, _)| a.size <= MAX_BYTES)
        .take(MAX_IMAGES);
    let mut made = false;
    for (info, cid, ext) in parts {
        if urls.contains_key(cid) {
            continue;
        }
        let Some(bytes) = envio_mime::attachment_bytes(raw, info.index) else {
            continue;
        };
        if !made {
            if let Err(e) = fs::create_dir_all(dir) {
                log::warn!("creating the inline image folder: {e}");
                return urls;
            }
            made = true;
        }
        let path = dir.join(format!("{}.{ext}", info.index));
        if let Err(e) = fs::write(&path, bytes) {
            log::warn!("writing an inline image: {e}");
            continue;
        }
        urls.insert(cid.clone(), file_url(&path));
    }
    urls
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

    #[test]
    fn images_are_written_and_named_by_content_id() {
        let dir = tempfile::tempdir().unwrap();
        let urls = write(dir.path(), MAIL, &infos());
        assert_eq!(urls.len(), 1, "{urls:?}");
        let url = &urls["logo@example.org"];
        let path = url.strip_prefix("file://").unwrap();
        assert!(path.ends_with(".png"), "{url}");
        assert_eq!(fs::read(path).unwrap(), b"hello");
    }

    #[test]
    fn urls_encode_unsafe_path_bytes() {
        assert_eq!(
            file_url(Path::new("/a b/c#d.png")),
            "file:///a%20b/c%23d.png"
        );
    }
}
