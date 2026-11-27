//! Multi-host self-update orchestration (`Antigravity` -> `T3 Code`) without killing T3 Code.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostTargets {
    pub antigravity_plugin_dir: Option<PathBuf>,
    pub t3_local_dir: Option<PathBuf>,
}

impl HostTargets {
    pub fn is_empty(&self) -> bool {
        self.antigravity_plugin_dir.is_none() && self.t3_local_dir.is_none()
    }
}

fn default_antigravity_plugin_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(
        PathBuf::from(home)
            .join(".gemini")
            .join("config")
            .join("plugins")
            .join("git-widget"),
    )
}

/// Strictly detects installed hosts.
/// Antigravity is detected ONLY via `sidecars/git_widget/sidecar.json` or `.exe` (never `settings.json`).
pub fn detect_installed_hosts_in(
    ag_plugin_dir: Option<&Path>,
    t3_local_dir: Option<&Path>,
) -> HostTargets {
    let antigravity_plugin_dir = ag_plugin_dir
        .filter(|dir| {
            let sidecar_dir = dir.join("sidecars").join("git_widget");
            sidecar_dir.join("sidecar.json").exists()
                || sidecar_dir.join("git-widget-sidecar.exe").exists()
        })
        .map(Path::to_path_buf);

    let t3_local_dir = t3_local_dir
        .filter(|dir| {
            dir.join("git-widget-sidecar.exe").exists() || dir.join("t3-loader.cjs").exists()
        })
        .map(Path::to_path_buf);

    HostTargets {
        antigravity_plugin_dir,
        t3_local_dir,
    }
}

pub fn detect_installed_hosts() -> HostTargets {
    let ag = default_antigravity_plugin_dir();
    let t3 = crate::t3_installer::git_widget_local_dir().ok();
    detect_installed_hosts_in(ag.as_deref(), t3.as_deref())
}

#[cfg(windows)]
pub fn wait_for_parent_pid(pid: u32, timeout_ms: u32) -> Result<(), String> {
    use std::ffi::c_void;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_OBJECT_0: u32 = 0x0000_0000;
    const WAIT_TIMEOUT: u32 = 0x0000_0102;

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn WaitForSingleObject(handle: *mut c_void, ms: u32) -> u32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    unsafe {
        let handle = OpenProcess(SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return Ok(());
        }
        let res = WaitForSingleObject(handle, timeout_ms);
        CloseHandle(handle);
        match res {
            WAIT_OBJECT_0 => Ok(()),
            WAIT_TIMEOUT => Err(format!("Timed out waiting for parent PID {pid} to exit")),
            other => Err(format!("WaitForSingleObject failed with code {other}")),
        }
    }
}

#[cfg(not(windows))]
pub fn wait_for_parent_pid(_pid: u32, _timeout_ms: u32) -> Result<(), String> {
    Ok(())
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

/// Replaces `target_exe` using rename-before-copy (`target.exe` -> `target.exe.old`)
/// so Windows file locks from another host do not require killing T3 Code.
pub fn replace_exe_rename_before_copy(source_exe: &Path, target_exe: &Path) -> Result<PathBuf, String> {
    if !source_exe.is_file() {
        return Err(format!("Source binary not found: {}", source_exe.display()));
    }
    if let Some(parent) = target_exe.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed creating {}: {e}", parent.display()))?;
    }
    let mut old_path = target_exe.with_extension("exe.old");
    if same_path(source_exe, target_exe) {
        return Ok(old_path);
    }

    if old_path.exists() && fs::remove_file(&old_path).is_err() {
        let pid = std::process::id();
        for seq in 0..1000u32 {
            let candidate = target_exe.with_extension(format!("exe.old.{pid}.{seq}"));
            if !candidate.exists() || fs::remove_file(&candidate).is_ok() {
                old_path = candidate;
                break;
            }
        }
    }
    if target_exe.exists() {
        let mut renamed = false;
        let mut last_err = None;
        for _ in 0..20 {
            match fs::rename(target_exe, &old_path) {
                Ok(()) => {
                    renamed = true;
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
        if !renamed {
            return Err(format!(
                "Failed staging old binary {}: {}",
                target_exe.display(),
                last_err.map_or_else(|| "unknown error".into(), |e| e.to_string())
            ));
        }
    }

    if let Err(e) = fs::copy(source_exe, target_exe) {
        if old_path.exists() && !target_exe.exists() {
            let _ = fs::rename(&old_path, target_exe);
        }
        return Err(format!(
            "Failed copying new binary to {}: {e}",
            target_exe.display()
        ));
    }

    let _ = fs::remove_file(&old_path);
    Ok(old_path)
}

/// Executes multi-host self-update in strict order: `Antigravity` -> `T3 Code`.
/// Never kills T3 Code and never modifies `app.asar`.
pub fn run_self_update(
    source_exe: Option<&Path>,
    parent_pid: u32,
    serve_port: Option<u16>,
    port_file: Option<&Path>,
    ag_dir_override: Option<&Path>,
    t3_dir_override: Option<&Path>,
) -> Result<(), String> {
    let targets = if ag_dir_override.is_some() || t3_dir_override.is_some() {
        detect_installed_hosts_in(ag_dir_override, t3_dir_override)
    } else {
        detect_installed_hosts()
    };
    if targets.is_empty() {
        return Err(
            "No installed Antigravity or T3 Code host detected; refusing self-update".to_string(),
        );
    }

    let src = match source_exe {
        Some(p) => p.to_path_buf(),
        None => std::env::current_exe()
            .map_err(|e| format!("Cannot locate current executable: {e}"))?,
    };

    wait_for_parent_pid(parent_pid, 15_000)?;

    let mut respawn_exe: Option<PathBuf> = None;

    if let Some(ag_dir) = targets.antigravity_plugin_dir.as_deref() {
        let ag_exe = ag_dir
            .join("sidecars")
            .join("git_widget")
            .join("git-widget-sidecar.exe");
        replace_exe_rename_before_copy(&src, &ag_exe)?;
        respawn_exe = Some(ag_exe);
    }

    if let Some(t3_dir) = targets.t3_local_dir.as_deref() {
        let t3_exe = t3_dir.join("git-widget-sidecar.exe");
        replace_exe_rename_before_copy(&src, &t3_exe)?;
        crate::t3_scripts::deploy_runtime_scripts(t3_dir)?;
        if ag_dir_override.is_none() && t3_dir_override.is_none() {
            let _ = crate::t3_scheduler::register_auto_patch_task(t3_dir, &t3_exe, None);
        }
        respawn_exe = Some(t3_exe);
    }

    if let Some(port) = serve_port {
        let exe = respawn_exe
            .ok_or_else(|| "No updated executable available for --serve respawn".to_string())?;
        let mut cmd = Command::new(&exe);
        cmd.args(["--serve", "--port", &port.to_string()]);
        if let Some(pf) = port_file {
            cmd.arg("--port-file").arg(pf);
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.spawn()
            .map_err(|e| format!("Failed respawning {}: {e}", exe.display()))?;
    }

    Ok(())
}

/// Best-effort cleanup of stale `gw-update-*` temp directories and `.exe.old*` backups.
pub fn cleanup_stale_update_dirs() {
    if let Ok(entries) = fs::read_dir(std::env::temp_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| name.starts_with("gw-update-"))
            {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }
    let hosts = detect_installed_hosts();
    for dir in [
        hosts.antigravity_plugin_dir.map(|d| d.join("sidecars").join("git_widget")),
        hosts.t3_local_dir,
    ]
    .into_iter()
    .flatten()
    {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("git-widget-sidecar.exe.old"))
                {
                    let _ = fs::remove_file(&p);
                }
            }
        }
    }
}
