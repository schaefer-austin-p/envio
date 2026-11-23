//! What belongs to the user rather than to an account: settings, contacts,
//! the addresses seen, and who may load remote images. Settings and contacts
//! are files outside the cache; the addresses seen are a cache table.

use envio_mime::Address;
use envio_store::{Contact, FolderRole, ImagesDefault, Settings, save_contacts, save_settings};

use super::Worker;
use crate::api::command::{DraftInput, ImagesChoice};
use crate::api::event::Event;
use crate::api::view::Suggestion;
use crate::clock::now_ms;
use crate::text;

/// Most suggestions one `Autocomplete` answers.
const SUGGESTIONS: usize = 8;

/// Whether `contact` matches what the user typed: the address, or any word
/// of the name, starts with it (ASCII case ignored).
fn matches(contact: &Contact, query: &str) -> bool {
    let starts = |text: &str| text.to_lowercase().starts_with(query);
    starts(&contact.address) || contact.name.split_whitespace().any(starts)
}

/// Whether remote images load for a sender, with no word from the user.
/// An allowed sender counts only when `verified` (the server vouched for
/// the From address): a forged From must not earn the allow.
pub(super) fn loads_images(
    settings: &Settings,
    choice: ImagesChoice,
    sender: &str,
    verified: bool,
) -> bool {
    match choice {
        ImagesChoice::Load => true,
        ImagesChoice::Default => {
            settings.images == ImagesDefault::Always
                || (verified && settings.images_from.contains(&sender.trim().to_lowercase()))
        }
    }
}

/// The sender is a saved contact or an allowed image sender, and the
/// server vouched for the From address (`verified`).
pub(super) fn is_known(
    settings: &Settings,
    contacts: &[Contact],
    sender: &str,
    verified: bool,
) -> bool {
    if !verified {
        return false;
    }
    let sender = sender.trim().to_lowercase();
    settings.images_from.contains(&sender)
        || contacts
            .iter()
            .any(|c| c.address.trim().eq_ignore_ascii_case(&sender))
}

impl Worker<'_> {
    /// Unread messages in the account's Inbox.
    pub(super) fn inbox_unread(&self, account: &str) -> u32 {
        let counts = self.store.db.unread_counts(account);
        let folders = self.store.db.folders(account);
        match (counts, folders) {
            (Ok(counts), Ok(folders)) => folders
                .iter()
                .find(|f| f.role == FolderRole::Inbox)
                .and_then(|f| counts.get(&f.id).copied())
                .unwrap_or(0),
            (Err(e), _) => {
                log::warn!("counting unread messages: {e}");
                0
            }
            (_, Err(e)) => {
                log::warn!("listing folders to count unread: {e}");
                0
            }
        }
    }

    /// The Inbox count, remembered as what the app was told.
    pub(super) fn seed_unread(&self, account: &str) -> u32 {
        let unread = self.inbox_unread(account);
        self.unread_sent
            .borrow_mut()
            .insert(account.to_owned(), unread);
        unread
    }

    /// Tells the app each account's Inbox count that changed since it was
    /// last told (an account the app has not heard of counts as 0).
    pub(super) fn refresh_unread(&self) {
        let mut changed = false;
        for account in self.store.accounts.list() {
            let unread = self.inbox_unread(&account.key);
            let known = self
                .unread_sent
                .borrow_mut()
                .insert(account.key.clone(), unread);
            if known.unwrap_or(0) != unread {
                changed = true;
                self.sink.emit(Event::AccountUnread {
                    account: account.key.clone(),
                    unread,
                });
            }
        }
        if changed {
            self.badge();
        }
    }

    pub(super) fn autocomplete(&self, query: String) {
        let typed = query.trim().to_lowercase();
        let mut items: Vec<Suggestion> = Vec::new();
        if !typed.is_empty() {
            items.extend(
                self.store
                    .contacts
                    .iter()
                    .filter(|c| matches(c, &typed))
                    .map(|c| Suggestion {
                        name: c.name.clone(),
                        address: c.address.clone(),
                        from_contacts: true,
                        last_seen: None,
                    }),
            );
            match self.store.db.suggest(&typed, SUGGESTIONS as u32) {
                Ok(seen) => {
                    for s in seen {
                        let known = items
                            .iter()
                            .any(|i| i.address.eq_ignore_ascii_case(&s.address));
                        if !known {
                            items.push(Suggestion {
                                name: s.name,
                                address: s.address,
                                from_contacts: false,
                                last_seen: Some(s.last_seen),
                            });
                        }
                    }
                }
                Err(e) => log::warn!("looking up addresses: {e}"),
            }
            items.truncate(SUGGESTIONS);
        }
        self.emit(Event::Suggestions { query, items });
    }

    pub(super) fn add_contact(&mut self, name: &str, address: &str) {
        let Some(parsed) = Address::parse(address) else {
            return self.emit(Event::Notice {
                text: text::notice::contact_not_saved(),
            });
        };
        let mut contacts = self.store.contacts.clone();
        let name = name.trim().to_owned();
        match contacts.iter_mut().find(|c| parsed.is(&c.address)) {
            Some(known) => {
                if !name.is_empty() {
                    known.name = name;
                }
            }
            None => contacts.push(Contact {
                name,
                address: parsed.email,
            }),
        }
        match save_contacts(&self.store.paths.contacts(), &contacts) {
            Ok(()) => self.store.contacts = contacts,
            Err(e) => {
                log::warn!("saving the contacts: {e}");
                self.emit(Event::Notice {
                    text: text::notice::contact_not_saved(),
                });
            }
        }
    }

    /// Writes `settings`; they take effect only if that worked. Either way
    /// the app is told what is now in effect.
    pub(super) fn save_settings(&mut self, settings: Settings) {
        match save_settings(&self.store.paths.settings(), &settings) {
            Ok(()) => {
                let window_changed =
                    settings.sync_window_days != self.store.settings.sync_window_days;
                self.store.settings = settings;
                if window_changed {
                    self.window_changed();
                }
            }
            Err(e) => {
                log::warn!("saving the settings: {e}");
                self.emit(Event::Notice {
                    text: text::notice::settings_not_saved(),
                });
            }
        }
        self.emit(Event::Settings(self.store.settings.clone()));
    }

    pub(super) fn set_notify(&mut self, account: &str, on: bool) {
        let mut settings = self.store.settings.clone();
        if on {
            settings.notify_off.remove(account);
        } else {
            settings.notify_off.insert(account.to_owned());
            self.poster.reset_unseen(account);
        }
        self.save_settings(settings);
    }

    pub(super) fn set_notify_text(&mut self, on: bool) {
        let mut settings = self.store.settings.clone();
        settings.notify_show_text = on;
        self.save_settings(settings);
    }

    pub(super) fn set_list_pane(&mut self, gu: u32) {
        let mut settings = self.store.settings.clone();
        settings.list_pane_gu = envio_store::clamp_list_pane_gu(gu.into());
        self.save_settings(settings);
    }

    pub(super) fn allow_images_from(&mut self, address: &str) {
        let mut settings = self.store.settings.clone();
        settings.images_from.insert(address.trim().to_lowercase());
        self.save_settings(settings);
    }

    pub(super) fn disallow_images_from(&mut self, address: &str) {
        let mut settings = self.store.settings.clone();
        settings.images_from.remove(&address.trim().to_lowercase());
        self.save_settings(settings);
    }

    /// Sending teaches autocomplete the people written to.
    pub(super) fn remember_recipients(&self, draft: &DraftInput) {
        let found: Vec<(String, String)> = draft
            .to
            .iter()
            .chain(&draft.cc)
            .chain(&draft.bcc)
            .filter_map(|typed| Address::parse(typed))
            .map(|a| (a.name.unwrap_or_default(), a.email))
            .collect();
        if let Err(e) = self.store.db.remember_addresses(&found, now_ms() / 1000) {
            log::warn!("remembering recipients: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(images: ImagesDefault, from: &[&str]) -> Settings {
        Settings {
            images,
            images_from: from.iter().map(|s| (*s).to_owned()).collect(),
            ..Settings::default()
        }
    }

    #[test]
    fn default_loads_images_for_a_verified_allowed_sender_or_when_always() {
        let ask = settings(ImagesDefault::Ask, &["ada@example.org"]);
        let load = |s: &Settings, c, who, v| loads_images(s, c, who, v);
        assert!(load(&ask, ImagesChoice::Default, "Ada@Example.org", true));
        assert!(!load(&ask, ImagesChoice::Default, "eve@example.org", true));
        assert!(load(&ask, ImagesChoice::Load, "eve@example.org", false));
        let always = settings(ImagesDefault::Always, &[]);
        assert!(load(
            &always,
            ImagesChoice::Default,
            "eve@example.org",
            false
        ));
    }

    #[test]
    fn an_allowed_sender_is_not_trusted_unverified() {
        let ask = settings(ImagesDefault::Ask, &["ada@example.org"]);
        assert!(!loads_images(
            &ask,
            ImagesChoice::Default,
            "ada@example.org",
            false
        ));
    }

    #[test]
    fn a_contact_or_allowed_sender_is_known_only_when_verified() {
        let contacts = [Contact {
            name: "Bob".into(),
            address: "Bob@example.org".into(),
        }];
        let s = settings(ImagesDefault::Ask, &["ada@example.org"]);
        assert!(is_known(&s, &contacts, "bob@example.org", true));
        assert!(is_known(&s, &contacts, "ADA@example.org", true));
        assert!(!is_known(&s, &contacts, "eve@example.org", true));
        assert!(!is_known(&s, &contacts, "bob@example.org", false));
        assert!(!is_known(&s, &contacts, "ADA@example.org", false));
    }

    #[test]
    fn a_contact_matches_by_address_or_any_word_of_the_name() {
        let c = Contact {
            name: "Grace Hopper".into(),
            address: "gh@navy.example".into(),
        };
        assert!(matches(&c, "gh@"));
        assert!(matches(&c, "hop"));
        assert!(!matches(&c, "race"));
    }
}
