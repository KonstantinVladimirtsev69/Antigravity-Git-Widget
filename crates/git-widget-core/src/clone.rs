//! Non-interactive streaming `git clone --progress` runner with timeouts and safe cleanup.

use crate::{
    clone_progress::{
        classify_clone_error, parse_git_progress_line, CloneProgressSnapshot, CloneStage,
        LogRingBuffer,
    },
    clone_validation::{parse_clone_url, prepare_clone_target},
    error::{CoreError, Result},
    git_cli::configure_git_command,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
    sync::watch,
    time::{sleep, sleep_until, Instant},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const LOG_RING_CAPACITY: usize = 40;
const CLEANUP_MAX_ATTEMPTS: usize = 20;
const CLEANUP_RETRY_DELAY: Duration = Duration::from_millis(100);

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

/// Forcefully kills `git.exe` and its child process tree (`git-remote-https`, `ssh`, etc.)
/// and awaits child handle exit before filesystem cleanup (R5).
pub async fn kill_process_tree(pid: Option<u32>, child: &mut Child) {
    #[cfg(windows)]
    if let Some(p) = pid {
        let mut tk = StdCommand::new("taskkill");
        tk.args(["/F", "/T", "/PID", &p.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000);
        let _ = tk.status();
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

fn clear_readonly_recursive(path: &Path) {
    if let Ok(meta) = fs::symlink_metadata(path) {
        let mut perms = meta.permissions();
        if perms.readonly() {
            perms.set_readonly(false);
            let _ = fs::set_permissions(path, perms);
        }
        if meta.is_dir() {
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    clear_readonly_recursive(&entry.path());
                }
            }
        }
    }
}

fn remove_target_once(target: &Path, existed_before: bool) -> std::io::Result<()> {
    if !target.exists() && target.symlink_metadata().is_err() {
        return Ok(());
    }
    if !existed_before {
        return fs::remove_dir_all(target);
    }
    for entry in fs::read_dir(target)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            fs::remove_dir_all(&p)?;
        } else {
            fs::remove_file(&p)?;
        }
    }
    Ok(())
}

/// Removes cloned artifacts strictly according to `existed_before` (P1), retrying up to ~2s
/// while clearing Windows `readonly` attributes on `.git/objects/pack/*` (EBUSY backstop).
pub async fn cleanup_clone_target(target: &Path, existed_before: bool) -> std::io::Result<()> {
    for attempt in 0..CLEANUP_MAX_ATTEMPTS {
        match remove_target_once(target, existed_before) {
            Ok(()) => return Ok(()),
            Err(err) if attempt + 1 < CLEANUP_MAX_ATTEMPTS => {
                let _ = err;
                clear_readonly_recursive(target);
                sleep(CLEANUP_RETRY_DELAY).await;
            }
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

/// Executes `git clone --progress` with streaming `\r`/`\n` progress, idle/total timeouts,
/// process-tree cancellation, and safe `existed_before` cleanup.
pub async fn run_clone<F>(
    opts: CloneOptions,
    mut cancel_rx: watch::Receiver<bool>,
    mut on_progress: F,
) -> Result<CloneOutcome>
where
    F: FnMut(CloneProgressSnapshot) + Send,
{
    let source = parse_clone_url(&opts.url)?;
    let raw_folder = opts.folder_name.as_deref().unwrap_or(&source.default_folder);
    let guard = prepare_clone_target(&opts.parent_dir, raw_folder)?;

    if *cancel_rx.borrow() {
        return Err(CoreError::InvalidInput("Clone cancelled by user.".into()));
    }

    let mut ring = LogRingBuffer::new(LOG_RING_CAPACITY);
    let mut stage = CloneStage::Starting;
    let mut percent = 1u8;
    let mut phase_label = "Starting clone...".to_string();
    on_progress(make_snapshot(
        stage, percent, &phase_label, &guard.target_path, &source.sanitized_url, true, None, Vec::new(),
    ));

    let mut child = build_clone_command(&source.normalized_url, &guard.target_path).spawn()?;
    let pid = child.id();
    let mut stderr = child.stderr.take();
    let total_deadline = Instant::now() + opts.timeouts.total;
    let mut last_output_at = Instant::now();
    let mut cancelled = false;
    let mut cancel_open = true;
    let mut timeout_kind: Option<&'static str> = None;
    let mut buf = [0u8; 4096];
    let mut pending = Vec::<u8>::new();

    if let Some(ref mut pipe) = stderr {
        loop {
            let idle_deadline = last_output_at + opts.timeouts.idle;
            tokio::select! {
                biased;
                res = cancel_rx.changed(), if cancel_open => {
                    match res {
                        Ok(()) if *cancel_rx.borrow() => { cancelled = true; break; }
                        Ok(()) => {}
                        Err(_) => { cancel_open = false; }
                    }
                }
                _ = sleep_until(idle_deadline) => { timeout_kind = Some("idle"); break; }
                _ = sleep_until(total_deadline) => { timeout_kind = Some("total"); break; }
                read_res = pipe.read(&mut buf) => {
                    let n = read_res?;
                    if n == 0 { break; }
                    last_output_at = Instant::now();
                    for &b in &buf[..n] {
                        if b == b'\r' || b == b'\n' {
                            if !pending.is_empty() {
                                let line = String::from_utf8_lossy(&pending).to_string();
                                pending.clear();
                                handle_stderr_line(&line, &mut ring, &mut stage, &mut percent, &mut phase_label, &guard.target_path, &source.sanitized_url, &mut on_progress);
                            }
                        } else {
                            pending.push(b);
                        }
                    }
                }
            }
        }
        if !cancelled && timeout_kind.is_none() && !pending.is_empty() {
            let line = String::from_utf8_lossy(&pending).to_string();
            handle_stderr_line(&line, &mut ring, &mut stage, &mut percent, &mut phase_label, &guard.target_path, &source.sanitized_url, &mut on_progress);
        }
    }
    drop(stderr);

    if cancelled || timeout_kind.is_some() {
        return abort_clone(pid, &mut child, &guard.target_path, guard.existed_before, &source.sanitized_url, percent, &ring, cancelled, timeout_kind, &opts.timeouts, &mut on_progress).await;
    }

    let status = loop {
        tokio::select! {
            biased;
            res = cancel_rx.changed(), if cancel_open => {
                match res {
                    Ok(()) if *cancel_rx.borrow() => { cancelled = true; break None; }
                    Ok(()) => {}
                    Err(_) => { cancel_open = false; }
                }
            }
            _ = sleep_until(total_deadline) => { timeout_kind = Some("total"); break None; }
            wait_res = child.wait() => break Some(wait_res?),
        }
    };

    let Some(status) = status else {
        return abort_clone(pid, &mut child, &guard.target_path, guard.existed_before, &source.sanitized_url, percent, &ring, cancelled, timeout_kind, &opts.timeouts, &mut on_progress).await;
    };

    let log_tail = ring.entries();
    if status.success() {
        on_progress(make_snapshot(CloneStage::Done, 100, "Clone complete", &guard.target_path, &source.sanitized_url, false, None, log_tail.clone()));
        Ok(CloneOutcome { target_path: guard.target_path, sanitized_url: source.sanitized_url, existed_before: guard.existed_before, log_tail })
    } else {
        let code = status.code().unwrap_or(-1);
        let _ = cleanup_clone_target(&guard.target_path, guard.existed_before).await;
        let msg = classify_clone_error(code, &log_tail, false, None);
        on_progress(make_snapshot(CloneStage::Error, percent, "Clone failed", &guard.target_path, &source.sanitized_url, false, Some(msg.clone()), log_tail));
        Err(CoreError::GitFailed { code, stderr: msg })
    }
}

#[allow(clippy::too_many_arguments)]
async fn abort_clone<F>(
    pid: Option<u32>,
    child: &mut Child,
    target_path: &Path,
    existed_before: bool,
    sanitized_url: &str,
    percent: u8,
    ring: &LogRingBuffer,
    cancelled: bool,
    timeout_kind: Option<&str>,
    timeouts: &CloneTimeouts,
    on_progress: &mut F,
) -> Result<CloneOutcome>
where
    F: FnMut(CloneProgressSnapshot) + Send,
{
    kill_process_tree(pid, child).await;
    let _ = cleanup_clone_target(target_path, existed_before).await;
    let log_tail = ring.entries();
    let msg = classify_clone_error(-1, &log_tail, cancelled, timeout_kind);
    let (stage, label) = if cancelled { (CloneStage::Cancelled, "Clone cancelled") } else { (CloneStage::Error, "Clone timed out") };
    on_progress(make_snapshot(stage, percent, label, target_path, sanitized_url, false, Some(msg.clone()), log_tail));
    if cancelled {
        Err(CoreError::InvalidInput(msg))
    } else {
        let secs = match timeout_kind { Some("idle") => timeouts.idle.as_secs().max(1), _ => timeouts.total.as_secs().max(1) };
        Err(CoreError::Timeout(secs, msg))
    }
}
