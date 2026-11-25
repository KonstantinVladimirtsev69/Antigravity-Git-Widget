//! Streaming parser for `git clone --progress` stderr output and bounded log buffer.

use crate::clone_validation::redact_url_credentials;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloneStage {
    Idle,
    Starting,
    Counting,
    Compressing,
    Receiving,
    Resolving,
    Updating,
    Done,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloneProgressSnapshot {
    pub stage: CloneStage,
    pub percent: u8,
    pub phase_label: String,
    pub target_path: String,
    pub sanitized_url: String,
    pub in_progress: bool,
    pub error: Option<String>,
    pub log_tail: Vec<String>,
}

impl CloneProgressSnapshot {
    pub fn new(
        stage: CloneStage,
        percent: u8,
        phase_label: impl Into<String>,
        target_path: &std::path::Path,
        sanitized_url: &str,
        in_progress: bool,
        error: Option<String>,
        log_tail: Vec<String>,
    ) -> Self {
        Self {
            stage,
            percent,
            phase_label: phase_label.into(),
            target_path: target_path.to_string_lossy().to_string(),
            sanitized_url: sanitized_url.to_string(),
            in_progress,
            error,
            log_tail,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogRingBuffer {
    max_len: usize,
    items: VecDeque<String>,
}

impl LogRingBuffer {
    pub fn new(max_len: usize) -> Self {
        Self {
            max_len: max_len.max(1),
            items: VecDeque::new(),
        }
    }

    pub fn push(&mut self, line: &str) {
        let cleaned = redact_url_credentials(line.trim());
        if cleaned.is_empty() {
            return;
        }
        if self.items.len() >= self.max_len {
            self.items.pop_front();
        }
        self.items.push_back(cleaned);
    }

    pub fn entries(&self) -> Vec<String> {
        self.items.iter().cloned().collect()
    }
}

fn extract_percent(line: &str) -> Option<u8> {
    let pct_idx = line.find('%')?;
    let prefix = line[..pct_idx].trim_end();
    let digits: String = prefix
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u8>().ok().map(|v| v.min(100))
}

/// Parses a single `\r`/`\n`-delimited `git clone --progress` line into a weighted stage and percentage.
pub fn parse_git_progress_line(line: &str) -> Option<(CloneStage, u8, String)> {
    let cleaned = line.trim();
    if cleaned.is_empty() {
        return None;
    }

    let (stage, label, weight_start, weight_span) = if cleaned.contains("Counting objects:") {
        (CloneStage::Counting, "Counting objects", 0u8, 10u8)
    } else if cleaned.contains("Compressing objects:") {
        (CloneStage::Compressing, "Compressing objects", 10, 15)
    } else if cleaned.contains("Receiving objects:") {
        (CloneStage::Receiving, "Receiving objects", 25, 55)
    } else if cleaned.contains("Resolving deltas:") {
        (CloneStage::Resolving, "Resolving deltas", 80, 15)
    } else if cleaned.contains("Updating files:") || cleaned.contains("Checking out files:") {
        (CloneStage::Updating, "Updating files", 95, 5)
    } else if cleaned.starts_with("Cloning into ") {
        return Some((CloneStage::Starting, 2, "Starting clone...".to_string()));
    } else {
        return None;
    };

    let raw_pct = extract_percent(cleaned).unwrap_or(if cleaned.contains("done.") { 100 } else { 0 });
    let overall = weight_start + ((u16::from(raw_pct) * u16::from(weight_span)) / 100) as u8;
    Some((stage, overall.min(99), format!("{label} ({raw_pct}%)")))
}

/// Produces a user-friendly, credential-redacted clone failure message (R4, P2).
pub fn classify_clone_error(
    code: i32,
    log_tail: &[String],
    cancelled: bool,
    timeout_kind: Option<&str>,
) -> String {
    if cancelled {
        return "Clone cancelled by user.".to_string();
    }
    match timeout_kind {
        Some("idle") => {
            return "Clone timed out: no progress output received within the idle limit.".to_string();
        }
        Some("total") => {
            return "Clone timed out: exceeded maximum total clone duration.".to_string();
        }
        _ => {}
    }

    let detail = redact_url_credentials(&log_tail.join(" | "));
    let lower = detail.to_ascii_lowercase();
    let msg = if lower.contains("authentication failed")
        || lower.contains("permission denied")
        || lower.contains("could not read username")
        || lower.contains("could not read password")
        || lower.contains("invalid username or token")
    {
        format!("Authentication failed: check your SSH key or Git credentials. {detail}")
    } else if lower.contains("could not resolve host")
        || lower.contains("connection refused")
        || lower.contains("unable to access")
        || lower.contains("connection timed out")
        || lower.contains("network is unreachable")
    {
        format!("Network error while connecting to remote repository. {detail}")
    } else if lower.contains("repository not found")
        || lower.contains("does not appear to be a git repository")
    {
        format!("Repository not found or access denied. {detail}")
    } else if detail.is_empty() {
        format!("git clone failed with exit code {code}")
    } else {
        format!("git clone failed ({code}): {detail}")
    };
    redact_url_credentials(&msg)
}

pub(crate) struct ProgressTracker<'a, F> {
    pub ring: LogRingBuffer,
    pub stage: CloneStage,
    pub percent: u8,
    pub phase_label: String,
    pub target_path: &'a std::path::Path,
    pub sanitized_url: &'a str,
    on_progress: F,
}

impl<'a, F: FnMut(CloneProgressSnapshot) + Send> ProgressTracker<'a, F> {
    pub fn new(capacity: usize, target_path: &'a std::path::Path, sanitized_url: &'a str, on_progress: F) -> Self {
        Self {
            ring: LogRingBuffer::new(capacity),
            stage: CloneStage::Starting,
            percent: 1,
            phase_label: "Starting clone...".to_string(),
            target_path,
            sanitized_url,
            on_progress,
        }
    }

    pub fn emit(&mut self, stage: CloneStage, percent: u8, label: &str, in_progress: bool, error: Option<String>) {
        (self.on_progress)(CloneProgressSnapshot::new(
            stage,
            percent,
            label,
            self.target_path,
            self.sanitized_url,
            in_progress,
            error,
            self.ring.entries(),
        ));
    }

    pub fn push_line(&mut self, raw_line: &str) {
        self.ring.push(raw_line);
        if let Some((s, p, lbl)) = parse_git_progress_line(raw_line) {
            self.stage = s;
            self.percent = self.percent.max(p);
            self.phase_label = lbl;
        }
        let (stage, pct, lbl) = (self.stage, self.percent, self.phase_label.clone());
        self.emit(stage, pct, &lbl, true, None);
    }
}
