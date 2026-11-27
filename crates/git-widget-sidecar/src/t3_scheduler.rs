//! Windows Task Scheduler management for silent `--ensure-t3-patched` auto-repatching.

use std::{fs, path::Path, process::Command};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub const TASK_NAME: &str = "GitWidget-T3Code-AutoPatch";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Pure-ASCII VBS launcher using ExpandEnvironmentStrings so Cyrillic usernames never break WSH.
const ENSURE_VBS: &str = concat!(
    "Set sh = CreateObject(\"WScript.Shell\")\r\n",
    "exe = sh.ExpandEnvironmentStrings(\"%LOCALAPPDATA%\\git-widget\\git-widget-sidecar.exe\")\r\n",
    "sh.Run \"\"\"\" & exe & \"\"\" --ensure-t3-patched\", 0, False\r\n"
);

pub fn write_silent_launcher(install_dir: &Path) -> Result<std::path::PathBuf, String> {
    fs::create_dir_all(install_dir)
        .map_err(|e| format!("Failed creating {}: {e}", install_dir.display()))?;
    let vbs_path = install_dir.join("ensure-patched.vbs");
    fs::write(&vbs_path, ENSURE_VBS)
        .map_err(|e| format!("Failed writing {}: {e}", vbs_path.display()))?;
    Ok(vbs_path)
}

pub fn build_register_task_script(task_name: &str, vbs_path: &Path) -> String {
    let vbs_escaped = vbs_path.to_string_lossy().replace('\'', "''");
    format!(
        "$ErrorActionPreference='Stop'; \
         $action = New-ScheduledTaskAction -Execute 'wscript.exe' -Argument '//B //Nologo \"{vbs_escaped}\"'; \
         $t1 = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME; \
         $t2 = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(1) -RepetitionInterval (New-TimeSpan -Minutes 15); \
         $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -Hidden -ExecutionTimeLimit (New-TimeSpan -Minutes 1); \
         Register-ScheduledTask -TaskName '{task_name}' -Action $action -Trigger @($t1,$t2) -Settings $settings -Force | Out-Null"
    )
}

pub fn register_auto_patch_task(
    install_dir: &Path,
    _sidecar_exe: &Path,
    _t3_dir: Option<&Path>,
) -> Result<(), String> {
    let vbs_path = write_silent_launcher(install_dir)?;

    #[cfg(windows)]
    {
        let ps = build_register_task_script(TASK_NAME, &vbs_path);
        let output = Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("Failed spawning powershell for Task Scheduler: {e}"))?;
        if !output.status.success() {
            // Fallback to schtasks if PowerShell ScheduledTasks module is restricted
            let tr = format!("wscript.exe //B //Nologo \"{}\"", vbs_path.display());
            let st = Command::new("schtasks")
                .args(["/Create", "/TN", TASK_NAME, "/SC", "MINUTE", "/MO", "15", "/TR", &tr, "/F"])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .map_err(|e| format!("Failed running schtasks: {e}"))?;
            if !st.status.success() {
                let err = String::from_utf8_lossy(&st.stderr);
                return Err(format!("Task Scheduler registration failed: {err}"));
            }
        }
    }

    Ok(())
}

pub fn unregister_auto_patch_task() -> Result<(), String> {
    #[cfg(windows)]
    {
        let _ = Command::new("schtasks")
            .args(["/Delete", "/TN", TASK_NAME, "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
    Ok(())
}
