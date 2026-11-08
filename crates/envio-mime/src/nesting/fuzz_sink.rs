//! A `TreeSink` that records html5ever's real tree, to measure its depth.

use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;

use html5ever::interface::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::{Attribute, ExpandedName, QualName, local_name, ns, parse_fragment};

/// A node id, plus the element name when it is an element.
#[derive(Clone)]
pub(super) struct Handle {
    id: usize,
    name: Option<Rc<QualName>>,
}

struct Node {
    parent: Option<usize>,
    children: Vec<usize>,
}

#[derive(Default)]
struct Dom {
    nodes: RefCell<Vec<Node>>,
    /// Deepest a node has been while attached, even if removed later.
    max_live: RefCell<usize>,
}

impl Dom {
    fn new_node(&self, name: Option<QualName>) -> Handle {
        let mut nodes = self.nodes.borrow_mut();
        nodes.push(Node {
            parent: None,
            children: Vec::new(),
        });
        Handle {
            id: nodes.len() - 1,
            name: name.map(Rc::new),
        }
    }

    fn detach(&self, child: usize) {
        let mut nodes = self.nodes.borrow_mut();
        if let Some(parent) = nodes[child].parent.take() {
            nodes[parent].children.retain(|&c| c != child);
        }
    }

    fn attach(&self, parent: usize, child: usize, before: Option<usize>) {
        self.detach(child);
        let mut nodes = self.nodes.borrow_mut();
        nodes[child].parent = Some(parent);
        match before.and_then(|b| nodes[parent].children.iter().position(|&c| c == b)) {
            Some(i) => nodes[parent].children.insert(i, child),
            None => nodes[parent].children.push(child),
        }
        drop(nodes);
        let depth = self.depth(child);
        let mut max = self.max_live.borrow_mut();
        *max = (*max).max(depth);
    }

    fn depth(&self, mut node: usize) -> usize {
        let nodes = self.nodes.borrow();
        let mut depth = 0;
        while let Some(parent) = nodes[node].parent {
            depth += 1;
            node = parent;
        }
        depth
    }

    fn max_depth(&self) -> usize {
        let nodes = self.nodes.borrow();
        let mut best = 0;
        let mut stack = vec![(0usize, 0usize)];
        while let Some((id, depth)) = stack.pop() {
            best = best.max(depth);
            stack.extend(nodes[id].children.iter().map(|&c| (c, depth + 1)));
        }
        best
    }
}

impl TreeSink for Dom {
    type Handle = Handle;
    /// Depth of the final tree, and the deepest any node was while live.
    type Output = (usize, usize);
    type ElemName<'a> = ExpandedName<'a>;

    fn finish(self) -> (usize, usize) {
        (self.max_depth(), *self.max_live.borrow())
    }
    fn parse_error(&self, _: Cow<'static, str>) {}
    fn get_document(&self) -> Handle {
        Handle { id: 0, name: None }
    }
    fn elem_name<'a>(&'a self, target: &'a Handle) -> ExpandedName<'a> {
        match &target.name {
            Some(name) => name.expanded(),
            None => unreachable!("html5ever asked for the name of a non-element"),
        }
    }
    fn create_element(&self, name: QualName, _: Vec<Attribute>, _: ElementFlags) -> Handle {
        self.new_node(Some(name))
    }
    fn create_comment(&self, _: StrTendril) -> Handle {
        self.new_node(None)
    }
    fn create_pi(&self, _: StrTendril, _: StrTendril) -> Handle {
        self.new_node(None)
    }
    fn append(&self, parent: &Handle, child: NodeOrText<Handle>) {
        if let NodeOrText::AppendNode(child) = child {
            self.attach(parent.id, child.id, None);
        }
    }
    fn append_based_on_parent_node(&self, e: &Handle, prev: &Handle, child: NodeOrText<Handle>) {
        let has_parent = self.nodes.borrow()[e.id].parent.is_some();
        if has_parent {
            self.append_before_sibling(e, child);
        } else {
            self.append(prev, child);
        }
    }
    fn append_doctype_to_document(&self, _: StrTendril, _: StrTendril, _: StrTendril) {}
    fn get_template_contents(&self, target: &Handle) -> Handle {
        target.clone()
    }
    fn same_node(&self, x: &Handle, y: &Handle) -> bool {
        x.id == y.id
    }
    fn set_quirks_mode(&self, _: QuirksMode) {}
    fn append_before_sibling(&self, sibling: &Handle, child: NodeOrText<Handle>) {
        if let NodeOrText::AppendNode(child) = child {
            let parent = self.nodes.borrow()[sibling.id].parent;
            if let Some(parent) = parent {
                self.attach(parent, child.id, Some(sibling.id));
            }
        }
    }
    fn add_attrs_if_missing(&self, _: &Handle, _: Vec<Attribute>) {}
    fn remove_from_parent(&self, target: &Handle) {
        self.detach(target.id);
    }
    fn reparent_children(&self, node: &Handle, new_parent: &Handle) {
        let kids = std::mem::take(&mut self.nodes.borrow_mut()[node.id].children);
        for kid in kids {
            self.nodes.borrow_mut()[kid].parent = None;
            self.attach(new_parent.id, kid, None);
        }
    }
}

/// How deep html5ever really nests `html` as a fragment in a `div`:
/// `(depth of the final tree, deepest any node was while live)`, both
/// counting the document root and the fragment's `html` element.
pub(super) fn real_depth(html: &str) -> (usize, usize) {
    let dom = Dom::default();
    dom.new_node(None); // the document, id 0
    parse_fragment(
        dom,
        Default::default(),
        QualName::new(None, ns!(html), local_name!("div")),
        Vec::new(),
        false,
    )
    .one(StrTendril::from_slice(html))
}
