//! Audit checkers — each module implements a single category of flake.nix analysis.

pub mod cache_alignment;
pub mod docker_layers;
pub mod follows_chain;
pub mod ifd_avoidance;
pub mod nixpkgs_pin;
pub mod source_filter;
pub mod version_stability;

pub use cache_alignment::check_cache_alignment;
pub use docker_layers::check_docker_layers;
pub use follows_chain::check_follows_chain;
pub use ifd_avoidance::check_ifd_avoidance;
#[allow(unused_imports)]
pub use nixpkgs_pin::{check_nixpkgs_pin, REQUIRED_BRANCH, UNSTABLE_PATTERNS};
pub use source_filter::check_source_filtering;
pub use version_stability::check_version_stability;

use crate::types::Finding;
use std::path::Path;

/// Run all single-flake checkers against the given content and repo path.
///
/// This does NOT include `check_cache_alignment`, which requires cross-repo
/// flake lock data. Use `check_cache_alignment` separately for org-wide analysis.
pub fn run_all_checkers(content: &str, repo_path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    findings.extend(check_nixpkgs_pin(content));
    findings.extend(check_follows_chain(content));
    findings.extend(check_source_filtering(content));
    findings.extend(check_version_stability(content));
    findings.extend(check_docker_layers(content));
    findings.extend(check_ifd_avoidance(content, repo_path));
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_flake_passes_all() {
        let content = r#"
        {
          inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
          inputs.substrate = {
            url = "github:pleme-io/substrate";
            inputs.nixpkgs.follows = "nixpkgs";
          };
          outputs = { self, nixpkgs, substrate }:
            let pkgs = import nixpkgs { system = "x86_64-linux"; };
            in { packages.default = pkgs.hello; };
        }
        "#;
        let dir = tempfile::tempdir().unwrap();
        let findings = run_all_checkers(content, dir.path());
        assert!(findings.is_empty(), "Expected no findings, got: {findings:?}");
    }

    #[test]
    fn multiple_violations_found() {
        let content = r#"
        {
          inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
          inputs.substrate.url = "github:pleme-io/substrate";
          outputs = { self, nixpkgs, substrate }:
            let
              pkgs = import nixpkgs { system = "x86_64-linux"; };
              version = builtins.currentTime;
            in {
              packages.default = pkgs.dockerTools.buildImage { name = "app"; };
            };
        }
        "#;
        let dir = tempfile::tempdir().unwrap();
        let findings = run_all_checkers(content, dir.path());
        assert!(findings.len() >= 3, "Expected 3+ findings, got: {findings:?}");
    }
}
