//! The Graph values the sync side needs, already parsed. Plain data.

/// One mail folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GFolder {
    pub id: String,
    pub parent_id: Option<String>,
    pub display_name: String,
    /// `inbox`, `sentitems`, ... when Graph names it.
    pub well_known: Option<String>,
    pub hidden: bool,
    pub class: Option<String>,
    pub child_count: u32,
}

/// A message's cheap metadata, from delta, search and filter replies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GMeta {
    pub id: String,
    /// Milliseconds since the epoch, UTC.
    pub received_ms: i64,
    pub is_read: bool,
    pub flagged: bool,
    pub categories: Vec<String>,
    pub is_draft: bool,
}

/// A message in a delta reply. A message that only changed has just its
/// id and the properties that changed (recorded: a read flip gave `id` and
/// `isRead` only), so every field but the id is optional: `None` means
/// "unchanged", not "false".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GChange {
    pub id: String,
    pub received_ms: Option<i64>,
    pub is_read: Option<bool>,
    pub flagged: Option<bool>,
    pub categories: Option<Vec<String>>,
    pub is_draft: Option<bool>,
}

/// Where a delta walk goes next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaNext {
    /// `@odata.nextLink`: more pages.
    More(String),
    /// `@odata.deltaLink`: the walk is done; keep this for next time.
    Done(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaPage {
    Page {
        changed: Vec<GChange>,
        /// Ids of messages that are gone.
        removed: Vec<String>,
        next: DeltaNext,
    },
    /// The link is no longer valid: the adapter resets the folder.
    Expired,
}

/// What a `$batch` summary read gives for one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GSummary {
    pub id: String,
    pub received_ms: i64,
    /// RFC 822 size in bytes (`Integer 0x0E08`); 0 if Graph gave none.
    pub size: u32,
    pub headers: Vec<(String, String)>,
}

/// A Graph error reply, reduced to what may be logged: the status and
/// Graph's error code, never its message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphError {
    pub status: u16,
    pub code: String,
    pub retry_after_s: Option<u64>,
}

/// One sub-reply of a `$batch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubReply {
    /// The request id the caller gave, as text.
    pub id: String,
    pub status: u16,
    pub retry_after_s: Option<u64>,
    pub body: Vec<u8>,
}
