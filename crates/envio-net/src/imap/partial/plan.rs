//! Choosing which parts of a big message to download: the text, and the
//! small inline images, never an attachment.

use super::structure::{Kind, Leaf, Node};

/// A text/plain or text/html part is fetched up to this many bytes.
pub const TEXT_CAP: u32 = 2 * 1024 * 1024;
/// Other text parts (a calendar invite) are fetched when they are this small.
const SMALL_TEXT: u32 = 256 * 1024;
/// An inline image is fetched when it is this small.
const IMAGE_CAP: u32 = 1024 * 1024;
/// All inline images together.
const IMAGES_TOTAL: u32 = 4 * 1024 * 1024;

/// What to download of a message: the leaves whose bytes are wanted, and
/// every other leaf (a stub in the assembled message).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// Section and the most bytes to fetch of it.
    pub fetch: Vec<(String, u32)>,
    pub stubs: Vec<String>,
}

pub fn plan(root: &Node) -> Plan {
    let mut out = Plan::default();
    let mut images = 0_u32;
    walk(root, &mut out, &mut images);
    out
}

fn walk(node: &Node, out: &mut Plan, images: &mut u32) {
    match &node.kind {
        Kind::Multipart { children, .. } => {
            for child in children {
                walk(child, out, images);
            }
        }
        Kind::Leaf(leaf) => match wanted(leaf, *images) {
            Some(limit) => {
                if leaf.content_type.starts_with("image/") {
                    *images += limit;
                }
                out.fetch.push((node.section.clone(), limit));
            }
            None => out.stubs.push(node.section.clone()),
        },
    }
}

/// How many bytes of `leaf` to fetch, or `None` to leave it out.
fn wanted(leaf: &Leaf, images_so_far: u32) -> Option<u32> {
    if leaf.attachment {
        return None;
    }
    match leaf.content_type.as_str() {
        "text/plain" | "text/html" => Some(leaf.octets.min(TEXT_CAP)),
        t if t.starts_with("text/") => (leaf.octets <= SMALL_TEXT).then_some(leaf.octets),
        t if t.starts_with("image/") => (leaf.has_content_id
            && leaf.octets <= IMAGE_CAP
            && images_so_far.saturating_add(leaf.octets) <= IMAGES_TOTAL)
            .then_some(leaf.octets),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn leaf(section: &str, content_type: &str, octets: u32) -> Node {
        Node {
            section: section.into(),
            kind: Kind::Leaf(Leaf {
                content_type: content_type.into(),
                attachment: false,
                has_content_id: false,
                octets,
                base64: false,
            }),
        }
    }

    fn with(mut node: Node, attachment: bool, cid: bool) -> Node {
        if let Kind::Leaf(l) = &mut node.kind {
            l.attachment = attachment;
            l.has_content_id = cid;
        }
        node
    }

    fn multi(subtype: &str, children: Vec<Node>) -> Node {
        Node {
            section: String::new(),
            kind: Kind::Multipart {
                subtype: subtype.into(),
                children,
            },
        }
    }

    const MB: u32 = 1024 * 1024;

    #[test]
    fn text_is_fetched_and_the_attachment_is_not() {
        let root = multi(
            "mixed",
            vec![
                leaf("1", "text/plain", 500),
                with(leaf("2", "application/pdf", 40 * MB), true, false),
            ],
        );
        let p = plan(&root);
        assert_eq!(p.fetch, [("1".to_owned(), 500)]);
        assert_eq!(p.stubs, ["2"]);
    }

    #[test]
    fn a_huge_text_part_is_cut_short() {
        let p = plan(&leaf("1", "text/html", 50 * MB));
        assert_eq!(p.fetch, [("1".to_owned(), TEXT_CAP)]);
    }

    #[test]
    fn an_attachment_that_is_text_is_a_stub() {
        let p = plan(&multi(
            "mixed",
            vec![with(leaf("1", "text/plain", 20), true, false)],
        ));
        assert!(p.fetch.is_empty());
        assert_eq!(p.stubs, ["1"]);
    }

    #[test]
    fn small_inline_images_are_fetched_up_to_a_total() {
        let kids = (1..=6)
            .map(|i| with(leaf(&i.to_string(), "image/png", MB), false, true))
            .collect();
        let p = plan(&multi("related", kids));
        assert_eq!(p.fetch.len(), 4);
        assert_eq!(p.stubs, ["5", "6"]);
    }

    #[test]
    fn a_big_or_unnamed_image_is_a_stub() {
        let p = plan(&multi(
            "mixed",
            vec![
                with(leaf("1", "image/jpeg", 9 * MB), false, true),
                leaf("2", "image/jpeg", 100),
            ],
        ));
        assert!(p.fetch.is_empty());
        assert_eq!(p.stubs, ["1", "2"]);
    }

    #[test]
    fn a_big_calendar_or_forwarded_message_is_a_stub() {
        let p = plan(&multi(
            "mixed",
            vec![
                leaf("1", "text/calendar", 100),
                leaf("2", "text/calendar", MB),
                leaf("3", "message/rfc822", 100),
            ],
        ));
        assert_eq!(p.fetch, [("1".to_owned(), 100)]);
        assert_eq!(p.stubs, ["2", "3"]);
    }
}
