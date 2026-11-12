//! Drafts: what compose holds, one JSON file per draft in the data folder,
//! replaced atomically on every save. Core debounces saves to about 1 s.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic::{is_temp, write_atomic};
use crate::error::{Result, StoreError};
use crate::ids::{fresh_key, is_key};

/// Missing fields load as empty, so older and newer drafts both open.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Draft {
    pub account: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    /// Paths of files to attach.
    pub attachments: Vec<PathBuf>,
    pub updated_at_ms: i64,
}

/// Readable drafts, then the ids of damaged ones.
type Scan = (Vec<(String, Draft)>, Vec<String>);

pub struct Drafts {
    dir: PathBuf,
}

impl Drafts {
    /// Opens the drafts folder and clears interrupted saves.
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_name().to_str().is_some_and(is_temp) {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(Self {
            dir: dir.to_owned(),
        })
    }

    /// Saves a new draft and returns its id.
    pub fn create(&self, draft: &Draft) -> Result<String> {
        let id = fresh_key(draft.updated_at_ms, |id| self.path(id).exists());
        self.save(&id, draft)?;
        Ok(id)
    }

    /// Writes the draft, recreating it if it is missing. Core must cancel a
    /// pending debounced save before it calls `delete`, or a late autosave
    /// brings a sent or discarded draft back.
    pub fn save(&self, id: &str, draft: &Draft) -> Result<()> {
        check(id)?;
        // An attachment path that is not UTF-8 cannot be written as JSON:
        // the save fails and the draft stays pending, it never panics.
        let bytes = serde_json::to_vec_pretty(draft).map_err(|error| StoreError::BadRecord {
            file: self.path(id).display().to_string(),
            reason: error.to_string(),
        })?;
        Ok(write_atomic(&self.path(id), &bytes)?)
    }

    pub fn load(&self, id: &str) -> Result<Draft> {
        check(id)?;
        let path = self.path(id);
        let bytes = fs::read(&path).map_err(StoreError::missing_or_io)?;
        serde_json::from_slice(&bytes).map_err(|error| StoreError::BadRecord {
            file: path.display().to_string(),
            reason: error.to_string(),
        })
    }

    /// Every readable draft, most recently changed first. A draft that cannot
    /// be read is skipped (see `damaged`), never deleted.
    pub fn list(&self) -> Result<Vec<(String, Draft)>> {
        Ok(self.scan()?.0)
    }

    /// Ids of drafts that could not be read or parsed, sorted. Their files are
    /// left in place.
    pub fn damaged(&self) -> Result<Vec<String>> {
        Ok(self.scan()?.1)
    }

    fn scan(&self) -> Result<Scan> {
        let mut drafts = Vec::new();
        let mut damaged = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let name = entry?.file_name();
            let Some(id) = name.to_str().and_then(|name| name.strip_suffix(".json")) else {
                continue;
            };
            if is_key(id) {
                match self.load(id) {
                    Ok(draft) => drafts.push((id.to_owned(), draft)),
                    Err(_) => damaged.push(id.to_owned()),
                }
            }
        }
        damaged.sort();
        drafts
            .sort_by(|(a_id, a), (b_id, b)| (b.updated_at_ms, b_id).cmp(&(a.updated_at_ms, a_id)));
        Ok((drafts, damaged))
    }

    /// Deletes a draft (sent, or discarded by the user).
    pub fn delete(&self, id: &str) -> Result<()> {
        check(id)?;
        let removed = fs::remove_file(self.path(id)).map_err(StoreError::missing_or_io);
        // Also when the draft file is already gone: a retry after a partial
        // failure still clears the folder.
        remove_dir_if_there(&self.files_dir(id))?;
        removed?;
        // Without this a power loss can bring a sent draft back.
        Ok(fs::File::open(&self.dir)?.sync_all()?)
    }

    /// Copies `source` into `files/<draft id>/` and returns the path of the
    /// copy, which is what the draft stores (the original may move or go
    /// away). The name is made safe and a collision gets ` (2)`, ` (3)`.
    pub fn keep_attachment(&self, draft_id: &str, source: &Path) -> Result<PathBuf> {
        check(draft_id)?;
        let mut input = fs::File::open(source).map_err(StoreError::missing_or_io)?;
        let folder = self.files_dir(draft_id);
        fs::create_dir_all(&folder)?;
        let name = safe_name(source);
        for n in 1_u32.. {
            let target = folder.join(numbered(&name, n));
            // `create_new` so two copies never share a name, even racing.
            let mut output = match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
            {
                Ok(output) => output,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            };
            let copied = io::copy(&mut input, &mut output).and_then(|_| output.sync_all());
            if let Err(error) = copied {
                drop(output);
                let _ = fs::remove_file(&target);
                return Err(error.into());
            }
            fs::File::open(&folder)?.sync_all()?;
            return Ok(target);
        }
        unreachable!("the loop only ends by returning")
    }

    /// Drops a copy `keep_attachment` made for a draft that went meanwhile
    /// (deleted or sent), and the draft's files folder when that is empty
    /// now. Best effort: what is left, the next start's `sweep_files` takes
    /// (it removes the folders of drafts that are gone).
    pub fn forget_copy(&self, draft_id: &str, copy: Option<&Path>) {
        if let Some(copy) = copy {
            let _ = fs::remove_file(copy);
        }
        if check(draft_id).is_ok() {
            // Fails when the folder is not empty or not there; both are fine.
            let _ = fs::remove_dir(self.files_dir(draft_id));
        }
    }

    /// Deletes the copies of drafts that no longer exist (a crash between
    /// deleting a draft and its files); returns how many folders went. A
    /// damaged draft still has its file, so its copies stay.
    pub fn sweep_files(&self) -> Result<usize> {
        let entries = match fs::read_dir(self.dir.join(FILES)) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        let mut swept = 0;
        for entry in entries {
            let entry = entry?;
            let id = entry.file_name();
            let gone = id.to_str().is_none_or(|id| !self.path(id).exists());
            if entry.file_type()?.is_dir() && gone {
                fs::remove_dir_all(entry.path())?;
                swept += 1;
            }
        }
        Ok(swept)
    }

    fn files_dir(&self, id: &str) -> PathBuf {
        self.dir.join(FILES).join(id)
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }
}

/// The folder of attachment copies inside the drafts folder.
const FILES: &str = "files";

fn remove_dir_if_there(dir: &Path) -> Result<()> {
    match fs::remove_dir_all(dir) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

/// The last part of the path with no separators and no leading dots, so a
/// copy can never leave its folder or hide.
fn safe_name(source: &Path) -> String {
    let name = source
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let name = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let name = name.replace('\0', "");
    let name = name.trim_start_matches('.').trim();
    if name.is_empty() {
        "attachment".to_owned()
    } else {
        name.to_owned()
    }
}

/// `report.pdf` is `report (2).pdf` for n = 2.
fn numbered(name: &str, n: u32) -> String {
    if n == 1 {
        return name.to_owned();
    }
    match name.rfind('.') {
        Some(dot) if dot > 0 => format!("{} ({n}){}", &name[..dot], &name[dot..]),
        _ => format!("{name} ({n})"),
    }
}

fn check(id: &str) -> Result<()> {
    if is_key(id) {
        Ok(())
    } else {
        Err(StoreError::NotFound)
    }
}
