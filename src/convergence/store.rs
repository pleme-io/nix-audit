//! SQLite-backed convergence tracking store.

use crate::types::{AuditResult, ConvergenceRecord};
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

/// Persistent convergence store backed by SQLite.
///
/// Records each audit run's compliance ratio, per-repo findings, and enables
/// historical trend analysis to track convergence over time.
pub struct ConvergenceStore {
    conn: Connection,
}

impl ConvergenceStore {
    /// Open (or create) a convergence database at the given path.
    ///
    /// Pass `":memory:"` for an in-memory database (useful for testing).
    pub fn new(path: &str) -> Result<Self> {
        let conn = if path == ":memory:" {
            Connection::open_in_memory().context("Failed to open in-memory SQLite database")?
        } else {
            // Ensure parent directory exists.
            if let Some(parent) = Path::new(path).parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("Failed to create directory {}", parent.display()))?;
                }
            }
            Connection::open(path)
                .with_context(|| format!("Failed to open SQLite database at {path}"))?
        };

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS audit_runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                total_repos INTEGER NOT NULL,
                passing_repos INTEGER NOT NULL,
                compliance_ratio REAL NOT NULL
            );
            CREATE TABLE IF NOT EXISTS findings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id INTEGER NOT NULL,
                repo TEXT NOT NULL,
                category TEXT NOT NULL,
                severity TEXT NOT NULL,
                message TEXT NOT NULL,
                fixed BOOLEAN NOT NULL DEFAULT FALSE
            );",
        )
        .context("Failed to create convergence tables")?;

        Ok(Self { conn })
    }

    /// Record a complete audit run (all results from one invocation).
    pub fn record_run(&self, results: &[AuditResult]) -> Result<i64> {
        let total_repos = results.len();
        let passing_repos = results.iter().filter(|r| r.converged).count();
        let compliance_ratio = if total_repos == 0 {
            1.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let ratio = passing_repos as f64 / total_repos as f64;
            ratio
        };

        let timestamp = results
            .first()
            .map(|r| r.timestamp.clone())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

        self.conn.execute(
            "INSERT INTO audit_runs (timestamp, total_repos, passing_repos, compliance_ratio)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![timestamp, total_repos, passing_repos, compliance_ratio],
        )?;

        let run_id = self.conn.last_insert_rowid();

        for result in results {
            for finding in &result.findings {
                self.conn.execute(
                    "INSERT INTO findings (run_id, repo, category, severity, message, fixed)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        run_id,
                        result.repo,
                        finding.category.to_string(),
                        finding.severity.to_string(),
                        finding.message,
                        finding.fixed,
                    ],
                )?;
            }
        }

        Ok(run_id)
    }

    /// Get the most recent audit run record.
    #[allow(dead_code)]
    pub fn get_latest_run(&self) -> Result<Option<ConvergenceRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, total_repos, passing_repos, compliance_ratio
             FROM audit_runs ORDER BY id DESC LIMIT 1",
        )?;

        let mut rows = stmt.query_map([], |row| {
            Ok(ConvergenceRecord {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                total_repos: row.get::<_, usize>(2)?,
                passing_repos: row.get::<_, usize>(3)?,
                compliance_ratio: row.get(4)?,
            })
        })?;

        match rows.next() {
            Some(Ok(record)) => Ok(Some(record)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Get the last N audit run records (most recent first).
    pub fn get_compliance_history(&self, n: usize) -> Result<Vec<ConvergenceRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, total_repos, passing_repos, compliance_ratio
             FROM audit_runs ORDER BY id DESC LIMIT ?1",
        )?;

        let rows = stmt.query_map(rusqlite::params![n], |row| {
            Ok(ConvergenceRecord {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                total_repos: row.get::<_, usize>(2)?,
                passing_repos: row.get::<_, usize>(3)?,
                compliance_ratio: row.get(4)?,
            })
        })?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    /// Get categories that have never been fully resolved across all runs.
    ///
    /// A "stubborn" category is one that appears in findings of every single run.
    pub fn get_stubborn_categories(&self) -> Result<Vec<(String, usize)>> {
        let mut stmt = self.conn.prepare(
            "SELECT category, COUNT(*) as cnt
             FROM findings
             WHERE fixed = FALSE
             GROUP BY category
             ORDER BY cnt DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
        })?;

        let mut categories = Vec::new();
        for row in rows {
            categories.push(row?);
        }
        Ok(categories)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Category, Finding, Severity};

    fn make_result(repo: &str, converged: bool) -> AuditResult {
        let findings = if converged {
            vec![]
        } else {
            vec![Finding {
                category: Category::NixpkgsPin,
                severity: Severity::Error,
                message: "test finding".into(),
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
    fn create_in_memory() {
        let store = ConvergenceStore::new(":memory:");
        assert!(store.is_ok());
    }

    #[test]
    fn record_and_retrieve() {
        let store = ConvergenceStore::new(":memory:").unwrap();
        let results = vec![
            make_result("repo-a", true),
            make_result("repo-b", false),
            make_result("repo-c", true),
        ];

        let run_id = store.record_run(&results).unwrap();
        assert_eq!(run_id, 1);

        let latest = store.get_latest_run().unwrap().unwrap();
        assert_eq!(latest.total_repos, 3);
        assert_eq!(latest.passing_repos, 2);
        assert!((latest.compliance_ratio - 2.0 / 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compliance_history() {
        let store = ConvergenceStore::new(":memory:").unwrap();

        store
            .record_run(&[make_result("a", false)])
            .unwrap();
        store
            .record_run(&[make_result("a", true)])
            .unwrap();
        store
            .record_run(&[make_result("a", true), make_result("b", true)])
            .unwrap();

        let history = store.get_compliance_history(10).unwrap();
        assert_eq!(history.len(), 3);
        // Most recent first.
        assert!((history[0].compliance_ratio - 1.0).abs() < f64::EPSILON);
        assert!((history[1].compliance_ratio - 1.0).abs() < f64::EPSILON);
        assert!((history[2].compliance_ratio - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn stubborn_categories() {
        let store = ConvergenceStore::new(":memory:").unwrap();

        let results = vec![
            make_result("repo-a", false),
            make_result("repo-b", false),
        ];
        store.record_run(&results).unwrap();

        let stubborn = store.get_stubborn_categories().unwrap();
        assert_eq!(stubborn.len(), 1);
        assert_eq!(stubborn[0].0, "nixpkgs_pin");
        assert_eq!(stubborn[0].1, 2);
    }

    #[test]
    fn empty_results() {
        let store = ConvergenceStore::new(":memory:").unwrap();
        store.record_run(&[]).unwrap();

        let latest = store.get_latest_run().unwrap().unwrap();
        assert_eq!(latest.total_repos, 0);
        assert!((latest.compliance_ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn no_runs_returns_none() {
        let store = ConvergenceStore::new(":memory:").unwrap();
        assert!(store.get_latest_run().unwrap().is_none());
    }
}
