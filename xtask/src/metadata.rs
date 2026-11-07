//! The workspace as `affected` sees it, from `cargo metadata`.

use std::collections::BTreeSet;

use crate::affected::Package;

/// Workspace members with their directories relative to the workspace
/// root and their dependencies on other members.
pub fn packages(meta: &cargo_metadata::Metadata) -> Vec<Package> {
    let root = meta.workspace_root.as_std_path();
    let members: Vec<_> = meta
        .packages
        .iter()
        .filter(|p| meta.workspace_members.contains(&p.id))
        .collect();
    let names: BTreeSet<String> = members.iter().map(|p| p.name.to_string()).collect();
    members
        .iter()
        .map(|p| Package {
            name: p.name.to_string(),
            dir: p
                .manifest_path
                .parent()
                .map(|d| {
                    d.as_std_path()
                        .strip_prefix(root)
                        .unwrap_or(d.as_std_path())
                        .to_path_buf()
                })
                .unwrap_or_default(),
            deps: p
                .dependencies
                .iter()
                .map(|d| d.name.to_string())
                .filter(|n| names.contains(n))
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_this_workspace() {
        let meta = cargo_metadata::MetadataCommand::new()
            .no_deps()
            .exec()
            .unwrap();
        let pkgs = packages(&meta);
        let core = pkgs.iter().find(|p| p.name == "envio-core").unwrap();
        assert_eq!(core.dir, std::path::Path::new("crates/envio-core"));
        let xtask = pkgs.iter().find(|p| p.name == "xtask").unwrap();
        assert!(
            xtask.deps.is_empty(),
            "outside crates are not members: {:?}",
            xtask.deps
        );
    }
}
