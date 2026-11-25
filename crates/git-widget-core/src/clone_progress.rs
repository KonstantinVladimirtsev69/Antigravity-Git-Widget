//! Streaming parser for `git clone --progress` stderr output and bounded log buffer.

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
            max_len,
            items: VecDeque::new(),
        }
    }

    pub fn push(&mut self, line: &str) {
        let _ = (self.max_len, line);
    }

    pub fn entries(&self) -> Vec<String> {
        self.items.iter().cloned().collect()
    }
}

pub fn parse_git_progress_line(_line: &str) -> Option<(CloneStage, u8, String)> {
    None
}

pub fn classify_clone_error(
    _code: i32,
    _log_tail: &[String],
    _cancelled: bool,
    _timeout_kind: Option<&str>,
) -> String {
    String::new()
}
