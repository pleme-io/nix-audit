//! nix-audit — Convergent Nix flake efficiency auditor.
//!
//! Scans flake.nix files for rebuild-causing misconfigurations, fixes them,
//! and verifies convergence. Each run finds fewer issues until zero remain.
//!
//! Powers the nix-efficiency Claude skill.

mod checkers;
mod convergence;
mod fix;
mod flake_lock;
mod types;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use types::{AuditResult, Category, Severity};
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
        /// Path to convergence database (auto-records when provided)
        #[arg(long)]
        db: Option<String>,
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
    /// Show convergence dashboard from historical audit data
    Converge {
        /// Run org-wide analysis
        #[arg(long)]
        org_wide: bool,
        /// Filter to a specific repo
        #[arg(long)]
        repo: Option<String>,
        /// Path to convergence database
        #[arg(long, default_value_t = default_db_path())]
        db: String,
    },
    /// Analyze flake.lock files for nixpkgs instances and drift
    LockAnalysis {
        /// Path to flake directory or parent
        path: PathBuf,
        /// Scan all subdirectories
        #[arg(long)]
        all: bool,
    },
}

#[derive(Clone, clap::ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

/// Default convergence database path under the user's data directory.
fn default_db_path() -> String {
    dirs::data_dir()
        .map(|d| d.join("nix-audit").join("convergence.db"))
        .unwrap_or_else(|| PathBuf::from("convergence.db"))
        .to_string_lossy()
        .to_string()
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

    let findings = checkers::run_all_checkers(&content, path);
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

        if let Some(ref fix_text) = finding.fix {
            println!("         fix: {}", fix_text.dimmed());
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

    println!("\n{}", "=== Summary ===".bold());
    println!("  Total:   {total}");
    println!("  Passing: {}", format!("{passing}").green());
    println!("  Failing: {}", format!("{failing}").red());

    if !category_counts.is_empty() {
        println!("\n  Findings by category:");
        for (cat, count) in &category_counts {
            println!("    {cat}: {count}");
        }
    }
}

// ── Main ────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Cmd::Check {
            path,
            all,
            format,
            db,
        } => {
            let flakes = find_flakes(&path, all);
            if flakes.is_empty() {
                println!("No flake.nix found at {}", path.display());
                return Ok(());
            }

            let mut results = Vec::new();
            for flake_dir in &flakes {
                match audit_flake(flake_dir) {
                    Ok(result) => {
                        if matches!(format, OutputFormat::Text) {
                            print_result(&result);
                        }
                        results.push(result);
                    }
                    Err(e) => eprintln!("Error auditing {}: {e}", flake_dir.display()),
                }
            }

            match format {
                OutputFormat::Text => print_summary(&results),
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&results)?);
                }
            }

            // Auto-record to convergence DB if provided.
            if let Some(db_path) = db {
                let store = convergence::ConvergenceStore::new(&db_path)?;
                store.record_run(&results)?;
                println!("\n  {} Recorded to {db_path}", "DB".cyan().bold());
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
                match fix::fix_flake(flake_dir, commit, push) {
                    Ok(fixed) => total_fixed += fixed.len(),
                    Err(e) => eprintln!("Error fixing {}: {e}", flake_dir.display()),
                }
            }

            println!("\n{total_fixed} issue(s) fixed across {} flake(s)", flakes.len());
        }

        Cmd::Standards => {
            println!("{}", "=== pleme-io Nix Standards ===".bold());
            println!(
                "  nixpkgs branch:    {}",
                checkers::REQUIRED_BRANCH.green()
            );
            println!("  follows:           ALL inputs must follow top-level nixpkgs");
            println!("  source filtering:  cleanSource or fileset (never bare src = ./.)");
            println!("  IFD:               committed Cargo.nix, no eval-time builds");
            println!("  version strings:   fixed at build, variable at runtime");
            println!("  Docker layers:     buildLayeredImage, maxLayers=120");
            println!("  binary cache:      Attic, priority 10");
        }

        Cmd::Converge {
            org_wide: _,
            repo: _,
            db,
        } => {
            let store = convergence::ConvergenceStore::new(&db)?;
            convergence::print_convergence_dashboard(&store)?;
        }

        Cmd::LockAnalysis { path, all } => {
            let flakes = find_flakes(&path, all);
            let mut lock_infos = Vec::new();

            for flake_dir in &flakes {
                let lock_path = flake_dir.join("flake.lock");
                if !lock_path.exists() {
                    continue;
                }
                match flake_lock::parse_flake_lock(&lock_path) {
                    Ok(info) => {
                        let count = flake_lock::count_nixpkgs_instances(&info);
                        let name = &info.repo_name;
                        let status = if count <= 1 {
                            "OK".green().bold()
                        } else {
                            "WARN".yellow().bold()
                        };
                        println!("[{status}] {name}: {count} nixpkgs instance(s)");
                        lock_infos.push((info.repo_name.clone(), info));
                    }
                    Err(e) => {
                        eprintln!("Error parsing {}: {e}", lock_path.display());
                    }
                }
            }

            if lock_infos.len() > 1 {
                let drift_reports = flake_lock::detect_drift(&lock_infos);
                if drift_reports.is_empty() {
                    println!("\n{}", "All repos use the same nixpkgs revision.".green());
                } else {
                    println!("\n{}", "=== Nixpkgs Drift Report ===".bold());
                    for report in &drift_reports {
                        println!("  Node: {}", report.node_name);
                        if let Some(ref majority) = report.majority_rev {
                            let short = &majority[..majority.len().min(12)];
                            println!("  Majority rev: {short}..");
                        }
                        println!(
                            "  Divergent: {}",
                            report.divergent_repos.join(", ")
                        );
                    }
                }

                // Also run cache alignment checker.
                let alignment = checkers::check_cache_alignment(&lock_infos);
                if !alignment.is_empty() {
                    println!("\n{}", "=== Cache Alignment Findings ===".bold());
                    for finding in &alignment {
                        println!("  [{:?}] {}", finding.severity, finding.message);
                    }
                }
            }
        }
    }

    Ok(())
}
