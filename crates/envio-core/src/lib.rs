//! Envio's logic. Qt-free: the app talks to it through qt/.

pub mod api;
pub mod build_info;
pub mod core;
pub mod health;
pub mod logging;
pub mod problem;
pub mod secrets;
pub mod servers;

mod clock;
mod fsutil;
// The helpers below are still partly consumed (tr_n, mask, more text).
#[allow(dead_code)]
mod i18n;
#[allow(dead_code)]
mod mask;
mod supervisor;
#[allow(dead_code)]
mod text;
mod worker;

pub use api::command::Command;
pub use api::event::Event;
pub use api::view::{AccountInfo, CacheState};
pub use core::{Config, Core, EventSink};
pub use envio_store::{Flags, FolderId, FolderRole, MessageRowId, Page};
pub use health::AccountStatus;
pub use problem::CertDetails;
pub use servers::{ServerAddr, Service};
