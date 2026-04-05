//! Convergence metrics computation from audit run history.

use crate::types::ConvergenceRecord;
use std::collections::BTreeMap;

/// Compute the compliance ratio from raw counts.
///
/// Returns a value between 0.0 (no repos passing) and 1.0 (all repos passing).
#[allow(clippy::cast_precision_loss, dead_code)]
pub fn compute_compliance_ratio(passing: usize, total: usize) -> f64 {
    if total == 0 {
        return 1.0;
    }
    passing as f64 / total as f64
}

/// Compute convergence velocity: the rate of compliance improvement per run.
///
/// Returns the average change in compliance ratio between consecutive runs.
/// Positive means improving, negative means regressing, zero means stalled.
pub fn compute_convergence_velocity(history: &[ConvergenceRecord]) -> f64 {
    if history.len() < 2 {
        return 0.0;
    }

    // History is most-recent-first, so reverse for chronological order.
    let mut deltas = Vec::new();
    for i in 1..history.len() {
        // history[i] is older than history[i-1]
        let delta = history[i - 1].compliance_ratio - history[i].compliance_ratio;
        deltas.push(delta);
    }

    if deltas.is_empty() {
        return 0.0;
    }

    #[allow(clippy::cast_precision_loss)]
    let avg = deltas.iter().sum::<f64>() / deltas.len() as f64;
    avg
}

/// Identify categories that persist across multiple runs without being fixed.
///
/// Returns a map of category name to the number of runs it appeared in.
pub fn identify_stubborn_categories(
    category_occurrences: &[(String, usize)],
) -> BTreeMap<String, usize> {
    category_occurrences
        .iter()
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compliance_ratio_all_passing() {
        assert!((compute_compliance_ratio(10, 10) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compliance_ratio_none_passing() {
        assert!((compute_compliance_ratio(0, 10) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compliance_ratio_empty() {
        assert!((compute_compliance_ratio(0, 0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn velocity_improving() {
        let history = vec![
            ConvergenceRecord {
                id: 3,
                timestamp: "t3".into(),
                total_repos: 10,
                passing_repos: 10,
                compliance_ratio: 1.0,
            },
            ConvergenceRecord {
                id: 2,
                timestamp: "t2".into(),
                total_repos: 10,
                passing_repos: 7,
                compliance_ratio: 0.7,
            },
            ConvergenceRecord {
                id: 1,
                timestamp: "t1".into(),
                total_repos: 10,
                passing_repos: 5,
                compliance_ratio: 0.5,
            },
        ];
        let v = compute_convergence_velocity(&history);
        assert!(v > 0.0, "Expected positive velocity, got {v}");
    }

    #[test]
    fn velocity_regressing() {
        let history = vec![
            ConvergenceRecord {
                id: 2,
                timestamp: "t2".into(),
                total_repos: 10,
                passing_repos: 3,
                compliance_ratio: 0.3,
            },
            ConvergenceRecord {
                id: 1,
                timestamp: "t1".into(),
                total_repos: 10,
                passing_repos: 8,
                compliance_ratio: 0.8,
            },
        ];
        let v = compute_convergence_velocity(&history);
        assert!(v < 0.0, "Expected negative velocity, got {v}");
    }

    #[test]
    fn velocity_single_run() {
        let history = vec![ConvergenceRecord {
            id: 1,
            timestamp: "t1".into(),
            total_repos: 10,
            passing_repos: 5,
            compliance_ratio: 0.5,
        }];
        assert!((compute_convergence_velocity(&history) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn stubborn_categories_mapping() {
        let occurrences = vec![
            ("nixpkgs_pin".to_string(), 5),
            ("follows_chain".to_string(), 3),
        ];
        let stubborn = identify_stubborn_categories(&occurrences);
        assert_eq!(stubborn["nixpkgs_pin"], 5);
        assert_eq!(stubborn["follows_chain"], 3);
    }
}
