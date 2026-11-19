//! Safe non-interactive git.exe runner with timeouts.

use crate::error::{CoreError, Result};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{process::Command, time::timeout};

pub const LOCAL_TIMEOUT_SECS: u64 = 10;
pub const NETWORK_TIMEOUT_SECS: u64 = 30;

pub struct GitOutput {
    pub success: bool,
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Executes `git -C <repo_path> <args...>` with `GIT_TERMINAL_PROMPT=0` and a timeout.
pub async fn run_git_raw(repo_path: &Path, args: &[&str], timeout_secs: u64) -> Result<GitOutput> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(repo_path)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_MESSAGES", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        // Prefer native Windows OpenSSH so non-ASCII user profile paths (e.g. Cyrillic)
        // resolve `~/.ssh/known_hosts` and `~/.ssh/config` reliably.
        if std::env::var_os("GIT_SSH_COMMAND").is_none()
            && Path::new(r"C:\Windows\System32\OpenSSH\ssh.exe").exists()
        {
            cmd.env(
                "GIT_SSH_COMMAND",
                "C:/Windows/System32/OpenSSH/ssh.exe -o StrictHostKeyChecking=accept-new",
            );
        }
        // CREATE_NO_WINDOW prevents flashing console windows on Windows.
        cmd.creation_flags(0x0800_0000);
    }

    let child = cmd.spawn()?;
    let output = timeout(Duration::from_secs(timeout_secs), child.wait_with_output())
        .await
        .map_err(|_| CoreError::Timeout(timeout_secs, args.join(" ")))??;

    Ok(GitOutput {
        success: output.status.success(),
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// Runs a git command and converts non-zero exit status into `CoreError::GitFailed`.
pub async fn run_git(repo_path: &Path, args: &[&str], timeout_secs: u64) -> Result<GitOutput> {
    let out = run_git_raw(repo_path, args, timeout_secs).await?;
    if !out.success {
        return Err(CoreError::GitFailed {
            code: out.code,
            stderr: out.stderr.trim().to_string(),
        });
    }
    Ok(out)
}
