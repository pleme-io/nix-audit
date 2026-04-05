//! Count distinct nixpkgs instances in a parsed flake.lock.

use crate::types::FlakeLockInfo;

/// Count the number of distinct nixpkgs nodes in a flake.lock.
///
/// A well-structured flake should have exactly one nixpkgs input, with all
/// other inputs following it. Multiple nixpkgs nodes indicate closure
/// duplication (each brings its own copy of the nixpkgs tree).
pub fn count_nixpkgs_instances(info: &FlakeLockInfo) -> usize {
    info.nixpkgs_nodes.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LockedInput;
    use std::collections::BTreeMap;

    #[test]
    fn single_nixpkgs() {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            "nixpkgs".to_string(),
            LockedInput {
                name: "nixpkgs".to_string(),
                rev: Some("abc123".to_string()),
                owner: Some("NixOS".to_string()),
                repo: Some("nixpkgs".to_string()),
                original_ref: Some("nixos-25.11".to_string()),
                is_nixpkgs: true,
                follows_refs: vec![],
            },
        );
        let info = FlakeLockInfo {
            repo_name: "test".to_string(),
            version: 7,
            nodes,
            nixpkgs_nodes: vec!["nixpkgs".to_string()],
            follows: BTreeMap::new(),
        };
        assert_eq!(count_nixpkgs_instances(&info), 1);
    }

    #[test]
    fn multiple_nixpkgs() {
        let mut nodes = BTreeMap::new();
        for name in &["nixpkgs", "nixpkgs_2"] {
            nodes.insert(
                (*name).to_string(),
                LockedInput {
                    name: (*name).to_string(),
                    rev: Some("abc123".to_string()),
                    owner: Some("NixOS".to_string()),
                    repo: Some("nixpkgs".to_string()),
                    original_ref: None,
                    is_nixpkgs: true,
                    follows_refs: vec![],
                },
            );
        }
        let info = FlakeLockInfo {
            repo_name: "test".to_string(),
            version: 7,
            nodes,
            nixpkgs_nodes: vec!["nixpkgs".to_string(), "nixpkgs_2".to_string()],
            follows: BTreeMap::new(),
        };
        assert_eq!(count_nixpkgs_instances(&info), 2);
    }

    #[test]
    fn zero_nixpkgs() {
        let info = FlakeLockInfo {
            repo_name: "test".to_string(),
            version: 7,
            nodes: BTreeMap::new(),
            nixpkgs_nodes: vec![],
            follows: BTreeMap::new(),
        };
        assert_eq!(count_nixpkgs_instances(&info), 0);
    }
}
