//! Everything the worker keeps on disk, opened together.

use std::mem;
use std::path::PathBuf;

use envio_store::{
    Accounts, Bodies, Db, Drafts, Journal, Opened, Outbox, StoreError, StorePaths, open_or_rebuild,
};
use envio_sync::Schedule;
use envio_sync::driver::{Local, Unsent};
use envio_sync::plan::sync::window_start;

use crate::api::view::CacheState;
use crate::core::Config;
use crate::secrets::{Secrets, SecretsError};

pub(crate) struct Store {
    pub paths: StorePaths,
    /// Where `accounts.json`, the passwords and the removal marker live.
    pub data_dir: PathBuf,
    pub db: Db,
    pub bodies: Bodies,
    pub journal: Journal,
    pub outbox: Outbox,
    pub drafts: Drafts,
    pub accounts: Accounts,
    pub secrets: Secrets,
    pub schedule: Schedule,
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
    pub fn open(config: &Config, now_ms: i64) -> envio_store::Result<(Store, CacheState)> {
        let paths = StorePaths::new(&config.cache_dir, &config.data_dir);
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
            drafts: Drafts::open(&paths.drafts())?,
            accounts: Accounts::open(&paths.accounts())?,
            secrets: Secrets::open(&config.data_dir).map_err(secrets_error)?,
            schedule: Schedule::default(),
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
        (local, &self.drafts)
    }

    /// What the account has that no server holds yet.
    pub fn unsent(&mut self, account: &str) -> envio_store::Result<Unsent> {
        let (local, drafts) = self.local_and_drafts();
        envio_sync::driver::unsent(&local, drafts, account).map_err(super::remove::store_error)
    }

    /// Reopens the cache database after damage found mid-session.
    pub fn rebuild(&mut self) -> envio_store::Result<()> {
        drop(mem::replace(&mut self.db, Db::open_in_memory()?));
        self.db = open_or_rebuild(&self.paths)?.0;
        Ok(())
    }

    /// Drops bodies outside the sync window, and orphans with no row (#41).
    pub fn sweep_bodies(&self, now_ms: i64) -> envio_store::Result<usize> {
        let cutoff = window_start(now_ms);
        self.db.mark_bodies_pruned(cutoff)?;
        self.bodies.retain(&self.db.bodies_to_keep(cutoff)?)
    }
}
