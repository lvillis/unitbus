use std::{collections::BTreeMap, time::SystemTime};

pub type JournalCursor = String;

/// How to handle malformed JSON lines from journalctl.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum ParseErrorMode {
    /// Return an error on the first malformed line.
    #[default]
    FailFast,
    /// Skip malformed lines up to `max_skipped`, then return an error.
    Skip { max_skipped: u32 },
}

/// Query filter for journald.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct JournalFilter {
    /// Optional unit name filter (shorthand names will be canonicalized).
    pub unit: Option<String>,
    /// Optional start time (inclusive).
    pub since: Option<SystemTime>,
    /// Optional end time (inclusive).
    pub until: Option<SystemTime>,
    /// Optional cursor for pagination.
    pub after_cursor: Option<JournalCursor>,
    /// Maximum number of entries to return (default: 200).
    pub limit: u32,
    /// Maximum total payload size (approximate; default: 1 MiB).
    pub max_bytes: u32,
    /// Maximum bytes to keep from `MESSAGE` (default: 16 KiB).
    pub max_message_bytes: u32,
    /// Optional process-level timeout for `journalctl` (defaults to `UnitBusOptions.journal_default_timeout`).
    pub timeout: Option<std::time::Duration>,
    /// How to handle malformed JSON lines.
    pub parse_error: ParseErrorMode,
}

impl Default for JournalFilter {
    fn default() -> Self {
        Self {
            unit: None,
            since: None,
            until: None,
            after_cursor: None,
            limit: 200,
            max_bytes: 1024 * 1024,
            max_message_bytes: 16 * 1024,
            timeout: None,
            parse_error: ParseErrorMode::FailFast,
        }
    }
}

/// One log entry from journald.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct JournalEntry {
    pub timestamp: SystemTime,
    pub cursor: Option<JournalCursor>,
    pub message: Option<String>,
    pub message_truncated: bool,
    pub priority: Option<u8>,
    pub unit: Option<String>,
    pub pid: Option<u32>,
    pub fields: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct JournalStats {
    pub bytes_read: u32,
    pub lines_read: u32,
    pub parse_errors: u32,
    pub skipped_lines: u32,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct JournalResult {
    /// Collected log entries (bounded by `limit` / `max_bytes`).
    pub entries: Vec<JournalEntry>,
    /// Cursor of the last returned entry (if present in the backend output).
    pub next_cursor: Option<JournalCursor>,
    /// `true` if the backend output was cut short due to `limit` or `max_bytes`.
    pub truncated: bool,
    /// Collection statistics (lines read, parse errors, etc).
    pub stats: JournalStats,
}

/// Options for `diagnose_unit_failure`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DiagnosisOptions {
    pub window_before: std::time::Duration,
    pub window_after: std::time::Duration,
    pub limit: u32,
    pub max_bytes: u32,
    pub max_message_bytes: u32,
    pub timeout: Option<std::time::Duration>,
    pub parse_error: ParseErrorMode,
}

impl Default for DiagnosisOptions {
    fn default() -> Self {
        Self {
            window_before: std::time::Duration::from_secs(30),
            window_after: std::time::Duration::from_secs(10),
            limit: 200,
            max_bytes: 1024 * 1024,
            max_message_bytes: 16 * 1024,
            timeout: None,
            parse_error: ParseErrorMode::FailFast,
        }
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Diagnosis {
    pub status: crate::types::unit::UnitStatus,
    pub logs: Vec<JournalEntry>,
    pub truncated: bool,
}
