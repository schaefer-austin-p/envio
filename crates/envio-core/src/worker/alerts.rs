//! New mail and the badge: the worker tells the app and the platform.

use envio_mime::Address;
use envio_store::AccountSettings;

use super::Worker;
use crate::alert;
use crate::api::event::{Event, NewMail};
use crate::text;

/// The addresses mail from which is the user's own, so a message they sent
/// to themselves never counts as new. The store keeps no identities: the
/// account's address is all there is.
pub(super) fn own_addresses(settings: &AccountSettings) -> Vec<Address> {
    vec![Address::new(None, &settings.email)]
}

impl Worker<'_> {
    /// Whether the user is looking at the app, so mail needs no alert.
    /// Whether the account raises alerts, so keeps Inbox IDLE in the
    /// background.
    pub(super) fn notifies(&self, account: &str) -> bool {
        !self.store.settings.notify_off.contains(account)
    }

    pub(super) fn focused(&self) -> bool {
        self.foreground
    }

    /// An Inbox sync found new mail: the app is told, and the platform
    /// shows an alert unless the user is looking or the account's switch
    /// is off.
    pub(super) fn new_mail(&self, mail: NewMail) {
        let name = self
            .store
            .accounts
            .get(&mail.account)
            .map(|a| a.settings.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(text::notice::new_mail_summary);
        // The card replaces the account's last one, so it counts all mail
        // alerted since the app was last on screen.
        let alerted = if self.focused() || !self.notifies(&mail.account) {
            mail.clone()
        } else {
            self.unseen.borrow_mut().add(&mail)
        };
        let card = alert::plan(&alerted, &name, &self.store.settings, self.focused());
        let (account, count) = (alerted.account.clone(), alerted.count);
        self.sink.emit(Event::NewMail(mail));
        if let Some(card) = card {
            self.poster.post(&card);
            log::info!("notified account {account} count {count}");
        }
    }

    /// A poster was just set: it hears the current total, unless none is
    /// known yet (the store is not open); the next refresh badges then.
    pub(super) fn poster_set(&self) {
        if !self.unread_sent.borrow().is_empty() {
            self.badge();
        }
    }

    /// The launcher badge: the Inbox unread count over all accounts.
    pub(super) fn badge(&self) {
        let total = self
            .unread_sent
            .borrow()
            .values()
            .fold(0_u32, |sum, unread| sum.saturating_add(*unread));
        self.poster.badge(total);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::alert::{Alert, Poster};
    use crate::worker::testing::{Sink, add_account, open, seed};

    #[derive(Default, Clone)]
    struct Recorder(Arc<Mutex<(Vec<Alert>, Vec<u32>)>>);

    impl Poster for Recorder {
        fn post(&self, alert: &Alert) {
            self.0.lock().unwrap().0.push(alert.clone());
        }
        fn badge(&self, total_unread: u32) {
            self.0.lock().unwrap().1.push(total_unread);
        }
    }

    fn mail(account: &str, count: u32) -> NewMail {
        NewMail {
            account: account.to_owned(),
            count,
            first: None,
        }
    }

    #[test]
    fn an_unfocused_app_posts_an_alert_and_tells_the_app() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let worker = open(&config, &sink);
        let recorder = Recorder::default();
        worker.poster.set(Box::new(recorder.clone()));
        worker.new_mail(mail(&seeded.account, 3));
        let alerts = recorder.0.lock().unwrap().0.clone();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].summary, "Test");
        assert_eq!(alerts[0].body, "3 new messages");
        assert!(
            sink.events()
                .iter()
                .any(|e| matches!(e, Event::NewMail(NewMail { count: 3, .. })))
        );
    }

    #[test]
    fn a_focused_app_posts_nothing_but_still_tells_the_app() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        worker.foreground = true;
        let recorder = Recorder::default();
        worker.poster.set(Box::new(recorder.clone()));
        worker.new_mail(mail(&seeded.account, 3));
        assert!(recorder.0.lock().unwrap().0.is_empty());
        assert!(sink.events().iter().any(|e| matches!(e, Event::NewMail(_))));
    }

    fn bodies(recorder: &Recorder) -> Vec<String> {
        let alerts = recorder.0.lock().unwrap().0.clone();
        alerts.into_iter().map(|a| a.body).collect()
    }

    #[test]
    fn separate_syncs_while_unfocused_count_all_new_mail_in_the_card() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let worker = open(&config, &sink);
        let recorder = Recorder::default();
        worker.poster.set(Box::new(recorder.clone()));
        for _ in 0..3 {
            worker.new_mail(mail(&seeded.account, 1));
        }
        assert_eq!(
            bodies(&recorder),
            ["1 new message", "2 new messages", "3 new messages"]
        );
    }

    #[test]
    fn mail_found_while_focused_does_not_count() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let recorder = Recorder::default();
        worker.poster.set(Box::new(recorder.clone()));
        worker.foreground = true;
        worker.new_mail(mail(&seeded.account, 2));
        worker.foreground = false;
        worker.new_mail(mail(&seeded.account, 1));
        assert_eq!(bodies(&recorder), ["1 new message"]);
    }

    #[test]
    fn the_count_starts_over_after_the_app_was_focused() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let recorder = Recorder::default();
        worker.poster.set(Box::new(recorder.clone()));
        worker.new_mail(mail(&seeded.account, 1));
        worker.new_mail(mail(&seeded.account, 1));
        worker.app_active(true);
        worker.app_active(false);
        worker.new_mail(mail(&seeded.account, 1));
        assert_eq!(
            bodies(&recorder),
            ["1 new message", "2 new messages", "1 new message"]
        );
    }

    #[test]
    fn turning_the_switch_off_starts_the_count_over() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let recorder = Recorder::default();
        worker.poster.set(Box::new(recorder.clone()));
        worker.new_mail(mail(&seeded.account, 2));
        worker.set_notify(&seeded.account, false);
        worker.set_notify(&seeded.account, true);
        worker.new_mail(mail(&seeded.account, 1));
        assert_eq!(bodies(&recorder), ["2 new messages", "1 new message"]);
    }

    #[test]
    fn the_badge_follows_the_summed_inbox_unread_count() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let worker = open(&config, &sink);
        let recorder = Recorder::default();
        worker.poster.set(Box::new(recorder.clone()));
        // The seeded message is unread; reading it changes the count.
        worker
            .store
            .db
            .set_flags(seeded.folder, 1, envio_store::Flags::SEEN)
            .unwrap();
        worker.emit(Event::MessagesChanged {
            folders: vec![seeded.folder],
        });
        assert_eq!(recorder.0.lock().unwrap().1, [0]);
    }

    fn badges(recorder: &Recorder) -> Vec<u32> {
        recorder.0.lock().unwrap().1.clone()
    }

    #[test]
    fn removing_the_only_account_clears_the_badge() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let recorder = Recorder::default();
        worker.poster.set(Box::new(recorder.clone()));
        worker.remove_account(&seeded.account, true);
        assert_eq!(badges(&recorder).last(), Some(&0));
    }

    #[test]
    fn removing_one_of_two_accounts_leaves_the_other_ones_count() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        add_account(&mut worker, "second", 2);
        let recorder = Recorder::default();
        worker.poster.set(Box::new(recorder.clone()));
        worker.refresh_unread();
        assert_eq!(badges(&recorder), [3]);
        worker.remove_account(&seeded.account, true);
        assert_eq!(badges(&recorder), [3, 2]);
    }

    #[test]
    fn a_new_poster_is_told_the_current_total_in_order_with_later_changes() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let recorder = Recorder::default();
        worker.poster.set(Box::new(recorder.clone()));
        runtime.block_on(worker.input(crate::worker::Input::PosterSet));
        assert_eq!(badges(&recorder), [1]);
        worker
            .store
            .db
            .set_flags(seeded.folder, 1, envio_store::Flags::SEEN)
            .unwrap();
        worker.emit(Event::MessagesChanged {
            folders: vec![seeded.folder],
        });
        assert_eq!(badges(&recorder), [1, 0]);
    }

    #[test]
    fn a_new_poster_gets_no_badge_while_no_total_is_known() {
        let dir = tempfile::tempdir().unwrap();
        let (config, _seeded) = seed(dir.path());
        let sink = Sink::default();
        let worker = open(&config, &sink);
        worker.unread_sent.borrow_mut().clear();
        let recorder = Recorder::default();
        worker.poster.set(Box::new(recorder.clone()));
        worker.poster_set();
        assert!(badges(&recorder).is_empty());
    }

    #[test]
    fn a_restarted_worker_tells_the_poster_the_current_total() {
        let dir = tempfile::tempdir().unwrap();
        let (config, _seeded) = seed(dir.path());
        let sink = Sink::default();
        let slot = Arc::new(crate::alert::PosterSlot::default());
        let recorder = Recorder::default();
        slot.set(Box::new(recorder.clone()));
        // A worker the supervisor starts after a panic: it announces itself
        // to the poster that is already set.
        let _worker = crate::worker::testing::open_with(&config, &sink, &slot);
        assert_eq!(badges(&recorder), [1]);
    }

    #[derive(Clone)]
    struct PanicsOnce(Arc<Mutex<Vec<u32>>>, Arc<std::sync::atomic::AtomicBool>);

    impl Poster for PanicsOnce {
        fn post(&self, _: &Alert) {}
        fn badge(&self, total: u32) {
            assert!(
                self.1.swap(true, std::sync::atomic::Ordering::SeqCst),
                "poster failed"
            );
            self.0.lock().unwrap().push(total);
        }
    }

    #[test]
    fn a_panicking_poster_does_not_end_alerts() {
        let slot = crate::alert::PosterSlot::default();
        let poster = PanicsOnce(Arc::default(), Arc::default());
        slot.set(Box::new(poster.clone()));
        let first = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| slot.badge(1)));
        assert!(first.is_err());
        // The same poster still gets the next call, and a new one can be set.
        slot.badge(2);
        assert_eq!(*poster.0.lock().unwrap(), [2]);
        let recorder = Recorder::default();
        slot.set(Box::new(recorder.clone()));
        slot.badge(3);
        assert_eq!(badges(&recorder), [3]);
    }

    #[test]
    fn mail_for_an_account_without_a_name_still_has_a_summary() {
        let dir = tempfile::tempdir().unwrap();
        let (config, _) = seed(dir.path());
        let sink = Sink::default();
        let worker = open(&config, &sink);
        let recorder = Recorder::default();
        worker.poster.set(Box::new(recorder.clone()));
        worker.new_mail(mail("gone", 2));
        let alerts = recorder.0.lock().unwrap().0.clone();
        assert!(!alerts[0].summary.is_empty());
    }

    #[test]
    fn the_own_address_is_the_accounts_email() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let worker = open(&config, &sink);
        let account = worker.store.accounts.get(&seeded.account).unwrap();
        let own = own_addresses(&account.settings);
        assert_eq!(own.len(), 1);
        assert_eq!(own[0].email, "me@example.org");
    }

    #[test]
    fn a_restarted_worker_keeps_the_last_known_app_state() {
        let dir = tempfile::tempdir().unwrap();
        let (mut config, _) = seed(dir.path());
        config.foreground = true;
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        worker.app_active(false);
        let slot = Arc::clone(&worker.poster);
        drop(worker);
        // The supervisor opens the next worker with the same slot.
        let again = crate::worker::testing::open_with(&config, &sink, &slot);
        assert!(!again.foreground);
    }
}
