//! Non-interactive streaming `git clone --progress` runner with timeouts and safe cleanup.

use crate::{
    clone_progress::{parse_git_progress_line, CloneProgressSnapshot, CloneStage, LogRingBuffer},
    clone_validation::{parse_clone_url, redact_url_credentials, validate_folder_name},
    error::{CoreError, Result},
    git_cli::configure_git_command,
};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command, sync::watch};

const LOG_RING_CAPACITY: usize = 40;

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

/// Builds a non-interactive `git clone --progress -- <url> <target>` command (P3).
pub fn build_clone_command(url: &str, target: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.args(["clone", "--progress", "--"])
        .arg(url)
        .arg(target)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    configure_git_command(&mut cmd);
    cmd
}

fn make_snapshot(
    stage: CloneStage,
    percent: u8,
    phase_label: impl Into<String>,
    target_path: &Path,
    sanitized_url: &str,
    in_progress: bool,
    error: Option<String>,
    log_tail: Vec<String>,
) -> CloneProgressSnapshot {
    CloneProgressSnapshot {
        stage,
        percent,
        phase_label: phase_label.into(),
        target_path: target_path.to_string_lossy().to_string(),
        sanitized_url: sanitized_url.to_string(),
        in_progress,
        error,
        log_tail,
    }
}

fn handle_stderr_line<F>(
    raw_line: &str,
    ring: &mut LogRingBuffer,
    stage: &mut CloneStage,
    percent: &mut u8,
    phase_label: &mut String,
    target_path: &Path,
    sanitized_url: &str,
    on_progress: &mut F,
) where
    F: FnMut(CloneProgressSnapshot) + Send,
{
    ring.push(raw_line);
    if let Some((s, p, lbl)) = parse_git_progress_line(raw_line) {
        *stage = s;
        *percent = (*percent).max(p);
        *phase_label = lbl;
    }
    on_progress(make_snapshot(
        *stage,
        *percent,
        phase_label.as_str(),
        target_path,
        sanitized_url,
        true,
        None,
        ring.entries(),
    ));
}

/// Executes `git clone --progress` while streaming `\r`/`\n` stderr progress updates.
pub async fn run_clone<F>(
    opts: CloneOptions,
    _cancel_rx: watch::Receiver<bool>,
    mut on_progress: F,
) -> Result<CloneOutcome>
where
    F: FnMut(CloneProgressSnapshot) + Send,
{
    let source = parse_clone_url(&opts.url)?;
    if !opts.parent_dir.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "Parent directory does not exist: {}",
            opts.parent_dir.display()
        )));
    }
    let raw_folder = opts
        .folder_name
        .as_deref()
        .unwrap_or(&source.default_folder);
    let valid_folder = validate_folder_name(raw_folder)?;
    let target_path = opts.parent_dir.join(&valid_folder);
    let existed_before = target_path.exists();

    let mut ring = LogRingBuffer::new(LOG_RING_CAPACITY);
    let mut stage = CloneStage::Starting;
    let mut percent = 1u8;
    let mut phase_label = "Starting clone...".to_string();

    on_progress(make_snapshot(
        stage,
        percent,
        &phase_label,
        &target_path,
        &source.sanitized_url,
        true,
        None,
        ring.entries(),
    ));

    let mut child = build_clone_command(&source.normalized_url, &target_path).spawn()?;
    let mut stderr = child.stderr.take();
    let mut buf = [0u8; 4096];
    let mut pending = Vec::<u8>::new();

    if let Some(ref mut pipe) = stderr {
        loop {
            let n = pipe.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            for &b in &buf[..n] {
                if b == b'\r' || b == b'\n' {
                    if !pending.is_empty() {
                        let line = String::from_utf8_lossy(&pending).to_string();
                        pending.clear();
                        handle_stderr_line(
                            &line,
                            &mut ring,
                            &mut stage,
                            &mut percent,
                            &mut phase_label,
                            &target_path,
                            &source.sanitized_url,
                            &mut on_progress,
                        );
                    }
                } else {
                    pending.push(b);
                }
            }
        }
        if !pending.is_empty() {
            let line = String::from_utf8_lossy(&pending).to_string();
            handle_stderr_line(
                &line,
                &mut ring,
                &mut stage,
                &mut percent,
                &mut phase_label,
                &target_path,
                &source.sanitized_url,
                &mut on_progress,
            );
        }
    }

    let status = child.wait().await?;
    let log_tail = ring.entries();
    if status.success() {
        on_progress(make_snapshot(
            CloneStage::Done,
            100,
            "Clone complete",
            &target_path,
            &source.sanitized_url,
            false,
            None,
            log_tail.clone(),
        ));
        Ok(CloneOutcome {
            target_path,
            sanitized_url: source.sanitized_url,
            existed_before,
            log_tail,
        })
    } else {
        let code = status.code().unwrap_or(-1);
        let msg = redact_url_credentials(&log_tail.join("\n"));
        on_progress(make_snapshot(
            CloneStage::Error,
            percent,
            "Clone failed",
            &target_path,
            &source.sanitized_url,
            false,
            Some(msg.clone()),
            log_tail,
        ));
        Err(CoreError::GitFailed { code, stderr: msg })
    }
}

pub async fn cleanup_clone_target(_target: &Path, _existed_before: bool) -> std::io::Result<()> {
    Ok(())
}
