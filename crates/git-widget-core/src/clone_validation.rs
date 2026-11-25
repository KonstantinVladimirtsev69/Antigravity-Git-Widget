//! URL normalization, credential redaction, folder validation, and target directory guard.

use crate::error::{CoreError, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

const FORBIDDEN_WINDOWS_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
const RESERVED_WINDOWS_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

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

/// Replaces `userinfo@` in any `scheme://userinfo@host` occurrences with `***@` (P2).
/// Leaves SCP-style `git@host:path` untouched because it has no `://`.
pub fn redact_url_credentials(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(scheme_idx) = rest.find("://") {
        let after_scheme = scheme_idx + 3;
        out.push_str(&rest[..after_scheme]);
        let tail = &rest[after_scheme..];
        let auth_end = tail
            .find(|c: char| matches!(c, '/' | '?' | '#' | ' ' | '\t' | '\r' | '\n' | '\'' | '"'))
            .unwrap_or(tail.len());
        let authority = &tail[..auth_end];
        if let Some(at_pos) = authority.rfind('@') {
            out.push_str("***@");
            out.push_str(&tail[at_pos + 1..auth_end]);
        } else {
            out.push_str(authority);
        }
        rest = &tail[auth_end..];
    }
    out.push_str(rest);
    out
}

fn percent_decode_utf8(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(val) = u8::from_str_radix(hex, 16) {
                    out.push(val);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Extracts an editable repository folder name from a clone URL or `owner/repo` shorthand.
pub fn derive_folder_name(raw_url: &str) -> Option<String> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_query = trimmed.split(['?', '#']).next().unwrap_or(trimmed);
    let cleaned = without_query.trim_end_matches(['/', '\\']);
    if cleaned.is_empty() {
        return None;
    }

    let last_seg = if let Some((_, after_scheme)) = cleaned.split_once("://") {
        let (_, path_part) = after_scheme.split_once(['/', '\\'])?;
        path_part.rsplit(['/', '\\']).next()?
    } else if let Some(seg) = cleaned.rsplit(['/', '\\']).next() {
        seg.rsplit_once(':').map_or(seg, |(_, p)| p)
    } else {
        return None;
    };

    let without_git = if last_seg.len() > 4 && last_seg[last_seg.len() - 4..].eq_ignore_ascii_case(".git") {
        &last_seg[..last_seg.len() - 4]
    } else {
        last_seg
    };

    let decoded = percent_decode_utf8(without_git).trim().to_string();
    (!decoded.is_empty()).then_some(decoded)
}

fn valid_shorthand_segment(seg: &str) -> bool {
    !seg.is_empty()
        && seg != "."
        && seg != ".."
        && !seg.starts_with('-')
        && !seg.chars().any(|c| c.is_whitespace() || c.is_control() || FORBIDDEN_WINDOWS_CHARS.contains(&c))
}

/// Parses supported clone URLs (`https://`, `http://`, `ssh://`, `file://`, `git@host:path`)
/// and `owner/repo` shorthand (expanded to `https://github.com/owner/repo.git`).
pub fn parse_clone_url(raw: &str) -> Result<CloneSource> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') || trimmed.chars().any(char::is_control) {
        return Err(CoreError::InvalidInput("Invalid clone URL".into()));
    }

    let normalized_url = if let Some(rest) = trimmed.strip_prefix("file://") {
        if rest.is_empty() || rest.starts_with('-') {
            return Err(CoreError::InvalidInput("Invalid file:// clone URL".into()));
        }
        trimmed.to_string()
    } else if trimmed.starts_with("https://")
        || trimmed.starts_with("http://")
        || trimmed.starts_with("ssh://")
    {
        let (_, after) = trimmed.split_once("://").unwrap();
        let (host, path) = after
            .split_once('/')
            .ok_or_else(|| CoreError::InvalidInput("Clone URL is missing repository path".into()))?;
        if host.is_empty() || path.is_empty() || trimmed.chars().any(char::is_whitespace) {
            return Err(CoreError::InvalidInput("Invalid clone URL".into()));
        }
        trimmed.to_string()
    } else if trimmed.contains("://") {
        return Err(CoreError::InvalidInput(
            "Unsupported URL scheme (use https://, http://, ssh://, file://, git@, or owner/repo)".into(),
        ));
    } else if let Some((user, rest)) = trimmed.split_once('@') {
        let (host, path) = rest
            .split_once(':')
            .ok_or_else(|| CoreError::InvalidInput("Invalid SSH clone URL".into()))?;
        if user.is_empty()
            || host.is_empty()
            || path.is_empty()
            || user.starts_with('-')
            || host.starts_with('-')
            || path.starts_with('-')
            || path == "."
            || path == ".."
            || trimmed.chars().any(char::is_whitespace)
        {
            return Err(CoreError::InvalidInput("Invalid SSH clone URL".into()));
        }
        trimmed.to_string()
    } else {
        let Some((owner, repo)) = trimmed.split_once('/') else {
            return Err(CoreError::InvalidInput(
                "Invalid repository URL or owner/repo shorthand".into(),
            ));
        };
        if repo.contains('/') || !valid_shorthand_segment(owner) || !valid_shorthand_segment(repo) {
            return Err(CoreError::InvalidInput("Invalid owner/repo shorthand".into()));
        }
        let repo_clean = repo.strip_suffix(".git").unwrap_or(repo);
        if repo_clean.is_empty() {
            return Err(CoreError::InvalidInput("Invalid owner/repo shorthand".into()));
        }
        format!("https://github.com/{owner}/{repo_clean}.git")
    };

    let sanitized_url = redact_url_credentials(&normalized_url);
    let default_folder = derive_folder_name(&normalized_url)
        .ok_or_else(|| CoreError::InvalidInput("Could not derive folder name from URL".into()))?;

    Ok(CloneSource {
        normalized_url,
        sanitized_url,
        default_folder,
    })
}

/// Validates a target subfolder name against path traversal, flag injection, and Windows rules (R3).
/// Allows internal spaces (e.g. `"my repo"` decoded from `%20`), rejecting leading/trailing spaces.
pub fn validate_folder_name(raw: &str) -> Result<String> {
    if raw.is_empty() || raw.trim().is_empty() || raw != raw.trim() {
        return Err(CoreError::InvalidInput(
            "Folder name cannot be empty or have leading/trailing spaces".into(),
        ));
    }
    if raw == "." || raw == ".." || raw.starts_with('-') || raw.ends_with('.') {
        return Err(CoreError::InvalidInput(format!("Invalid folder name: {raw}")));
    }
    if raw.chars().any(|c| c.is_control() || FORBIDDEN_WINDOWS_CHARS.contains(&c)) {
        return Err(CoreError::InvalidInput(format!(
            "Folder name contains forbidden characters: {raw}"
        )));
    }
    let stem = raw.split('.').next().unwrap_or(raw).to_ascii_uppercase();
    if RESERVED_WINDOWS_NAMES.contains(&stem.as_str()) {
        return Err(CoreError::InvalidInput(format!(
            "Folder name uses a reserved Windows device name: {raw}"
        )));
    }
    Ok(raw.to_string())
}

/// Validates `parent_dir` and `folder_name`, blocking existing non-empty directories (R3, P1).
pub fn prepare_clone_target(parent_dir: &Path, folder_name: &str) -> Result<CloneTargetGuard> {
    if !parent_dir.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "Parent directory does not exist: {}",
            parent_dir.display()
        )));
    }
    let valid_name = validate_folder_name(folder_name)?;
    let target_path = parent_dir.join(&valid_name);
    let existed_before = if target_path.exists() || target_path.symlink_metadata().is_ok() {
        if !target_path.is_dir() {
            return Err(CoreError::InvalidInput(
                "Target path exists and is not a directory. Choose another folder name.".into(),
            ));
        }
        if fs::read_dir(&target_path)?.next().is_some() {
            return Err(CoreError::InvalidInput(
                "Target folder already exists and is not empty. Choose another folder name.".into(),
            ));
        }
        true
    } else {
        false
    };
    Ok(CloneTargetGuard {
        target_path,
        existed_before,
    })
}
