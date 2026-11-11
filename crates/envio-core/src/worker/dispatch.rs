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
        Command::ListFolders { account } => {
            worker.list_folders(&account);
            Flow::Continue
        }
        Command::ListMessages { folder, page } => {
            worker.list_messages(folder, page);
            Flow::Continue
        }
        Command::Search { account, query } => {
            worker.search(&account, query);
            Flow::Continue
        }
        Command::OpenMessage {
            id,
            images,
            width_px,
        } => {
            worker.open_message(id, images, width_px);
            Flow::Continue
        }
        Command::SaveAttachment { id, index, dir } => {
            worker.save_attachment(id, index, &dir);
            Flow::Continue
        }
        Command::Act { ids, action } => {
            worker.act(&ids, action);
            Flow::Continue
        }
        Command::Undo { undo } => {
            worker.undo_action(undo);
            Flow::Continue
        }
        Command::StartCompose {
            compose,
            from,
            draft_id,
            mailto,
        } => {
            worker.start_compose(compose, from, (draft_id.as_deref(), mailto.as_deref()));
            Flow::Continue
        }
        Command::DraftChanged { compose, draft } => {
            worker.draft_changed(compose, draft);
            Flow::Continue
        }
        Command::CloseCompose { compose, discard } => {
            worker.close_compose(compose, discard);
            Flow::Continue
        }
        Command::Send { compose, draft } => {
            worker.send(compose, &draft);
            Flow::Continue
        }
        Command::ListOutbox => {
            worker.emit_outbox();
            Flow::Continue
        }
        Command::DecideOutbox { item, decision } => {
            worker.decide_outbox(&item, decision);
            Flow::Continue
        }
        Command::EditOutbox { item, compose } => {
            worker.edit_outbox(&item, compose);
            Flow::Continue
        }
        #[cfg(feature = "test-hooks")]
        Command::Crash => panic!("test crash"),
    }
}
