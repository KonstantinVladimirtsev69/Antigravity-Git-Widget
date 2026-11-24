//! CLI argument parsing for multi-host installer and standalone server mode.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallTarget {
    Antigravity,
    T3Code,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliMode {
    Serve {
        port: u16,
        port_file: Option<PathBuf>,
    },
    Install {
        target: Option<InstallTarget>,
        t3_dir: Option<PathBuf>,
        explicit_flag: bool,
        auto_close_t3: bool,
    },
    UninstallT3 {
        t3_dir: Option<PathBuf>,
    },
    EnsureT3Patched {
        t3_dir: Option<PathBuf>,
    },
    SelfUpdate {
        parent_pid: u32,
        serve_port: Option<u16>,
        port_file: Option<PathBuf>,
        ag_dir: Option<PathBuf>,
        t3_local_dir: Option<PathBuf>,
    },
}

fn next_value(args: &[String], idx: &mut usize, flag: &str) -> Result<String, String> {
    *idx += 1;
    args.get(*idx)
        .cloned()
        .ok_or_else(|| format!("Missing value for {flag}"))
}

fn parse_target(raw: &str) -> Result<InstallTarget, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "antigravity" | "ag" | "agy" => Ok(InstallTarget::Antigravity),
        "t3code" | "t3" | "t3-code" => Ok(InstallTarget::T3Code),
        "both" | "all" => Ok(InstallTarget::Both),
        other => Err(format!(
            "Invalid --target '{other}' (expected: antigravity | t3code | both)"
        )),
    }
}

pub fn parse_cli_args(args: &[String], has_antigravity_port_env: bool) -> Result<CliMode, String> {
    let mut is_install = false;
    let mut is_serve = false;
    let mut is_uninstall_t3 = false;
    let mut is_ensure_t3 = false;
    let mut auto_close_t3 = false;
    let mut target: Option<InstallTarget> = None;
    let mut t3_dir: Option<PathBuf> = None;
    let mut port: Option<u16> = None;
    let mut port_file: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "--install" => is_install = true,
            "--serve" => is_serve = true,
            "--uninstall-t3" => is_uninstall_t3 = true,
            "--ensure-t3-patched" => is_ensure_t3 = true,
            "--auto-close-t3" => auto_close_t3 = true,
            "--target" => {
                let val = next_value(args, &mut i, "--target")?;
                target = Some(parse_target(&val)?);
            }
            "--t3-dir" => {
                let val = next_value(args, &mut i, "--t3-dir")?;
                t3_dir = Some(PathBuf::from(val));
            }
            "--port" => {
                let val = next_value(args, &mut i, "--port")?;
                let parsed = val
                    .parse::<u16>()
                    .map_err(|_| format!("Invalid --port value: '{val}'"))?;
                port = Some(parsed);
            }
            "--port-file" => {
                let val = next_value(args, &mut i, "--port-file")?;
                port_file = Some(PathBuf::from(val));
            }
            other if other.starts_with("--target=") => {
                target = Some(parse_target(&other["--target=".len()..])?);
            }
            other if other.starts_with("--t3-dir=") => {
                t3_dir = Some(PathBuf::from(&other["--t3-dir=".len()..]));
            }
            other if other.starts_with("--port=") => {
                let val = &other["--port=".len()..];
                port = Some(
                    val.parse::<u16>()
                        .map_err(|_| format!("Invalid --port value: '{val}'"))?,
                );
            }
            other if other.starts_with("--port-file=") => {
                port_file = Some(PathBuf::from(&other["--port-file=".len()..]));
            }
            _ => {}
        }
        i += 1;
    }

    if is_uninstall_t3 {
        return Ok(CliMode::UninstallT3 { t3_dir });
    }
    if is_ensure_t3 {
        return Ok(CliMode::EnsureT3Patched { t3_dir });
    }
    if is_serve {
        return Ok(CliMode::Serve {
            port: port.unwrap_or(39871),
            port_file,
        });
    }
    if is_install || !has_antigravity_port_env {
        return Ok(CliMode::Install {
            target,
            t3_dir,
            explicit_flag: is_install,
            auto_close_t3,
        });
    }

    let env_port = std::env::var("ANTIGRAVITY_SIDECAR_WEB_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(39871);
    Ok(CliMode::Serve {
        port: port.unwrap_or(env_port),
        port_file,
    })
}
