//! What the front end may know before the worker has started.

use std::path::{Path, PathBuf};

use envio_store::{Settings, StorePaths, peek_settings};

/// The settings saved in `data_dir` (the data folder of `paths::app_dirs`),
/// for the first frame: the look, dark mode and Fullscreen are in place
/// before the worker's `Event::Settings` arrives (#463). A small file read
/// on the calling thread. Read-only; a missing or unreadable file gives the
/// defaults, as the worker does (which also sets a bad file aside).
pub fn saved_settings(data_dir: &Path) -> Settings {
    peek_settings(&StorePaths::new(PathBuf::new(), data_dir).settings())
}

#[cfg(test)]
mod tests {
    use super::*;
    use envio_store::{DarkMode, ThemeName, load_settings, save_settings};

    fn file(dir: &Path) -> PathBuf {
        StorePaths::new(dir.join("cache"), dir).settings()
    }

    #[test]
    fn first_run_is_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(saved_settings(dir.path()), Settings::default());
    }

    #[test]
    fn the_saved_look_dark_mode_and_fullscreen_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let saved = Settings {
            theme: ThemeName::Airmail,
            dark: DarkMode::Dark,
            fullscreen: true,
            sync_window_days: 90,
            ..Settings::default()
        };
        save_settings(&file(dir.path()), &saved).unwrap();
        let got = saved_settings(dir.path());
        assert_eq!(got.theme, ThemeName::Airmail);
        assert_eq!(got.dark, DarkMode::Dark);
        assert!(got.fullscreen);
        // The worker loads the same file, from the same path, the same way.
        assert_eq!(got, load_settings(&file(dir.path())).unwrap());
    }

    #[test]
    fn a_corrupt_file_is_the_defaults_and_stays_for_the_worker_to_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(file(dir.path()), b"garbage").unwrap();
        assert_eq!(saved_settings(dir.path()), Settings::default());
        assert!(file(dir.path()).exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
