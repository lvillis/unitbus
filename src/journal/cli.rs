use crate::types::journal::{
    JournalEntry, JournalFilter, JournalResult, JournalStats, ParseErrorMode,
};
use crate::{Error, Result, UnitBusOptions, util};

use futures_lite::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use futures_util::FutureExt;

use std::collections::BTreeMap;
use std::process::Stdio;

const STDERR_MAX_BYTES: usize = 8 * 1024;

pub(crate) async fn query_journalctl(
    opts: &UnitBusOptions,
    mut filter: JournalFilter,
) -> Result<JournalResult> {
    if filter.limit == 0 {
        return Err(Error::invalid_input("journal limit must be > 0"));
    }
    if filter.max_bytes == 0 {
        return Err(Error::invalid_input("journal max_bytes must be > 0"));
    }
    if filter.max_message_bytes == 0 {
        return Err(Error::invalid_input(
            "journal max_message_bytes must be > 0",
        ));
    }

    if let Some(unit) = &filter.unit {
        filter.unit = Some(util::canonicalize_unit_name(unit)?);
    }
    if let Some(cursor) = &filter.after_cursor {
        util::validate_no_control("cursor", cursor)?;
    }

    let timeout = filter.timeout.unwrap_or(opts.journal_default_timeout);
    let wants_cursor = filter.after_cursor.is_some();

    #[cfg(feature = "tracing")]
    tracing::debug!(
        unit = filter.unit.as_deref().unwrap_or(""),
        limit = filter.limit,
        max_bytes = filter.max_bytes,
        max_message_bytes = filter.max_message_bytes,
        has_cursor = wants_cursor,
        has_since = filter.since.is_some(),
        has_until = filter.until.is_some(),
        ?timeout,
        "journalctl query"
    );

    let mut cmd = async_process::Command::new("journalctl");
    cmd.arg("--no-pager").arg("--output=json");

    if let Some(unit) = &filter.unit {
        cmd.arg("-u").arg(unit);
    }

    if let Some(since) = filter.since {
        let since = util::unix_seconds(since)?;
        cmd.arg(format!("--since=@{since}"));
    }

    if let Some(until) = filter.until {
        let until = util::unix_seconds(until)?;
        cmd.arg(format!("--until=@{until}"));
    }

    if let Some(cursor) = &filter.after_cursor {
        cmd.arg(format!("--after-cursor={cursor}"));
    }

    let lines = filter.limit.saturating_add(1);
    cmd.arg(format!("--lines={lines}"));
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            return Error::BackendUnavailable {
                backend: "journalctl",
                detail: "journalctl not found".to_string(),
            };
        }
        Error::IoError {
            context: format!("spawn journalctl failed: {e}"),
        }
    })?;

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            return Err(Error::IoError {
                context: "journalctl stdout not captured".to_string(),
            });
        }
    };

    let mut stderr = child.stderr.take();
    let mut stderr_buf = Vec::<u8>::new();
    let mut stderr_tmp = [0u8; 1024];

    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    let mut collector = JournalCollector::new(&filter);

    let mut deadline = crate::runtime::sleep(timeout).fuse();

    loop {
        line.clear();

        let n = if let Some(s) = &mut stderr {
            futures_util::select! {
                _ = deadline => {
                    let _ = child.kill();
                    let _ = child.status().await;
                    return Err(Error::Timeout { action: "journalctl", timeout });
                }
                n = s.read(&mut stderr_tmp).fuse() => {
                    let n = n.map_err(|e| Error::IoError { context: format!("read journalctl stderr: {e}") })?;
                    if n == 0 {
                        stderr = None;
                    } else {
                        push_limited(&mut stderr_buf, &stderr_tmp[..n], STDERR_MAX_BYTES);
                    }
                    continue;
                }
                n = reader.read_line(&mut line).fuse() => {
                    n.map_err(|e| Error::IoError { context: format!("read journalctl stdout: {e}") })?
                }
            }
        } else {
            futures_util::select! {
                _ = deadline => {
                    let _ = child.kill();
                    let _ = child.status().await;
                    return Err(Error::Timeout { action: "journalctl", timeout });
                }
                n = reader.read_line(&mut line).fuse() => {
                    n.map_err(|e| Error::IoError { context: format!("read journalctl stdout: {e}") })?
                }
            }
        };

        if n == 0 {
            break;
        }

        let line_trimmed = line.trim_end_matches(&['\r', '\n'][..]);
        match collector.push_line(line_trimmed) {
            Ok(CollectAction::Continue) => {}
            Ok(CollectAction::StopTruncated) => break,
            Err(e) => {
                let _ = child.kill();
                let _ = child.status().await;
                return Err(e);
            }
        }
    }

    if collector.truncated {
        let _ = child.kill();
    }

    #[cfg(feature = "tracing")]
    if collector.truncated {
        tracing::warn!(
            unit = filter.unit.as_deref().unwrap_or(""),
            limit = filter.limit,
            bytes_read = collector.stats.bytes_read,
            lines_read = collector.stats.lines_read,
            "journalctl output truncated"
        );
    }

    let status = child.status().await.map_err(|e| Error::IoError {
        context: format!("wait journalctl: {e}"),
    })?;

    if let Some(s) = &mut stderr {
        let _ = drain_to_end_limited(s, &mut stderr_buf, STDERR_MAX_BYTES).await;
    }

    if !collector.truncated && !status.success() {
        let stderr_str = String::from_utf8_lossy(&stderr_buf);
        if let Some(err) = classify_journalctl_failure(wants_cursor, stderr_str.as_ref()) {
            return Err(err);
        }
        return Err(Error::process_error(
            "journalctl",
            status.code(),
            stderr_str.as_ref(),
        ));
    }

    #[cfg(feature = "tracing")]
    tracing::debug!(
        unit = filter.unit.as_deref().unwrap_or(""),
        entries = collector.entries.len(),
        truncated = collector.truncated,
        bytes_read = collector.stats.bytes_read,
        lines_read = collector.stats.lines_read,
        parse_errors = collector.stats.parse_errors,
        skipped_lines = collector.stats.skipped_lines,
        "journalctl result"
    );

    let entries = collector.entries;
    let next_cursor = entries.last().and_then(|e| e.cursor.clone());

    Ok(JournalResult {
        entries,
        next_cursor,
        truncated: collector.truncated,
        stats: collector.stats,
    })
}

async fn drain_to_end_limited(
    stderr: &mut async_process::ChildStderr,
    out: &mut Vec<u8>,
    cap: usize,
) -> std::io::Result<()> {
    let mut tmp = [0u8; 1024];
    loop {
        let n = stderr.read(&mut tmp).await?;
        if n == 0 {
            return Ok(());
        }
        push_limited(out, &tmp[..n], cap);
    }
}

fn push_limited(out: &mut Vec<u8>, chunk: &[u8], cap: usize) {
    if out.len() >= cap {
        return;
    }
    let remaining = cap.saturating_sub(out.len());
    let n = std::cmp::min(chunk.len(), remaining);
    out.extend_from_slice(&chunk[..n]);
}

fn classify_journalctl_failure(wants_cursor: bool, stderr: &str) -> Option<Error> {
    let lower = stderr.to_ascii_lowercase();

    if wants_cursor
        && lower.contains("after-cursor")
        && (lower.contains("unknown option")
            || lower.contains("unrecognized option")
            || lower.contains("invalid option"))
    {
        return Some(Error::BackendUnavailable {
            backend: "journalctl(after-cursor)",
            detail: stderr.to_string(),
        });
    }

    if lower.contains("permission denied")
        || lower.contains("operation not permitted")
        || lower.contains("access denied")
    {
        return Some(Error::PermissionDenied {
            action: "read_journal",
            detail: stderr.to_string(),
        });
    }

    None
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CollectAction {
    Continue,
    StopTruncated,
}

struct JournalCollector {
    limit: u32,
    max_bytes: u32,
    max_message_bytes: u32,
    parse_error: ParseErrorMode,
    stats: JournalStats,
    entries: Vec<JournalEntry>,
    truncated: bool,
    skipped: u32,
}

impl JournalCollector {
    fn new(filter: &JournalFilter) -> Self {
        Self {
            limit: filter.limit,
            max_bytes: filter.max_bytes,
            max_message_bytes: filter.max_message_bytes,
            parse_error: filter.parse_error.clone(),
            stats: JournalStats::default(),
            entries: Vec::new(),
            truncated: false,
            skipped: 0,
        }
    }

    fn push_line(&mut self, line: &str) -> Result<CollectAction> {
        self.stats.lines_read = self.stats.lines_read.saturating_add(1);

        let line_len = u32::try_from(line.len()).unwrap_or(u32::MAX);
        let next_bytes = self.stats.bytes_read.saturating_add(line_len);
        if next_bytes > self.max_bytes {
            self.truncated = true;
            return Ok(CollectAction::StopTruncated);
        }
        self.stats.bytes_read = next_bytes;

        if self.stats.lines_read > self.limit {
            self.truncated = true;
            return Ok(CollectAction::StopTruncated);
        }

        match parse_entry(line, self.max_message_bytes) {
            Ok(entry) => self.entries.push(entry),
            Err(e) => match &self.parse_error {
                ParseErrorMode::FailFast => return Err(e),
                ParseErrorMode::Skip { max_skipped } => {
                    self.stats.parse_errors = self.stats.parse_errors.saturating_add(1);
                    self.stats.skipped_lines = self.stats.skipped_lines.saturating_add(1);
                    self.skipped = self.skipped.saturating_add(1);
                    if self.skipped > *max_skipped {
                        return Err(e);
                    }
                }
            },
        }

        Ok(CollectAction::Continue)
    }
}

fn parse_entry(line: &str, max_message_bytes: u32) -> Result<JournalEntry> {
    let v: serde_json::Value = serde_json::from_str(line)
        .map_err(|_| Error::parse_error("journalctl json line parse", line))?;
    let obj = v
        .as_object()
        .ok_or_else(|| Error::parse_error("journalctl json line is not an object", line))?;

    let ts = parse_timestamp_micros(obj).ok_or_else(|| {
        Error::parse_error("journalctl missing/invalid __REALTIME_TIMESTAMP", line)
    })?;
    let timestamp = util::system_time_from_unix_micros(ts);

    let cursor = obj
        .get("__CURSOR")
        .and_then(|v| v.as_str())
        .and_then(non_empty);

    let (message, message_truncated) = match obj.get("MESSAGE") {
        Some(serde_json::Value::String(s)) => {
            let max = usize::try_from(max_message_bytes).unwrap_or(0);
            let (t, tr) = crate::util::truncate_string_bytes(s, max);
            (Some(t), tr)
        }
        Some(v) => {
            let bytes = json_value_to_bytes(v);
            let max = usize::try_from(max_message_bytes).unwrap_or(0);
            let truncated = bytes.len() > max;
            let slice = if truncated { &bytes[..max] } else { &bytes };
            (Some(String::from_utf8_lossy(slice).into_owned()), truncated)
        }
        None => (None, false),
    };

    let priority = obj.get("PRIORITY").and_then(|v| match v {
        serde_json::Value::String(s) => s.parse::<u8>().ok(),
        serde_json::Value::Number(n) => n.as_u64().and_then(|n| u8::try_from(n).ok()),
        _ => None,
    });

    let unit = obj
        .get("_SYSTEMD_UNIT")
        .and_then(|v| v.as_str())
        .and_then(non_empty);

    let pid = obj.get("_PID").and_then(|v| match v {
        serde_json::Value::String(s) => s.parse::<u32>().ok(),
        serde_json::Value::Number(n) => n.as_u64().and_then(|n| u32::try_from(n).ok()),
        _ => None,
    });

    let mut fields = BTreeMap::new();
    for (k, v) in obj {
        fields.insert(k.clone(), json_value_to_bytes(v));
    }

    Ok(JournalEntry {
        timestamp,
        cursor,
        message,
        message_truncated,
        priority,
        unit,
        pid,
        fields,
    })
}

fn parse_timestamp_micros(obj: &serde_json::Map<String, serde_json::Value>) -> Option<u64> {
    parse_u64(obj, "__REALTIME_TIMESTAMP").or_else(|| parse_u64(obj, "_SOURCE_REALTIME_TIMESTAMP"))
}

fn parse_u64(obj: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<u64> {
    obj.get(key).and_then(|v| match v {
        serde_json::Value::String(s) => s.parse::<u64>().ok(),
        serde_json::Value::Number(n) => n.as_u64(),
        _ => None,
    })
}

fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn json_value_to_bytes(v: &serde_json::Value) -> Vec<u8> {
    match v {
        serde_json::Value::String(s) => s.as_bytes().to_vec(),
        serde_json::Value::Number(_) | serde_json::Value::Bool(_) | serde_json::Value::Null => {
            serde_json::to_vec(v).unwrap_or_default()
        }
        serde_json::Value::Array(arr) => {
            if let Some(bytes) = try_byte_array(arr) {
                return bytes;
            }
            serde_json::to_vec(v).unwrap_or_default()
        }
        serde_json::Value::Object(_) => serde_json::to_vec(v).unwrap_or_default(),
    }
}

fn try_byte_array(arr: &[serde_json::Value]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        let n = v.as_u64()?;
        let b = u8::try_from(n).ok()?;
        out.push(b);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn collector_marks_truncated_when_limit_exceeded() {
        let filter = JournalFilter {
            limit: 1,
            max_bytes: 1024 * 1024,
            ..Default::default()
        };
        let mut collector = JournalCollector::new(&filter);

        let a = r#"{"__REALTIME_TIMESTAMP":"1","MESSAGE":"a"}"#;
        let b = r#"{"__REALTIME_TIMESTAMP":"2","MESSAGE":"b"}"#;

        assert_eq!(collector.push_line(a).expect("ok"), CollectAction::Continue);
        assert_eq!(
            collector.push_line(b).expect("ok"),
            CollectAction::StopTruncated
        );
        assert!(collector.truncated);
        assert_eq!(collector.entries.len(), 1);
        assert_eq!(collector.stats.lines_read, 2);
    }

    #[test]
    fn collector_skip_mode_tracks_errors_and_stops_after_threshold() {
        let filter = JournalFilter {
            parse_error: ParseErrorMode::Skip { max_skipped: 1 },
            ..Default::default()
        };
        let mut collector = JournalCollector::new(&filter);

        let bad = r#"{"__REALTIME_TIMESTAMP":"1","MESSAGE":"oops""#;
        let ok = r#"{"__REALTIME_TIMESTAMP":"2","MESSAGE":"ok"}"#;

        assert_eq!(
            collector.push_line(bad).expect("skipped"),
            CollectAction::Continue
        );
        assert_eq!(collector.stats.parse_errors, 1);
        assert_eq!(collector.stats.skipped_lines, 1);
        assert_eq!(collector.entries.len(), 0);

        assert_eq!(
            collector.push_line(ok).expect("ok"),
            CollectAction::Continue
        );
        assert_eq!(collector.entries.len(), 1);

        let err = collector.push_line(bad).expect_err("exceed max_skipped");
        let Error::ParseError { .. } = err else {
            panic!("unexpected error: {err:?}");
        };
    }

    #[test]
    fn classify_after_cursor_unknown_option_as_backend_unavailable() {
        let err = classify_journalctl_failure(true, "Unknown option --after-cursor=abc")
            .expect("classified");
        let Error::BackendUnavailable { backend, .. } = err else {
            panic!("unexpected error: {err:?}");
        };
        assert_eq!(backend, "journalctl(after-cursor)");
    }

    #[test]
    fn classify_permission_denied_as_permission_error() {
        let err = classify_journalctl_failure(false, "Failed to open journal: Permission denied")
            .expect("classified");
        let Error::PermissionDenied { action, .. } = err else {
            panic!("unexpected error: {err:?}");
        };
        assert_eq!(action, "read_journal");
    }

    #[test]
    fn parse_entry_extracts_basic_fields() {
        let line = r#"{"__REALTIME_TIMESTAMP":"1000000","__CURSOR":"c","MESSAGE":"hello","PRIORITY":"6","_SYSTEMD_UNIT":"nginx.service","_PID":"123"}"#;
        let e = parse_entry(line, 16 * 1024).expect("parse ok");
        assert_eq!(e.cursor.as_deref(), Some("c"));
        assert_eq!(e.message.as_deref(), Some("hello"));
        assert_eq!(e.priority, Some(6));
        assert_eq!(e.unit.as_deref(), Some("nginx.service"));
        assert_eq!(e.pid, Some(123));
        assert!(!e.message_truncated);
        assert!(e.fields.contains_key("MESSAGE"));
    }

    #[test]
    fn parse_entry_truncates_message() {
        let line = r#"{"__REALTIME_TIMESTAMP":"1","MESSAGE":"abcdef"}"#;
        let e = parse_entry(line, 3).expect("parse ok");
        assert_eq!(e.message.as_deref(), Some("abc"));
        assert!(e.message_truncated);
    }

    #[test]
    fn parse_entry_accepts_non_string_message() {
        let line = r#"{"__REALTIME_TIMESTAMP":"1","MESSAGE":[104,101,108,108,111]}"#;
        let e = parse_entry(line, 16 * 1024).expect("parse ok");
        assert_eq!(e.message.as_deref(), Some("hello"));
        assert!(!e.message_truncated);
    }

    #[test]
    fn json_value_to_bytes_handles_byte_arrays() {
        let v = serde_json::json!([0, 255, 1]);
        assert_eq!(json_value_to_bytes(&v), vec![0, 255, 1]);
    }

    #[test]
    fn json_value_to_bytes_falls_back_to_json_text() {
        let v = serde_json::json!(["a", "b"]);
        let bytes = json_value_to_bytes(&v);
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "[\"a\",\"b\"]");
    }
}
