//! Pure planners: server state and cache state in, steps out. No I/O, so
//! each is a table test.

pub mod folders;
pub mod outbox;
pub mod pending;
pub mod replay;
pub mod sync;
