//! Checker: all flake inputs must follow the top-level nixpkgs to avoid closure duplication.

use crate::types::{Category, Finding, Severity};
use regex::Regex;

/// Check that all flake inputs follow the top-level nixpkgs input.
///
/// Scans for inputs that declare a `.url` but lack a corresponding
/// `.inputs.nixpkgs.follows = "nixpkgs"` declaration. Inputs named
/// `nixpkgs`, `flake-utils`, or `systems` are excluded since they
/// either are nixpkgs or have no nixpkgs dependency.
pub fn check_follows_chain(content: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    let url_re = Regex::new(r#"(\w[\w-]*)\.url\s*="#).unwrap();
    let follows_re = Regex::new(r#"(\w[\w-]*)\.inputs\.nixpkgs\.follows"#).unwrap();

    let inputs_with_url: Vec<String> = url_re
        .captures_iter(content)
        .filter_map(|c| {
            let name = c.get(1)?.as_str().to_string();
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
        let has_follows = inputs_with_follows.contains(input);
        if !has_follows {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correct_follows_passes() {
        let content = r#"
        {
          inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
          inputs.substrate.url = "github:pleme-io/substrate";
          inputs.substrate.inputs.nixpkgs.follows = "nixpkgs";
        }
        "#;
        assert!(check_follows_chain(content).is_empty());
    }

    #[test]
    fn missing_follows_warns() {
        let content = r#"
        {
          inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
          inputs.substrate.url = "github:pleme-io/substrate";
        }
        "#;
        let findings = check_follows_chain(content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].category, Category::FollowsChain);
        assert!(findings[0].message.contains("substrate"));
    }

    #[test]
    fn flake_utils_excluded() {
        let content = r#"
        {
          inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
          inputs.flake-utils.url = "github:numtide/flake-utils";
        }
        "#;
        assert!(check_follows_chain(content).is_empty());
    }

    #[test]
    fn nested_form_passes() {
        let content = r#"
        {
          inputs = {
            nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
            substrate = {
              url = "github:pleme-io/substrate";
              inputs.nixpkgs.follows = "nixpkgs";
            };
          };
        }
        "#;
        assert!(check_follows_chain(content).is_empty());
    }

    #[test]
    fn multiple_missing_follows() {
        let content = r#"
        {
          inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
          inputs.foo.url = "github:example/foo";
          inputs.bar.url = "github:example/bar";
        }
        "#;
        let findings = check_follows_chain(content);
        assert_eq!(findings.len(), 2);
    }
}
