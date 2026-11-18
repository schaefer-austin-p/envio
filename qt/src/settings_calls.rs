//! The settings invokables of `EnvioCore`: each edits a copy of the settings
//! last received and sends it to the core to save.

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use envio_core::Command;

use crate::convert as cv;
use crate::core_object::qobject;

impl qobject::EnvioCore {
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

    /// The Notifications switch of one account.
    pub fn set_notify(self: Pin<&mut Self>, account: &QString, on: bool) {
        let settings =
            envio_ui::settings_with_notify(&self.rust().settings, &String::from(account), on);
        self.send_command(Command::SaveSettings(settings));
    }

    /// "Show sender and subject" in new-mail notifications.
    pub fn set_notify_text(self: Pin<&mut Self>, on: bool) {
        let settings = envio_ui::settings_with_show_text(&self.rust().settings, on);
        self.send_command(Command::SaveSettings(settings));
    }
}
