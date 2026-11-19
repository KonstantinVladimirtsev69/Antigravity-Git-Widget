//! Self-installer for standalone `git-widget-sidecar.exe` distribution.

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

/// Returns `true` if the binary should run as an installer instead of a sidecar server.
pub fn should_run_installer() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--install") {
        return true;
    }
    if args.iter().any(|a| a == "--serve") {
        return false;
    }
    std::env::var_os("ANTIGRAVITY_SIDECAR_WEB_PORT").is_none()
}

pub fn run_self_install() {
    let explicit_flag = std::env::args().any(|a| a == "--install");
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .expect("Cannot resolve USERPROFILE / HOME directory");

    let plugin_dir = home
        .join(".gemini")
        .join("config")
        .join("plugins")
        .join("git-widget");

    let current_exe = std::env::current_exe().expect("Cannot locate current executable");
    match install_to_dir(&current_exe, &plugin_dir, true) {
        Ok(target_exe) => {
            println!("[git-widget] Installed successfully!");
            println!("  Plugin dir : {}", plugin_dir.display());
            println!("  Executable : {}", target_exe.display());
            println!("\nRestart Antigravity 2.0 or open 'Git Widget' in the right auxiliary pane.");
        }
        Err(err) => {
            eprintln!("[git-widget] Installation failed: {err}");
        }
    }

    if !explicit_flag {
        print!("\nPress Enter to close...");
        let _ = io::stdout().flush();
        let mut buf = String::new();
        let _ = io::stdin().read_line(&mut buf);
    }
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
    use std::{fs, time::{SystemTime, UNIX_EPOCH}};

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
