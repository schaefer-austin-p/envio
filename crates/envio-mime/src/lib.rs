//! Envio's mail formats: parse incoming mail into envelopes and bodies,
//! render bodies as Qt rich text, build outgoing mail, reply, forward and
//! `mailto:`. Pure: no I/O, no other envio crate. Mail is untrusted input,
//! so nothing here panics on it.

mod address;
mod attachments;
mod build;
mod colours;
mod envelope;
mod hidden;
mod images;
mod linear;
mod link;
mod mail;
mod mailto;
mod message;
mod nesting;
mod plain;
mod preview;
mod render;
mod reply;
mod sanitize;
mod style;
mod tagnames;
mod thread;
mod widths;

pub use address::{Address, same_email};
pub use attachments::{AttachmentInfo, attachment_bytes};
pub use build::{Built, Outgoing, OutgoingAttachment, build, new_message_id};
pub use colours::{parse_colour, sets_colours};
pub use envelope::{Envelope, envelope, header_suggests_attachments};
pub use link::{LinkParts, link_parts, strip_bidi_controls};
pub use mailto::{MailtoDraft, parse_mailto};
pub use message::{Body, Message, parse, quote_text};
pub use plain::plain_to_rich;
pub use preview::{PREVIEW_CHARS, preview};
pub use render::render;
pub use reply::{ForwardDraft, ForwardLabels, Quote, ReplyDraft, ReplyKind, forward, reply};
pub use sanitize::{RenderOptions, Rendered, sanitize_html};
pub use thread::{normalize_id, thread_root};
