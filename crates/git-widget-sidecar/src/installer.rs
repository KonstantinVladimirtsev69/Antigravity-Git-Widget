//! Multi-host self-installer for `git-widget-sidecar.exe` (Antigravity 2.0 + T3 Code).

use git_widget_sidecar::{cli::InstallTarget, t3_installer};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

const LOGO_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f05032" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <circle cx="6" cy="6" r="2.5"/>
  <circle cx="6" cy="18" r="2.5"/>
  <circle cx="18" cy="8" r="2.5"/>
  <path d="M6 8.5v7"/>
  <path d="M18 10.5c0 4-6 3.5-12 5"/>
</svg>
"##;

fn prompt_target_selection() -> InstallTarget {
    println!("=== Git Widget Installer ===");
    println!("Select installation target:");
    println!("  [1] Antigravity 2.0 (AUX_PANE plugin)");
    println!("  [2] T3 Code (Desktop + Tailscale Mobile Web)");
    println!("  [3] Both (Antigravity 2.0 + T3 Code)");
    print!("Enter choice [1/2/3] (default 3): ");
    let _ = io::stdout().flush();
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
    match input.trim() {
        "1" => InstallTarget::Antigravity,
        "2" => InstallTarget::T3Code,
        _ => InstallTarget::Both,
    }
}

pub fn run_install(
    target_opt: Option<InstallTarget>,
    t3_dir: Option<&Path>,
    explicit_flag: bool,
    auto_close_t3: bool,
) -> Result<(), String> {
    let interactive = !explicit_flag || target_opt.is_none();
    let target = target_opt.unwrap_or_else(|| {
        if explicit_flag {
            InstallTarget::Antigravity
        } else {
            prompt_target_selection()
        }
    });

    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Cannot locate current executable: {e}"))?;

    let mut errors: Vec<String> = Vec::new();

    if matches!(target, InstallTarget::Antigravity | InstallTarget::Both) {
        if let Err(e) = install_antigravity(&current_exe) {
            eprintln!("[git-widget] Antigravity installation failed: {e}");
            errors.push(e);
        }
    }

    if matches!(target, InstallTarget::T3Code | InstallTarget::Both) {
        if let Err(e) = t3_installer::install_t3_code(&current_exe, t3_dir, auto_close_t3, interactive) {
            eprintln!("[git-widget] T3 Code installation failed: {e}");
            errors.push(e);
        }
    }

    if !explicit_flag {
        print!("\nPress Enter to close...");
        let _ = io::stdout().flush();
        let mut buf = String::new();
        let _ = io::stdin().read_line(&mut buf);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn install_antigravity(current_exe: &Path) -> Result<(), String> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| "Cannot resolve USERPROFILE / HOME directory".to_string())?;

    let plugin_dir = home
        .join(".gemini")
        .join("config")
        .join("plugins")
        .join("git-widget");

    let target_exe =
        install_to_dir(current_exe, &plugin_dir, true).map_err(|e| e.to_string())?;
    println!("[git-widget] Antigravity 2.0 plugin installed!");
    println!("  Plugin dir : {}", plugin_dir.display());
    println!("  Executable : {}", target_exe.display());
    Ok(())
}

pub fn install_to_dir(
    source_exe: &Path,
    plugin_dir: &Path,
    stop_running: bool,
) -> io::Result<PathBuf> {
    let assets_dir = plugin_dir.join("assets");
    let sidecar_dir = plugin_dir.join("sidecars").join("git_widget");
    fs::create_dir_all(&assets_dir)?;
    fs::create_dir_all(&sidecar_dir)?;

    let target_exe = sidecar_dir.join("git-widget-sidecar.exe");

    if stop_running && !same_path(source_exe, &target_exe) {
        stop_other_instances();
    }

    if !same_path(source_exe, &target_exe) {
        fs::copy(source_exe, &target_exe)?;
    }

    fs::write(assets_dir.join("logo.svg"), LOGO_SVG)?;

    let plugin_json = serde_json::json!({
        "name": "git-widget",
        "displayName": "Git Widget",
        "description": "Native VS Code-style Git Source Control & Diff Review for Antigravity AUX_PANE",
        "logo": "assets/logo.svg"
    });
    fs::write(
        plugin_dir.join("plugin.json"),
        serde_json::to_string_pretty(&plugin_json).unwrap(),
    )?;

    let sidecar_json = serde_json::json!({
        "command": target_exe.to_string_lossy(),
        "args": [],
        "restart_policy": "always",
        "display_name": "Git Widget",
        "description": "Antigravity AUX_PANE Git workflow sidecar",
        "has_web_ui": true,
        "agent_permissions": {
            "allows_user_interactions": true
        },
        "ui_config": {
            "display_name": "Git Widget",
            "views": [{
                "path": "/",
                "entrypoint": "SIDECAR_UI_ENTRYPOINT_AUX_PANE",
                "title": "Git Widget"
            }]
        }
    });
    fs::write(
        sidecar_dir.join("sidecar.json"),
        serde_json::to_string_pretty(&sidecar_json).unwrap(),
    )?;

    Ok(target_exe)
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

fn stop_other_instances() {
    #[cfg(windows)]
    {
        let pid = std::process::id();
        let ps = format!(
            "Get-Process -Name 'git-widget-sidecar' -ErrorAction SilentlyContinue | Where-Object {{ $_.Id -ne {pid} }} | Stop-Process -Force"
        );
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .output();
        thread::sleep(Duration::from_millis(350));
    }
}

#[cfg(test)]
mod tests {
    use super::install_to_dir;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn installs_plugin_manifests_and_binary() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("gw_inst_test_{nonce}"));
        fs::create_dir_all(&base).unwrap();

        let fake_exe = base.join("git-widget-sidecar.exe");
        fs::write(&fake_exe, b"MZ-FAKE-BIN").unwrap();

        let plugin_dir = base.join("plugin");
        let target = install_to_dir(&fake_exe, &plugin_dir, false).unwrap();

        assert!(target.exists());
        assert!(plugin_dir.join("plugin.json").exists());
        assert!(plugin_dir.join("assets").join("logo.svg").exists());
        let sidecar_raw =
            fs::read_to_string(plugin_dir.join("sidecars").join("git_widget").join("sidecar.json"))
                .unwrap();
        assert!(sidecar_raw.contains("SIDECAR_UI_ENTRYPOINT_AUX_PANE"));

        let _ = fs::remove_dir_all(&base);
    }
}
