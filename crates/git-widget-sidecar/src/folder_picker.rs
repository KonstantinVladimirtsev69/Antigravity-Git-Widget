//! Native Win32 COM STA folder picker dialog (`IFileOpenDialog`).

#[cfg(windows)]
pub async fn pick_folder(_initial_dir: Option<String>) -> Result<Option<String>, String> {
    Err("folder_picker not implemented".to_string())
}

#[cfg(not(windows))]
pub async fn pick_folder(_initial_dir: Option<String>) -> Result<Option<String>, String> {
    Err("Native folder picker is only supported on Windows".to_string())
}
