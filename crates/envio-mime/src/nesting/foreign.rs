//! The nesting model in svg and math: foreign elements only ever sit at
//! the top of the stack.

use html5ever::tokenizer::Tag;

use super::{Model, Ns};

impl Model {
    /// A foreign start tag: `/>` really closes it.
    pub(super) fn open(&mut self, tag: &Tag, ns: Ns) {
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

    /// html5ever's foreign end tag: walk down through foreign elements and
    /// pop through the first one of that name. Past them it would apply
    /// the HTML rules with svg or math still open, which we do not model.
    pub(super) fn end_foreign(&mut self, name: &str) {
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
            self.truncate(base + index);
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

    /// html5ever pops foreign elements back to an HTML one. Stopping at an
    /// integration point instead would build HTML inside it: trip.
    pub(super) fn leave_foreign(&mut self) -> bool {
        while let Some(top) = self.stack.last() {
            if top.ns == Ns::Html {
                break;
            }
            if top.integration {
                self.tripped = true;
                return false;
            }
            self.pop();
        }
        true
    }
}
