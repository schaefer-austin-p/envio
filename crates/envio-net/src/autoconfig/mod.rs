//! Finding an account's servers from its address: Thunderbird's ISPDB,
//! the provider's autoconfig file, the well-known URL, DNS SRV, then a
//! guess for the manual form. The order and choices are in `plan`; the
//! lookups come in the next task.

pub mod plan;
pub mod xml;

pub use plan::{Found, Setting, Source, SrvRecord};
