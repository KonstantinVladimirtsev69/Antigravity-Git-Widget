//! Diff inspection and per-hunk accept (stage) / reject (revert) operations.

pub use crate::diff_parser::{parse_unified_diff, CommitInfo, DiffFile, DiffHunk};
use crate::{
    error::{CoreError, Result},
    git_cli::{run_git, run_git_raw, LOCAL_TIMEOUT_SECS},
    status::get_status,
    validation::{ensure_inside_repo, validate_ref_name},
};
use std::{fs, path::Path, process::Stdio};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Returns all changed files (staged, unstaged, untracked) with parsed hunks.
pub async fn get_diff_files(repo_path: &Path) -> Result<Vec<DiffFile>> {
    let (unstaged_res, staged_res, untracked_res) = tokio::join!(
        run_git(repo_path, &["diff", "--no-color", "--unified=3"], LOCAL_TIMEOUT_SECS),
        run_git(
            repo_path,
            &["diff", "--cached", "--no-color", "--unified=3"],
            LOCAL_TIMEOUT_SECS
        ),
        run_git_raw(
            repo_path,
            &["ls-files", "--others", "--exclude-standard"],
            LOCAL_TIMEOUT_SECS
        ),
    );
    let unstaged = unstaged_res?;
    let staged = staged_res?;

    let mut files = Vec::new();
    files.extend(parse_unified_diff(&unstaged.stdout, false));
    files.extend(parse_unified_diff(&staged.stdout, true));

    if let Ok(ut) = untracked_res {
        if ut.success {
            for rel in ut.stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
                let Ok(full) = ensure_inside_repo(repo_path, rel) else {
                    continue;
                };
                let lines: Vec<String> = fs::read_to_string(&full)
                    .map(|c| c.lines().take(200).map(|l| format!("+{l}")).collect())
                    .unwrap_or_else(|_| vec!["+<binary or unreadable>".to_string()]);
                let additions = lines.len();
                files.push(DiffFile {
                    path: rel.to_string(),
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
    }
    Ok(files)
}

/// Returns diff of all commits in current branch compared to a base branch (`main`/`master`/`developer`).
pub async fn get_branch_diff_files(repo_path: &Path, base_branch: Option<&str>) -> Result<Vec<DiffFile>> {
    let base = match base_branch {
        Some(b) if !b.trim().is_empty() => validate_ref_name(b, "base branch")?.to_string(),
        _ => {
            let status = get_status(repo_path).await?;
            ["main", "master", "developer"]
                .into_iter()
                .find(|b| *b != status.branch && status.local_branches.iter().any(|lb| lb == b))
                .map(ToString::to_string)
                .unwrap_or_else(|| "HEAD~1".to_string())
        }
    };

    let range = format!("{base}...HEAD");
    let out = run_git_raw(
        repo_path,
        &["diff", "--no-color", "--unified=3", &range],
        LOCAL_TIMEOUT_SECS,
    )
    .await?;
    if !out.success {
        return Ok(Vec::new());
    }
    Ok(parse_unified_diff(&out.stdout, true))
}

/// Lists recent commits on current branch.
pub async fn get_recent_commits(repo_path: &Path, limit: usize) -> Result<Vec<CommitInfo>> {
    let n = format!("-n{limit}");
    let out = run_git_raw(
        repo_path,
        &["log", &n, "--format=%H%x09%h%x09%s%x09%an%x09%cr"],
        LOCAL_TIMEOUT_SECS,
    )
    .await?;
    if !out.success {
        return Ok(Vec::new());
    }
    let commits = out
        .stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(5, '\t').collect();
            if parts.len() < 5 {
                return None;
            }
            Some(CommitInfo {
                hash: parts[0].to_string(),
                short_hash: parts[1].to_string(),
                subject: parts[2].to_string(),
                author: parts[3].to_string(),
                date_rel: parts[4].to_string(),
            })
        })
        .collect();
    Ok(commits)
}

/// Returns diff files for a specific commit hash.
pub async fn get_commit_diff_files(repo_path: &Path, commit_hash: &str) -> Result<Vec<DiffFile>> {
    let hash = validate_ref_name(commit_hash, "commit hash")?;
    let out = run_git_raw(
        repo_path,
        &["show", "--no-color", "--unified=3", "--format=", hash],
        LOCAL_TIMEOUT_SECS,
    )
    .await?;
    if !out.success {
        return Err(CoreError::GitFailed {
            code: out.code,
            stderr: out.stderr.trim().to_string(),
        });
    }
    Ok(parse_unified_diff(&out.stdout, true))
}

async fn apply_patch(repo_path: &Path, patch: &str, args: &[&str]) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(repo_path)
        .args(["-c", "core.quotePath=false"])
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
    ensure_inside_repo(repo_path, file_path)?;
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
    let safe_path = ensure_inside_repo(repo_path, file_path)?;
    let files = get_diff_files(repo_path).await?;
    let df = files
        .into_iter()
        .find(|f| !f.staged && f.path == file_path)
        .ok_or_else(|| CoreError::InvalidInput(format!("unstaged diff not found: {file_path}")))?;
    if df.status == "untracked" {
        if safe_path.exists() {
            fs::remove_file(safe_path)?;
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
    ensure_inside_repo(repo_path, file_path)?;
    run_git(repo_path, &["add", "--", file_path], LOCAL_TIMEOUT_SECS).await?;
    Ok(())
}

pub async fn unstage_file(repo_path: &Path, file_path: &str) -> Result<()> {
    ensure_inside_repo(repo_path, file_path)?;
    let out = run_git_raw(repo_path, &["reset", "HEAD", "--", file_path], LOCAL_TIMEOUT_SECS).await?;
    if !out.success {
        run_git(repo_path, &["rm", "--cached", "-f", "--", file_path], LOCAL_TIMEOUT_SECS).await?;
    }
    Ok(())
}

pub async fn discard_file(repo_path: &Path, file_path: &str) -> Result<()> {
    let safe_path = ensure_inside_repo(repo_path, file_path)?;
    let out = run_git_raw(repo_path, &["checkout", "--", file_path], LOCAL_TIMEOUT_SECS).await?;
    if !out.success && safe_path.exists() {
        fs::remove_file(safe_path)?;
    }
    Ok(())
}
