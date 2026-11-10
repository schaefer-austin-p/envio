//! An in-memory IMAP and SMTP server for driver tests: folders with
//! UIDVALIDITY, UIDNEXT and MODSEQ, messages with flags and arrival dates,
//! and failures injected per operation. It follows IMAP's rules where the
//! driver relies on them: UIDs only grow, MOVE is atomic, and without MOVE
//! a move is COPY then delete, which a failure can cut in between.

use std::collections::BTreeMap;

use envio_net::imap::{Caps, Flag, FlagUpdate, Folder, FolderState, MessageFlags, Role, Summary};
use envio_net::smtp::SendError;
use envio_net::{NetError, Result};
use envio_sync::server::{MailSender, MailServer};

/// 2027-01-15, inside every test's window.
pub const NOW_SECS: i64 = 1_800_000_000;
pub const NOW_MS: i64 = NOW_SECS * 1_000;

#[derive(Debug, Clone)]
pub struct FakeMessage {
    pub eml: Vec<u8>,
    pub flags: MessageFlags,
    /// Arrival, unix seconds.
    pub date: i64,
    pub modseq: u64,
}

#[derive(Debug, Clone)]
pub struct FakeFolder {
    pub role: Option<Role>,
    pub uid_validity: u32,
    pub uid_next: u32,
    pub modseq: u64,
    pub messages: BTreeMap<u32, FakeMessage>,
}

pub struct FakeServer {
    pub caps: Caps,
    pub folders: BTreeMap<String, FakeFolder>,
    selected: Option<String>,
    failures: Vec<Failure>,
    /// Every operation, in order, e.g. `select INBOX`.
    pub calls: Vec<String>,
}

struct Failure {
    op: &'static str,
    /// Only calls whose detail (e.g. the folder path) is this.
    only: Option<String>,
    skip: usize,
    error: NetError,
}

impl FakeServer {
    /// A server with MOVE, UIDPLUS, CONDSTORE and IDLE, and an INBOX.
    pub fn new() -> Self {
        let mut server = Self {
            caps: Caps::from_names(["MOVE", "UIDPLUS", "CONDSTORE", "IDLE"]),
            folders: BTreeMap::new(),
            selected: None,
            failures: Vec::new(),
            calls: Vec::new(),
        };
        server.add_folder("INBOX", Some(Role::Inbox));
        server
    }

    pub fn add_folder(&mut self, path: &str, role: Option<Role>) {
        self.folders.insert(
            path.to_owned(),
            FakeFolder {
                role,
                uid_validity: 7,
                uid_next: 1,
                modseq: 1,
                messages: BTreeMap::new(),
            },
        );
    }

    /// Delivers a message that arrived at `date`; returns its UID.
    pub fn deliver(&mut self, path: &str, eml: &[u8], date: i64) -> u32 {
        let folder = self.folders.get_mut(path).expect("no such folder");
        let uid = folder.uid_next;
        folder.uid_next += 1;
        folder.modseq += 1;
        folder.messages.insert(
            uid,
            FakeMessage {
                eml: eml.to_vec(),
                flags: MessageFlags::default(),
                date,
                modseq: folder.modseq,
            },
        );
        uid
    }

    /// A short message with this Message-ID, arrived now.
    pub fn deliver_id(&mut self, path: &str, id: &str) -> u32 {
        self.deliver(path, &eml(id), NOW_SECS)
    }

    /// The next call of `op` (e.g. "select", "move_to") fails with `error`.
    pub fn fail_on(&mut self, op: &'static str, error: NetError) {
        self.fail_after(op, 0, error);
    }

    /// The call of `op` after `skip` successful ones fails with `error`.
    pub fn fail_after(&mut self, op: &'static str, skip: usize, error: NetError) {
        self.failures.push(Failure {
            op,
            only: None,
            skip,
            error,
        });
    }

    /// The next call of `op` with `detail` (e.g. a folder path) fails.
    pub fn fail_on_detail(&mut self, op: &'static str, detail: &str, error: NetError) {
        self.failures.push(Failure {
            op,
            only: Some(detail.to_owned()),
            skip: 0,
            error,
        });
    }

    pub fn uids(&self, path: &str) -> Vec<u32> {
        self.folders[path].messages.keys().copied().collect()
    }

    pub fn message(&self, path: &str, uid: u32) -> Option<&FakeMessage> {
        self.folders[path].messages.get(&uid)
    }

    /// The Message-IDs in `path`, by UID.
    pub fn ids(&self, path: &str) -> Vec<String> {
        self.folders[path]
            .messages
            .values()
            .filter_map(|message| message_id(&message.eml))
            .collect()
    }

    /// The server renumbers the folder (a UIDVALIDITY change).
    pub fn renumber(&mut self, path: &str) {
        let folder = self.folders.get_mut(path).expect("no such folder");
        folder.uid_validity += 1;
        let messages: Vec<FakeMessage> =
            std::mem::take(&mut folder.messages).into_values().collect();
        folder.uid_next = 1;
        for message in messages {
            folder.messages.insert(folder.uid_next, message);
            folder.uid_next += 1;
        }
    }

    /// Another device sets the message's flags.
    pub fn set_flags(&mut self, path: &str, uid: u32, flags: MessageFlags) {
        let folder = self.folders.get_mut(path).expect("no such folder");
        folder.modseq += 1;
        let message = folder.messages.get_mut(&uid).expect("no such message");
        message.flags = flags;
        message.modseq = folder.modseq;
    }

    fn call(&mut self, op: &'static str, detail: &str) -> Result<()> {
        self.calls
            .push(format!("{op} {detail}").trim_end().to_owned());
        let Some(at) = self.failures.iter().position(|failure| {
            failure.op == op && failure.only.as_deref().is_none_or(|only| only == detail)
        }) else {
            return Ok(());
        };
        if self.failures[at].skip > 0 {
            self.failures[at].skip -= 1;
            return Ok(());
        }
        Err(self.failures.remove(at).error)
    }

    fn selected(&mut self) -> Result<&mut FakeFolder> {
        let path = self
            .selected
            .clone()
            .ok_or_else(|| NetError::Protocol("no folder selected".into()))?;
        Ok(self.folders.get_mut(&path).expect("selected folder exists"))
    }

    fn copy(&mut self, uids: &[u32], target: &str) -> Result<()> {
        let copies: Vec<FakeMessage> = {
            let folder = self.selected()?;
            uids.iter()
                .filter_map(|uid| folder.messages.get(uid).cloned())
                .collect()
        };
        let target = self
            .folders
            .get_mut(target)
            .ok_or_else(|| NetError::Refused("[TRYCREATE] no such folder".into()))?;
        for mut copy in copies {
            target.modseq += 1;
            copy.modseq = target.modseq;
            target.messages.insert(target.uid_next, copy);
            target.uid_next += 1;
        }
        Ok(())
    }

    fn expunge(&mut self, uids: &[u32]) -> Result<()> {
        let folder = self.selected()?;
        for uid in uids {
            folder.messages.remove(uid);
        }
        folder.modseq += 1;
        Ok(())
    }
}

impl Default for FakeServer {
    fn default() -> Self {
        Self::new()
    }
}

pub fn eml(id: &str) -> Vec<u8> {
    format!(
        "From: Ada <ada@example.org>\r\nTo: bob@example.org\r\nSubject: About {id}\r\n\
         Message-ID: <{id}@example.org>\r\nDate: Thu, 14 Jan 2027 10:00:00 +0000\r\n\r\n\
         Hello about {id}.\r\n"
    )
    .into_bytes()
}

fn message_id(eml: &[u8]) -> Option<String> {
    envio_mime::envelope(eml).and_then(|envelope| envelope.message_id)
}

fn bare(id: &str) -> &str {
    id.trim().trim_start_matches('<').trim_end_matches('>')
}

fn set(flags: &mut MessageFlags, flag: &Flag, on: bool) {
    match flag {
        Flag::Seen => flags.seen = on,
        Flag::Answered => flags.answered = on,
        Flag::Flagged => flags.flagged = on,
        Flag::Deleted => flags.deleted = on,
        Flag::Draft => flags.draft = on,
        Flag::Keyword(word) => {
            flags.keywords.retain(|kept| kept != word);
            if on {
                flags.keywords.push(word.clone());
            }
        }
    }
}

fn header(eml: &[u8]) -> Vec<u8> {
    let end = eml
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map_or(eml.len(), |at| at + 4);
    eml[..end].to_vec()
}

impl MailServer for FakeServer {
    fn caps(&self) -> Caps {
        self.caps
    }

    async fn list_folders(&mut self) -> Result<Vec<Folder>> {
        self.call("list_folders", "")?;
        Ok(self
            .folders
            .iter()
            .map(|(path, folder)| Folder {
                path: path.clone(),
                name: path.clone(),
                delimiter: Some("/".into()),
                role: folder.role,
                selectable: true,
            })
            .collect())
    }

    async fn select(&mut self, path: &str) -> Result<FolderState> {
        self.call("select", path)?;
        let folder = self
            .folders
            .get(path)
            .ok_or_else(|| NetError::Refused("no such folder".into()))?;
        let state = FolderState {
            uid_validity: folder.uid_validity,
            uid_next: folder.uid_next,
            exists: u32::try_from(folder.messages.len()).unwrap_or(u32::MAX),
            highest_modseq: self.caps.condstore.then_some(folder.modseq),
        };
        self.selected = Some(path.to_owned());
        Ok(state)
    }

    /// Floors to a UTC day; real servers use server-local time.
    async fn search_since(&mut self, unix_secs: i64) -> Result<Vec<u32>> {
        self.call("search_since", "")?;
        let day = unix_secs - unix_secs.rem_euclid(86_400);
        let folder = self.selected()?;
        Ok(folder
            .messages
            .iter()
            .filter(|(_, message)| message.date >= day)
            .map(|(uid, _)| *uid)
            .collect())
    }

    async fn search_message_id(&mut self, message_id: &str) -> Result<Vec<u32>> {
        self.call("search_message_id", message_id)?;
        let wanted = bare(message_id).to_owned();
        let folder = self.selected()?;
        Ok(folder
            .messages
            .iter()
            .filter(|(_, message)| {
                !message.flags.deleted && self::message_id(&message.eml).as_deref() == Some(&wanted)
            })
            .map(|(uid, _)| *uid)
            .collect())
    }

    async fn fetch_summaries(&mut self, uids: &[u32]) -> Result<Vec<Summary>> {
        self.call("fetch_summaries", &format!("{uids:?}"))?;
        let folder = self.selected()?;
        Ok(uids
            .iter()
            .filter_map(|uid| {
                folder.messages.get(uid).map(|message| Summary {
                    uid: *uid,
                    flags: message.flags.clone(),
                    size: u32::try_from(message.eml.len()).unwrap_or(u32::MAX),
                    internal_date: Some(message.date),
                    modseq: Some(message.modseq),
                    header: header(&message.eml),
                })
            })
            .collect())
    }

    async fn fetch_flags(
        &mut self,
        uids: &[u32],
        changed_since: Option<u64>,
    ) -> Result<Vec<FlagUpdate>> {
        self.call("fetch_flags", &format!("{changed_since:?}"))?;
        let folder = self.selected()?;
        Ok(uids
            .iter()
            .filter_map(|uid| folder.messages.get(uid).map(|message| (uid, message)))
            .filter(|(_, message)| changed_since.is_none_or(|since| message.modseq > since))
            .map(|(uid, message)| FlagUpdate {
                uid: *uid,
                flags: message.flags.clone(),
                modseq: Some(message.modseq),
            })
            .collect())
    }

    async fn fetch_body(&mut self, uid: u32, _size: usize) -> Result<Option<Vec<u8>>> {
        self.call("fetch_body", &uid.to_string())?;
        let folder = self.selected()?;
        Ok(folder.messages.get(&uid).map(|message| message.eml.clone()))
    }

    async fn store_flags(&mut self, uids: &[u32], add: bool, flags: &[Flag]) -> Result<()> {
        self.call("store_flags", &format!("{uids:?} {add} {flags:?}"))?;
        let folder = self.selected()?;
        folder.modseq += 1;
        let modseq = folder.modseq;
        for uid in uids {
            if let Some(message) = folder.messages.get_mut(uid) {
                for flag in flags {
                    set(&mut message.flags, flag, add);
                }
                message.modseq = modseq;
            }
        }
        Ok(())
    }

    async fn move_to(&mut self, uids: &[u32], target: &str) -> Result<()> {
        self.call("move_to", &format!("{uids:?} {target}"))?;
        if self.caps.r#move {
            self.copy(uids, target)?;
            return self.expunge(uids);
        }
        self.copy(uids, target)?;
        self.delete(uids).await
    }

    async fn delete(&mut self, uids: &[u32]) -> Result<()> {
        self.call("delete", &format!("{uids:?}"))?;
        if self.caps.uidplus {
            return self.expunge(uids);
        }
        // Without UIDPLUS the real session can only flag \Deleted.
        let folder = self.selected()?;
        folder.modseq += 1;
        let modseq = folder.modseq;
        for uid in uids {
            if let Some(message) = folder.messages.get_mut(uid) {
                message.flags.deleted = true;
                message.modseq = modseq;
            }
        }
        Ok(())
    }

    async fn append(&mut self, path: &str, flags: &[Flag], eml: &[u8]) -> Result<()> {
        self.call("append", path)?;
        if !self.folders.contains_key(path) {
            return Err(NetError::Refused("[TRYCREATE] no such folder".into()));
        }
        let uid = self.deliver(path, eml, NOW_SECS);
        let message = self
            .folders
            .get_mut(path)
            .and_then(|folder| folder.messages.get_mut(&uid))
            .expect("just delivered");
        for flag in flags {
            set(&mut message.flags, flag, true);
        }
        Ok(())
    }
}

/// An SMTP server that answers each send with the next scripted result
/// (default: accepted) and keeps what it accepted.
#[derive(Default)]
pub struct FakeSender {
    pub results: Vec<std::result::Result<(), SendError>>,
    /// (from, recipients, eml) of each send attempt.
    pub attempts: Vec<(String, Vec<String>, Vec<u8>)>,
}

impl MailSender for FakeSender {
    async fn send(
        &mut self,
        from: &str,
        to: &[String],
        eml: &[u8],
    ) -> std::result::Result<(), SendError> {
        self.attempts
            .push((from.to_owned(), to.to_vec(), eml.to_vec()));
        if self.results.is_empty() {
            Ok(())
        } else {
            self.results.remove(0)
        }
    }
}
