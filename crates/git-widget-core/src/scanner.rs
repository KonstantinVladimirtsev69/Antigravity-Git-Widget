//! Recursive repository scanner supporting standard .git dirs and worktrees.

use crate::{error::Result, integrate::IntegrationMode};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

const IGNORED_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".venv",
    "venv",
    ".git",
    "dist",
    "build",
    "__pycache__",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoInfo {
    pub name: String,
    pub path: String,
    pub is_worktree: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfig {
    pub roots: Vec<String>,
    pub max_depth: usize,
    #[serde(default)]
    pub github_token: Option<String>,
    #[serde(default)]
    pub github_oauth_client_id: Option<String>,
    #[serde(default)]
    pub integration_mode: IntegrationMode,
    #[serde(default)]
    pub update_repo: Option<String>,
    #[serde(default)]
    pub last_clone_dir: Option<String>,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            roots: vec![r"C:\Projects\".to_string()],
            max_depth: 3,
            github_token: None,
            github_oauth_client_id: None,
            integration_mode: IntegrationMode::default(),
            update_repo: None,
            last_clone_dir: None,
        }
    }
}

/// Scans `roots` up to `max_depth` for `.git` directories or worktree `.git` files.
pub fn scan_repositories(roots: &[PathBuf], max_depth: usize) -> Result<Vec<RepoInfo>> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();

    for root in roots {
        if root.exists() {
            walk_dir(root, 0, max_depth, &mut seen, &mut found);
        }
    }

    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(found)
}

fn walk_dir(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    seen: &mut HashSet<PathBuf>,
    out: &mut Vec<RepoInfo>,
) {
    let git_marker = dir.join(".git");
    if git_marker.exists() {
        let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        if seen.insert(canonical.clone()) {
            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| canonical.to_string_lossy().to_string());
            out.push(RepoInfo {
                name,
                path: dir.to_string_lossy().to_string(),
                is_worktree: git_marker.is_file(),
            });
        }
        return;
    }

    if depth >= max_depth {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = entry.file_name();
        let name_str = dir_name.to_string_lossy();
        if name_str.starts_with('.') || IGNORED_DIRS.contains(&name_str.as_ref()) {
            continue;
        }
        walk_dir(&path, depth + 1, max_depth, seen, out);
    }
}
