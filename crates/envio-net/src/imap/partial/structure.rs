//! A message's MIME tree as BODYSTRUCTURE gives it, reduced to what choosing
//! parts needs.

use async_imap::imap_proto::{BodyContentCommon, BodyStructure, ContentEncoding};

/// One part of the tree. `section` is its IMAP section number (`1.2`); the
/// top of a multipart message has the empty section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub section: String,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Multipart {
        subtype: String,
        children: Vec<Node>,
    },
    Leaf(Leaf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaf {
    /// `type/subtype`, lower case. A `message/rfc822` part is a leaf: its
    /// inside is never looked at.
    pub content_type: String,
    /// Content-Disposition is `attachment`.
    pub attachment: bool,
    pub has_content_id: bool,
    /// Size on the wire (transfer-encoded), in bytes.
    pub octets: u32,
    /// Content-Transfer-Encoding is base64.
    pub base64: bool,
}

impl Leaf {
    /// The decoded size, estimated from the encoded one.
    pub fn decoded_size(&self) -> u32 {
        if self.base64 {
            self.octets / 4 * 3
        } else {
            self.octets
        }
    }
}

/// The tree of a BODYSTRUCTURE.
pub fn from_proto(structure: &BodyStructure<'_>) -> Node {
    let section = match structure {
        BodyStructure::Multipart { .. } => String::new(),
        _ => "1".to_owned(),
    };
    node(structure, section)
}

fn node(structure: &BodyStructure<'_>, section: String) -> Node {
    let kind = match structure {
        BodyStructure::Multipart { common, bodies, .. } => Kind::Multipart {
            subtype: common.ty.subtype.to_ascii_lowercase(),
            children: bodies
                .iter()
                .enumerate()
                .map(|(i, child)| {
                    let path = if section.is_empty() {
                        (i + 1).to_string()
                    } else {
                        format!("{section}.{}", i + 1)
                    };
                    node(child, path)
                })
                .collect(),
        },
        BodyStructure::Basic { common, other, .. }
        | BodyStructure::Text { common, other, .. }
        | BodyStructure::Message { common, other, .. } => Kind::Leaf(Leaf {
            content_type: content_type(common),
            attachment: common
                .disposition
                .as_ref()
                .is_some_and(|d| d.ty.eq_ignore_ascii_case("attachment")),
            has_content_id: other.id.is_some(),
            octets: other.octets,
            base64: matches!(other.transfer_encoding, ContentEncoding::Base64),
        }),
    };
    Node { section, kind }
}

fn content_type(common: &BodyContentCommon<'_>) -> String {
    format!("{}/{}", common.ty.ty, common.ty.subtype).to_ascii_lowercase()
}
