//! The screens' state, outside Qt. `Ui::apply` turns core events into model
//! changes and one-shot signals; the UI calls become commands. `qt/` only
//! mirrors the outputs onto its models and signals.

mod convert;
mod dates;
mod link;
mod lists;
mod mirror;
mod models;
mod rows;
mod signals;
mod ui;

pub use dates::{DateBucket, bucket};
pub use link::{LinkView, link_view};
pub use models::{Model, ModelChange, Op};
pub use rows::{AccountRow, FolderRowUi, MessageRowUi, OutboxRowUi, Row, RowValue, SuggestionRow};
pub use signals::{OlderRow, Signal};
pub use ui::{Output, Ui};
