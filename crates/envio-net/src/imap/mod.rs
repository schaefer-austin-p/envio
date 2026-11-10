//! Reading mail: an IMAP session with the operations sync needs (Plan 5
//! wraps them in its `MailServer` trait). Every call runs under the
//! command timeout; any error leaves the session unusable, and the caller
//! opens a new one.

mod client;
pub mod names;
mod ops;
pub mod query;

use async_imap::Session;

use crate::connect;
use crate::error::Result;
use crate::server::{Credentials, Endpoint};
use crate::tls::Roots;
use client::{Stream, map_error};

pub use names::Role;
pub use ops::SCAN;
pub use query::{Flag, MessageFlags};

/// The extensions envio uses when the server has them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Caps {
    pub condstore: bool,
    pub qresync: bool,
    pub idle: bool,
    pub r#move: bool,
    pub uidplus: bool,
    pub special_use: bool,
    /// LITERAL+ or LITERAL- (IMAP4rev2 has the latter).
    pub literal_plus: bool,
}

impl Caps {
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut caps = Self::default();
        for name in names {
            match name.to_ascii_uppercase().as_str() {
                "CONDSTORE" => caps.condstore = true,
                "QRESYNC" => {
                    caps.qresync = true;
                    caps.condstore = true;
                }
                "IDLE" => caps.idle = true,
                "MOVE" => caps.r#move = true,
                "UIDPLUS" => caps.uidplus = true,
                "SPECIAL-USE" => caps.special_use = true,
                "LITERAL+" | "LITERAL-" => caps.literal_plus = true,
                "IMAP4REV2" => {
                    caps.r#move = true;
                    caps.uidplus = true;
                    caps.special_use = true;
                    caps.literal_plus = true;
                }
                _ => {}
            }
        }
        caps
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    /// The name on the wire (modified UTF-7); what `select` takes.
    pub path: String,
    /// For display: decoded, or the raw path if it doesn't decode.
    pub name: String,
    pub delimiter: Option<String>,
    pub role: Option<Role>,
    pub selectable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderState {
    pub uid_validity: u32,
    pub uid_next: u32,
    pub exists: u32,
    /// With CONDSTORE only.
    pub highest_modseq: Option<u64>,
}

/// What the message list needs, before the body is fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub uid: u32,
    pub flags: MessageFlags,
    pub size: u32,
    /// Unix seconds.
    pub internal_date: Option<i64>,
    pub modseq: Option<u64>,
    /// The raw header block, for envio-mime.
    pub header: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagUpdate {
    pub uid: u32,
    pub flags: MessageFlags,
    pub modseq: Option<u64>,
}

pub struct ImapSession {
    session: Session<Stream>,
    caps: Caps,
    /// EXISTS of the selected folder, as of SELECT.
    exists: u32,
}

impl ImapSession {
    /// Connects, secures the connection, signs in and reads capabilities.
    pub async fn open(
        endpoint: &Endpoint,
        roots: &Roots,
        credentials: &Credentials,
    ) -> Result<Self> {
        let mut session = client::open(endpoint, roots, credentials).await?;
        let caps = connect::within(async {
            let caps = session.capabilities().await.map_err(map_error)?;
            Ok(Caps::from_names(caps.iter().filter_map(|cap| match cap {
                async_imap::types::Capability::Atom(name) => Some(name.as_str()),
                _ => None,
            })))
        })
        .await?;
        Ok(Self {
            session,
            caps,
            exists: 0,
        })
    }

    pub fn caps(&self) -> Caps {
        self.caps
    }

    /// A round trip that keeps the connection alive. Unsolicited responses
    /// (new mail, expunges) are not surfaced yet.
    pub async fn noop(&mut self) -> Result<()> {
        connect::within(async { self.session.noop().await.map_err(map_error) }).await
    }

    pub async fn logout(mut self) -> Result<()> {
        connect::within(async { self.session.logout().await.map_err(map_error) }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_are_read_case_insensitively() {
        let caps = Caps::from_names(["IMAP4rev1", "condstore", "IDLE", "LITERAL+"]);
        assert!(caps.condstore && caps.idle && caps.literal_plus);
        assert!(!caps.r#move && !caps.qresync);
    }

    #[test]
    fn imap4rev2_and_qresync_imply_what_they_include() {
        let caps = Caps::from_names(["IMAP4rev2", "QRESYNC"]);
        assert!(caps.r#move && caps.uidplus && caps.special_use && caps.literal_plus);
        assert!(caps.qresync && caps.condstore);
    }
}
