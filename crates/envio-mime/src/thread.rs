//! Message-ID normalization and the thread root.

/// A Message-ID without its angle brackets or outer spaces; `None` when
/// nothing is left or the id holds a control character or whitespace.
pub fn normalize_id(raw: &str) -> Option<String> {
    let id = raw.trim();
    let id = id.strip_prefix('<').unwrap_or(id);
    let id = id.strip_suffix('>').unwrap_or(id).trim();
    let bad = |c: char| c.is_control() || c.is_whitespace();
    (!id.is_empty() && !id.contains(bad)).then(|| id.to_owned())
}

/// The id a thread is grouped under: the first reference, else the
/// In-Reply-To, else the message's own id. Normalized; empty when the
/// message has no ids at all.
pub fn thread_root(message_id: &str, in_reply_to: Option<&str>, references: &[String]) -> String {
    references
        .iter()
        .find_map(|r| normalize_id(r))
        .or_else(|| in_reply_to.and_then(normalize_id))
        .or_else(|| normalize_id(message_id))
        .unwrap_or_default()
}
