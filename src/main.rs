//! nix-audit — Convergent Nix flake efficiency auditor.
//!
//! Scans flake.nix files for rebuild-causing misconfigurations, fixes them,
//! and verifies convergence. Each run finds fewer issues until zero remain.
//!
//! Powers the nix-efficiency Claude skill.

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use colored::Colorize;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "nix-audit", about = "Convergent Nix flake efficiency auditor")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Audit a single flake or directory of flakes
    Check {
        /// Path to flake directory or parent containing multiple flakes
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Scan all subdirectories for flake.nix files
        #[arg(long)]
        all: bool,

        /// Output format
        #[arg(long, default_value = "text")]
        format: OutputFormat,
    },

    /// Auto-fix violations that can be fixed without manual intervention
    Fix {
        /// Path to flake directory or parent
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Fix all subdirectories
        #[arg(long)]
        all: bool,

        /// Commit fixes automatically
        #[arg(long)]
        commit: bool,

        /// Push after commit
        #[arg(long)]
        push: bool,
    },

    /// Show the org-wide standards
    Standards,
}

#[derive(Clone, clap::ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

// ── Standards (single source of truth) ──────────────────────────────────

const REQUIRED_BRANCH: &str = "nixos-25.11";

const UNSTABLE_PATTERNS: &[&str] = &[
    "nixos-unstable",
    "nixpkgs-unstable",
    "/master",
    "/main",
];

// ── Audit Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditResult {
    repo: String,
    path: String,
    timestamp: String,
    findings: Vec<Finding>,
    converged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Finding {
    category: Category,
    severity: Severity,
    message: String,
    fix: Option<String>,
    fixed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum Category {
    NixpkgsPin,
    FollowsChain,
    SourceFiltering,
    IfdAvoidance,
    VersionStability,
    DockerLayers,
    CacheAlignment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Severity {
    Error,
    Warning,
    Info,
}

// ── Checkers ────────────────────────────────────────────────────────────

fn check_nixpkgs_pin(content: &str) -> Vec<Finding> {
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

fn check_follows_chain(content: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Find all inputs that have a .url but no .inputs.nixpkgs.follows
    let url_re = Regex::new(r#"(\w[\w-]*)\.url\s*="#).unwrap();
    let follows_re = Regex::new(r#"(\w[\w-]*)\.inputs\.nixpkgs\.follows"#).unwrap();

    let inputs_with_url: Vec<String> = url_re
        .captures_iter(content)
        .filter_map(|c| {
            let name = c.get(1)?.as_str().to_string();
            // Skip nixpkgs itself and flake-utils (no nixpkgs dep)
            if name == "nixpkgs" || name == "flake-utils" || name == "systems" {
                None
            } else {
                Some(name)
            }
        })
        .collect();

    let inputs_with_follows: Vec<String> = follows_re
        .captures_iter(content)
        .filter_map(|c| Some(c.get(1)?.as_str().to_string()))
        .collect();

    for input in &inputs_with_url {
        // Check if this input is defined as a nested attrset (likely has follows)
        // or as a bare URL (likely missing follows)
        let has_follows = inputs_with_follows.contains(input);
        if !has_follows {
            // Check if it's defined inline with follows in the nested form
            let nested_pattern = format!("{input} = {{");
            let inline_follows = format!("inputs.{input}.inputs.nixpkgs.follows");
            if !content.contains(&nested_pattern) && !content.contains(&inline_follows) {
                findings.push(Finding {
                    category: Category::FollowsChain,
                    severity: Severity::Warning,
                    message: format!("Input '{input}' may not follow top-level nixpkgs"),
                    fix: Some(format!(
                        "Add: inputs.{input}.inputs.nixpkgs.follows = \"nixpkgs\";"
                    )),
                    fixed: false,
                });
            }
        }
    }

    findings
}

fn check_source_filtering(content: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Check for unfiltered source
    if content.contains("src = ./.") || content.contains("src = self") {
        let has_filter = content.contains("cleanSource")
            || content.contains("cleanCargoSource")
            || content.contains("fileset")
            || content.contains("gitignoreSource");

        if !has_filter {
            findings.push(Finding {
                category: Category::SourceFiltering,
                severity: Severity::Warning,
                message: "Source may include non-build files (.git, target/, flake.lock)".into(),
                fix: Some("Use cleanSource, cleanCargoSource, or lib.fileset".into()),
                fixed: false,
            });
        }
    }

    findings
}

fn check_version_stability(content: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    if content.contains("builtins.currentTime") {
        findings.push(Finding {
            category: Category::VersionStability,
            severity: Severity::Error,
            message: "builtins.currentTime used — derivation hash changes every evaluation".into(),
            fix: Some("Use a fixed version from Cargo.toml or package metadata".into()),
            fixed: false,
        });
    }

    if content.contains("builtins.getEnv") && (content.contains("GIT") || content.contains("SHA")) {
        findings.push(Finding {
            category: Category::VersionStability,
            severity: Severity::Error,
            message: "Git SHA injected at build time — busts cache on every commit".into(),
            fix: Some("Inject GIT_SHA at runtime via Docker ENV or K8s, not at build time".into()),
            fixed: false,
        });
    }

    findings
}

fn check_docker_layers(content: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    if content.contains("buildImage") && !content.contains("buildLayeredImage") {
        findings.push(Finding {
            category: Category::DockerLayers,
            severity: Severity::Warning,
            message: "Using buildImage instead of buildLayeredImage — no layer caching".into(),
            fix: Some("Switch to dockerTools.buildLayeredImage with maxLayers = 120".into()),
            fixed: false,
        });
    }

    findings
}

// ── Audit Runner ────────────────────────────────────────────────────────

fn audit_flake(path: &Path) -> Result<AuditResult> {
    let flake_path = path.join("flake.nix");
    let content = fs::read_to_string(&flake_path)
        .with_context(|| format!("Failed to read {}", flake_path.display()))?;

    let repo = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let mut findings = Vec::new();
    findings.extend(check_nixpkgs_pin(&content));
    findings.extend(check_follows_chain(&content));
    findings.extend(check_source_filtering(&content));
    findings.extend(check_version_stability(&content));
    findings.extend(check_docker_layers(&content));

    let converged = findings.is_empty();

    Ok(AuditResult {
        repo,
        path: path.display().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        findings,
        converged,
    })
}

fn find_flakes(root: &Path, all: bool) -> Vec<PathBuf> {
    if !all {
        if root.join("flake.nix").exists() {
            return vec![root.to_path_buf()];
        }
        return vec![];
    }

    WalkDir::new(root)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() == "flake.nix")
        .map(|e| e.path().parent().unwrap().to_path_buf())
        .collect()
}

// ── Fixer ───────────────────────────────────────────────────────────────

fn fix_flake(path: &Path, commit: bool, push: bool) -> Result<Vec<Finding>> {
    let flake_path = path.join("flake.nix");
    let content = fs::read_to_string(&flake_path)?;
    let mut new_content = content.clone();
    let mut fixed = Vec::new();

    // Fix unstable pins
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

    if new_content != content {
        fs::write(&flake_path, &new_content)?;
        println!(
            "  {} {}",
            "FIXED".green().bold(),
            path.file_name().unwrap().to_string_lossy()
        );

        if commit {
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
        }
    }

    Ok(fixed)
}

// ── Display ─────────────────────────────────────────────────────────────

fn print_result(result: &AuditResult) {
    let status = if result.converged {
        "PASS".green().bold()
    } else {
        "FAIL".red().bold()
    };

    println!("{} {}", status, result.repo.bold());

    for finding in &result.findings {
        let severity = match finding.severity {
            Severity::Error => "ERROR".red(),
            Severity::Warning => "WARN".yellow(),
            Severity::Info => "INFO".blue(),
        };

        let icon = if finding.fixed { "+" } else { "x" };
        println!("  [{severity}] {icon} {}", finding.message);

        if let Some(ref fix) = finding.fix {
            println!("         fix: {}", fix.dimmed());
        }
    }
}

fn print_summary(results: &[AuditResult]) {
    let total = results.len();
    let passing = results.iter().filter(|r| r.converged).count();
    let failing = total - passing;

    let mut category_counts: BTreeMap<Category, usize> = BTreeMap::new();
    for result in results {
        for finding in &result.findings {
            *category_counts.entry(finding.category.clone()).or_insert(0) += 1;
        }
    }

    println!("\n{}", "═══ Summary ═══".bold());
    println!("  Total:   {total}");
    println!("  Passing: {}", format!("{passing}").green());
    println!("  Failing: {}", format!("{failing}").red());

    if !category_counts.is_empty() {
        println!("\n  Findings by category:");
        for (cat, count) in &category_counts {
            println!("    {cat:?}: {count}");
        }
    }
}

// ── Main ────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Cmd::Check { path, all, format } => {
            let flakes = find_flakes(&path, all);

            if flakes.is_empty() {
                println!("No flake.nix found at {}", path.display());
                return Ok(());
            }

            let mut results = Vec::new();
            for flake_dir in &flakes {
                match audit_flake(flake_dir) {
                    Ok(result) => {
                        match format {
                            OutputFormat::Text => print_result(&result),
                            OutputFormat::Json => {}
                        }
                        results.push(result);
                    }
                    Err(e) => {
                        eprintln!("Error auditing {}: {e}", flake_dir.display());
                    }
                }
            }

            match format {
                OutputFormat::Text => print_summary(&results),
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&results)?);
                }
            }

            let has_errors = results.iter().any(|r| !r.converged);
            if has_errors {
                std::process::exit(1);
            }
        }

        Cmd::Fix {
            path,
            all,
            commit,
            push,
        } => {
            let flakes = find_flakes(&path, all);
            let mut total_fixed = 0;

            for flake_dir in &flakes {
                match fix_flake(flake_dir, commit, push) {
                    Ok(fixed) => total_fixed += fixed.len(),
                    Err(e) => eprintln!("Error fixing {}: {e}", flake_dir.display()),
                }
            }

            println!("\n{total_fixed} issue(s) fixed across {} flake(s)", flakes.len());
        }

        Cmd::Standards => {
            println!("{}", "═══ pleme-io Nix Standards ═══".bold());
            println!("  nixpkgs branch:    {}", REQUIRED_BRANCH.green());
            println!("  follows:           ALL inputs must follow top-level nixpkgs");
            println!("  source filtering:  cleanSource or fileset (never bare src = ./.)");
            println!("  IFD:               committed Cargo.nix, no eval-time builds");
            println!("  version strings:   fixed at build, variable at runtime");
            println!("  Docker layers:     buildLayeredImage, maxLayers=120");
            println!("  binary cache:      Attic, priority 10");
        }
    }

    Ok(())
}
