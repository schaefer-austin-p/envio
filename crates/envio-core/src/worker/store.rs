//! Everything the worker keeps on disk, opened together.

use std::path::PathBuf;
use std::sync::Arc;

use envio_store::{
    Accounts, Bodies, Contact, Db, Drafts, Journal, MessageRowId, Opened, Outbox, Settings,
    StoreError, StorePaths, load_contacts, load_settings, open_or_rebuild,
};
use envio_sync::Schedule;
use envio_sync::driver::{Local, Unsent};
use envio_sync::plan::sync::window_start;

use crate::api::view::CacheState;
use crate::core::Config;
use crate::drafts::DraftBook;
use crate::secrets::{Secrets, SecretsError};

pub(crate) struct Store {
    pub paths: StorePaths,
    /// Where `accounts.json`, the passwords and the removal marker live.
    pub data_dir: PathBuf,
    pub db: Db,
    pub bodies: Bodies,
    pub journal: Journal,
    pub outbox: Outbox,
    /// Shared with the draft thread, which saves through the same handle.
    pub drafts: Arc<Drafts>,
    pub accounts: Accounts,
    pub secrets: Secrets,
    pub schedule: Schedule,
    /// `settings.json`: loaded at open, replaced only after a save worked.
    pub settings: Settings,
    /// `contacts.json`.
    pub contacts: Vec<Contact>,
    /// The message the reader last asked to open. Its inline images are the
    /// ones kept on disk; a view of any other message writes none.
    pub open_message: Option<MessageRowId>,
}

pub(super) fn secrets_error(error: SecretsError) -> StoreError {
    match error {
        SecretsError::Io(e) => StoreError::Io(e),
        SecretsError::Damaged => StoreError::BadRecord {
            file: crate::secrets::FILE_NAME.into(),
            reason: "damaged".into(),
        },
        SecretsError::Newer(version) => StoreError::BadRecord {
            file: crate::secrets::FILE_NAME.into(),
            reason: format!("version {version} is from a newer envio"),
        },
    }
}

impl Store {
    pub fn open(
        config: &Config,
        book: &DraftBook,
        now_ms: i64,
    ) -> envio_store::Result<(Store, CacheState)> {
        let paths = StorePaths::new(&config.cache_dir, &config.data_dir);
        // Inline images are copies, rewritten when a message opens.
        super::inline::clear_all(&paths.inline());
        let (db, opened) = open_or_rebuild(&paths)?;
        let cache = match opened {
            Opened::Created => CacheState::Fresh,
            Opened::Existing => CacheState::Existing,
            Opened::Rebuilt { .. } => CacheState::Rebuilt,
        };
        let mut store = Store {
            bodies: Bodies::new(paths.bodies()),
            journal: Journal::open(&paths.journal())?,
            outbox: Outbox::open(&paths.outbox())?,
            drafts: book.drafts()?,
            accounts: Accounts::open(&paths.accounts())?,
            secrets: Secrets::open(&config.data_dir).map_err(secrets_error)?,
            schedule: Schedule::default(),
            // A file that cannot be read or set aside is not worth refusing
            // mail for: start from the defaults.
            settings: load_settings(&paths.settings()).unwrap_or_else(|e| {
                log::warn!("loading the settings: {e}");
                Settings::default()
            }),
            contacts: load_contacts(&paths.contacts()).unwrap_or_else(|e| {
                log::warn!("loading the contacts: {e}");
                Vec::new()
            }),
            open_message: None,
            db,
            data_dir: config.data_dir.clone(),
            paths,
        };
        // A failed resume keeps its marker and is retried at the next start;
        // it must not lock the user out of their mail.
        match super::remove::resume_removals(&mut store) {
            Ok(finished) if !finished.is_empty() => {
                log::info!("finished {} interrupted removals", finished.len());
            }
            Ok(_) => {}
            Err(error) => log::error!("finishing interrupted removals: {error}"),
        }
        if let Err(e) = store.sweep_bodies(now_ms) {
            log::warn!("sweeping bodies: {e}");
        }
        Ok((store, cache))
    }

    /// The store as sync uses it.
    pub fn local(&mut self) -> Local<'_> {
        Local {
            db: &self.db,
            bodies: &self.bodies,
            journal: &mut self.journal,
            outbox: &self.outbox,
            schedule: &mut self.schedule,
        }
    }

    /// The store as sync uses it, with the drafts beside it.
    pub fn local_and_drafts(&mut self) -> (Local<'_>, &Drafts) {
        let local = Local {
            db: &self.db,
            bodies: &self.bodies,
            journal: &mut self.journal,
            outbox: &self.outbox,
            schedule: &mut self.schedule,
        };
        (local, &*self.drafts)
    }

    /// What the account has that no server holds yet.
    pub fn unsent(&mut self, account: &str) -> envio_store::Result<Unsent> {
        let (local, drafts) = self.local_and_drafts();
        envio_sync::driver::unsent(&local, drafts, account).map_err(super::remove::store_error)
    }

    /// Reopens the cache database after damage found mid-session. The old
    /// handle is kept unless the new one opens, so a failed rebuild leaves
    /// the store as it was.
    pub fn rebuild(&mut self) -> envio_store::Result<()> {
        self.db = open_or_rebuild(&self.paths)?.0;
        // Row ids start over: the folders named by the old ones are stale.
        super::inline::clear_all(&self.paths.inline());
        self.open_message = None;
        Ok(())
    }

    /// Drops bodies outside the sync window, and orphans with no row (#41).
    /// With window All nothing is pruned by date.
    pub fn sweep_bodies(&self, now_ms: i64) -> envio_store::Result<usize> {
        let cutoff = window_start(now_ms, self.settings.sync_window()).unwrap_or(i64::MIN);
        self.db.mark_bodies_pruned(cutoff)?;
        self.bodies.retain(&self.db.bodies_to_keep(cutoff)?)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::super::testing::{Sink, open, seed};

    /// The seeded message is dated 0 (decades old) and has a body on disk.
    fn body_survives_sweep(days: u32) -> bool {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let uid = worker.store.db.message(seeded.message).unwrap().uid;
        worker
            .store
            .bodies
            .write(seeded.folder, uid, b"Subject: hi\r\n\r\nbody")
            .unwrap();
        worker
            .store
            .db
            .set_body_state(seeded.message, envio_store::BodyState::Downloaded)
            .unwrap();
        worker.store.settings.sync_window_days = days;
        worker.store.sweep_bodies(1_800_000_000_000).unwrap();
        worker.store.bodies.read(seeded.folder, uid).is_ok()
    }

    #[test]
    fn the_body_sweep_prunes_by_date_only_inside_a_window() {
        assert!(!body_survives_sweep(30));
    }

    #[test]
    fn the_all_window_prunes_no_body_by_date() {
        assert!(body_survives_sweep(0));
    }

    #[test]
    fn a_rebuild_clears_the_inline_images() {
        let dir = tempfile::tempdir().unwrap();
        let (config, _) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let stale = worker.store.paths.inline().join("7");
        fs::create_dir_all(&stale).unwrap();
        fs::write(stale.join("0.png"), b"x").unwrap();
        worker.store.rebuild().unwrap();
        assert!(!worker.store.paths.inline().exists());
    }

    #[test]
    fn a_failed_rebuild_keeps_the_open_database() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        // The file at the path is now damaged; the open handle keeps its inode.
        let db_path = worker.store.paths.db();
        let garbage = config.cache_dir.join("garbage");
        fs::write(&garbage, b"not a database, long enough to be read as one").unwrap();
        fs::rename(&garbage, &db_path).unwrap();
        // Moving it aside needs a writable folder.
        fs::set_permissions(&config.cache_dir, fs::Permissions::from_mode(0o500)).unwrap();
        let as_root = fs::write(config.cache_dir.join("probe"), b"x").is_ok();
        let result = worker.store.rebuild();
        fs::set_permissions(&config.cache_dir, fs::Permissions::from_mode(0o700)).unwrap();
        if as_root {
            eprintln!("skipped: running as root, read-only has no effect");
            return;
        }
        assert!(result.is_err());
        let folders = worker.store.db.folders(&seeded.account).unwrap();
        assert_eq!(folders.len(), 1);
    }
}
