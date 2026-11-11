//! Everything an event carries.

use crate::health::AccountStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheState {
    /// There was no cache; sync fills it.
    Fresh,
    Existing,
    /// The old cache was damaged and is being rebuilt from the server.
    Rebuilt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountInfo {
    pub key: String,
    pub name: String,
    pub email: String,
    pub status: AccountStatus,
}

/// Work an account has that no server holds yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsentCounts {
    pub outbox: usize,
    pub drafts: usize,
    pub actions: usize,
}
