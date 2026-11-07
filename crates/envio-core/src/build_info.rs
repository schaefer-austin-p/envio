//! Which build is running, for the About text and the start-up log.

/// What the About text and the log show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInfo {
    pub version: String,
    pub commit: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BuildInfoError {
    BadManifest(String),
    NoVersion,
}

impl BuildInfo {
    /// `manifest_json` is the click's manifest.json; `commit` is the git
    /// commit the build script found, empty when there was none.
    pub fn from_manifest(manifest_json: &str, commit: &str) -> Result<Self, BuildInfoError> {
        let manifest: serde_json::Value = serde_json::from_str(manifest_json)
            .map_err(|e| BuildInfoError::BadManifest(e.to_string()))?;
        let version = manifest
            .get("version")
            .and_then(serde_json::Value::as_str)
            .ok_or(BuildInfoError::NoVersion)?
            .to_owned();
        let commit = if commit.is_empty() { "unknown" } else { commit }.to_owned();
        Ok(Self { version, commit })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_version_and_commit() {
        let info = BuildInfo::from_manifest(r#"{"name":"envio.aps","version":"1.2.3"}"#, "abc123")
            .unwrap();
        assert_eq!(
            info,
            BuildInfo {
                version: "1.2.3".into(),
                commit: "abc123".into()
            }
        );
    }

    #[test]
    fn empty_commit_reads_unknown() {
        let info = BuildInfo::from_manifest(r#"{"version":"1.0.0"}"#, "").unwrap();
        assert_eq!(info.commit, "unknown");
    }

    #[test]
    fn missing_version_is_an_error() {
        assert_eq!(
            BuildInfo::from_manifest(r#"{"name":"x"}"#, "c"),
            Err(BuildInfoError::NoVersion)
        );
    }

    #[test]
    fn broken_json_is_an_error() {
        assert!(matches!(
            BuildInfo::from_manifest("{", "c"),
            Err(BuildInfoError::BadManifest(_))
        ));
    }

    /// The About text shows manifest.json's version; a release that bumps
    /// only Cargo.toml's (or only the manifest's) fails here.
    #[test]
    fn manifest_version_is_the_workspace_version() {
        let info = BuildInfo::from_manifest(include_str!("../../../manifest.json"), "").unwrap();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    }
}
