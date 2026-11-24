//! Embedded T3 Code runtime JS hooks and deployment helpers.

use std::{fs, path::Path};

pub const T3_LOADER_CJS: &str = include_str!("../assets/t3-loader.cjs");
pub const T3_SERVER_HOOK_CJS: &str = include_str!("../assets/t3-server-hook.cjs");
pub const T3_UI_BRIDGE_JS: &str = include_str!("../assets/t3-ui-bridge.js");

pub fn deploy_runtime_scripts(install_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(install_dir)
        .map_err(|e| format!("Failed creating {}: {e}", install_dir.display()))?;
    fs::write(install_dir.join("t3-loader.cjs"), T3_LOADER_CJS)
        .map_err(|e| format!("Failed writing t3-loader.cjs: {e}"))?;
    fs::write(install_dir.join("t3-server-hook.cjs"), T3_SERVER_HOOK_CJS)
        .map_err(|e| format!("Failed writing t3-server-hook.cjs: {e}"))?;
    fs::write(install_dir.join("t3-ui-bridge.js"), T3_UI_BRIDGE_JS)
        .map_err(|e| format!("Failed writing t3-ui-bridge.js: {e}"))?;
    Ok(())
}
