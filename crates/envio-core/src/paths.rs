//! Where the app keeps its files: the folders confinement allows, named
//! after the app id.

use std::path::PathBuf;

pub const APP_ID: &str = "envio.aps";

/// `(cache, data)` folders for the app. `env` looks up an environment
/// variable (pass `&|name| std::env::var(name).ok()`). An unset or empty
/// `XDG_CACHE_HOME` / `XDG_DATA_HOME` falls back to `$HOME/.cache` /
/// `$HOME/.local/share`.
pub fn app_dirs(env: &dyn Fn(&str) -> Option<String>) -> (PathBuf, PathBuf) {
    let get = |name: &str| env(name).filter(|v| !v.is_empty());
    let home = get("HOME").unwrap_or_default();
    let base = |var: &str, under_home: &str| {
        get(var).map_or_else(|| PathBuf::from(&home).join(under_home), PathBuf::from)
    };
    (
        base("XDG_CACHE_HOME", ".cache").join(APP_ID),
        base("XDG_DATA_HOME", ".local/share").join(APP_ID),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn xdg_vars_win() {
        let get = env(&[
            ("HOME", "/home/u"),
            ("XDG_CACHE_HOME", "/c"),
            ("XDG_DATA_HOME", "/d"),
        ]);
        let (cache, data) = app_dirs(&get);
        assert_eq!(cache, PathBuf::from("/c/envio.aps"));
        assert_eq!(data, PathBuf::from("/d/envio.aps"));
    }

    #[test]
    fn home_defaults() {
        let get = env(&[("HOME", "/home/u"), ("XDG_CACHE_HOME", "")]);
        let (cache, data) = app_dirs(&get);
        assert_eq!(cache, PathBuf::from("/home/u/.cache/envio.aps"));
        assert_eq!(data, PathBuf::from("/home/u/.local/share/envio.aps"));
    }
}
