//! Checker: source trees should be filtered to avoid including non-build files.

use crate::types::{Category, Finding, Severity};

/// Check that source references use filtering (cleanSource, fileset, etc.).
///
/// Bare `src = ./.` or `src = self` without any filtering function will
/// include `.git/`, `target/`, `flake.lock`, and other non-build artifacts
/// in the derivation hash, causing unnecessary rebuilds.
pub fn check_source_filtering(content: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtered_source_passes() {
        let content = r#"
          src = cleanSource ./.;
        "#;
        assert!(check_source_filtering(content).is_empty());
    }

    #[test]
    fn fileset_passes() {
        let content = r#"
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [ ./src ./Cargo.toml ./Cargo.lock ];
          };
        "#;
        assert!(check_source_filtering(content).is_empty());
    }

    #[test]
    fn bare_self_warns() {
        let content = r#"
          src = self;
          name = "my-tool";
        "#;
        let findings = check_source_filtering(content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].category, Category::SourceFiltering);
    }

    #[test]
    fn bare_dot_warns() {
        let content = r#"
          src = ./.;
          name = "my-tool";
        "#;
        let findings = check_source_filtering(content);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn clean_cargo_source_passes() {
        let content = r#"
          src = cleanCargoSource ./.;
        "#;
        assert!(check_source_filtering(content).is_empty());
    }

    #[test]
    fn no_source_at_all_passes() {
        let content = r#"
        {
          outputs = { self, nixpkgs }: {};
        }
        "#;
        assert!(check_source_filtering(content).is_empty());
    }
}
