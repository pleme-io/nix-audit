//! Detect nixpkgs revision drift across multiple flake.lock files.

use crate::types::{DriftReport, FlakeLockInfo};
use std::collections::BTreeMap;

/// Detect nixpkgs revision drift across multiple repos.
///
/// Compares all nixpkgs-locked revisions across the provided flake lock infos.
/// Returns a `DriftReport` for each distinct nixpkgs node name where more than
/// one revision is in use, identifying the majority revision and divergent repos.
pub fn detect_drift(infos: &[(String, FlakeLockInfo)]) -> Vec<DriftReport> {
    // Group: nixpkgs node name -> rev -> list of repos.
    let mut by_node: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();

    for (repo_name, info) in infos {
        for node_name in &info.nixpkgs_nodes {
            if let Some(node) = info.nodes.get(node_name) {
                if let Some(ref rev) = node.rev {
                    by_node
                        .entry(node_name.clone())
                        .or_default()
                        .entry(rev.clone())
                        .or_default()
                        .push(repo_name.clone());
                }
            }
        }
    }

    let mut reports = Vec::new();

    for (node_name, revisions) in by_node {
        if revisions.len() <= 1 {
            continue;
        }

        let majority_rev = revisions
            .iter()
            .max_by_key(|(_, repos)| repos.len())
            .map(|(rev, _)| rev.clone());

        let divergent_repos = if let Some(ref majority) = majority_rev {
            revisions
                .iter()
                .filter(|(rev, _)| rev != &majority)
                .flat_map(|(_, repos)| repos.clone())
                .collect()
        } else {
            vec![]
        };

        reports.push(DriftReport {
            node_name,
            revisions,
            majority_rev,
            divergent_repos,
        });
    }

    reports
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LockedInput;

    fn make_info(repo: &str, rev: &str) -> (String, FlakeLockInfo) {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            "nixpkgs".to_string(),
            LockedInput {
                name: "nixpkgs".to_string(),
                rev: Some(rev.to_string()),
                owner: Some("NixOS".to_string()),
                repo: Some("nixpkgs".to_string()),
                original_ref: None,
                is_nixpkgs: true,
                follows_refs: vec![],
            },
        );
        (
            repo.to_string(),
            FlakeLockInfo {
                repo_name: repo.to_string(),
                version: 7,
                nodes,
                nixpkgs_nodes: vec!["nixpkgs".to_string()],
                follows: BTreeMap::new(),
            },
        )
    }

    #[test]
    fn no_drift_when_aligned() {
        let infos = vec![
            make_info("a", "rev111"),
            make_info("b", "rev111"),
            make_info("c", "rev111"),
        ];
        let reports = detect_drift(&infos);
        assert!(reports.is_empty());
    }

    #[test]
    fn drift_detected() {
        let infos = vec![
            make_info("a", "rev111"),
            make_info("b", "rev111"),
            make_info("c", "rev222"),
        ];
        let reports = detect_drift(&infos);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].majority_rev.as_deref(), Some("rev111"));
        assert_eq!(reports[0].divergent_repos, vec!["c"]);
    }

    #[test]
    fn empty_input() {
        let reports = detect_drift(&[]);
        assert!(reports.is_empty());
    }

    #[test]
    fn single_repo_no_drift() {
        let infos = vec![make_info("only", "rev111")];
        let reports = detect_drift(&infos);
        assert!(reports.is_empty());
    }
}
