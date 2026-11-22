//! Moving an Outlook.com account from IMAP to Graph (#342): who qualifies
//! and the next step of the one-shot move. Pure; `worker/migrate_graph.rs`
//! drives it.

pub mod eligible;
pub mod plan;

pub use eligible::eligible;
