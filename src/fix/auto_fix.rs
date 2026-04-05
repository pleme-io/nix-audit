//! Auto-fix engine: apply safe, deterministic fixes to flake.nix files.

use crate::checkers::nixpkgs_pin::{REQUIRED_BRANCH, UNSTABLE_PATTERNS};
use crate::fix::patch_generator::generate_follows_patch;
use crate::types::{Category, Finding, Severity};
use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Auto-fix a single flake.nix file, optionally committing and pushing.
///
/// Currently supports:
/// - Replacing unstable nixpkgs pins with the required branch
/// - Adding missing `inputs.X.inputs.nixpkgs.follows = "nixpkgs"` declarations
/// - Replacing `buildImage` with `buildLayeredImage`
///
/// Returns the list of findings that were fixed.
pub fn fix_flake(path: &Path, commit: bool, push: bool) -> Result<Vec<Finding>> {
    let flake_path = path.join("flake.nix");
    let content = fs::read_to_string(&flake_path)?;
    let mut new_content = content.clone();
    let mut fixed = Vec::new();

    // Fix unstable pins.
    for pattern in UNSTABLE_PATTERNS {
        if new_content.contains(pattern) {
            new_content = new_content.replace(pattern, REQUIRED_BRANCH);
            fixed.push(Finding {
                category: Category::NixpkgsPin,
                severity: Severity::Error,
                message: format!("Fixed: replaced '{pattern}' with '{REQUIRED_BRANCH}'"),
                fix: None,
                fixed: true,
            });
        }
    }

    // Fix missing follows chains.
    let follows_patched = generate_follows_patch(&new_content);
    if follows_patched != new_content {
        let old_content = new_content.clone();
        new_content = follows_patched;
        fixed.push(Finding {
            category: Category::FollowsChain,
            severity: Severity::Warning,
            message: "Fixed: added missing nixpkgs follows declarations".into(),
            fix: None,
            fixed: true,
        });
        // Count how many follows were added.
        let added = new_content.matches(".inputs.nixpkgs.follows").count()
            - old_content.matches(".inputs.nixpkgs.follows").count();
        if added > 0 {
            fixed.last_mut().unwrap().message =
                format!("Fixed: added {added} missing nixpkgs follows declaration(s)");
        }
    }

    // Fix buildImage -> buildLayeredImage.
    if new_content.contains("buildImage") && !new_content.contains("buildLayeredImage") {
        new_content = new_content.replace("buildImage", "buildLayeredImage");
        fixed.push(Finding {
            category: Category::DockerLayers,
            severity: Severity::Warning,
            message: "Fixed: replaced buildImage with buildLayeredImage".into(),
            fix: None,
            fixed: true,
        });
    }

    if new_content != content {
        fs::write(&flake_path, &new_content)?;
        println!(
            "  {} {}",
            "FIXED".green().bold(),
            path.file_name().unwrap().to_string_lossy()
        );

        if commit {
            commit_fix(path, push)?;
        }
    }

    Ok(fixed)
}

/// Stage, commit, and optionally push a flake.nix fix.
fn commit_fix(path: &Path, push: bool) -> Result<()> {
    let repo_name = path.file_name().unwrap().to_string_lossy();

    let status = Command::new("git")
        .args(["add", "flake.nix"])
        .current_dir(path)
        .status()?;

    if status.success() {
        let msg = format!(
            "fix: pin nixpkgs to {} (nix-audit compliance)\n\nCo-Authored-By: nix-audit <noreply@pleme.io>",
            REQUIRED_BRANCH
        );
        Command::new("git")
            .args(["commit", "-m", &msg])
            .current_dir(path)
            .status()?;

        if push {
            let push_status = Command::new("git")
                .args(["push", "origin", "main"])
                .current_dir(path)
                .status()?;

            if push_status.success() {
                println!("  {} {repo_name}", "PUSHED".cyan().bold());
            } else {
                println!("  {} {repo_name} (push failed)", "WARN".yellow().bold());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn fix_unstable_pin() {
        let dir = tempfile::tempdir().unwrap();
        let flake = dir.path().join("flake.nix");
        fs::write(
            &flake,
            r#"{ inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable"; }"#,
        )
        .unwrap();

        let fixed = fix_flake(dir.path(), false, false).unwrap();
        assert!(!fixed.is_empty());

        let content = fs::read_to_string(&flake).unwrap();
        assert!(content.contains(REQUIRED_BRANCH));
        assert!(!content.contains("nixos-unstable"));
    }

    #[test]
    fn fix_build_image() {
        let dir = tempfile::tempdir().unwrap();
        let flake = dir.path().join("flake.nix");
        fs::write(
            &flake,
            r#"{ outputs = { self }: { image = pkgs.dockerTools.buildImage { name = "app"; }; }; }"#,
        )
        .unwrap();

        let fixed = fix_flake(dir.path(), false, false).unwrap();
        assert!(!fixed.is_empty());

        let content = fs::read_to_string(&flake).unwrap();
        assert!(content.contains("buildLayeredImage"));
    }

    #[test]
    fn no_changes_needed() {
        let dir = tempfile::tempdir().unwrap();
        let flake = dir.path().join("flake.nix");
        let original = r#"{ inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11"; }"#;
        fs::write(&flake, original).unwrap();

        let fixed = fix_flake(dir.path(), false, false).unwrap();
        assert!(fixed.is_empty());

        let content = fs::read_to_string(&flake).unwrap();
        assert_eq!(content, original);
    }
}
