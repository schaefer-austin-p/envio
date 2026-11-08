//! Outgoing mail: RFC 5322 bytes from what the user composed.

use std::hash::{BuildHasher, RandomState};

use mail_builder::MessageBuilder;
use mail_builder::headers::address::Address as BuilderAddress;

use crate::address::{Address, dedup, has_specials, is_special};

/// A message as composed, ready to turn into bytes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outgoing {
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    /// Gets the mail but is never written into it.
    pub bcc: Vec<Address>,
    pub subject: String,
    /// Plain UTF-8 text with `\n` line ends.
    pub text: String,
    /// Without angle brackets.
    pub in_reply_to: Option<String>,
    /// Without angle brackets, oldest first.
    pub references: Vec<String>,
    pub attachments: Vec<OutgoingAttachment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OutgoingAttachment {
    pub filename: String,
    /// `type/subtype`.
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// The bytes to send and to keep in Sent, plus the SMTP recipients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Built {
    /// Without angle brackets.
    pub message_id: String,
    pub bytes: Vec<u8>,
    /// To, Cc and Bcc emails, each once, in that order.
    pub recipients: Vec<String>,
}

/// Builds the message. `date_unix` is the Date header (seconds) and
/// `message_id` (without angle brackets) comes from [`new_message_id`],
/// so a retry from the Outbox sends the same id.
///
/// An unsafe `message_id` is replaced by a fresh one. envio-core must store
/// [`Built::message_id`], not the id it passed in, so every retry sends the
/// id of the first try.
pub fn build(outgoing: &Outgoing, date_unix: i64, message_id: &str) -> Built {
    // Last line of defence: nothing with a control character or a space
    // reaches a header, whatever the caller passed in.
    let message_id = if unsafe_email(message_id) {
        new_message_id(&outgoing.from.email)
    } else {
        message_id.to_owned()
    };
    let subject: String = outgoing
        .subject
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    let to = safe_list(&outgoing.to);
    let cc = safe_list(&outgoing.cc);
    let mut builder = MessageBuilder::new()
        .from(builder_address(&outgoing.from))
        .subject(subject)
        .date(date_unix)
        .message_id(message_id.as_str())
        .text_body(outgoing.text.replace("\r\n", "\n"));
    if !to.is_empty() {
        builder = builder.to(address_list(&to));
    }
    if !cc.is_empty() {
        builder = builder.cc(address_list(&cc));
    }
    if let Some(id) = outgoing.in_reply_to.as_deref().filter(|id| !unsafe_id(id)) {
        builder = builder.in_reply_to(id);
    }
    let refs: Vec<&str> = outgoing
        .references
        .iter()
        .map(String::as_str)
        .filter(|id| !unsafe_id(id))
        .collect();
    if !refs.is_empty() {
        builder = builder.references(refs);
    }
    for file in &outgoing.attachments {
        builder = builder.attachment(
            safe_content_type(&file.content_type),
            file.filename.as_str(),
            file.bytes.as_slice(),
        );
    }
    let mut bytes = Vec::new();
    builder.serialize(&mut bytes);
    Built {
        message_id,
        bytes,
        recipients: recipients(outgoing),
    }
}

/// A fresh Message-ID (without angle brackets): 128 random bits in hex,
/// then `@` and the sender's domain. Never the device's hostname.
///
/// A domain that isn't plain ASCII letters, digits, dots and hyphens (an
/// IDN such as `例え.jp`, which is not punycoded here) gives `@invalid`.
/// The id stays unique; only the domain part is lost.
pub fn new_message_id(from_email: &str) -> String {
    let domain = from_email
        .rsplit_once('@')
        .map(|(_, d)| d.trim())
        .filter(|d| {
            !d.is_empty()
                && d.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        })
        .unwrap_or("invalid");
    let mut random = [0u8; 16];
    if getrandom::fill(&mut random).is_err() {
        // No OS randomness: fall back to std's per-process random keys.
        let state = RandomState::new();
        let (a, b) = (state.hash_one(1u8), state.hash_one(2u8));
        random[..8].copy_from_slice(&a.to_le_bytes());
        random[8..].copy_from_slice(&b.to_le_bytes());
    }
    let hex: String = random.iter().map(|b| format!("{b:02x}")).collect();
    format!("{hex}@{domain}")
}

/// `type/subtype` of RFC 2045 token characters, else `application/octet-stream`.
fn safe_content_type(content_type: &str) -> &str {
    let token = |t: &str| {
        !t.is_empty()
            && t.chars()
                .all(|c| c.is_ascii_alphanumeric() || "!#$&-^_.+".contains(c))
    };
    match content_type.split_once('/') {
        Some((kind, sub)) if token(kind) && token(sub) => content_type,
        _ => "application/octet-stream",
    }
}

/// Empty, or holding a control character or whitespace.
fn unsafe_token(text: &str) -> bool {
    text.is_empty() || text.contains(|c: char| c.is_control() || c.is_whitespace())
}

/// An email that is empty, or holds a control character, whitespace or an
/// address special: mail-builder writes it unquoted, so `a@b>,evil@x` would
/// add a recipient.
fn unsafe_email(text: &str) -> bool {
    unsafe_token(text) || has_specials(text)
}

/// Like [`unsafe_email`], but a Message-ID may carry its own `<` and `>`.
fn unsafe_id(text: &str) -> bool {
    unsafe_token(text) || has_specials(&text.replace(['<', '>'], ""))
}

/// The addresses that are safe to write into a header.
fn safe_list(addresses: &[Address]) -> Vec<Address> {
    addresses
        .iter()
        .filter(|a| !unsafe_email(&a.email))
        .cloned()
        .collect()
}

fn builder_address(address: &Address) -> BuilderAddress<'static> {
    let clean = |text: &str, replacement: char| -> String {
        text.chars()
            .map(|c| if c.is_control() { replacement } else { c })
            .filter(|c| *c != '\0')
            .collect()
    };
    let name = address.name.as_deref().map(|n| clean(n, ' '));
    let email: String = address
        .email
        .chars()
        .filter(|&c| !c.is_control() && !c.is_whitespace() && !is_special(c))
        .collect();
    BuilderAddress::new_address(name, email)
}

fn address_list(addresses: &[Address]) -> BuilderAddress<'static> {
    BuilderAddress::new_list(addresses.iter().map(builder_address).collect())
}

fn recipients(outgoing: &Outgoing) -> Vec<String> {
    let all = [&outgoing.to, &outgoing.cc, &outgoing.bcc]
        .into_iter()
        .flat_map(|list| safe_list(list))
        .collect();
    dedup(all).into_iter().map(|a| a.email).collect()
}
