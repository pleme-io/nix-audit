//! Convergence tracking: persistent audit history, metrics, and dashboard.

pub mod dashboard;
pub mod metrics;
pub mod store;

pub use dashboard::print_convergence_dashboard;
#[allow(unused_imports)]
pub use metrics::{compute_compliance_ratio, compute_convergence_velocity};
pub use store::ConvergenceStore;
