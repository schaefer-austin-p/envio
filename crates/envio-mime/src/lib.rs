//! Envio's mail formats: parse incoming mail into envelopes and bodies,
//! render bodies as Qt rich text, build outgoing mail, reply, forward and
//! `mailto:`. Pure: no I/O, no other envio crate. Mail is untrusted input,
//! so nothing here panics on it.

mod address;
mod attachments;
mod build;
mod envelope;
mod images;
mod mail;
mod mailto;
mod message;
mod plain;
mod preview;
mod render;
mod reply;
mod sanitize;
mod thread;

pub use address::{Address, same_email};
pub use attachments::{AttachmentInfo, attachment_bytes};
pub use build::{Built, Outgoing, OutgoingAttachment, build, new_message_id};
pub use envelope::{Envelope, envelope};
pub use mailto::{MailtoDraft, parse_mailto};
pub use message::{Body, Message, parse};
pub use plain::plain_to_rich;
pub use preview::{PREVIEW_CHARS, preview};
pub use render::render;
pub use reply::{ForwardDraft, ForwardLabels, Quote, ReplyDraft, ReplyKind, forward, reply};
pub use sanitize::{RenderOptions, Rendered, sanitize_html};
pub use thread::{normalize_id, thread_root};
