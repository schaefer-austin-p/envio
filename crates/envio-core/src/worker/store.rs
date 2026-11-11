//! Everything the worker keeps on disk, opened together.

use std::mem;

use envio_store::{
    Accounts, Bodies, Db, Drafts, Journal, Opened, Outbox, StoreError, StorePaths, open_or_rebuild,
};
use envio_sync::Schedule;
use envio_sync::driver::Local;
use envio_sync::plan::sync::window_start;

use crate::api::view::CacheState;
use crate::core::Config;
use crate::secrets::{Secrets, SecretsError};

pub(crate) struct Store {
    pub paths: StorePaths,
    pub db: Db,
    pub bodies: Bodies,
    pub journal: Journal,
    pub outbox: Outbox,
    #[allow(dead_code)] // Task 12
    pub drafts: Drafts,
    pub accounts: Accounts,
    pub secrets: Secrets,
    pub schedule: Schedule,
}

fn secrets_error(error: SecretsError) -> StoreError {
    match error {
        SecretsError::Io(e) => StoreError::Io(e),
        SecretsError::Damaged => StoreError::BadRecord {
            file: crate::secrets::FILE_NAME.into(),
            reason: "damaged".into(),
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
        let store = Store {
            bodies: Bodies::new(paths.bodies()),
            journal: Journal::open(&paths.journal())?,
            outbox: Outbox::open(&paths.outbox())?,
            drafts: Drafts::open(&paths.drafts())?,
            accounts: Accounts::open(&paths.accounts())?,
            secrets: Secrets::open(&config.data_dir).map_err(secrets_error)?,
            schedule: Schedule::default(),
            db,
            paths,
        };
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
