//! Core Git operations, repository scanner, and safety guards for git-widget.

pub mod branches;
pub mod diff;
pub mod error;
pub mod git_cli;
pub mod github;
pub mod graph;
pub mod integrate;
pub mod ops;
pub mod safety;
pub mod scanner;
pub mod stash;
pub mod status;
pub mod sync;

pub use error::{CoreError, Result};

/// Returns crate version.
pub fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
