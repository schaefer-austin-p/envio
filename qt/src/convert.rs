//! Plain conversions between the words and numbers QML passes and
//! envio-core's types. No Qt types, no decisions; the JSON is in `json`.

use envio_core::{
    ComposeId, CountKind, DarkMode, FolderId, ImagesDefault, ListRef, MessageRowId, OutboxDecision,
    SearchScope, Service, Settings, StartFrom, ThemeName, UserAction,
};

pub fn service_word(service: Service) -> &'static str {
    match service {
        Service::Imap => "imap",
        Service::Smtp => "smtp",
    }
}

pub fn service_of(word: &str) -> Option<Service> {
    match word {
        "imap" => Some(Service::Imap),
        "smtp" => Some(Service::Smtp),
        _ => None,
    }
}

/// `act` takes "archive", "delete", "move", "read", "unread", "flag",
/// "unflag"; anything else is dropped with a log line.
pub fn user_action(word: &str, folder: i64) -> Option<UserAction> {
    let action = match word {
        "archive" => UserAction::Archive,
        "delete" => UserAction::Delete,
        "move" => UserAction::Move {
            to: FolderId(folder),
        },
        "read" => UserAction::MarkRead(true),
        "unread" => UserAction::MarkRead(false),
        "flag" => UserAction::Flag(true),
        "unflag" => UserAction::Flag(false),
        _ => {
            log::warn!("ignored unknown action {word:?}");
            return None;
        }
    };
    Some(action)
}

/// "inboxes" or "folder" (with its id).
pub fn list_ref(kind: &str, id: i64) -> Option<ListRef> {
    match kind {
        "inboxes" => Some(ListRef::Inboxes),
        "folder" => Some(ListRef::Folder(FolderId(id))),
        _ => {
            log::warn!("ignored unknown list kind {kind:?}");
            None
        }
    }
}

/// "all", "account:<key>" or "folder:<id>".
pub fn search_scope(text: &str) -> Option<SearchScope> {
    if text == "all" {
        return Some(SearchScope::All);
    }
    if let Some(key) = text.strip_prefix("account:") {
        return Some(SearchScope::Account(key.to_owned()));
    }
    if let Some(id) = text.strip_prefix("folder:").and_then(|i| i.parse().ok()) {
        return Some(SearchScope::Folder(FolderId(id)));
    }
    log::warn!("ignored unknown search scope {text:?}");
    None
}

/// "new" (account), "reply", "replyall", "forward", "edit" (message id), "draft"
/// (the stored draft id in `extra`) or "mailto" (the URL in `extra`).
/// Returns the start and the command's `draft_id` and `mailto`.
pub fn start_from(
    kind: &str,
    id: i64,
    account: &str,
    extra: &str,
) -> Option<(StartFrom, Option<String>, Option<String>)> {
    let from = match kind {
        "new" => StartFrom::New {
            account: account.to_owned(),
        },
        "reply" => StartFrom::Reply {
            id: MessageRowId(id),
            all: false,
        },
        "replyall" => StartFrom::Reply {
            id: MessageRowId(id),
            all: true,
        },
        "forward" => StartFrom::Forward {
            id: MessageRowId(id),
        },
        "edit" => StartFrom::Edit {
            id: MessageRowId(id),
        },
        "draft" => return Some((StartFrom::Draft, Some(extra.to_owned()), None)),
        "mailto" => return Some((StartFrom::Mailto, None, Some(extra.to_owned()))),
        _ => {
            log::warn!("ignored unknown compose kind {kind:?}");
            return None;
        }
    };
    Some((from, None, None))
}

pub fn decision(word: &str) -> Option<OutboxDecision> {
    match word {
        "sendagain" => Some(OutboxDecision::SendAgain),
        "movetosent" => Some(OutboxDecision::MoveToSent),
        "retry" => Some(OutboxDecision::Retry),
        "delete" => Some(OutboxDecision::Delete),
        _ => {
            log::warn!("ignored unknown outbox decision {word:?}");
            None
        }
    }
}

/// "selected", "results", "waiting", "newmail" or "keptdays".
pub fn count_kind(word: &str) -> Option<CountKind> {
    match word {
        "selected" => Some(CountKind::Selected),
        "results" => Some(CountKind::Results),
        "waiting" => Some(CountKind::Waiting),
        "newmail" => Some(CountKind::NewMail),
        "keptdays" => Some(CountKind::KeptDays),
        _ => None,
    }
}

pub fn compose_id(id: i64) -> ComposeId {
    ComposeId(u64::try_from(id).unwrap_or(0))
}

pub fn theme_word(t: ThemeName) -> &'static str {
    match t {
        ThemeName::Carbon => "carbon",
        ThemeName::Airmail => "airmail",
    }
}

pub fn dark_word(d: DarkMode) -> &'static str {
    match d {
        DarkMode::System => "system",
        DarkMode::Light => "light",
        DarkMode::Dark => "dark",
    }
}

/// The values the Settings pages edit.
pub struct SettingsEdit<'a> {
    pub theme: &'a str,
    pub dark: &'a str,
    pub images_always: bool,
    pub default_account: &'a str,
    pub sync_window_days: u32,
}

/// The settings with the edited values replaced; unknown words keep what
/// was there.
pub fn settings_with(current: &Settings, edit: &SettingsEdit) -> Settings {
    Settings {
        theme: match edit.theme {
            "airmail" => ThemeName::Airmail,
            "carbon" => ThemeName::Carbon,
            _ => current.theme,
        },
        dark: match edit.dark {
            "light" => DarkMode::Light,
            "dark" => DarkMode::Dark,
            "system" => DarkMode::System,
            _ => current.dark,
        },
        images: if edit.images_always {
            ImagesDefault::Always
        } else {
            ImagesDefault::Ask
        },
        default_account: (!edit.default_account.is_empty())
            .then(|| edit.default_account.to_owned()),
        sync_window_days: edit.sync_window_days,
        ..current.clone()
    }
}

/// The `olderRow` property: 0 hidden, 1 ready, 2 loading, 3 done.
pub fn older_row_code(row: envio_ui::OlderRow) -> i32 {
    match row {
        envio_ui::OlderRow::Hidden => 0,
        envio_ui::OlderRow::Ready => 1,
        envio_ui::OlderRow::Loading => 2,
        envio_ui::OlderRow::Done => 3,
    }
}

/// The sync window choices, in days (0 is all mail); anything else is
/// refused.
pub fn sync_window_choice(days: i32) -> Option<u32> {
    matches!(days, 0 | 30 | 90 | 365).then(|| u32::try_from(days).unwrap_or(30))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_action_is_ignored_with_a_log() {
        assert_eq!(user_action("archive", 0), Some(UserAction::Archive));
        assert_eq!(
            user_action("move", 7),
            Some(UserAction::Move { to: FolderId(7) })
        );
        assert_eq!(user_action("read", 0), Some(UserAction::MarkRead(true)));
        assert_eq!(user_action("unflag", 0), Some(UserAction::Flag(false)));
        assert_eq!(user_action("explode", 0), None);
        assert_eq!(list_ref("sideways", 1), None);
        assert_eq!(decision("shrug"), None);
        assert_eq!(search_scope("folder:x"), None);
    }

    #[test]
    fn edit_kind_starts_from_a_message_row() {
        let (from, draft_id, mailto) = start_from("edit", 7, "", "").unwrap();
        assert_eq!(
            from,
            StartFrom::Edit {
                id: MessageRowId(7)
            }
        );
        assert_eq!((draft_id, mailto), (None, None));
    }

    #[test]
    fn sync_window_takes_only_the_four_choices() {
        for days in [0, 30, 90, 365] {
            assert_eq!(sync_window_choice(days), Some(days as u32));
        }
        assert_eq!(sync_window_choice(7), None);
        assert_eq!(sync_window_choice(-1), None);
    }
}
