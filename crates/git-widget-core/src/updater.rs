//! Domain logic for checking, downloading, and verifying GitHub Releases updates.

use crate::{error::{CoreError, Result}, github_oauth::curl_binary};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt, fs::{self, File}, io::{BufReader, Read}, path::{Path, PathBuf},
    process::Stdio, sync::atomic::{AtomicU64, Ordering}, time::{SystemTime, UNIX_EPOCH},
};
use tokio::{io::AsyncWriteExt, process::Command};

pub const DEFAULT_UPDATE_REPO: &str = "KonstantinVladimirtsev69/Antigravity-Git-Widget";
pub const RELEASE_EXE_ASSET: &str = "git-widget-sidecar.exe";
pub const RELEASE_SUMS_ASSET: &str = "SHA256SUMS.txt";

const ALLOWED_URL_PREFIXES: &[&str] = &[
    "https://api.github.com/", "https://github.com/",
    "https://objects.githubusercontent.com/", "https://release-assets.githubusercontent.com/",
];
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl SemVer {
    /// Parses a stable `MAJOR.MINOR.PATCH` string (optional `v`/`V` prefix and `+build`),
    /// rejecting pre-release suffixes (`-rc.1`) and non-3-part versions.
    pub fn parse_stable(raw: &str) -> Option<Self> {
        let s = raw.trim();
        let s = s.strip_prefix('v').or_else(|| s.strip_prefix('V')).unwrap_or(s);
        let s = s.split('+').next().unwrap_or(s);
        if s.is_empty() || s.contains('-') { return None; }
        let mut parts = s.split('.');
        let num = |p: &str| (!p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())).then(|| p.parse().ok()).flatten();
        let (major, minor, patch) = (num(parts.next()?)?, num(parts.next()?)?, num(parts.next()?)?);
        parts.next().is_none().then_some(Self { major, minor, patch })
    }
}

impl fmt::Display for SemVer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateReleaseInfo {
    pub repo: String,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release_notes: String,
    pub html_url: Option<String>,
    pub published_at: Option<String>,
    pub exe_download_url: Option<String>,
    pub exe_api_url: Option<String>,
    pub sums_download_url: Option<String>,
    pub sums_api_url: Option<String>,
}

impl UpdateReleaseInfo {
    pub fn has_required_assets(&self) -> bool {
        (self.exe_download_url.is_some() || self.exe_api_url.is_some())
            && (self.sums_download_url.is_some() || self.sums_api_url.is_some())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedReleasePaths {
    pub temp_dir: PathBuf,
    pub exe_path: PathBuf,
    pub sums_path: PathBuf,
    pub version: String,
}

fn valid_segment(seg: &str) -> bool {
    !seg.is_empty() && seg != "." && seg != ".." && !seg.starts_with('-') && !seg.contains("..")
        && seg.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Validates strict `owner/repo` format without URL schemes, whitespace, traversal, or flag prefixes.
pub fn validate_update_repo(raw: &str) -> Result<String> {
    let s = raw.trim();
    if s.is_empty() || s.contains("://") || s.chars().any(char::is_whitespace) {
        return Err(CoreError::InvalidInput("Invalid update repository format (expected owner/repo)".into()));
    }
    let (owner, repo) = s.split_once('/').ok_or_else(|| CoreError::InvalidInput("Update repository must be owner/repo".into()))?;
    if repo.contains('/') || !valid_segment(owner) || !valid_segment(repo) {
        return Err(CoreError::InvalidInput("Invalid owner/repo characters in update repository".into()));
    }
    Ok(format!("{owner}/{repo}"))
}

/// Resolves the update repository: config override -> runtime env -> default repository.
pub fn resolve_update_repo(config_override: Option<&str>, runtime_env: Option<&str>) -> String {
    config_override.and_then(|s| validate_update_repo(s).ok())
        .or_else(|| runtime_env.and_then(|s| validate_update_repo(s).ok()))
        .unwrap_or_else(|| DEFAULT_UPDATE_REPO.to_string())
}

/// Ensures release/asset URLs strictly match GitHub HTTPS prefixes or explicit test bases.
pub fn validate_release_url(url: &str, allowed_bases: &[&str]) -> Result<()> {
    if url.is_empty() || url.starts_with('-') || url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(CoreError::InvalidInput("Invalid release URL".into()));
    }
    if ALLOWED_URL_PREFIXES.iter().any(|p| url.starts_with(p)) { return Ok(()); }
    for b in allowed_bases.iter().map(|s| s.trim_end_matches('/')).filter(|s| !s.is_empty()) {
        if url == b || url.starts_with(&format!("{b}/")) { return Ok(()); }
    }
    Err(CoreError::InvalidInput(format!("Disallowed release URL host: {url}")))
}

/// Parses `SHA256SUMS.txt` (handling UTF-8 BOM, CRLF/LF, and `*`/`./` filename prefixes).
pub fn parse_sha256sums_for_asset(raw: &str, asset_name: &str) -> Result<String> {
    for line in raw.trim_start_matches('\u{FEFF}').lines() {
        let mut tokens = line.split_whitespace();
        let (Some(hash), Some(name)) = (tokens.next(), tokens.next()) else { continue };
        if name.trim_start_matches('*').trim_start_matches("./") == asset_name {
            if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) { return Ok(hash.to_ascii_lowercase()); }
            return Err(CoreError::InvalidInput(format!("Malformed SHA-256 digest for {asset_name}")));
        }
    }
    Err(CoreError::InvalidInput(format!("Missing SHA-256 entry for {asset_name} in SHA256SUMS.txt")))
}

/// Streams `path` through `Sha256` and verifies it matches `expected_hex` (case-insensitive).
pub fn verify_file_sha256(path: &Path, expected_hex: &str) -> Result<()> {
    let expected = expected_hex.trim();
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CoreError::InvalidInput("Invalid expected SHA-256 digest".into()));
    }
    let (mut reader, mut hasher, mut buf) = (BufReader::new(File::open(path)?), Sha256::new(), [0u8; 16 * 1024]);
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(CoreError::InvalidInput(format!("SHA-256 mismatch: expected {expected}, got {actual}")));
    }
    Ok(())
}

fn check_http_status(status: u16, repo: &str, context: &str) -> Result<()> {
    match status {
        200 => Ok(()),
        403 | 429 => Err(CoreError::InvalidInput(format!(
            "GitHub API rate limit exceeded (HTTP {status}). Sign in with GitHub in Settings or try again later."
        ))),
        404 => Err(CoreError::InvalidInput(format!("No published release found for {repo} ({context})"))),
        other => Err(CoreError::InvalidInput(format!("GitHub returned HTTP {other} for {repo} ({context})"))),
    }
}

async fn run_curl(args: &[&str], url: &str, token: Option<&str>) -> Result<Vec<u8>> {
    let mut cmd = Command::new(curl_binary());
    cmd.args(args).args(["-H", "User-Agent: antigravity-git-widget"]);
    let cfg = token.map(str::trim).filter(|s| !s.is_empty()).map(|t| {
        format!(
            "header = \"Authorization: Bearer {}\"\n",
            t.replace(['\r', '\n', '"', '\\'], "")
        )
    });
    if cfg.is_some() { cmd.args(["-K", "-"]).stdin(Stdio::piped()); } else { cmd.stdin(Stdio::null()); }
    cmd.arg(url).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let mut child = cmd.spawn()?;
    if let (Some(c), Some(mut stdin)) = (cfg, child.stdin.take()) { let _ = stdin.write_all(c.as_bytes()).await; }
    let out = child.wait_with_output().await?;
    if !out.status.success() {
        return Err(CoreError::GitFailed {
            code: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(out.stdout)
}

pub async fn fetch_latest_release(repo: &str, current_version: &str, token: Option<&str>) -> Result<UpdateReleaseInfo> {
    fetch_latest_release_with_base("https://api.github.com", repo, current_version, token, &[]).await
}

pub async fn fetch_latest_release_with_base(
    api_base: &str, repo: &str, current_version: &str, token: Option<&str>, allowed_bases: &[&str],
) -> Result<UpdateReleaseInfo> {
    let valid_repo = validate_update_repo(repo)?;
    let current = SemVer::parse_stable(current_version).ok_or_else(|| CoreError::InvalidInput(format!("Invalid current version: {current_version}")))?;
    let url = format!("{}/repos/{valid_repo}/releases/latest", api_base.trim_end_matches('/'));
    validate_release_url(&url, allowed_bases)?;

    let raw = run_curl(&["-sS", "--connect-timeout", "5", "--max-time", "10", "-H", "Accept: application/vnd.github+json", "-w", "\n%{http_code}"], &url, token).await?;
    let text = String::from_utf8_lossy(&raw);
    let (body, code_str) = text.rsplit_once('\n').ok_or_else(|| CoreError::InvalidInput("Malformed HTTP response from curl".into()))?;
    let status: u16 = code_str.trim().parse().map_err(|_| CoreError::InvalidInput("Invalid HTTP status from curl".into()))?;
    check_http_status(status, &valid_repo, "releases/latest")?;

    let val: serde_json::Value = serde_json::from_str(body).map_err(|e| CoreError::InvalidInput(format!("Invalid GitHub release JSON: {e}")))?;
    let tag = val.get("tag_name").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| CoreError::InvalidInput("GitHub release JSON missing tag_name".into()))?;
    let skip = val.get("draft").and_then(|v| v.as_bool()).unwrap_or(false) || val.get("prerelease").and_then(|v| v.as_bool()).unwrap_or(false);
    let parsed_latest = if skip { None } else { SemVer::parse_stable(tag) };

    let (mut exe_dl, mut exe_api, mut sums_dl, mut sums_api) = (None, None, None, None);
    for a in val.get("assets").and_then(|v| v.as_array()).into_iter().flatten() {
        let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let dl = a.get("browser_download_url").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(ToString::to_string);
        let api = a.get("url").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(ToString::to_string);
        if name == RELEASE_EXE_ASSET { (exe_dl, exe_api) = (dl, api); }
        else if name == RELEASE_SUMS_ASSET { (sums_dl, sums_api) = (dl, api); }
    }

    Ok(UpdateReleaseInfo {
        repo: valid_repo, current_version: current.to_string(), latest_version: parsed_latest.map(|v| v.to_string()),
        update_available: parsed_latest.is_some_and(|lv| lv > current),
        release_notes: val.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        html_url: val.get("html_url").and_then(|v| v.as_str()).map(ToString::to_string),
        published_at: val.get("published_at").and_then(|v| v.as_str()).map(ToString::to_string),
        exe_download_url: exe_dl, exe_api_url: exe_api, sums_download_url: sums_dl, sums_api_url: sums_api,
    })
}

fn select_asset_url<'a>(has_tok: bool, api: Option<&'a str>, dl: Option<&'a str>, name: &str) -> Result<(&'a str, bool)> {
    match (has_tok, api, dl) {
        (true, Some(u), _) => Ok((u, true)),
        (_, _, Some(u)) => Ok((u, false)),
        (_, Some(u), None) => Ok((u, has_tok)),
        _ => Err(CoreError::InvalidInput(format!("Missing download URL for {name}"))),
    }
}

async fn download_asset(url: &str, dest: &Path, token: Option<&str>, use_api_auth: bool, repo: &str, name: &str) -> Result<()> {
    let dest_str = dest.to_str().ok_or_else(|| CoreError::InvalidInput("Non-UTF8 temp path".into()))?;
    let mut args = vec!["-sS", "-L", "--connect-timeout", "10", "--max-time", "120", "-o", dest_str, "-w", "%{http_code}"];
    if use_api_auth { args.extend(["-H", "Accept: application/octet-stream"]); }
    let out = run_curl(&args, url, if use_api_auth { token } else { None }).await?;
    let status: u16 = String::from_utf8_lossy(&out).trim().parse()
        .map_err(|_| CoreError::InvalidInput("Invalid HTTP status during asset download".into()))?;
    check_http_status(status, repo, name)
}

pub async fn download_verified_release(info: &UpdateReleaseInfo, current_version: &str, token: Option<&str>) -> Result<VerifiedReleasePaths> {
    download_verified_release_with_bases(info, current_version, token, &[]).await
}

pub async fn download_verified_release_with_bases(
    info: &UpdateReleaseInfo, current_version: &str, token: Option<&str>, allowed_bases: &[&str],
) -> Result<VerifiedReleasePaths> {
    let current = SemVer::parse_stable(current_version).ok_or_else(|| CoreError::InvalidInput(format!("Invalid current version: {current_version}")))?;
    let latest_raw = info.latest_version.as_deref().ok_or_else(|| CoreError::InvalidInput("Release has no stable installable version".into()))?;
    let latest = SemVer::parse_stable(latest_raw).ok_or_else(|| CoreError::InvalidInput(format!("Release version is not stable SemVer: {latest_raw}")))?;
    if latest <= current { return Err(CoreError::InvalidInput(format!("Downgrade or same version rejected: {latest} <= {current}"))); }
    if !info.update_available || !info.has_required_assets() { return Err(CoreError::InvalidInput("Release is missing required update assets".into())); }

    let has_tok = token.map(str::trim).is_some_and(|s| !s.is_empty());
    let (exe_url, exe_auth) = select_asset_url(has_tok, info.exe_api_url.as_deref(), info.exe_download_url.as_deref(), RELEASE_EXE_ASSET)?;
    let (sums_url, sums_auth) = select_asset_url(has_tok, info.sums_api_url.as_deref(), info.sums_download_url.as_deref(), RELEASE_SUMS_ASSET)?;
    validate_release_url(exe_url, allowed_bases)?;
    validate_release_url(sums_url, allowed_bases)?;

    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let temp_dir = std::env::temp_dir().join(format!("gw-update-{ts}-{}-{seq}", std::process::id()));
    fs::create_dir_all(&temp_dir)?;

    let (sums_path, exe_path) = (temp_dir.join(RELEASE_SUMS_ASSET), temp_dir.join(RELEASE_EXE_ASSET));
    let res: Result<()> = async {
        download_asset(sums_url, &sums_path, token, sums_auth, &info.repo, RELEASE_SUMS_ASSET).await?;
        download_asset(exe_url, &exe_path, token, exe_auth, &info.repo, RELEASE_EXE_ASSET).await?;
        let expected_hex = parse_sha256sums_for_asset(&fs::read_to_string(&sums_path)?, RELEASE_EXE_ASSET)?;
        verify_file_sha256(&exe_path, &expected_hex)
    }.await;

    if let Err(err) = res {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(err);
    }
    Ok(VerifiedReleasePaths { temp_dir, exe_path, sums_path, version: latest.to_string() })
}
