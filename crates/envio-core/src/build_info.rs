//! Which build is running, for the About text and the start-up log: its
//! version, and for a development build what it was made from, when it
//! was compiled and when it was installed. An official release has only
//! the version (#198).

use std::path::Path;
use std::time::UNIX_EPOCH;

/// What the About text and the log show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInfo {
    /// The click's version, from manifest.json.
    pub version: String,
    /// None in an official release.
    pub dev: Option<Dev>,
}

/// What only a development build knows about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dev {
    /// `branch@hash`, `-dirty` with uncommitted changes, or "unknown".
    pub source: String,
    /// When it was compiled, in Unix seconds.
    pub built: Option<u64>,
    /// When it was installed on this device, in Unix seconds. None
    /// outside an installed click (a desktop or test build).
    pub installed: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BuildInfoError {
    BadManifest(String),
    NoVersion,
}

impl BuildInfo {
    /// `manifest_json` is the click's manifest.json; `source` and `built`
    /// are the stamp the build left (envio-stamp), which an official
    /// release leaves out.
    pub fn from_manifest(
        manifest_json: &str,
        source: Option<&str>,
        built: Option<&str>,
    ) -> Result<Self, BuildInfoError> {
        let manifest: serde_json::Value = serde_json::from_str(manifest_json)
            .map_err(|e| BuildInfoError::BadManifest(e.to_string()))?;
        let version = manifest
            .get("version")
            .and_then(serde_json::Value::as_str)
            .ok_or(BuildInfoError::NoVersion)?
            .to_owned();
        let dev = source.map(|s| Dev {
            source: if s.is_empty() { "unknown" } else { s }.to_owned(),
            built: built.and_then(|b| b.parse().ok()),
            installed: None,
        });
        Ok(Self { version, dev })
    }

    /// Adds when the click holding `app_dir` (the binary's folder) was
    /// installed: click writes `.click/status` then. An official release
    /// doesn't show it, so it isn't read.
    pub fn installed_from(mut self, app_dir: &Path) -> Self {
        if let Some(dev) = &mut self.dev {
            dev.installed = std::fs::metadata(app_dir.join(".click/status"))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{"name":"envio.aps","version":"1.2.3"}"#;

    fn click_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".click")).unwrap();
        std::fs::write(dir.path().join(".click/status"), "").unwrap();
        dir
    }

    #[test]
    fn a_development_build_has_its_source_and_build_time() {
        let info = BuildInfo::from_manifest(
            MANIFEST,
            Some("master@0123abcdef-dirty"),
            Some("1700000000"),
        )
        .unwrap();
        assert_eq!(
            info,
            BuildInfo {
                version: "1.2.3".into(),
                dev: Some(Dev {
                    source: "master@0123abcdef-dirty".into(),
                    built: Some(1_700_000_000),
                    installed: None,
                }),
            }
        );
    }

    #[test]
    fn a_stamp_without_a_source_or_time_reads_unknown() {
        let dev = BuildInfo::from_manifest(MANIFEST, Some(""), Some("soon"))
            .unwrap()
            .dev
            .unwrap();
        assert_eq!((dev.source.as_str(), dev.built), ("unknown", None));
    }

    #[test]
    fn an_official_release_has_only_the_version() {
        let dir = click_dir();
        let info = BuildInfo::from_manifest(MANIFEST, None, Some("1700000000"))
            .unwrap()
            .installed_from(dir.path());
        assert_eq!(
            info,
            BuildInfo {
                version: "1.2.3".into(),
                dev: None
            }
        );
    }

    #[test]
    fn the_install_time_is_when_click_wrote_its_status() {
        let info = BuildInfo::from_manifest(MANIFEST, Some("x@1"), None).unwrap();
        // Outside a click there is none.
        let bare = tempfile::tempdir().unwrap();
        let outside = info.clone().installed_from(bare.path());
        assert_eq!(outside.dev.unwrap().installed, None);

        let dir = click_dir();
        let installed = info
            .installed_from(dir.path())
            .dev
            .unwrap()
            .installed
            .unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(now.abs_diff(installed) < 60, "{installed} vs {now}");
    }

    #[test]
    fn missing_version_is_an_error() {
        assert_eq!(
            BuildInfo::from_manifest(r#"{"name":"x"}"#, None, None),
            Err(BuildInfoError::NoVersion)
        );
    }

    #[test]
    fn broken_json_is_an_error() {
        assert!(matches!(
            BuildInfo::from_manifest("{", None, None),
            Err(BuildInfoError::BadManifest(_))
        ));
    }

    /// The About text shows manifest.json's version; a release that bumps
    /// only Cargo.toml's (or only the manifest's) fails here.
    #[test]
    fn manifest_version_is_the_workspace_version() {
        let info =
            BuildInfo::from_manifest(include_str!("../../../manifest.json"), None, None).unwrap();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    }
}
