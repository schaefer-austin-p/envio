//! Envio's mail formats: parse incoming mail into envelopes and bodies,
//! render bodies as Qt rich text, build outgoing mail, reply, forward and
//! `mailto:`. Pure: no I/O, no other envio crate. Mail is untrusted input,
//! so nothing here panics on it.

mod address;
mod align;
mod attach_part;
mod attachments;
mod auth;
mod backgrounds;
mod build;
mod colours;
mod content_type;
mod cssdecl;
pub mod dkim;
mod emptydiv;
mod envelope;
mod fontsize;
mod from_field;
mod hidden;
mod image_label;
mod image_size;
mod images;
mod invite;
mod layout;
mod linear;
mod link;
mod mail;
mod mailto;
mod message;
mod natural;
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
mod work;

pub use address::{Address, same_email};
pub use align::aligned;
pub use attachments::{
    AttachmentInfo, attachment_bytes, attachments_bytes, decode_part, with_attachments,
};
pub use auth::sender_verified;
pub use build::{Built, CalendarPart, Outgoing, OutgoingAttachment, build, new_message_id};
pub use colours::{parse_colour, sets_colours};
pub use content_type::content_type_for;
pub use envelope::{Envelope, envelope, header_suggests_attachments};
pub use fontsize::font_cap_px;
pub use image_size::size as image_size;
pub use invite::{
    Invite, InviteMethod, NaiveParts, PartStat, Person, When, calendar_file, invite_of,
    parse_invite, reply_ics, reply_outgoing,
};
pub use layout::{Layout, layout};
pub use link::{LinkParts, link_parts, strip_bidi_controls};
pub use mailto::{MailtoDraft, parse_mailto};
pub use message::{Body, Message, parse, quote_text};
pub use natural::Unsized;
pub use plain::plain_to_rich;
pub use preview::{PREVIEW_CHARS, preview};
pub use render::render;
pub use reply::{ForwardDraft, ForwardLabels, Quote, ReplyDraft, ReplyKind, forward, reply};
pub use sanitize::{RenderOptions, Rendered, sanitize_html};
pub use thread::{normalize_id, thread_root};
#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub use work::take_work;
