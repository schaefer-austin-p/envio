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
        Command::AppActive(active) => {
            worker.app_active(active);
            Flow::Continue
        }
        Command::Refresh { account } => {
            worker.refresh(account.as_deref());
            Flow::Continue
        }
        #[cfg(feature = "test-hooks")]
        Command::Crash => panic!("test crash"),
    }
}
