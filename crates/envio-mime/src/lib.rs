//! Envio's mail formats: parse incoming mail into envelopes and bodies,
//! render bodies as Qt rich text, build outgoing mail, reply, forward and
//! `mailto:`. Pure: no I/O, no other envio crate. Mail is untrusted input,
//! so nothing here panics on it.

mod address;
mod preview;
mod thread;

pub use address::{Address, same_email};
pub use preview::{PREVIEW_CHARS, preview};
pub use thread::{normalize_id, thread_root};
