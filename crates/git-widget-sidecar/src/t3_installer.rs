//! T3 Code installation, rollback, and background `--ensure-t3-patched` orchestration.

use crate::{
    t3_asar::{is_boot_patched, patch_app_asar, unpatch_app_asar, PatchOutcome},
    t3_fuses::verify_safe_to_patch,
    t3_scheduler::{register_auto_patch_task, unregister_auto_patch_task},
    t3_scripts::deploy_runtime_scripts,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Default, Serialize, Deserialize)]
struct T3InstallConfig {
    t3_dir: Option<String>,
}

pub fn git_widget_local_dir() -> Result<PathBuf, String> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "Cannot resolve LOCALAPPDATA directory".to_string())?;
    Ok(local.join("git-widget"))
}

fn config_file_path(install_dir: &Path) -> PathBuf {
    install_dir.join("t3-config.json")
}

fn load_saved_t3_dir(install_dir: &Path) -> Option<PathBuf> {
    let raw = fs::read_to_string(config_file_path(install_dir)).ok()?;
    let cfg: T3InstallConfig = serde_json::from_str(&raw).ok()?;
    cfg.t3_dir.map(PathBuf::from)
}

fn save_t3_dir(install_dir: &Path, t3_dir: &Path) {
    let cfg = T3InstallConfig {
        t3_dir: Some(t3_dir.to_string_lossy().to_string()),
    };
    if let Ok(json) = serde_json::to_string_pretty(&cfg) {
        let _ = fs::write(config_file_path(install_dir), json);
    }
}

pub fn resolve_t3_dir(override_dir: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(p) = override_dir {
        return Ok(p.to_path_buf());
    }
    if let Ok(install_dir) = git_widget_local_dir() {
        if let Some(saved) = load_saved_t3_dir(&install_dir) {
            if saved.exists() {
                return Ok(saved);
            }
        }
    }
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "Cannot resolve LOCALAPPDATA directory".to_string())?;
    Ok(local.join("Programs").join("t3code"))
}

fn find_t3_executable(t3_dir: &Path) -> Result<PathBuf, String> {
    let entries = fs::read_dir(t3_dir)
        .map_err(|e| format!("T3 Code directory not found ({}): {e}", t3_dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".exe")
                && !lower.starts_with("uninstall")
                && !lower.contains("elevate")
            {
                return Ok(path);
            }
        }
    }
    Err(format!(
        "T3 Code executable (*.exe) not found in {}",
        t3_dir.display()
    ))
}

fn is_file_locked(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    OpenOptions::new().read(true).write(true).open(path).is_err()
}

fn close_t3_code_processes() {
    #[cfg(windows)]
    {
        let ps = "Get-Process | Where-Object { $_.ProcessName -like 'T3 Code*' -or $_.ProcessName -eq 't3code' } | Stop-Process -Force -ErrorAction SilentlyContinue";
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", ps])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        thread::sleep(Duration::from_millis(700));
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

fn copy_sidecar_exe(source_exe: &Path, target_exe: &Path) -> Result<(), String> {
    if same_path(source_exe, target_exe) {
        return Ok(());
    }
    fs::copy(source_exe, target_exe).map_err(|e| {
        format!(
            "Failed copying {} to {}: {e}",
            source_exe.display(),
            target_exe.display()
        )
    })?;
    Ok(())
}

pub fn install_t3_code(
    source_exe: &Path,
    t3_dir_override: Option<&Path>,
    auto_close_t3: bool,
    interactive: bool,
) -> Result<(), String> {
    let t3_dir = resolve_t3_dir(t3_dir_override)?;
    let exe_path = find_t3_executable(&t3_dir)?;
    let asar_path = t3_dir.join("resources").join("app.asar");
    if !asar_path.exists() {
        return Err(format!("app.asar not found at {}", asar_path.display()));
    }

    verify_safe_to_patch(&exe_path)?;

    if is_file_locked(&asar_path) {
        let should_close = if auto_close_t3 {
            true
        } else if interactive {
            print!("T3 Code is currently running and locks app.asar. Close T3 Code now? [Y/n]: ");
            let _ = io::stdout().flush();
            let mut input = String::new();
            let _ = io::stdin().read_line(&mut input);
            let ans = input.trim().to_ascii_lowercase();
            ans.is_empty() || ans == "y" || ans == "yes" || ans == "д" || ans == "да"
        } else {
            false
        };
        if should_close {
            close_t3_code_processes();
        }
        if is_file_locked(&asar_path) {
            return Err(
                "T3 Code is still running and locks app.asar. Close T3 Code and retry (or pass --auto-close-t3)."
                    .to_string(),
            );
        }
    }

    let install_dir = git_widget_local_dir()?;
    deploy_runtime_scripts(&install_dir)?;
    let target_exe = install_dir.join("git-widget-sidecar.exe");
    copy_sidecar_exe(source_exe, &target_exe)?;
    save_t3_dir(&install_dir, &t3_dir);

    let loader_path = install_dir.join("t3-loader.cjs");
    let outcome = patch_app_asar(&asar_path, &loader_path)?;
    register_auto_patch_task(&install_dir, &target_exe, Some(&t3_dir))?;

    println!("[git-widget] T3 Code integration installed!");
    println!("  T3 Code dir : {}", t3_dir.display());
    println!("  Runtime dir : {}", install_dir.display());
    println!(
        "  ASAR status : {}",
        match outcome {
            PatchOutcome::Patched => "patched (backup saved to app.asar.bak)",
            PatchOutcome::AlreadyPatched => "already patched (idempotent)",
        }
    );
    println!("  Auto-patch  : Windows Task Scheduler registered");
    Ok(())
}

pub fn uninstall_t3_code(t3_dir_override: Option<&Path>) -> Result<(), String> {
    let _ = unregister_auto_patch_task();

    if let Ok(t3_dir) = resolve_t3_dir(t3_dir_override) {
        let asar_path = t3_dir.join("resources").join("app.asar");
        if asar_path.exists() {
            if is_file_locked(&asar_path) {
                return Err(
                    "T3 Code is running and locks app.asar. Close T3 Code before --uninstall-t3."
                        .to_string(),
                );
            }
            let restored = unpatch_app_asar(&asar_path)?;
            if restored {
                println!("[git-widget] Restored {}", asar_path.display());
            }
        }
    }

    if let Ok(install_dir) = git_widget_local_dir() {
        if install_dir.exists() {
            let _ = fs::remove_dir_all(&install_dir);
        }
    }

    println!("[git-widget] T3 Code integration removed cleanly.");
    Ok(())
}

/// Silent background self-healing check (Prohibition P4: never kills T3 Code, exits 0 if locked).
pub fn ensure_t3_patched(t3_dir_override: Option<&Path>) -> Result<(), String> {
    let t3_dir = match resolve_t3_dir(t3_dir_override) {
        Ok(d) if d.exists() => d,
        _ => return Ok(()),
    };
    let asar_path = t3_dir.join("resources").join("app.asar");
    if !asar_path.exists() {
        return Ok(());
    }
    // Fast-path (< 10ms): header + boot.cjs read only, zero disk writes.
    if is_boot_patched(&asar_path).unwrap_or(false) {
        return Ok(());
    }
    // Prohibition P4: silently defer if T3 Code is running and holds a lock.
    if is_file_locked(&asar_path) {
        return Ok(());
    }
    let exe_path = match find_t3_executable(&t3_dir) {
        Ok(p) => p,
        Err(_) => return Ok(()),
    };
    // Prohibition P3: never patch if future T3 Code enables ASAR integrity validation.
    if verify_safe_to_patch(&exe_path).is_err() {
        return Ok(());
    }
    let install_dir = git_widget_local_dir()?;
    let loader_path = install_dir.join("t3-loader.cjs");
    if !loader_path.exists() {
        deploy_runtime_scripts(&install_dir)?;
    }
    let _ = patch_app_asar(&asar_path, &loader_path);
    Ok(())
}
