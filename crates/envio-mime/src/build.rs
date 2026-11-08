//! Outgoing mail: RFC 5322 bytes from what the user composed.

use std::hash::{BuildHasher, RandomState};

use mail_builder::MessageBuilder;
use mail_builder::headers::address::Address as BuilderAddress;

use crate::address::{Address, dedup};

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
pub fn build(outgoing: &Outgoing, date_unix: i64, message_id: &str) -> Built {
    let mut builder = MessageBuilder::new()
        .from(builder_address(&outgoing.from))
        .subject(outgoing.subject.as_str())
        .date(date_unix)
        .message_id(message_id)
        .text_body(outgoing.text.replace("\r\n", "\n"));
    if !outgoing.to.is_empty() {
        builder = builder.to(address_list(&outgoing.to));
    }
    if !outgoing.cc.is_empty() {
        builder = builder.cc(address_list(&outgoing.cc));
    }
    if let Some(id) = &outgoing.in_reply_to {
        builder = builder.in_reply_to(id.as_str());
    }
    if !outgoing.references.is_empty() {
        let refs: Vec<&str> = outgoing.references.iter().map(String::as_str).collect();
        builder = builder.references(refs);
    }
    for file in &outgoing.attachments {
        builder = builder.attachment(
            file.content_type.as_str(),
            file.filename.as_str(),
            file.bytes.as_slice(),
        );
    }
    let mut bytes = Vec::new();
    builder.serialize(&mut bytes);
    Built {
        message_id: message_id.to_owned(),
        bytes,
        recipients: recipients(outgoing),
    }
}

/// A fresh Message-ID (without angle brackets): 128 random bits in hex,
/// then `@` and the sender's domain. Never the device's hostname.
pub fn new_message_id(from_email: &str) -> String {
    let domain = from_email
        .rsplit_once('@')
        .map(|(_, d)| d.trim())
        .filter(|d| !d.is_empty() && !d.contains(['<', '>', ' ']))
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

fn builder_address(address: &Address) -> BuilderAddress<'_> {
    BuilderAddress::new_address(address.name.as_deref(), address.email.as_str())
}

fn address_list(addresses: &[Address]) -> BuilderAddress<'_> {
    BuilderAddress::new_list(addresses.iter().map(builder_address).collect())
}

fn recipients(outgoing: &Outgoing) -> Vec<String> {
    let all = [&outgoing.to, &outgoing.cc, &outgoing.bcc]
        .into_iter()
        .flatten()
        .cloned()
        .collect();
    dedup(all).into_iter().map(|a| a.email).collect()
}
