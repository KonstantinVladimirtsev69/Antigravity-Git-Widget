//! Non-interactive streaming `git clone --progress` runner with timeouts and safe cleanup.

use crate::{
    clone_progress::CloneProgressSnapshot,
    error::{CoreError, Result},
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{process::Command, sync::watch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloneTimeouts {
    pub idle: Duration,
    pub total: Duration,
}

impl Default for CloneTimeouts {
    fn default() -> Self {
        Self {
            idle: Duration::from_secs(300),
            total: Duration::from_secs(900),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CloneOptions {
    pub url: String,
    pub parent_dir: PathBuf,
    pub folder_name: Option<String>,
    pub timeouts: CloneTimeouts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneOutcome {
    pub target_path: PathBuf,
    pub sanitized_url: String,
    pub existed_before: bool,
    pub log_tail: Vec<String>,
}

pub fn build_clone_command(_url: &str, _target: &Path) -> Command {
    Command::new("git")
}

pub async fn run_clone<F>(
    _opts: CloneOptions,
    _cancel_rx: watch::Receiver<bool>,
    _on_progress: F,
) -> Result<CloneOutcome>
where
    F: FnMut(CloneProgressSnapshot) + Send,
{
    Err(CoreError::InvalidInput("not implemented".into()))
}

pub async fn cleanup_clone_target(_target: &Path, _existed_before: bool) -> std::io::Result<()> {
    Ok(())
}
