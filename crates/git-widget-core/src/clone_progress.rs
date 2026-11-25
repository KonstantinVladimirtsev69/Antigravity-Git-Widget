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

pub fn classify_clone_error(
    _code: i32,
    _log_tail: &[String],
    _cancelled: bool,
    _timeout_kind: Option<&str>,
) -> String {
    String::new()
}
