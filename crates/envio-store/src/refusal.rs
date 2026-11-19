//! Why the server refused a draft upload. Only the kind is kept, never the
//! server's text; the label the user sees is built in `envio-core`.

use serde::{Deserialize, Serialize};

/// The reason a draft is not on the server, as far as the refusal says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefusalKind {
    /// The mailbox is over its quota.
    OverQuota,
    /// The Drafts folder is read-only, or the account may not write to it.
    ReadOnly,
    /// The server found the message too large.
    TooLarge,
    /// A refusal that says none of the above (or a protocol error).
    Other,
}
