//! Core types for nix-audit: findings, audit results, flake lock info, convergence records.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

// ── Audit Types ─────────────────────────────────────────────────────────

/// Result of auditing a single flake.nix file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResult {
    /// Repository name (derived from directory name).
    pub repo: String,
    /// Filesystem path to the flake directory.
    pub path: String,
    /// ISO-8601 timestamp of the audit run.
    pub timestamp: String,
    /// All findings discovered during the audit.
    pub findings: Vec<Finding>,
    /// Whether the flake has zero findings (fully converged).
    pub converged: bool,
}

/// A single audit finding within a flake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Which checker produced this finding.
    pub category: Category,
    /// How severe the finding is.
    pub severity: Severity,
    /// Human-readable description of the issue.
    pub message: String,
    /// Suggested fix, if one exists.
    pub fix: Option<String>,
    /// Whether this finding was auto-fixed.
    pub fixed: bool,
}

/// Categories of audit checks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// nixpkgs must be pinned to the required branch.
    NixpkgsPin,
    /// All inputs must follow top-level nixpkgs.
    FollowsChain,
    /// Source trees should be filtered to avoid cache-busting.
    SourceFiltering,
    /// Import-from-derivation must be avoided or pre-committed.
    IfdAvoidance,
    /// Version strings must be stable across evaluations.
    VersionStability,
    /// Docker images should use layered builds.
    DockerLayers,
    /// nixpkgs revisions must align across the org.
    CacheAlignment,
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NixpkgsPin => write!(f, "nixpkgs_pin"),
            Self::FollowsChain => write!(f, "follows_chain"),
            Self::SourceFiltering => write!(f, "source_filtering"),
            Self::IfdAvoidance => write!(f, "ifd_avoidance"),
            Self::VersionStability => write!(f, "version_stability"),
            Self::DockerLayers => write!(f, "docker_layers"),
            Self::CacheAlignment => write!(f, "cache_alignment"),
        }
    }
}

/// Severity levels for findings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Must be fixed before the flake is considered converged.
    Error,
    /// Should be fixed but does not block convergence.
    Warning,
    /// Informational note.
    Info,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error => write!(f, "error"),
            Self::Warning => write!(f, "warning"),
            Self::Info => write!(f, "info"),
        }
    }
}

// ── Flake Lock Types ────────────────────────────────────────────────────

/// Parsed representation of a flake.lock file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeLockInfo {
    /// Name of the repo or flake this lock belongs to.
    pub repo_name: String,
    /// Lock file version (typically 7).
    pub version: u32,
    /// Map of node name to locked input information.
    pub nodes: BTreeMap<String, LockedInput>,
    /// Node names that are nixpkgs inputs.
    pub nixpkgs_nodes: Vec<String>,
    /// Map of input name to the node it follows (e.g., "nixpkgs").
    pub follows: BTreeMap<String, String>,
}

/// A single locked input from a flake.lock node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedInput {
    /// Node name in the lock file.
    pub name: String,
    /// Git revision hash, if locked.
    pub rev: Option<String>,
    /// Owner of the original flake reference (e.g., "NixOS").
    pub owner: Option<String>,
    /// Repository name of the original flake reference (e.g., "nixpkgs").
    pub repo: Option<String>,
    /// The ref (branch/tag) of the original flake reference.
    pub original_ref: Option<String>,
    /// Whether this node is a nixpkgs input.
    pub is_nixpkgs: bool,
    /// Input names this node follows (array-form follows references).
    pub follows_refs: Vec<String>,
}

// ── Convergence Types ───────────────────────────────────────────────────

/// A single convergence tracking record from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvergenceRecord {
    /// Auto-incremented run ID.
    pub id: i64,
    /// ISO-8601 timestamp of the run.
    pub timestamp: String,
    /// Total number of repos audited.
    pub total_repos: usize,
    /// Number of repos that passed (zero findings).
    pub passing_repos: usize,
    /// Ratio of passing to total repos (0.0 to 1.0).
    pub compliance_ratio: f64,
}

/// Snapshot of the entire org's audit state at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct OrgSnapshot {
    /// ISO-8601 timestamp.
    pub timestamp: String,
    /// All audit results from this snapshot.
    pub results: Vec<AuditResult>,
    /// Overall compliance ratio.
    pub compliance_ratio: f64,
    /// Findings grouped by category with counts.
    pub category_counts: BTreeMap<Category, usize>,
}

/// Report of nixpkgs revision drift across repos.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftReport {
    /// The nixpkgs node name.
    pub node_name: String,
    /// Map of revision hash to list of repos using that revision.
    pub revisions: BTreeMap<String, Vec<String>>,
    /// The most common revision (majority rev).
    pub majority_rev: Option<String>,
    /// Repos that diverge from the majority revision.
    pub divergent_repos: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_display() {
        assert_eq!(Category::NixpkgsPin.to_string(), "nixpkgs_pin");
        assert_eq!(Category::FollowsChain.to_string(), "follows_chain");
        assert_eq!(Category::CacheAlignment.to_string(), "cache_alignment");
    }

    #[test]
    fn severity_display() {
        assert_eq!(Severity::Error.to_string(), "error");
        assert_eq!(Severity::Warning.to_string(), "warning");
        assert_eq!(Severity::Info.to_string(), "info");
    }

    #[test]
    fn category_hash_eq() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Category::NixpkgsPin);
        set.insert(Category::NixpkgsPin);
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn audit_result_serialization() {
        let result = AuditResult {
            repo: "test-repo".into(),
            path: "/tmp/test".into(),
            timestamp: "2024-01-01T00:00:00Z".into(),
            findings: vec![Finding {
                category: Category::NixpkgsPin,
                severity: Severity::Error,
                message: "test".into(),
                fix: Some("fix it".into()),
                fixed: false,
            }],
            converged: false,
        };
        let json = serde_json::to_string(&result).unwrap();
        let _: AuditResult = serde_json::from_str(&json).unwrap();
    }
}
