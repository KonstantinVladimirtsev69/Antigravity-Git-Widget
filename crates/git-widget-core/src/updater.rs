//! Domain logic for checking and verifying GitHub Releases updates.

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    path::{Path, PathBuf},
};

pub const DEFAULT_UPDATE_REPO: &str = "KonstantinVladimirtsev69/Antigravity-Git-Widget";
pub const RELEASE_EXE_ASSET: &str = "git-widget-sidecar.exe";
pub const RELEASE_SUMS_ASSET: &str = "SHA256SUMS.txt";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl SemVer {
    pub fn parse_stable(_raw: &str) -> Option<Self> {
        None
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
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedReleasePaths {
    pub temp_dir: PathBuf,
    pub exe_path: PathBuf,
    pub sums_path: PathBuf,
    pub version: String,
}

pub fn validate_update_repo(_raw: &str) -> Result<String> {
    todo!("TDD RED: validate_update_repo")
}

pub fn resolve_update_repo(_config_override: Option<&str>, _runtime_env: Option<&str>) -> String {
    todo!("TDD RED: resolve_update_repo")
}

pub fn validate_release_url(_url: &str, _allowed_bases: &[&str]) -> Result<()> {
    todo!("TDD RED: validate_release_url")
}

pub fn parse_sha256sums_for_asset(_raw: &str, _asset_name: &str) -> Result<String> {
    todo!("TDD RED: parse_sha256sums_for_asset")
}

pub fn verify_file_sha256(_path: &Path, _expected_hex: &str) -> Result<()> {
    todo!("TDD RED: verify_file_sha256")
}

pub async fn fetch_latest_release(
    _repo: &str,
    _current_version: &str,
    _token: Option<&str>,
) -> Result<UpdateReleaseInfo> {
    todo!("TDD RED: fetch_latest_release")
}

pub async fn fetch_latest_release_with_base(
    _api_base: &str,
    _repo: &str,
    _current_version: &str,
    _token: Option<&str>,
    _allowed_bases: &[&str],
) -> Result<UpdateReleaseInfo> {
    todo!("TDD RED: fetch_latest_release_with_base")
}

pub async fn download_verified_release(
    _info: &UpdateReleaseInfo,
    _current_version: &str,
    _token: Option<&str>,
) -> Result<VerifiedReleasePaths> {
    todo!("TDD RED: download_verified_release")
}

pub async fn download_verified_release_with_bases(
    _info: &UpdateReleaseInfo,
    _current_version: &str,
    _token: Option<&str>,
    _allowed_bases: &[&str],
) -> Result<VerifiedReleasePaths> {
    todo!("TDD RED: download_verified_release_with_bases")
}
