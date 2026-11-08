//! The tag names a document uses that the sanitizer would strip.
//!
//! Ammonia drops a tag it doesn't allow (keeping the text) before any style
//! check, so `<section style="display:none">` would leak its text. Those
//! names are let through ammonia for this one document instead, and the
//! image pass removes them again (hidden ones with their content).

use std::cell::RefCell;
use std::collections::BTreeSet;

use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};

use crate::sanitize::{DROP_WITH_CONTENT, TAGS};

#[derive(Default)]
struct Names(RefCell<BTreeSet<String>>);

impl TokenSink for Names {
    type Handle = ();

    fn process_token(&self, token: Token, _line: u64) -> TokenSinkResult<()> {
        if let Token::TagToken(Tag {
            kind: TagKind::StartTag,
            name,
            ..
        }) = token
        {
            let plain = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            if plain && !TAGS.contains(&&*name) && !DROP_WITH_CONTENT.contains(&&*name) {
                self.0.borrow_mut().insert(name.to_string());
            }
        }
        TokenSinkResult::Continue
    }
}

/// Start-tag names in `html` that are neither kept nor dropped with their
/// content, so only made of letters, digits, `-` and `_`.
pub(crate) fn stripped_names(html: &str) -> BTreeSet<String> {
    let tokenizer = Tokenizer::new(Names::default(), TokenizerOpts::default());
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice(html));
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    tokenizer.sink.0.take()
}
