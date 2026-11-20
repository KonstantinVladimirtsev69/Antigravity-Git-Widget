//! Input validation and repository path containment guards.

use crate::error::{CoreError, Result};
use std::path::{Component, Path, PathBuf};

/// Validates a non-empty git CLI argument that must not start with `-` or contain NUL bytes.
pub fn validate_git_arg<'a>(val: &'a str, field: &str) -> Result<&'a str> {
    let trimmed = val.trim();
    if trimmed.is_empty() || trimmed.contains('\0') || trimmed.starts_with('-') {
        return Err(CoreError::InvalidInput(format!("invalid {field}: '{val}'")));
    }
    Ok(trimmed)
}

/// Validates a single git ref, branch name, remote name, or commit hash.
pub fn validate_ref_name<'a>(val: &'a str, field: &str) -> Result<&'a str> {
    let trimmed = validate_git_arg(val, field)?;
    if trimmed.chars().any(char::is_whitespace) || trimmed.contains("..") {
        return Err(CoreError::InvalidInput(format!("invalid {field}: '{val}'")));
    }
    Ok(trimmed)
}

/// Ensures `rel_path` is a safe relative path inside `repo_path` (no traversal or absolute escape).
pub fn ensure_inside_repo(repo_path: &Path, rel_path: &str) -> Result<PathBuf> {
    let trimmed = rel_path.trim();
    if trimmed.is_empty() || trimmed.contains('\0') {
        return Err(CoreError::InvalidInput("file path cannot be empty".into()));
    }
    let rel = Path::new(trimmed);
    if rel.is_absolute() {
        return Err(CoreError::InvalidInput(format!(
            "path must be relative to repository: {rel_path}"
        )));
    }
    for comp in rel.components() {
        match comp {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CoreError::InvalidInput(format!(
                    "path escapes repository: {rel_path}"
                )));
            }
        }
    }

    let joined = repo_path.join(rel);
    if let (Ok(canon_repo), Ok(canon_target)) = (repo_path.canonicalize(), joined.canonicalize()) {
        if !canon_target.starts_with(&canon_repo) {
            return Err(CoreError::InvalidInput(format!(
                "path escapes repository: {rel_path}"
            )));
        }
    }
    Ok(joined)
}
