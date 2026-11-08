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
}

/// The model of the tree builder's stack. Foreign elements only ever sit at
/// the top: anything that would put HTML inside svg or math trips the guard.
#[derive(Default)]
struct Model {
    stack: Vec<Open>,
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
            _ if VOID.contains(&name) => {}
            _ => {
                self.imply_end(name);
                self.push(tag.name.clone(), Ns::Html, false);
                return raw_text(name);
            }
        }
        TokenSinkResult::Continue
    }

    /// A foreign start tag: `/>` really closes it.
    fn open(&mut self, tag: &Tag, ns: Ns) {
        if tag.self_closing {
            self.count();
        } else {
            let integration = match ns {
                Ns::Svg => matches!(&*tag.name, "foreignobject" | "desc" | "title"),
                Ns::Math => matches!(
                    &*tag.name,
                    "mi" | "mo" | "mn" | "ms" | "mtext" | "annotation-xml"
                ),
                Ns::Html => false,
            };
            self.push(tag.name.clone(), ns, integration);
        }
    }

    /// The end tags a start tag implies, but only where html5ever surely
    /// pops at least as much.
    fn imply_end(&mut self, name: &str) {
        let (targets, through): (&[&str], &[&str]) = match name {
            "li" => (&["li"], &["p"]),
            "dd" | "dt" => (&["dd", "dt"], &["p"]),
            "p" => (&["p"], &["li", "dd", "dt", "tr"]),
            "td" | "th" => (&["td", "th"], &["p", "li", "dd", "dt"]),
            "tr" => (&["tr"], &["td", "th", "p", "li", "dd", "dt"]),
            "option" | "optgroup" => (&["option"], &[]),
            _ => return,
        };
        self.pop_through(targets, through);
        if matches!(name, "li" | "dd" | "dt") && self.top_is("p") {
            // html5ever closes a p in button scope before inserting these.
            self.stack.pop();
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

    /// html5ever's foreign end tag: walk down through foreign elements and
    /// pop through the first one of that name. Past them it would apply
    /// the HTML rules with svg or math still open, which we do not model.
    fn end_foreign(&mut self, name: &str) {
        let foreign = self
            .stack
            .iter()
            .rev()
            .take_while(|open| open.ns != Ns::Html)
            .count();
        let base = self.stack.len() - foreign;
        if let Some(index) = self.stack[base..]
            .iter()
            .rposition(|open| &*open.name == name)
        {
            self.stack.truncate(base + index);
            return;
        }
        let heading = |n: &str| matches!(n, "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
        if self.stack[..base]
            .iter()
            .any(|open| &*open.name == name || (heading(name) && heading(&open.name)))
        {
            self.tripped = true;
        }
    }

    fn end_html(&mut self, name: &str) {
        if self.top_is(name) {
            self.stack.pop();
            return;
        }
        let through: &[&str] = match name {
            "p" => &["li", "dd", "dt", "tr"],
            "li" => &["p", "dd", "dt", "tr"],
            "dd" | "dt" => &["p", "li", "dd", "dt", "tr"],
            "td" | "th" => &["p", "li", "dd", "dt"],
            "tr" => &["td", "th", "p", "li", "dd", "dt"],
            "table" => &["p", "li", "dd", "dt", "td", "th", "tr"],
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
            if open.ns != Ns::Html {
                return;
            }
            if targets.contains(&&*open.name) {
                self.stack.truncate(index);
                return;
            }
            if !through.contains(&&*open.name) {
                return;
            }
        }
    }

    /// html5ever pops foreign elements back to an HTML one. Stopping at an
    /// integration point instead would build HTML inside it: trip.
    fn leave_foreign(&mut self) -> bool {
        while let Some(top) = self.stack.last() {
            if top.ns == Ns::Html {
                break;
            }
            if top.integration {
                self.tripped = true;
                return false;
            }
            self.stack.pop();
        }
        true
    }

    fn top_is(&self, name: &str) -> bool {
        self.stack
            .last()
            .is_some_and(|open| open.ns == Ns::Html && &*open.name == name)
    }

    fn push(&mut self, name: LocalName, ns: Ns, integration: bool) {
        self.count();
        self.stack.push(Open {
            name,
            ns,
            integration,
        });
        if self.stack.len() > MAX_DEPTH {
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
