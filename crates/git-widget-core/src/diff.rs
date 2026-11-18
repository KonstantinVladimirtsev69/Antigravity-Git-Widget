//! Unified diff parser and per-hunk accept (stage) / reject (revert) operations.

use crate::{
    error::{CoreError, Result},
    git_cli::{run_git, run_git_raw, LOCAL_TIMEOUT_SECS},
    status::get_status,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, process::Stdio};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub index: usize,
    pub header: String,
    pub lines: Vec<String>,
    pub raw_patch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffFile {
    pub path: String,
    pub status: String,
    pub staged: bool,
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<DiffHunk>,
}

/// Returns all changed files (staged, unstaged, untracked) with parsed hunks.
pub async fn get_diff_files(repo_path: &Path) -> Result<Vec<DiffFile>> {
    let status = get_status(repo_path).await?;
    let unstaged = run_git(repo_path, &["diff", "--no-color", "--unified=3"], LOCAL_TIMEOUT_SECS).await?;
    let staged = run_git(
        repo_path,
        &["diff", "--cached", "--no-color", "--unified=3"],
        LOCAL_TIMEOUT_SECS,
    )
    .await?;

    let mut files = Vec::new();
    files.extend(parse_unified_diff(&unstaged.stdout, false));
    files.extend(parse_unified_diff(&staged.stdout, true));

    for ch in status.changes {
        if ch.code == "??" {
            let full = repo_path.join(&ch.path);
            let lines: Vec<String> = fs::read_to_string(&full)
                .map(|c| c.lines().take(200).map(|l| format!("+{l}")).collect())
                .unwrap_or_else(|_| vec!["+<binary or unreadable>".to_string()]);
            let additions = lines.len();
            files.push(DiffFile {
                path: ch.path,
                status: "untracked".to_string(),
                staged: false,
                additions,
                deletions: 0,
                hunks: vec![DiffHunk {
                    index: 0,
                    header: "@@ -0,0 +1 @@ (new file)".to_string(),
                    lines,
                    raw_patch: String::new(),
                }],
            });
        }
    }
    Ok(files)
}

fn parse_unified_diff(raw: &str, staged: bool) -> Vec<DiffFile> {
    let normalized = raw.replace("\r\n", "\n");
    let mut result = Vec::new();
    let sections: Vec<&str> = normalized.split("\ndiff --git ").collect();

    for (idx, sec) in sections.into_iter().enumerate() {
        let block = if idx == 0 {
            sec.strip_prefix("diff --git ").unwrap_or("")
        } else {
            sec
        };
        if block.trim().is_empty() {
            continue;
        }
        if let Some(df) = parse_file_section(block, staged) {
            result.push(df);
        }
    }
    result
}

fn parse_file_section(block: &str, staged: bool) -> Option<DiffFile> {
    let mut lines_iter = block.lines();
    let first_line = lines_iter.next()?;
    let path = first_line
        .split(" b/")
        .nth(1)
        .unwrap_or(first_line)
        .trim()
        .to_string();

    let mut header_lines = vec![format!("diff --git {first_line}")];
    let mut hunks = Vec::new();
    let mut cur_header = String::new();
    let mut cur_lines: Vec<String> = Vec::new();
    let mut additions = 0usize;
    let mut deletions = 0usize;
    let mut in_hunk = false;

    for line in lines_iter {
        if line.starts_with("@@ ") {
            if in_hunk {
                let raw_patch = build_patch(&header_lines, &cur_header, &cur_lines);
                hunks.push(DiffHunk {
                    index: hunks.len(),
                    header: cur_header.clone(),
                    lines: cur_lines.clone(),
                    raw_patch,
                });
                cur_lines.clear();
            }
            in_hunk = true;
            cur_header = line.to_string();
        } else if in_hunk {
            if line.starts_with('+') && !line.starts_with("+++") {
                additions += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                deletions += 1;
            }
            cur_lines.push(line.to_string());
        } else {
            header_lines.push(line.to_string());
        }
    }

    if in_hunk {
        let raw_patch = build_patch(&header_lines, &cur_header, &cur_lines);
        hunks.push(DiffHunk {
            index: hunks.len(),
            header: cur_header,
            lines: cur_lines,
            raw_patch,
        });
    }

    Some(DiffFile {
        path,
        status: if staged { "staged" } else { "modified" }.to_string(),
        staged,
        additions,
        deletions,
        hunks,
    })
}

fn build_patch(headers: &[String], hunk_header: &str, hunk_lines: &[String]) -> String {
    let mut out = headers.join("\n");
    out.push('\n');
    out.push_str(hunk_header);
    out.push('\n');
    for l in hunk_lines {
        out.push_str(l);
        out.push('\n');
    }
    out
}

async fn apply_patch(repo_path: &Path, patch: &str, args: &[&str]) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(repo_path)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let mut child = cmd.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(patch.as_bytes()).await?;
    }
    let out = child.wait_with_output().await?;
    if !out.status.success() {
        return Err(CoreError::GitFailed {
            code: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(())
}

/// Accepts (stages) a single hunk of an unstaged file.
pub async fn accept_hunk(repo_path: &Path, file_path: &str, hunk_index: usize) -> Result<()> {
    let files = get_diff_files(repo_path).await?;
    let df = files
        .into_iter()
        .find(|f| !f.staged && f.path == file_path)
        .ok_or_else(|| CoreError::InvalidInput(format!("unstaged diff not found: {file_path}")))?;
    let hunk = df
        .hunks
        .into_iter()
        .find(|h| h.index == hunk_index)
        .ok_or_else(|| CoreError::InvalidInput(format!("hunk {hunk_index} not found")))?;
    if hunk.raw_patch.is_empty() {
        run_git(repo_path, &["add", "--", file_path], LOCAL_TIMEOUT_SECS).await?;
        return Ok(());
    }
    apply_patch(repo_path, &hunk.raw_patch, &["apply", "--cached", "--whitespace=nowarn", "-"]).await
}

/// Rejects (reverts) a single hunk in the working tree.
pub async fn reject_hunk(repo_path: &Path, file_path: &str, hunk_index: usize) -> Result<()> {
    let files = get_diff_files(repo_path).await?;
    let df = files
        .into_iter()
        .find(|f| !f.staged && f.path == file_path)
        .ok_or_else(|| CoreError::InvalidInput(format!("unstaged diff not found: {file_path}")))?;
    if df.status == "untracked" {
        let full = repo_path.join(file_path);
        if full.exists() {
            fs::remove_file(full)?;
        }
        return Ok(());
    }
    let hunk = df
        .hunks
        .into_iter()
        .find(|h| h.index == hunk_index)
        .ok_or_else(|| CoreError::InvalidInput(format!("hunk {hunk_index} not found")))?;
    apply_patch(repo_path, &hunk.raw_patch, &["apply", "-R", "--whitespace=nowarn", "-"]).await
}

pub async fn stage_file(repo_path: &Path, file_path: &str) -> Result<()> {
    run_git(repo_path, &["add", "--", file_path], LOCAL_TIMEOUT_SECS).await?;
    Ok(())
}

pub async fn unstage_file(repo_path: &Path, file_path: &str) -> Result<()> {
    run_git(repo_path, &["reset", "HEAD", "--", file_path], LOCAL_TIMEOUT_SECS).await?;
    Ok(())
}

pub async fn discard_file(repo_path: &Path, file_path: &str) -> Result<()> {
    let out = run_git_raw(repo_path, &["checkout", "--", file_path], LOCAL_TIMEOUT_SECS).await?;
    if !out.success {
        let full = repo_path.join(file_path);
        if full.exists() {
            fs::remove_file(full)?;
        }
    }
    Ok(())
}
