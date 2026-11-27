//! Update-time T3 Code ASAR watcher (`--ensure-t3-patched --watch-update`).
//! Observes T3 Code installer replacement of `app.asar` without ever killing T3 Code (Prohibition P4).

use crate::t3_installer::{
    git_widget_local_dir, resolve_t3_dir, try_ensure_t3_patched_in, PatchAttempt,
};
use std::{
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchOutcome {
    Repatched,
    RepatchedAfterUnlock,
    StillPatchedTimeout,
    UnsafeFuses,
    MissingTarget,
    UnlockTimeout,
}

#[derive(Debug, Clone)]
pub struct WatchConfig {
    pub poll_interval: Duration,
    pub update_wait_timeout: Duration,
    pub unlock_wait_timeout: Duration,
    pub unlock_poll_interval: Duration,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(50),
            update_wait_timeout: Duration::from_secs(90),
            unlock_wait_timeout: Duration::from_secs(600),
            unlock_poll_interval: Duration::from_millis(500),
        }
    }
}

/// Watches `t3_dir` for an unpatched `app.asar` after a T3 Code update.
/// If `app.asar` is locked by a newly restarted T3 Code, notifies once via `on_locked`
/// and waits for unlock rather than killing T3 Code.
pub fn watch_and_repatch_t3_with<F: FnMut()>(
    t3_dir_override: Option<&Path>,
    install_dir: &Path,
    cfg: &WatchConfig,
    mut on_locked: F,
) -> Result<WatchOutcome, String> {
    let t3_dir = match resolve_t3_dir(t3_dir_override) {
        Ok(d) if d.exists() => d,
        _ => return Ok(WatchOutcome::MissingTarget),
    };

    let start = Instant::now();
    loop {
        match try_ensure_t3_patched_in(&t3_dir, install_dir) {
            Ok(PatchAttempt::PatchedNow) => return Ok(WatchOutcome::Repatched),
            Ok(PatchAttempt::UnsafeFuses) => return Ok(WatchOutcome::UnsafeFuses),
            Ok(PatchAttempt::Locked) => break,
            Ok(PatchAttempt::AlreadyPatched | PatchAttempt::MissingTarget) | Err(_) => {
                if start.elapsed() >= cfg.update_wait_timeout {
                    return Ok(WatchOutcome::StillPatchedTimeout);
                }
                thread::sleep(cfg.poll_interval);
            }
        }
    }

    // Brief grace sleep so a transient write lock from installer.exe does not trigger a false toast.
    thread::sleep(cfg.unlock_poll_interval);
    let mut notified = false;
    let lock_start = Instant::now();
    loop {
        match try_ensure_t3_patched_in(&t3_dir, install_dir) {
            Ok(PatchAttempt::PatchedNow | PatchAttempt::AlreadyPatched) => {
                return Ok(WatchOutcome::RepatchedAfterUnlock);
            }
            Ok(PatchAttempt::UnsafeFuses) => return Ok(WatchOutcome::UnsafeFuses),
            Ok(PatchAttempt::Locked | PatchAttempt::MissingTarget) | Err(_) => {
                if !notified {
                    notified = true;
                    on_locked();
                }
                if lock_start.elapsed() >= cfg.unlock_wait_timeout {
                    return Ok(WatchOutcome::UnlockTimeout);
                }
                thread::sleep(cfg.unlock_poll_interval);
            }
        }
    }
}

pub fn watch_and_repatch_t3(t3_dir_override: Option<&Path>) -> Result<WatchOutcome, String> {
    let install_dir = git_widget_local_dir()?;
    watch_and_repatch_t3_with(
        t3_dir_override,
        &install_dir,
        &WatchConfig::default(),
        show_t3_restart_toast,
    )
}

/// Best-effort Windows toast prompting the user to restart T3 Code once to restore Git Widget.
pub fn show_t3_restart_toast() {
    #[cfg(windows)]
    {
        let ps = concat!(
            "$ErrorActionPreference='SilentlyContinue';",
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null;",
            "[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null;",
            "$xml = New-Object Windows.Data.Xml.Dom.XmlDocument;",
            "$xml.LoadXml('<toast><visual><binding template=\"ToastText02\"><text id=\"1\">Git Widget</text><text id=\"2\">T3 Code updated. Restart T3 Code once to restore Git Widget.</text></binding></visual></toast>');",
            "$toast = [Windows.UI.Notifications.ToastNotification]::new($xml);",
            "$appId = '{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe';",
            "[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($appId).Show($toast)"
        );
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", ps])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}
