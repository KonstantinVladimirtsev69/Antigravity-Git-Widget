//! Unified error types for git-widget-core.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("path is not a git repository: {0}")]
    NotAGitRepo(String),

    #[error("git command timed out after {0}s: {1}")]
    Timeout(u64, String),

    #[error("git command failed ({code}): {stderr}")]
    GitFailed { code: i32, stderr: String },

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
