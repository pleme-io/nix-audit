//! Checker: import-from-derivation (IFD) must be avoided or pre-committed.

use crate::types::{Category, Finding, Severity};
use std::path::Path;

/// Check for import-from-derivation risks (crate2nix without committed Cargo.nix).
///
/// When a flake.nix references crate2nix but no `Cargo.nix` file exists at the
/// repo root, Nix must run a derivation at evaluation time (IFD) to generate it.
/// This blocks parallel evaluation and prevents pure `nix flake check`.
pub fn check_ifd_avoidance(content: &str, repo_path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    if content.contains("crate2nix") {
        let cargo_nix_path = repo_path.join("Cargo.nix");
        if !cargo_nix_path.exists() {
            findings.push(Finding {
                category: Category::IfdAvoidance,
                severity: Severity::Warning,
                message: "crate2nix referenced but no Cargo.nix committed — IFD at eval time"
                    .into(),
                fix: Some(
                    "Run `crate2nix generate` and commit Cargo.nix, or set nixConfig.allow-import-from-derivation = true".into(),
                ),
                fixed: false,
            });
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn no_crate2nix_passes() {
        let content = r#"
        {
          inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
        }
        "#;
        let dir = tempfile::tempdir().unwrap();
        assert!(check_ifd_avoidance(content, dir.path()).is_empty());
    }

    #[test]
    fn crate2nix_without_cargo_nix_warns() {
        let content = r#"
        {
          inputs.crate2nix.url = "github:nix-community/crate2nix";
        }
        "#;
        let dir = tempfile::tempdir().unwrap();
        let findings = check_ifd_avoidance(content, dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].category, Category::IfdAvoidance);
    }

    #[test]
    fn crate2nix_with_cargo_nix_passes() {
        let content = r#"
        {
          inputs.crate2nix.url = "github:nix-community/crate2nix";
        }
        "#;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.nix"), "# generated").unwrap();
        assert!(check_ifd_avoidance(content, dir.path()).is_empty());
    }
}
