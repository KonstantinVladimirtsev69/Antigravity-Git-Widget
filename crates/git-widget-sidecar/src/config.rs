//! Persistent scanner configuration stored in plugin directory.

use git_widget_core::scanner::ScannerConfig;
use std::{fs, path::PathBuf};

fn settings_path() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(
        PathBuf::from(home)
            .join(".gemini")
            .join("config")
            .join("plugins")
            .join("git-widget")
            .join("settings.json"),
    )
}

pub fn load_config() -> ScannerConfig {
    let Some(path) = settings_path() else {
        return ScannerConfig::default();
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return ScannerConfig::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_config(cfg: &ScannerConfig) -> std::io::Result<()> {
    let Some(path) = settings_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cfg).unwrap_or_default();
    fs::write(path, json)
}
