//! Flake lock file parsing and analysis.

pub mod drift;
pub mod nixpkgs_counter;
pub mod parser;

pub use drift::detect_drift;
pub use nixpkgs_counter::count_nixpkgs_instances;
pub use parser::parse_flake_lock;
