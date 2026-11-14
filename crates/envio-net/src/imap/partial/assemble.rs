//! Putting the downloaded parts back into one message. A part that was not
//! downloaded keeps its headers and gets an empty body, plus two headers
//! saying where it lives on the server and how big it is:
//!
//! ```text
//! X-Envio-Section: 1.2
//! X-Envio-Size: 31457280
//! ```
//!
//! envio-mime reads them (`AttachmentInfo::section`, `size`). A text part cut
//! at [`TEXT_CAP`] gets `X-Envio-Truncated: <its size>` so the reader can say
//! so. Headers of those names that came from the server are renamed first
//! (see [`neutralize`]): only the ones written here count.

use std::collections::HashMap;

use super::plan::TEXT_CAP;
use super::structure::{Kind, Node};
use crate::imap::untrusted::neutralize;

/// The downloaded bytes by name: `1.2` for a part's body, `1.2.MIME` for its
/// headers. The message's own headers come separately.
pub type Fetched = HashMap<String, Vec<u8>>;

/// The message: `header` (the message's own headers, as the server has
/// them) and every part of `root`.
pub fn assemble(root: &Node, header: &[u8], fetched: &Fetched) -> Vec<u8> {
    let mut out = Vec::new();
    match &root.kind {
        Kind::Multipart { .. } => {
            out.extend_from_slice(&without_content_type(&neutralize(header)));
            multipart(&mut out, root, fetched);
        }
        Kind::Leaf(_) => leaf(&mut out, root, header, fetched),
    }
    out
}

fn multipart(out: &mut Vec<u8>, node: &Node, f: &Fetched) {
    let Kind::Multipart { subtype, children } = &node.kind else {
        return;
    };
    let boundary = format!("=_envio_{}", node.section.replace('.', "_"));
    out.extend_from_slice(
        format!("Content-Type: multipart/{subtype}; boundary=\"{boundary}\"\r\n\r\n").as_bytes(),
    );
    for child in children {
        out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        match &child.kind {
            Kind::Multipart { .. } => multipart(out, child, f),
            Kind::Leaf(_) => {
                let header = f.get(&format!("{}.MIME", child.section));
                leaf(out, child, header.map_or(&[][..], Vec::as_slice), f);
            }
        }
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
}

/// A leaf: its headers, then its body, or the stub marks and nothing.
fn leaf(out: &mut Vec<u8>, node: &Node, header: &[u8], f: &Fetched) {
    let Kind::Leaf(info) = &node.kind else {
        return;
    };
    let header = neutralize(header);
    match f.get(&node.section) {
        Some(body) => {
            out.extend_from_slice(&without_blank_line(&header));
            if info.octets > TEXT_CAP {
                out.extend_from_slice(format!("X-Envio-Truncated: {}\r\n", info.octets).as_bytes());
            }
            out.extend_from_slice(b"\r\n");
            out.extend_from_slice(body);
        }
        None => {
            out.extend_from_slice(&without_blank_line(&header));
            out.extend_from_slice(
                format!(
                    "X-Envio-Section: {}\r\nX-Envio-Size: {}\r\n\r\n",
                    node.section,
                    info.decoded_size()
                )
                .as_bytes(),
            );
        }
    }
}

/// `header` without its closing blank line, every line ending in CRLF.
fn without_blank_line(header: &[u8]) -> Vec<u8> {
    let mut end = header.len();
    while end > 0 && matches!(header[end - 1], b'\r' | b'\n') {
        end -= 1;
    }
    let mut out = header[..end].to_vec();
    if end > 0 {
        out.extend_from_slice(b"\r\n");
    }
    out
}

/// `header` without its Content-Type field (and its folded lines), and
/// without the closing blank line.
fn without_content_type(header: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut skipping = false;
    for line in without_blank_line(header).split_inclusive(|b| *b == b'\n') {
        let folded = matches!(line.first(), Some(b' ' | b'\t'));
        if !folded {
            skipping = line.len() >= 13 && line[..13].eq_ignore_ascii_case(b"content-type:");
        }
        if !skipping {
            out.extend_from_slice(line);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::structure::Leaf;
    use super::*;

    fn leaf_node(section: &str, content_type: &str, octets: u32, base64: bool) -> Node {
        Node {
            section: section.into(),
            kind: Kind::Leaf(Leaf {
                content_type: content_type.into(),
                attachment: false,
                has_content_id: false,
                octets,
                base64,
            }),
        }
    }

    fn fetched(items: &[(&str, &str)]) -> Fetched {
        items
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.as_bytes().to_vec()))
            .collect()
    }

    #[test]
    fn a_multipart_message_keeps_its_headers_and_gets_new_boundaries() {
        let root = Node {
            section: String::new(),
            kind: Kind::Multipart {
                subtype: "mixed".into(),
                children: vec![
                    leaf_node("1", "text/plain", 5, false),
                    leaf_node("2", "application/pdf", 4000, true),
                ],
            },
        };
        let header =
            "Subject: Hi\r\nContent-Type: multipart/mixed;\r\n boundary=\"old\"\r\nX-A: b\r\n\r\n";
        let f = fetched(&[
            ("1.MIME", "Content-Type: text/plain\r\n\r\n"),
            ("1", "hello"),
            (
                "2.MIME",
                "Content-Type: application/pdf\r\nContent-Transfer-Encoding: base64\r\n\r\n",
            ),
        ]);
        let eml = String::from_utf8(assemble(&root, header.as_bytes(), &f)).unwrap();
        assert_eq!(
            eml,
            "Subject: Hi\r\nX-A: b\r\n\
             Content-Type: multipart/mixed; boundary=\"=_envio_\"\r\n\r\n\
             --=_envio_\r\nContent-Type: text/plain\r\n\r\nhello\r\n\
             --=_envio_\r\nContent-Type: application/pdf\r\n\
             Content-Transfer-Encoding: base64\r\n\
             X-Envio-Section: 2\r\nX-Envio-Size: 3000\r\n\r\n\r\n\
             --=_envio_--\r\n"
        );
    }

    #[test]
    fn a_single_part_message_is_its_header_and_body_or_a_stub() {
        let root = leaf_node("1", "text/plain", 5, false);
        let header = "Subject: Hi\r\nContent-Type: text/plain\r\n\r\n";
        let f = fetched(&[("1", "hello")]);
        assert_eq!(
            assemble(&root, header.as_bytes(), &f),
            b"Subject: Hi\r\nContent-Type: text/plain\r\n\r\nhello"
        );
        let stub = String::from_utf8(assemble(&root, header.as_bytes(), &Fetched::new())).unwrap();
        assert!(
            stub.ends_with("X-Envio-Section: 1\r\nX-Envio-Size: 5\r\n\r\n"),
            "{stub}"
        );
    }

    #[test]
    fn nested_multiparts_get_their_own_boundaries() {
        let inner = Node {
            section: "1".into(),
            kind: Kind::Multipart {
                subtype: "alternative".into(),
                children: vec![leaf_node("1.1", "text/plain", 1, false)],
            },
        };
        let root = Node {
            section: String::new(),
            kind: Kind::Multipart {
                subtype: "mixed".into(),
                children: vec![inner],
            },
        };
        let f = fetched(&[
            ("1.1.MIME", "Content-Type: text/plain\r\n\r\n"),
            ("1.1", "x"),
        ]);
        let eml = String::from_utf8(assemble(&root, b"A: b\r\n\r\n", &f)).unwrap();
        assert!(eml.contains("boundary=\"=_envio_1\""), "{eml}");
        assert!(
            eml.contains("--=_envio_1\r\nContent-Type: text/plain\r\n\r\nx\r\n--=_envio_1--"),
            "{eml}"
        );
    }
}
