//! Terminal dashboard for convergence tracking.

use crate::convergence::metrics::{compute_convergence_velocity, identify_stubborn_categories};
use crate::convergence::store::ConvergenceStore;
use anyhow::Result;
use colored::Colorize;

/// Print the convergence dashboard to the terminal.
///
/// Shows: current compliance %, per-category finding counts, delta arrows
/// (up/down from previous run), last N runs trend line, and stubborn categories.
pub fn print_convergence_dashboard(store: &ConvergenceStore) -> Result<()> {
    let history = store.get_compliance_history(10)?;

    if history.is_empty() {
        println!("{}", "No audit runs recorded yet.".dimmed());
        return Ok(());
    }

    let latest = &history[0];
    let previous = history.get(1);

    // Header.
    println!("\n{}", "=== Convergence Dashboard ===".bold());

    // Current compliance.
    #[allow(clippy::cast_precision_loss)]
    let pct = latest.compliance_ratio * 100.0;
    let pct_str = format!("{pct:.1}%");
    let colored_pct = if pct >= 100.0 {
        pct_str.green().bold()
    } else if pct >= 80.0 {
        pct_str.yellow().bold()
    } else {
        pct_str.red().bold()
    };
    println!(
        "  Compliance: {} ({}/{} repos passing)",
        colored_pct, latest.passing_repos, latest.total_repos
    );

    // Delta arrow from previous run.
    if let Some(prev) = previous {
        let delta = latest.compliance_ratio - prev.compliance_ratio;
        let arrow = if delta > 0.001 {
            format!("  {} +{:.1}% from previous run", "\u{2191}", delta * 100.0).green()
        } else if delta < -0.001 {
            format!("  {} {:.1}% from previous run", "\u{2193}", delta * 100.0).red()
        } else {
            format!("  {} no change from previous run", "\u{2192}").dimmed()
        };
        println!("{arrow}");
    }

    // Velocity.
    let velocity = compute_convergence_velocity(&history);
    let vel_str = format!("{velocity:+.3} per run");
    let colored_vel = if velocity > 0.001 {
        vel_str.green()
    } else if velocity < -0.001 {
        vel_str.red()
    } else {
        vel_str.dimmed()
    };
    println!("  Velocity:   {colored_vel}");

    // Trend line (last N runs, chronological order).
    println!("\n  {}", "Trend (last 10 runs):".bold());
    print!("    ");
    for record in history.iter().rev() {
        let ratio = record.compliance_ratio;
        let block = if ratio >= 1.0 {
            "\u{2588}".green()
        } else if ratio >= 0.8 {
            "\u{2593}".green()
        } else if ratio >= 0.5 {
            "\u{2592}".yellow()
        } else if ratio >= 0.2 {
            "\u{2591}".red()
        } else {
            "\u{2591}".red().dimmed()
        };
        print!("{block} ");
    }
    println!();

    // Stubborn categories.
    let stubborn_data = store.get_stubborn_categories()?;
    if !stubborn_data.is_empty() {
        let stubborn = identify_stubborn_categories(&stubborn_data);
        println!("\n  {}", "Stubborn Categories (unfixed):".bold());
        for (category, count) in &stubborn {
            println!("    {category}: {count} occurrence(s)");
        }
    }

    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AuditResult, Category, Finding, Severity};

    fn make_result(repo: &str, converged: bool) -> AuditResult {
        let findings = if converged {
            vec![]
        } else {
            vec![Finding {
                category: Category::NixpkgsPin,
                severity: Severity::Error,
                message: "test".into(),
                fix: None,
                fixed: false,
            }]
        };
        AuditResult {
            repo: repo.into(),
            path: format!("/tmp/{repo}"),
            timestamp: "2024-01-01T00:00:00Z".into(),
            findings,
            converged,
        }
    }

    #[test]
    fn dashboard_with_no_runs() {
        let store = ConvergenceStore::new(":memory:").unwrap();
        // Should not error.
        print_convergence_dashboard(&store).unwrap();
    }

    #[test]
    fn dashboard_with_runs() {
        let store = ConvergenceStore::new(":memory:").unwrap();
        store
            .record_run(&[make_result("a", false), make_result("b", false)])
            .unwrap();
        store
            .record_run(&[make_result("a", true), make_result("b", false)])
            .unwrap();
        store
            .record_run(&[make_result("a", true), make_result("b", true)])
            .unwrap();

        // Should not error.
        print_convergence_dashboard(&store).unwrap();
    }
}
