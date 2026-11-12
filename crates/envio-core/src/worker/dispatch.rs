//! One match arm per command, each calling a handler in its own module.

use super::Worker;
use crate::api::command::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flow {
    Continue,
    Stop,
}

/// `at_ms` is when the command was sent.
pub(crate) async fn handle(worker: &mut Worker<'_>, command: Command, at_ms: i64) -> Flow {
    match command {
        Command::Shutdown => Flow::Stop,
        Command::AppActive(active) => {
            worker.app_active(active);
            Flow::Continue
        }
        Command::Refresh { account } => {
            worker.refresh(account.as_deref());
            Flow::Continue
        }
        Command::Detect { email } => {
            worker.detect(email);
            Flow::Continue
        }
        Command::Connect(request) => {
            worker.connect(*request);
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
        Command::ListMessages { list, after } => {
            worker.list_messages(list, after);
            Flow::Continue
        }
        Command::Search { scope, query } => {
            worker.search(scope, query);
            Flow::Continue
        }
        Command::SearchServer { scope, query } => {
            worker.search_server(scope, query);
            Flow::Continue
        }
        Command::LoadOlder { list } => {
            worker.load_older(list);
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
            worker.undo_action(undo, at_ms);
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
        // `Core::send` gives these to the draft thread; one that got here
        // anyway is not saved from here.
        Command::DraftChanged { .. } | Command::Attach { .. } => {
            log::warn!("{} reached the worker; dropped", command.name());
            Flow::Continue
        }
        Command::CloseCompose { compose, .. } => {
            worker.forget_waiting(compose);
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
        Command::Autocomplete { query } => {
            worker.autocomplete(query);
            Flow::Continue
        }
        Command::AddContact { name, address } => {
            worker.add_contact(&name, &address);
            Flow::Continue
        }
        Command::SaveSettings(mut settings) => {
            // The app builds this from the settings it last heard; the
            // allowed senders change only by Allow/DisallowImagesFrom, so a
            // save made before their echo arrived keeps the current ones.
            settings
                .images_from
                .clone_from(&worker.store.settings.images_from);
            worker.save_settings(settings);
            Flow::Continue
        }
        Command::AllowImagesFrom { address } => {
            worker.allow_images_from(&address);
            Flow::Continue
        }
        Command::DisallowImagesFrom { address } => {
            worker.disallow_images_from(&address);
            Flow::Continue
        }
        #[cfg(feature = "test-hooks")]
        Command::Crash => panic!("test crash"),
        #[cfg(feature = "test-hooks")]
        Command::Stall { ms } => {
            std::thread::sleep(std::time::Duration::from_millis(ms));
            Flow::Continue
        }
    }
}
