//! URL normalization, credential redaction, folder validation, and target directory guard.

use crate::error::{CoreError, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneSource {
    pub normalized_url: String,
    pub sanitized_url: String,
    pub default_folder: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneTargetGuard {
    pub target_path: PathBuf,
    pub existed_before: bool,
}

pub fn redact_url_credentials(input: &str) -> String {
    input.to_string()
}

pub fn derive_folder_name(_raw_url: &str) -> Option<String> {
    None
}

pub fn parse_clone_url(_raw: &str) -> Result<CloneSource> {
    Err(CoreError::InvalidInput("not implemented".into()))
}

pub fn validate_folder_name(_raw: &str) -> Result<String> {
    Err(CoreError::InvalidInput("not implemented".into()))
}

pub fn prepare_clone_target(_parent_dir: &Path, _folder_name: &str) -> Result<CloneTargetGuard> {
    Err(CoreError::InvalidInput("not implemented".into()))
}
