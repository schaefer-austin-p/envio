//! A cheap guard against deeply nested HTML. ammonia (html5ever) is
//! quadratic in the depth of its stack of open elements and Qt's layout is
//! super-linear, so mail nested deeper than [`MAX_DEPTH`] never reaches
//! them.
//!
//! html5ever's own tokenizer reads the markup (linear, and right about
//! comments, quoting, names and CDATA); a small model of the tree builder's
//! stack of open elements sits behind it. The model must never count *less*
//! than the tree builder: where html5ever's rules are subtle (adoption
//! agency, scopes) it keeps elements open, and where foreign content could
//! make the two disagree about what is markup at all, the guard gives up.
//! Counting too much only sends very odd mail to the plain-text fallback.

use std::cell::{Cell, RefCell};

use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use html5ever::{LocalName, TokenizerResult};

mod foreign;
#[cfg(test)]
mod fuzz;
#[cfg(test)]
mod fuzz_baseline;
#[cfg(test)]
mod fuzz_gen;
#[cfg(test)]
mod fuzz_sink;
mod reopen;
mod tables;
mod tags;

use tags::{BLOCK, FORMATTING, VOID, breaks_out, raw_text};

/// Deepest element nesting we hand to ammonia and Qt.
pub(crate) const MAX_DEPTH: usize = 256;

/// Whatever the depth, this many start tags is too many.
const MAX_START_TAGS: usize = 50_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ns {
    Html,
    Svg,
    Math,
}

/// One entry of the model's stack of open elements.
struct Open {
    name: LocalName,
    ns: Ns,
    /// svg foreignObject, desc, title; MathML text and annotation-xml.
    integration: bool,
    /// A table part outside any table, which html5ever ignores: it is
    /// counted but is neither closed nor passed by an implied end.
    inert: bool,
    /// Formatting elements html5ever will reopen until this marker closes.
    debt: usize,
}

/// The model of the tree builder's stack. Foreign elements only ever sit at
/// the top: anything that would put HTML inside svg or math trips the guard.
#[derive(Default)]
struct Model {
    stack: Vec<Open>,
    /// All reopen debt, also what no marker carries (see `reopen`).
    debt: usize,
    starts: usize,
    tripped: bool,
}

#[derive(Default)]
struct Sink {
    model: RefCell<Model>,
    /// `Tokenizer::end` must not be stopped.
    ending: Cell<bool>,
}

/// Whether the element nesting of `html` (as html5ever would build it)
/// goes above [`MAX_DEPTH`], or it has too many start tags. Linear in the
/// input: html5ever's tokenizer plus constant work per tag at a bounded
/// depth.
pub(crate) fn too_deep(html: &str) -> bool {
    let tokenizer = Tokenizer::new(Sink::default(), TokenizerOpts::default());
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice(html));
    if !matches!(tokenizer.feed(&input), TokenizerResult::Done) {
        // The sink stops the tokenizer only once it has tripped.
        return true;
    }
    tokenizer.sink.ending.set(true);
    tokenizer.end();
    tokenizer.sink.model.borrow().tripped
}

impl TokenSink for Sink {
    type Handle = ();

    fn process_token(&self, token: Token, _line: u64) -> TokenSinkResult<()> {
        let Token::TagToken(tag) = token else {
            return TokenSinkResult::Continue;
        };
        let mut model = self.model.borrow_mut();
        if model.tripped {
            return TokenSinkResult::Continue;
        }
        let next = match tag.kind {
            TagKind::StartTag => model.start(&tag),
            TagKind::EndTag => {
                model.end(&tag.name);
                TokenSinkResult::Continue
            }
        };
        if model.tripped && !self.ending.get() {
            // Stops `feed`: nothing after this matters.
            return TokenSinkResult::Script(());
        }
        next
    }

    /// CDATA sections exist only in foreign content.
    fn adjusted_current_node_present_but_not_in_html_namespace(&self) -> bool {
        self.model
            .borrow()
            .stack
            .last()
            .is_some_and(|open| open.ns != Ns::Html)
    }
}

impl Model {
    fn start(&mut self, tag: &Tag) -> TokenSinkResult<()> {
        if let Some(top) = self.stack.last().filter(|open| open.ns != Ns::Html) {
            if top.integration {
                // html5ever would build HTML inside svg or math here.
                self.tripped = true;
                return TokenSinkResult::Continue;
            }
            if !breaks_out(tag) {
                let ns = top.ns;
                self.open(tag, ns);
                return TokenSinkResult::Continue;
            }
            if !self.leave_foreign() {
                return TokenSinkResult::Continue;
            }
        }
        self.start_html(tag)
    }

    fn start_html(&mut self, tag: &Tag) -> TokenSinkResult<()> {
        let name = &*tag.name;
        match name {
            "svg" => self.open(tag, Ns::Svg),
            "math" => self.open(tag, Ns::Math),
            "col" => self.start_col(),
            _ if VOID.contains(&name) => {}
            "table" => self.start_table(),
            "td" | "th" | "tr" => self.start_table_part(&tag.name),
            "tbody" | "thead" | "tfoot" | "caption" | "colgroup" => self.start_section(&tag.name),
            _ => {
                self.imply_end(name);
                self.push(tag.name.clone(), Ns::Html, false);
                return raw_text(name);
            }
        }
        TokenSinkResult::Continue
    }

    /// The end tags a start tag implies, but only where html5ever surely
    /// pops at least as much.
    fn imply_end(&mut self, name: &str) {
        let (targets, through): (&[&str], &[&str]) = match name {
            "li" => (&["li"], &["p"]),
            "dd" | "dt" => (&["dd", "dt"], &["p"]),
            "p" => (&["p"], &["li", "dd", "dt", "tr"]),
            "option" | "optgroup" => (&["option"], &[]),
            _ => return,
        };
        self.pop_through(targets, through);
        if matches!(name, "li" | "dd" | "dt") && self.top_is("p") {
            // html5ever closes a p in button scope before inserting these.
            self.pop();
        }
    }

    fn end(&mut self, name: &LocalName) {
        if self.stack.last().is_some_and(|open| open.ns != Ns::Html) {
            if !matches!(&**name, "p" | "br") {
                self.end_foreign(name);
                return;
            }
            if !self.leave_foreign() {
                return;
            }
        }
        self.end_html(name);
    }

    fn end_html(&mut self, name: &str) {
        if self.end_table_part(name) {
            return;
        }
        if self.top_is(name) {
            self.pop();
            return;
        }
        let through: &[&str] = match name {
            "p" => &["li", "dd", "dt", "tr"],
            "li" => &["p", "dd", "dt", "tr"],
            "dd" | "dt" => &["p", "li", "dd", "dt", "tr"],
            _ if FORMATTING.contains(&name) => return,
            _ if BLOCK.contains(&name) => &["p", "li", "dd", "dt", "tr"],
            _ => return,
        };
        self.pop_through(&[name], through);
    }

    /// Closes the nearest open HTML element named in `targets`, but only
    /// through elements named in `through`.
    fn pop_through(&mut self, targets: &[&str], through: &[&str]) {
        for (index, open) in self.stack.iter().enumerate().rev() {
            if open.ns != Ns::Html || open.inert {
                return;
            }
            if targets.contains(&&*open.name) {
                self.truncate(index);
                return;
            }
            if !through.contains(&&*open.name) {
                return;
            }
        }
    }

    fn top_is(&self, name: &str) -> bool {
        self.stack
            .last()
            .is_some_and(|open| open.ns == Ns::Html && !open.inert && &*open.name == name)
    }

    fn push(&mut self, name: LocalName, ns: Ns, integration: bool) {
        self.count();
        self.stack.push(Open {
            name,
            ns,
            integration,
            inert: false,
            debt: 0,
        });
        self.check_depth();
    }

    fn check_depth(&mut self) {
        if self.stack.len() + self.debt > MAX_DEPTH {
            self.tripped = true;
        }
    }

    fn count(&mut self) {
        self.starts += 1;
        if self.starts > MAX_START_TAGS {
            self.tripped = true;
        }
    }
}
