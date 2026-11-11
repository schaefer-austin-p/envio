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
        Command::Detect { email } => {
            worker.detect(email).await;
            Flow::Continue
        }
        Command::Connect(request) => {
            worker.connect(*request).await;
            Flow::Continue
        }
        Command::UpdatePassword { account, password } => {
            worker.update_password(&account, password);
            Flow::Continue
        }
        Command::AcceptCertificate {
            account,
            service,
            fingerprint,
        } => {
            worker.accept_certificate(&account, service, &fingerprint);
            Flow::Continue
        }
        Command::RemoveAccount { account, confirmed } => {
            worker.remove_account(&account, confirmed);
            Flow::Continue
        }
        #[cfg(feature = "test-hooks")]
        Command::Crash => panic!("test crash"),
    }
}
