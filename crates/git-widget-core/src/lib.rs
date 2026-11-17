//! Core Git operations, repository scanner, and safety guards for git-widget.

pub mod diff;
pub mod error;
pub mod git_cli;
pub mod ops;
pub mod safety;
pub mod scanner;
pub mod status;

pub use error::{CoreError, Result};

/// Returns crate version.
pub fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
