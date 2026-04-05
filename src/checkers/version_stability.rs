//! Checker: version strings must be deterministic and not change every evaluation.

use crate::types::{Category, Finding, Severity};

/// Check for non-deterministic version injection patterns.
///
/// `builtins.currentTime` changes the derivation hash on every evaluation.
/// Git SHA injection via `builtins.getEnv` busts the cache on every commit.
/// Both should use fixed metadata from `Cargo.toml` or runtime injection.
pub fn check_version_stability(content: &str) -> Vec<Finding> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_content_passes() {
        let content = r#"
        {
          version = "1.0.0";
        }
        "#;
        assert!(check_version_stability(content).is_empty());
    }

    #[test]
    fn current_time_fails() {
        let content = r#"
          version = builtins.currentTime;
        "#;
        let findings = check_version_stability(content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Error);
        assert!(findings[0].message.contains("currentTime"));
    }

    #[test]
    fn git_sha_env_fails() {
        let content = r#"
          GIT_SHA = builtins.getEnv "GIT_SHA";
        "#;
        let findings = check_version_stability(content);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("Git SHA"));
    }

    #[test]
    fn get_env_without_git_passes() {
        let content = r#"
          HOME = builtins.getEnv "HOME";
        "#;
        assert!(check_version_stability(content).is_empty());
    }

    #[test]
    fn both_violations() {
        let content = r#"
          version = builtins.currentTime;
          GIT_SHA = builtins.getEnv "GIT_SHA";
        "#;
        let findings = check_version_stability(content);
        assert_eq!(findings.len(), 2);
    }
}
