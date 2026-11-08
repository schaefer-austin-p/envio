//! Tables in the nesting model.
//!
//! html5ever's insertion mode inside a table follows from the nearest
//! "structure" element on its stack (td or th: InCell, tr: InRow, a
//! section: InTableBody, caption, colgroup, table, template). The model
//! closes tables, sections, rows and cells exactly as those modes do: a
//! table the model kept open after html5ever closed it would later anchor
//! a pop through elements html5ever still has open.
//!
//! Table parts outside a table, or right inside a template (where what
//! html5ever does depends on the template's first child), are counted but
//! inert: they never anchor or pass a pop, which never pops too much.

use html5ever::LocalName;

use super::{Model, Ns};

/// The elements that set html5ever's table insertion modes.
const STRUCTURE: &[&str] = &[
    "td", "th", "tr", "tbody", "thead", "tfoot", "caption", "colgroup", "table", "template",
];

/// Cells, rows and sections, from the innermost: the levels an end tag may
/// close through.
const LEVELS: [&[&str]; 3] = [&["td", "th"], &["tr"], &["tbody", "thead", "tfoot"]];

impl Model {
    /// A tbody, thead, tfoot, caption or colgroup start tag. Every table
    /// mode clears the stack back to the table before inserting it.
    pub(super) fn start_section(&mut self, name: &LocalName) {
        let Some(table) = self.open_table() else {
            self.push_inert(name);
            return;
        };
        self.cut(table + 1);
        self.push(name.clone(), Ns::Html, false);
    }

    /// A table start tag: a cell or caption nests it; the other table
    /// modes close the open table first. Right in a template, html5ever
    /// may be in a table mode with no table to close and ignore it: inert.
    pub(super) fn start_table(&mut self) {
        let name = LocalName::from("table");
        if self
            .nearest_structure()
            .is_some_and(|index| &*self.stack[index].name == "template")
        {
            self.push_inert(&name);
            return;
        }
        if let Some(nearest) = self.in_table()
            && !matches!(&*self.stack[nearest].name, "td" | "th" | "caption")
            && let Some(table) = self.table_below(nearest)
        {
            self.cut(table);
        }
        self.push(name, Ns::Html, false);
    }

    /// A col start tag (void). In a table, html5ever opens a colgroup for
    /// it: counted. In a template it can switch html5ever to its
    /// column-group mode, which ignores raw-text start tags the model
    /// would follow into raw text: trip.
    pub(super) fn start_col(&mut self) {
        match self.nearest_structure() {
            Some(index) if &*self.stack[index].name == "template" => self.tripped = true,
            Some(_) if !self.top_is("colgroup") => {
                self.push(LocalName::from("colgroup"), Ns::Html, false);
            }
            _ => {}
        }
    }

    /// A td, th or tr start tag. Inside a table every mode clears the
    /// stack back to the nearest structure element, closes an open cell,
    /// caption or column group, and closes the row for a new row. Then
    /// html5ever inserts a missing tbody and tr; the model counts them
    /// unless the row or section is right on top, an upper bound.
    pub(super) fn start_table_part(&mut self, name: &LocalName) {
        let Some(nearest) = self.in_table() else {
            self.push_inert(name);
            return;
        };
        self.cut(nearest + 1);
        if ["td", "th", "caption", "colgroup"]
            .iter()
            .any(|n| self.top_is(n))
        {
            self.pop();
        }
        let is_row = &**name == "tr";
        if is_row && self.top_is("tr") {
            self.pop();
        }
        let section = ["tbody", "thead", "tfoot"].iter().any(|n| self.top_is(n));
        if !is_row && !self.top_is("tr") {
            if !section {
                self.push(LocalName::from("tbody"), Ns::Html, false);
            }
            self.push(LocalName::from("tr"), Ns::Html, false);
        } else if is_row && !section {
            self.push(LocalName::from("tbody"), Ns::Html, false);
        }
        self.push(name.clone(), Ns::Html, false);
    }

    /// The end tag of a table part or a template; false for other names.
    pub(super) fn end_table_part(&mut self, name: &str) -> bool {
        let close = match name {
            "template" => self.nearest_named("template"),
            "table" => self.open_table(),
            "caption" => self
                .in_table()
                .filter(|&nearest| &*self.stack[nearest].name == "caption"),
            "td" | "th" => self.end_through(name, 0),
            "tr" => self.end_through(name, 1),
            "tbody" | "thead" | "tfoot" => self.end_through(name, 2),
            // html5ever closes a colgroup only when it is the current
            // node; keeping it open is an upper bound.
            "colgroup" => None,
            _ => return false,
        };
        if let Some(index) = close {
            self.cut(index);
        }
        true
    }

    /// The `name` element an end tag at `level` of [`LEVELS`] closes:
    /// html5ever's cell, row and body modes close the inner levels first.
    fn end_through(&self, name: &str, level: usize) -> Option<usize> {
        let mut index = self.in_table()?;
        for names in &LEVELS[..level] {
            if names.contains(&&*self.stack[index].name) {
                index = self.structure_below(index)?;
            }
        }
        (&*self.stack[index].name == name).then_some(index)
    }

    /// The nearest structure element, unless it is a template.
    fn in_table(&self) -> Option<usize> {
        self.nearest_structure()
            .filter(|&index| &*self.stack[index].name != "template")
    }

    fn nearest_structure(&self) -> Option<usize> {
        self.structure_in(self.stack.len())
    }

    fn structure_below(&self, index: usize) -> Option<usize> {
        self.structure_in(index)
    }

    /// The last structure element in `stack[..end]`. Inert ones don't
    /// count: html5ever never inserted them.
    fn structure_in(&self, end: usize) -> Option<usize> {
        self.stack[..end].iter().rposition(|open| {
            open.ns == Ns::Html && !open.inert && STRUCTURE.contains(&&*open.name)
        })
    }

    /// The table at or below `index`, unless a template comes first.
    fn table_below(&self, index: usize) -> Option<usize> {
        let mut index = index;
        loop {
            match &*self.stack[index].name {
                "table" => return Some(index),
                "template" => return None,
                _ => index = self.structure_below(index)?,
            }
        }
    }

    /// The table html5ever's table modes act on, if one is open.
    fn open_table(&self) -> Option<usize> {
        self.in_table()
            .and_then(|nearest| self.table_below(nearest))
    }

    fn nearest_named(&self, name: &str) -> Option<usize> {
        self.stack
            .iter()
            .rposition(|open| open.ns == Ns::Html && !open.inert && &*open.name == name)
    }

    fn push_inert(&mut self, name: &LocalName) {
        self.push(name.clone(), Ns::Html, false);
        if let Some(open) = self.stack.last_mut() {
            open.inert = true;
        }
    }
}
