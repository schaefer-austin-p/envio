//! The screens' state, outside Qt. `Ui::apply` turns core events into model
//! changes and one-shot signals; the UI calls become commands. `qt/` only
//! mirrors the outputs onto its models and signals.

mod convert;
mod dates;
mod heading;
mod link;
mod lists;
mod mirror;
mod models;
mod oauth_names;
mod rows;
mod signals;
mod ui;

pub use dates::{DateBucket, bucket};
pub use link::{LinkView, link_view};
pub use models::{Model, ModelChange, Op};
pub use oauth_names::{
    KIND_GRAPH, KIND_IMAP, KIND_NAMES, OUTCOME_NAMES, kind_name, outcome_name, parse_kind,
};
pub use rows::{AccountRow, FolderRowUi, MessageRowUi, OutboxRowUi, Row, RowValue, SuggestionRow};
pub use signals::{OlderRow, Signal};
pub use ui::{Output, Ui};
