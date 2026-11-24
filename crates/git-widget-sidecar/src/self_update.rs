//! Multi-host self-update orchestration (`Antigravity` -> `T3 Code`) without killing T3 Code.

use std::path::{Path, PathBuf};

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

pub fn detect_installed_hosts_in(
    _ag_plugin_dir: Option<&Path>,
    _t3_local_dir: Option<&Path>,
) -> HostTargets {
    HostTargets::default()
}

pub fn detect_installed_hosts() -> HostTargets {
    HostTargets::default()
}

pub fn wait_for_parent_pid(_pid: u32, _timeout_ms: u32) -> Result<(), String> {
    Err("not implemented".to_string())
}

pub fn replace_exe_rename_before_copy(
    _source_exe: &Path,
    _target_exe: &Path,
) -> Result<PathBuf, String> {
    Err("not implemented".to_string())
}

pub fn run_self_update(
    _source_exe: Option<&Path>,
    _parent_pid: u32,
    _serve_port: Option<u16>,
    _port_file: Option<&Path>,
    _ag_dir_override: Option<&Path>,
    _t3_dir_override: Option<&Path>,
) -> Result<(), String> {
    Err("not implemented".to_string())
}

pub fn cleanup_stale_update_dirs() {}
