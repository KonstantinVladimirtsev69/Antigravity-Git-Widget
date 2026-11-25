//! Non-interactive streaming `git clone --progress` runner with timeouts and safe cleanup.

use crate::{
    clone_progress::{
        classify_clone_error, CloneProgressSnapshot, CloneStage, ProgressTracker,
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
        let p = entry?.path();
        if p.is_dir() { fs::remove_dir_all(&p)?; } else { fs::remove_file(&p)?; }
    }
    Ok(())
}

/// Removes cloned artifacts strictly according to `existed_before` (P1), retrying up to ~2s
/// while clearing Windows `readonly` attributes on `.git/objects/pack/*` (EBUSY backstop).
pub async fn cleanup_clone_target(target: &Path, existed_before: bool) -> std::io::Result<()> {
    for attempt in 0..CLEANUP_MAX_ATTEMPTS {
        match remove_target_once(target, existed_before) {
            Ok(()) => return Ok(()),
            Err(_) if attempt + 1 < CLEANUP_MAX_ATTEMPTS => {
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
    on_progress: F,
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

    let mut tracker = ProgressTracker::new(
        LOG_RING_CAPACITY,
        &guard.target_path,
        &source.sanitized_url,
        on_progress,
    );
    tracker.emit(CloneStage::Starting, 1, "Starting clone...", true, None);

    let mut child = build_clone_command(&source.normalized_url, &guard.target_path).spawn()?;
    let pid = child.id();
    let mut stderr = child.stderr.take();
    let total_deadline = Instant::now() + opts.timeouts.total;
    let mut last_output_at = Instant::now();
    let (mut cancelled, mut cancel_open, mut timeout_kind) = (false, true, None);
    let (mut buf, mut pending) = ([0u8; 4096], Vec::<u8>::new());

    if let Some(ref mut pipe) = stderr {
        loop {
            let idle_deadline = last_output_at + opts.timeouts.idle;
            tokio::select! {
                biased;
                res = cancel_rx.changed(), if cancel_open => match res {
                    Ok(()) if *cancel_rx.borrow() => { cancelled = true; break; }
                    Ok(()) => {}
                    Err(_) => { cancel_open = false; }
                },
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
                                tracker.push_line(&line);
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
            tracker.push_line(&line);
        }
    }
    drop(stderr);

    if cancelled || timeout_kind.is_some() {
        return abort_clone(pid, &mut child, guard.existed_before, cancelled, timeout_kind, &opts.timeouts, &mut tracker).await;
    }

    let status = loop {
        tokio::select! {
            biased;
            res = cancel_rx.changed(), if cancel_open => match res {
                Ok(()) if *cancel_rx.borrow() => { cancelled = true; break None; }
                Ok(()) => {}
                Err(_) => { cancel_open = false; }
            },
            _ = sleep_until(total_deadline) => { timeout_kind = Some("total"); break None; }
            wait_res = child.wait() => break Some(wait_res?),
        }
    };

    let Some(status) = status else {
        return abort_clone(pid, &mut child, guard.existed_before, cancelled, timeout_kind, &opts.timeouts, &mut tracker).await;
    };

    let log_tail = tracker.ring.entries();
    if status.success() {
        tracker.emit(CloneStage::Done, 100, "Clone complete", false, None);
        Ok(CloneOutcome {
            target_path: guard.target_path,
            sanitized_url: source.sanitized_url,
            existed_before: guard.existed_before,
            log_tail,
        })
    } else {
        let code = status.code().unwrap_or(-1);
        let _ = cleanup_clone_target(&guard.target_path, guard.existed_before).await;
        let msg = classify_clone_error(code, &log_tail, false, None);
        tracker.emit(CloneStage::Error, tracker.percent, "Clone failed", false, Some(msg.clone()));
        Err(CoreError::GitFailed { code, stderr: msg })
    }
}

async fn abort_clone<F: FnMut(CloneProgressSnapshot) + Send>(
    pid: Option<u32>,
    child: &mut Child,
    existed_before: bool,
    cancelled: bool,
    timeout_kind: Option<&str>,
    timeouts: &CloneTimeouts,
    tracker: &mut ProgressTracker<'_, F>,
) -> Result<CloneOutcome> {
    kill_process_tree(pid, child).await;
    let _ = cleanup_clone_target(tracker.target_path, existed_before).await;
    let log_tail = tracker.ring.entries();
    let msg = classify_clone_error(-1, &log_tail, cancelled, timeout_kind);
    let (stage, label) = if cancelled { (CloneStage::Cancelled, "Clone cancelled") } else { (CloneStage::Error, "Clone timed out") };
    tracker.emit(stage, tracker.percent, label, false, Some(msg.clone()));
    if cancelled {
        Err(CoreError::InvalidInput(msg))
    } else {
        let secs = match timeout_kind { Some("idle") => timeouts.idle.as_secs().max(1), _ => timeouts.total.as_secs().max(1) };
        Err(CoreError::Timeout(secs, msg))
    }
}
