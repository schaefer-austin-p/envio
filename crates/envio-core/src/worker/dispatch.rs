//! One match arm per command, each calling a handler in its own module.

use super::Worker;
use crate::api::command::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flow {
    Continue,
    Stop,
}

pub(crate) async fn handle(worker: &mut Worker<'_>, command: Command) -> Flow {
    match command {
        Command::Shutdown => {
            worker.flush_drafts();
            Flow::Stop
        }
        #[cfg(feature = "test-hooks")]
        Command::Crash => panic!("test crash"),
    }
}
