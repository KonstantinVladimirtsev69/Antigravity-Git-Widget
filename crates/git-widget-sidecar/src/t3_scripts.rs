//! Embedded T3 Code runtime JS hooks and deployment helpers.

use std::{fs, path::Path};

pub const T3_LOADER_CJS: &str = include_str!("../assets/t3-loader.cjs");
pub const T3_SERVER_HOOK_CJS: &str = include_str!("../assets/t3-server-hook.cjs");
pub const T3_UI_BRIDGE_JS_TEMPLATE: &str = include_str!("../assets/t3-ui-bridge.js");
pub const T3_PROJECT_BRIDGE_JS: &str = include_str!("../assets/t3-project-bridge.js");
pub const T3_UI_BRIDGE_CSS: &str = include_str!("../assets/t3-ui-bridge.css");

pub fn render_ui_bridge_js() -> String {
    let css_literal = serde_json::to_string(T3_UI_BRIDGE_CSS).unwrap_or_else(|_| "\"\"".into());
    T3_UI_BRIDGE_JS_TEMPLATE
        .replace("__GW_BRIDGE_CSS__", &css_literal)
        .replace("__GW_PROJECT_BRIDGE_JS__", T3_PROJECT_BRIDGE_JS)
}

pub fn deploy_runtime_scripts(install_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(install_dir)
        .map_err(|e| format!("Failed creating {}: {e}", install_dir.display()))?;
    fs::write(install_dir.join("t3-loader.cjs"), T3_LOADER_CJS)
        .map_err(|e| format!("Failed writing t3-loader.cjs: {e}"))?;
    fs::write(install_dir.join("t3-server-hook.cjs"), T3_SERVER_HOOK_CJS)
        .map_err(|e| format!("Failed writing t3-server-hook.cjs: {e}"))?;
    fs::write(install_dir.join("t3-ui-bridge.js"), render_ui_bridge_js())
        .map_err(|e| format!("Failed writing t3-ui-bridge.js: {e}"))?;
    Ok(())
}
