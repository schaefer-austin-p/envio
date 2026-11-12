//! What each model holds right now, kept by applying every change `Ui`
//! hands out. A model attached late (or re-created) starts from it.

use std::collections::HashMap;

use crate::models::{Model, ModelChange, Op};
use crate::rows::Row;
use crate::ui::Output;

#[derive(Debug, Default)]
pub struct Mirror {
    rows: HashMap<Model, Vec<Row>>,
}

impl Mirror {
    pub fn rows(&self, model: Model) -> &[Row] {
        self.rows.get(&model).map_or(&[], Vec::as_slice)
    }

    /// Apply the model changes among `outs`, as the models will.
    pub fn track(&mut self, outs: &[Output]) {
        for out in outs {
            if let Output::Change(change) = out {
                self.apply(change);
            }
        }
    }

    fn apply(&mut self, ModelChange { model, op }: &ModelChange) {
        let rows = self.rows.entry(*model).or_default();
        let len = rows.len();
        match op {
            Op::Reset(new) => rows.clone_from(new),
            Op::Insert { at, rows: new } if *at <= len => {
                rows.splice(*at..*at, new.iter().cloned());
            }
            Op::Update { at, row } if *at < len => rows[*at] = row.clone(),
            Op::Remove { at, count } if at + count <= len => {
                rows.drain(*at..at + count);
            }
            // The glue's models ignore an op that does not fit, too.
            _ => {}
        }
    }
}
