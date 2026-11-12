//! The six list models and the operations that change them.

use crate::rows::Row;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Model {
    Accounts,
    Folders,
    Messages,
    Outbox,
    Search,
    Suggestions,
}

/// One change to a model, in the order a Qt list model applies it.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Reset(Vec<Row>),
    Insert { at: usize, rows: Vec<Row> },
    Update { at: usize, row: Row },
    Remove { at: usize, count: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelChange {
    pub model: Model,
    pub op: Op,
}

impl ModelChange {
    pub fn new(model: Model, op: Op) -> Self {
        Self { model, op }
    }
}
