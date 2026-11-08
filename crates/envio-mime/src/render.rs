//! One entry point for the reader: any [`Message`] to Qt rich text.

use crate::message::{Body, Message};
use crate::plain::{escape_into, plain_to_rich};
use crate::sanitize::{RenderOptions, Rendered, sanitize_html};

/// The message body as Qt rich text. A raw ("strange") message is shown
/// as escaped preformatted text.
pub fn render(message: &Message, options: &RenderOptions) -> Rendered {
    let html = match message {
        Message::Raw(text) => {
            let mut html = String::from("<pre>");
            escape_into(text, &mut html);
            html.push_str("</pre>");
            html
        }
        Message::Parsed { body, .. } => match body {
            Body::Html(html) => return sanitize_html(html, options),
            Body::Text {
                text,
                flowed,
                delsp,
            } => plain_to_rich(text, *flowed, *delsp),
            Body::Empty => String::new(),
        },
    };
    Rendered {
        html,
        ..Rendered::default()
    }
}
