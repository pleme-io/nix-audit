//! Checker: nixpkgs must be pinned to the required stable branch.

use crate::types::{Category, Finding, Severity};

/// Required nixpkgs branch for all pleme-io flakes.
pub const REQUIRED_BRANCH: &str = "nixos-25.11";

/// Patterns that indicate an unstable or unpinned nixpkgs reference.
pub const UNSTABLE_PATTERNS: &[&str] = &[
    "nixos-unstable",
    "nixpkgs-unstable",
    "/master",
    "/main",
];

/// Check that nixpkgs is pinned to the required stable branch.
///
/// Reports an error for each unstable pattern found, and a warning if
/// the required branch is not present at all.
pub fn check_nixpkgs_pin(content: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    for pattern in UNSTABLE_PATTERNS {
        if content.contains(pattern) {
            findings.push(Finding {
                category: Category::NixpkgsPin,
                severity: Severity::Error,
                message: format!("nixpkgs uses unstable branch: contains '{pattern}'"),
                fix: Some(format!(
                    "Replace '{pattern}' with '{REQUIRED_BRANCH}' in flake.nix"
                )),
                fixed: false,
            });
        }
    }

    if findings.is_empty() && !content.contains(REQUIRED_BRANCH) && content.contains("nixpkgs") {
        findings.push(Finding {
            category: Category::NixpkgsPin,
            severity: Severity::Warning,
            message: format!("nixpkgs pin not found or doesn't match '{REQUIRED_BRANCH}'"),
            fix: None,
            fixed: false,
        });
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correct_pin_passes() {
        let content = r#"
        {
          inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
        }
        "#;
        assert!(check_nixpkgs_pin(content).is_empty());
    }

    #[test]
    fn unstable_branch_fails() {
        let content = r#"
        {
          inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
        }
        "#;
        let findings = check_nixpkgs_pin(content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].category, Category::NixpkgsPin);
        assert_eq!(findings[0].severity, Severity::Error);
    }

    #[test]
    fn main_branch_fails() {
        let content = r#"
        {
          inputs.nixpkgs.url = "github:NixOS/nixpkgs/main";
        }
        "#;
        let findings = check_nixpkgs_pin(content);
        assert!(!findings.is_empty());
    }

    #[test]
    fn missing_branch_warns() {
        let content = r#"
        {
          inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";
        }
        "#;
        let findings = check_nixpkgs_pin(content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn no_nixpkgs_at_all_passes() {
        let content = r#"
        {
          inputs.flake-utils.url = "github:numtide/flake-utils";
        }
        "#;
        assert!(check_nixpkgs_pin(content).is_empty());
    }

    #[test]
    fn multiple_unstable_patterns() {
        let content = r#"
        {
          inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
          inputs.nixpkgs2.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
        }
        "#;
        let findings = check_nixpkgs_pin(content);
        assert_eq!(findings.len(), 2);
    }
}
