//! Parse flake.lock JSON (version 7) into structured `FlakeLockInfo`.

use crate::types::{FlakeLockInfo, LockedInput};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::path::Path;

/// Parse a flake.lock file at the given path into a `FlakeLockInfo`.
///
/// Supports flake.lock version 7 (the current standard). Extracts locked
/// revisions, follows references, and identifies nixpkgs nodes by checking
/// `original.owner == "NixOS"` and `original.repo == "nixpkgs"`.
pub fn parse_flake_lock(path: &Path) -> Result<FlakeLockInfo> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read flake.lock at {}", path.display()))?;

    parse_flake_lock_content(&content, path)
}

/// Parse flake.lock content from a string (for testing).
pub fn parse_flake_lock_content(content: &str, path: &Path) -> Result<FlakeLockInfo> {
    let lock: serde_json::Value =
        serde_json::from_str(content).context("Failed to parse flake.lock as JSON")?;

    let version = lock
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    if version != 7 {
        bail!("Unsupported flake.lock version: {version} (expected 7)");
    }

    let nodes_obj = lock
        .get("nodes")
        .and_then(serde_json::Value::as_object)
        .context("flake.lock missing 'nodes' object")?;

    let repo_name = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let mut nodes = BTreeMap::new();
    let mut nixpkgs_nodes = Vec::new();
    let mut follows = BTreeMap::new();

    for (name, node) in nodes_obj {
        // Skip the "root" node — it just lists top-level inputs.
        if name == "root" {
            // Scan root node inputs for follows references.
            if let Some(inputs) = node.get("inputs").and_then(serde_json::Value::as_object) {
                for (input_name, input_val) in inputs {
                    // Array-form means follows: ["nixpkgs"] means this input follows nixpkgs.
                    if let Some(arr) = input_val.as_array() {
                        if let Some(target) = arr.first().and_then(serde_json::Value::as_str) {
                            follows.insert(input_name.clone(), target.to_string());
                        }
                    }
                }
            }
            continue;
        }

        let locked = node.get("locked");
        let original = node.get("original");

        let rev = locked
            .and_then(|l| l.get("rev"))
            .and_then(serde_json::Value::as_str)
            .map(String::from);

        let owner = original
            .and_then(|o| o.get("owner"))
            .and_then(serde_json::Value::as_str)
            .map(String::from);

        let repo = original
            .and_then(|o| o.get("repo"))
            .and_then(serde_json::Value::as_str)
            .map(String::from);

        let original_ref = original
            .and_then(|o| o.get("ref"))
            .and_then(serde_json::Value::as_str)
            .map(String::from);

        let is_nixpkgs = owner.as_deref() == Some("NixOS")
            && repo.as_deref() == Some("nixpkgs");

        // Detect follows references in this node's inputs.
        let mut follows_refs = Vec::new();
        if let Some(inputs) = node.get("inputs").and_then(serde_json::Value::as_object) {
            for (_input_name, input_val) in inputs {
                if let Some(arr) = input_val.as_array() {
                    for element in arr {
                        if let Some(s) = element.as_str() {
                            follows_refs.push(s.to_string());
                        }
                    }
                }
            }
        }

        if is_nixpkgs {
            nixpkgs_nodes.push(name.clone());
        }

        nodes.insert(
            name.clone(),
            LockedInput {
                name: name.clone(),
                rev,
                owner,
                repo,
                original_ref,
                is_nixpkgs,
                follows_refs,
            },
        );
    }

    Ok(FlakeLockInfo {
        repo_name,
        version: u32::try_from(version).unwrap_or(7),
        nodes,
        nixpkgs_nodes,
        follows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const MOCK_LOCK: &str = r#"{
  "nodes": {
    "nixpkgs": {
      "locked": {
        "lastModified": 1700000000,
        "narHash": "sha256-abc123",
        "owner": "NixOS",
        "repo": "nixpkgs",
        "rev": "abc123def456789012345678901234567890abcd",
        "type": "github"
      },
      "original": {
        "owner": "NixOS",
        "ref": "nixos-25.11",
        "repo": "nixpkgs",
        "type": "github"
      }
    },
    "flake-utils": {
      "locked": {
        "lastModified": 1699000000,
        "narHash": "sha256-def456",
        "owner": "numtide",
        "repo": "flake-utils",
        "rev": "def456abc789012345678901234567890abcdef12",
        "type": "github"
      },
      "original": {
        "owner": "numtide",
        "repo": "flake-utils",
        "type": "github"
      }
    },
    "substrate": {
      "inputs": {
        "nixpkgs": ["nixpkgs"]
      },
      "locked": {
        "lastModified": 1698000000,
        "narHash": "sha256-ghi789",
        "owner": "pleme-io",
        "repo": "substrate",
        "rev": "789012abc345678901234567890abcdef12345678",
        "type": "github"
      },
      "original": {
        "owner": "pleme-io",
        "repo": "substrate",
        "type": "github"
      }
    },
    "root": {
      "inputs": {
        "nixpkgs": "nixpkgs",
        "flake-utils": "flake-utils",
        "substrate": "substrate"
      }
    }
  },
  "root": "root",
  "version": 7
}"#;

    #[test]
    fn parse_mock_lock() {
        let path = PathBuf::from("/tmp/test-repo/flake.lock");
        let info = parse_flake_lock_content(MOCK_LOCK, &path).unwrap();

        assert_eq!(info.version, 7);
        assert_eq!(info.nixpkgs_nodes, vec!["nixpkgs"]);
        assert_eq!(info.nodes.len(), 3);

        let nixpkgs = &info.nodes["nixpkgs"];
        assert!(nixpkgs.is_nixpkgs);
        assert_eq!(
            nixpkgs.rev.as_deref(),
            Some("abc123def456789012345678901234567890abcd")
        );
        assert_eq!(nixpkgs.original_ref.as_deref(), Some("nixos-25.11"));
    }

    #[test]
    fn parse_follows_references() {
        let path = PathBuf::from("/tmp/test-repo/flake.lock");
        let info = parse_flake_lock_content(MOCK_LOCK, &path).unwrap();

        let substrate = &info.nodes["substrate"];
        assert_eq!(substrate.follows_refs, vec!["nixpkgs"]);
    }

    #[test]
    fn non_nixpkgs_not_flagged() {
        let path = PathBuf::from("/tmp/test-repo/flake.lock");
        let info = parse_flake_lock_content(MOCK_LOCK, &path).unwrap();

        let flake_utils = &info.nodes["flake-utils"];
        assert!(!flake_utils.is_nixpkgs);
    }

    #[test]
    fn wrong_version_rejected() {
        let content = r#"{"version": 5, "nodes": {}}"#;
        let path = PathBuf::from("/tmp/test-repo/flake.lock");
        let result = parse_flake_lock_content(content, &path);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_json_rejected() {
        let path = PathBuf::from("/tmp/test-repo/flake.lock");
        let result = parse_flake_lock_content("not json", &path);
        assert!(result.is_err());
    }
}
