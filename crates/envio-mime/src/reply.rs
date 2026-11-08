//! Reply, reply all and forward drafts from a message being read.

use crate::address::{Address, dedup};
use crate::build::Outgoing;
use crate::envelope::Envelope;
use crate::preview::is_signature_line;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyKind {
    Sender,
    All,
}

/// The text being replied to and the line above it ("On …, … wrote:"),
/// translated and filled in by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quote<'a> {
    pub text: &'a str,
    pub attribution: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReplyDraft {
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub subject: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    /// The attribution line plus the original, `> `-quoted, signature
    /// dropped. Kept apart so compose can fold it.
    pub quoted: String,
}

impl ReplyDraft {
    /// Lines in [`Self::quoted`], for "Show quoted text, N lines".
    pub fn quoted_lines(&self) -> usize {
        self.quoted.lines().count()
    }

    /// The message to send: the user's text, a blank line, the quote.
    pub fn into_outgoing(self, from: Address, text: &str) -> Outgoing {
        Outgoing {
            from,
            to: self.to,
            cc: self.cc,
            subject: self.subject,
            text: join(text, &self.quoted),
            in_reply_to: self.in_reply_to,
            references: self.references,
            ..Outgoing::default()
        }
    }
}

/// Header labels for a forwarded message, translated by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ForwardLabels {
    /// e.g. "Forwarded message".
    pub forwarded_message: String,
    pub from: String,
    pub date: String,
    pub subject: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ForwardDraft {
    pub subject: String,
    /// The header block plus the original text.
    pub forwarded: String,
}

impl ForwardDraft {
    /// The message to send. Recipients and the original attachments (see
    /// [`crate::attachment_bytes`]) are added by the caller.
    pub fn into_outgoing(self, from: Address, text: &str) -> Outgoing {
        Outgoing {
            from,
            subject: self.subject,
            text: join(text, &self.forwarded),
            ..Outgoing::default()
        }
    }
}

/// A reply to `original`. `me` holds the user's own emails, which never end
/// up in Cc.
pub fn reply(original: &Envelope, quote: &Quote<'_>, me: &[String], kind: ReplyKind) -> ReplyDraft {
    let is_me = |a: &Address| me.iter().any(|m| a.is(m));
    let from_me = !original.from.is_empty() && original.from.iter().all(is_me);
    let to = if from_me {
        // Replying to my own sent message goes to its recipients.
        original.to.clone()
    } else if !original.reply_to.is_empty() {
        original.reply_to.clone()
    } else {
        original.from.clone()
    };
    let to = dedup(to);
    let cc = match kind {
        ReplyKind::Sender => Vec::new(),
        ReplyKind::All => {
            let all = original.to.iter().chain(&original.cc).cloned().collect();
            dedup(all)
                .into_iter()
                .filter(|a| !is_me(a) && !to.iter().any(|t| t.is(&a.email)))
                .collect()
        }
    };
    let mut references = original.references.clone();
    if references.is_empty()
        && let Some(parent) = &original.in_reply_to
    {
        // RFC 5322 §3.6.4: no References, a single In-Reply-To.
        references.push(parent.clone());
    }
    if let Some(id) = &original.message_id
        && !references.contains(id)
    {
        references.push(id.clone());
    }
    ReplyDraft {
        to,
        cc,
        subject: prefixed(&original.subject, "Re: ", &["re:"]),
        in_reply_to: original.message_id.clone(),
        references,
        quoted: quoted(quote),
    }
}

/// A forward of `original`. `date_text` is the original date, formatted by
/// the caller.
pub fn forward(
    original: &Envelope,
    original_text: &str,
    labels: &ForwardLabels,
    date_text: &str,
) -> ForwardDraft {
    let list = |addresses: &[Address]| {
        addresses
            .iter()
            .map(Address::display)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let forwarded = format!(
        "---------- {} ----------\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n\n{}",
        labels.forwarded_message,
        labels.from,
        list(&original.from),
        labels.date,
        date_text,
        labels.subject,
        original.subject,
        labels.to,
        list(&original.to),
        original_text.trim_end(),
    );
    ForwardDraft {
        subject: prefixed(&original.subject, "Fwd: ", &["fwd:", "fw:"]),
        forwarded,
    }
}

/// `subject` with `prefix` unless it already starts with one of `known`
/// (lower case), ignoring case.
fn prefixed(subject: &str, prefix: &str, known: &[&str]) -> String {
    let subject = subject.trim();
    let lower = subject.to_lowercase();
    if known.iter().any(|k| lower.starts_with(k)) {
        subject.to_owned()
    } else {
        format!("{prefix}{subject}")
    }
}

/// The attribution line, then each line up to the signature as `> line`
/// (`>` alone for blank lines, `>>` for lines already quoted).
fn quoted(quote: &Quote<'_>) -> String {
    let mut out = String::from(quote.attribution.trim_end());
    for line in quote.text.trim_end().lines() {
        if is_signature_line(line) {
            break;
        }
        out.push('\n');
        out.push('>');
        if !line.is_empty() && !line.starts_with('>') {
            out.push(' ');
        }
        out.push_str(line);
    }
    out
}

fn join(text: &str, tail: &str) -> String {
    format!("{}\n\n{}", text.trim_end(), tail)
}
