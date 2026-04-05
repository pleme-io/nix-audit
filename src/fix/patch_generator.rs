//! Generate patched flake.nix content with missing follows declarations added.

use regex::Regex;

/// Generate a patched version of flake.nix content with missing follows added.
///
/// For each input that has a `.url` declaration but no corresponding
/// `.inputs.nixpkgs.follows = "nixpkgs"`, this function inserts the missing
/// follows line immediately after the URL line. Inputs named `nixpkgs`,
/// `flake-utils`, and `systems` are excluded.
///
/// Returns the full corrected flake.nix content (unchanged if nothing to fix).
pub fn generate_follows_patch(content: &str) -> String {
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

    // Find inputs that need follows added.
    let mut missing: Vec<String> = Vec::new();
    for input in &inputs_with_url {
        if !inputs_with_follows.contains(input) {
            // Also check for nested form.
            let nested_pattern = format!("{input} = {{");
            let inline_follows = format!("inputs.{input}.inputs.nixpkgs.follows");
            if !content.contains(&nested_pattern) && !content.contains(&inline_follows) {
                missing.push(input.clone());
            }
        }
    }

    if missing.is_empty() {
        return content.to_string();
    }

    // Insert follows lines after each URL line.
    let mut result = content.to_string();
    for input in &missing {
        let url_pattern = format!("{input}.url");
        if let Some(pos) = result.find(&url_pattern) {
            // Find the end of this line.
            if let Some(eol) = result[pos..].find('\n') {
                let insert_pos = pos + eol + 1;
                let follows_line =
                    format!("    inputs.{input}.inputs.nixpkgs.follows = \"nixpkgs\";\n");
                result.insert_str(insert_pos, &follows_line);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_missing_follows() {
        let content = r#"
    inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    inputs.substrate.url = "github:pleme-io/substrate";
"#;
        let patched = generate_follows_patch(content);
        assert!(patched.contains("inputs.substrate.inputs.nixpkgs.follows = \"nixpkgs\""));
    }

    #[test]
    fn skips_existing_follows() {
        let content = r#"
    inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    inputs.substrate.url = "github:pleme-io/substrate";
    inputs.substrate.inputs.nixpkgs.follows = "nixpkgs";
"#;
        let patched = generate_follows_patch(content);
        assert_eq!(patched, content);
    }

    #[test]
    fn skips_nixpkgs_and_flake_utils() {
        let content = r#"
    inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    inputs.flake-utils.url = "github:numtide/flake-utils";
"#;
        let patched = generate_follows_patch(content);
        assert_eq!(patched, content);
    }

    #[test]
    fn multiple_missing_follows() {
        let content = r#"
    inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    inputs.foo.url = "github:example/foo";
    inputs.bar.url = "github:example/bar";
"#;
        let patched = generate_follows_patch(content);
        assert!(patched.contains("inputs.foo.inputs.nixpkgs.follows"));
        assert!(patched.contains("inputs.bar.inputs.nixpkgs.follows"));
    }

    #[test]
    fn nested_form_not_patched() {
        let content = r#"
    inputs = {
      nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
      substrate = {
        url = "github:pleme-io/substrate";
        inputs.nixpkgs.follows = "nixpkgs";
      };
    };
"#;
        let patched = generate_follows_patch(content);
        assert_eq!(patched, content);
    }
}
