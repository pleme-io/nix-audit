//! Auto-fix engine and patch generation for flake.nix files.

pub mod auto_fix;
pub mod patch_generator;

pub use auto_fix::fix_flake;
#[allow(unused_imports)]
pub use patch_generator::generate_follows_patch;
