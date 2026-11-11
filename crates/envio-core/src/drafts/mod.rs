//! Drafts outside the worker: the book of what is saved and pending, and the
//! thread that saves it. Saving never waits for a network step and survives
//! a worker panic.

mod book;
mod keeper;

pub(crate) use book::DraftBook;
pub(crate) use keeper::DraftKeeper;
