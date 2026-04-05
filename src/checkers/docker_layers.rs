//! Checker: Docker images should use buildLayeredImage for layer caching.

use crate::types::{Category, Finding, Severity};

/// Check that Docker images use `buildLayeredImage` instead of `buildImage`.
///
/// `dockerTools.buildImage` produces a single-layer image with no layer
/// caching. `buildLayeredImage` splits the closure into layers so that
/// unchanged layers are reused across builds and pulls.
pub fn check_docker_layers(content: &str) -> Vec<Finding> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layered_image_passes() {
        let content = r#"
          image = pkgs.dockerTools.buildLayeredImage {
            name = "my-app";
            maxLayers = 120;
          };
        "#;
        assert!(check_docker_layers(content).is_empty());
    }

    #[test]
    fn build_image_warns() {
        let content = r#"
          image = pkgs.dockerTools.buildImage {
            name = "my-app";
          };
        "#;
        let findings = check_docker_layers(content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].category, Category::DockerLayers);
    }

    #[test]
    fn no_docker_at_all_passes() {
        let content = r#"
        {
          outputs = { self, nixpkgs }: {
            packages.x86_64-linux.default = nixpkgs.hello;
          };
        }
        "#;
        assert!(check_docker_layers(content).is_empty());
    }

    #[test]
    fn both_present_passes() {
        // If buildLayeredImage is present, buildImage substring also matches,
        // but we only flag when buildLayeredImage is absent.
        let content = r#"
          image = pkgs.dockerTools.buildLayeredImage {
            name = "my-app";
          };
        "#;
        assert!(check_docker_layers(content).is_empty());
    }
}
