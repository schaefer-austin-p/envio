//! The settings invokables of `EnvioCore`: each edits a copy of the settings
//! last received and sends it to the core to save.

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use envio_core::Command;

use crate::convert as cv;
use crate::core_object::qobject;

impl qobject::EnvioCore {
    pub fn list_pane_min_gu(&self) -> i32 {
        i32::try_from(envio_core::LIST_PANE_MIN_GU).unwrap_or(i32::MAX)
    }

    pub fn list_pane_max_gu(&self) -> i32 {
        i32::try_from(envio_core::LIST_PANE_MAX_GU).unwrap_or(i32::MAX)
    }

    pub fn list_pane_default_gu(&self) -> i32 {
        i32::try_from(envio_core::LIST_PANE_DEFAULT_GU).unwrap_or(i32::MAX)
    }

    #[allow(clippy::too_many_arguments)] // the QML signature
    pub fn save_settings(
        self: Pin<&mut Self>,
        theme: &QString,
        dark_mode: &QString,
        images_always: bool,
        default_account: &QString,
    ) {
        let edit = cv::SettingsEdit {
            theme: &String::from(theme),
            dark: &String::from(dark_mode),
            images_always,
            default_account: &String::from(default_account),
            sync_window_days: self.rust().settings.sync_window_days,
        };
        let settings = cv::settings_with(&self.rust().settings, &edit);
        self.send_command(Command::SaveSettings(settings));
    }

    /// `days` is 30, 90, 365 or 0 (all mail); anything else is ignored.
    pub fn set_sync_window(self: Pin<&mut Self>, days: i32) {
        let Some(days) = cv::sync_window_choice(days) else {
            log::warn!("ignored the sync window {days}");
            return;
        };
        let settings = {
            let current = &self.rust().settings;
            let edit = cv::SettingsEdit {
                theme: cv::theme_word(current.theme),
                dark: cv::dark_word(current.dark),
                images_always: current.images == envio_core::ImagesDefault::Always,
                default_account: current.default_account.as_deref().unwrap_or(""),
                sync_window_days: days,
            };
            cv::settings_with(current, &edit)
        };
        self.send_command(Command::SaveSettings(settings));
    }

    /// The Notifications switch of one account; the core applies it to its
    /// own settings.
    pub fn set_notify(self: Pin<&mut Self>, account: &QString, on: bool) {
        self.send_command(Command::SetNotify {
            account: String::from(account),
            on,
        });
    }

    /// "Show sender and subject" in new-mail notifications.
    pub fn set_notify_text(self: Pin<&mut Self>, on: bool) {
        self.send_command(Command::SetNotifyText { on });
    }

    /// The list pane's width in grid units, from the splitter; the core
    /// clamps it and saves it.
    pub fn set_list_pane(self: Pin<&mut Self>, gu: i32) {
        self.send_command(Command::SetListPane {
            gu: u32::try_from(gu).unwrap_or(0),
        });
    }
}
