//! Checker: nixpkgs revisions must align across the org for maximum cache sharing.

use crate::types::{Category, Finding, FlakeLockInfo, Severity};
use std::collections::BTreeMap;

/// Check that all repos use the same nixpkgs revision.
///
/// When repos pin different nixpkgs revisions, the binary cache cannot be
/// shared across them. This checker compares nixpkgs locked revisions across
/// all provided flake lock infos and flags repos that diverge from the majority.
pub fn check_cache_alignment(lock_infos: &[(String, FlakeLockInfo)]) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Collect all nixpkgs revisions across repos.
    let mut rev_to_repos: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (repo_name, info) in lock_infos {
        for node_name in &info.nixpkgs_nodes {
            if let Some(node) = info.nodes.get(node_name) {
                if let Some(ref rev) = node.rev {
                    rev_to_repos
                        .entry(rev.clone())
                        .or_default()
                        .push(repo_name.clone());
                }
            }
        }
    }

    if rev_to_repos.len() <= 1 {
        return findings;
    }

    // Find the majority revision.
    let majority_rev = rev_to_repos
        .iter()
        .max_by_key(|(_, repos)| repos.len())
        .map(|(rev, _)| rev.clone());

    if let Some(ref majority) = majority_rev {
        for (rev, repos) in &rev_to_repos {
            if rev != majority {
                for repo in repos {
                    let short_majority = &majority[..majority.len().min(8)];
                    let short_rev = &rev[..rev.len().min(8)];
                    findings.push(Finding {
                        category: Category::CacheAlignment,
                        severity: Severity::Warning,
                        message: format!(
                            "Repo '{repo}' uses nixpkgs rev {short_rev}.. but majority is {short_majority}.."
                        ),
                        fix: Some(format!(
                            "Run `nix flake lock --update-input nixpkgs` in {repo} to align"
                        )),
                        fixed: false,
                    });
                }
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LockedInput;

    fn make_lock_info(repo: &str, rev: &str) -> (String, FlakeLockInfo) {
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
    fn aligned_revs_pass() {
        let infos = vec![
            make_lock_info("repo-a", "abc12345deadbeef"),
            make_lock_info("repo-b", "abc12345deadbeef"),
        ];
        assert!(check_cache_alignment(&infos).is_empty());
    }

    #[test]
    fn divergent_revs_warn() {
        let infos = vec![
            make_lock_info("repo-a", "abc12345deadbeef"),
            make_lock_info("repo-b", "abc12345deadbeef"),
            make_lock_info("repo-c", "def67890cafebabe"),
        ];
        let findings = check_cache_alignment(&infos);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("repo-c"));
    }

    #[test]
    fn single_repo_passes() {
        let infos = vec![make_lock_info("repo-a", "abc12345deadbeef")];
        assert!(check_cache_alignment(&infos).is_empty());
    }

    #[test]
    fn empty_passes() {
        let infos: Vec<(String, FlakeLockInfo)> = vec![];
        assert!(check_cache_alignment(&infos).is_empty());
    }
}
