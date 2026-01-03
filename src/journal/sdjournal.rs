use crate::types::journal::{
    JournalEntry, JournalFilter, JournalResult, JournalStats, ParseErrorMode,
};
use crate::{Error, Result, UnitBusOptions};

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub(crate) async fn query_sdjournal(
    opts: &UnitBusOptions,
    filter: JournalFilter,
) -> Result<JournalResult> {
    let mut filter = filter;
    let timeout = filter
        .timeout
        .take()
        .unwrap_or(opts.journal_default_timeout);

    let unit = match filter.unit.take() {
        Some(u) => Some(crate::util::canonicalize_unit_name(&u)?),
        None => None,
    };

    let since_realtime = filter
        .since
        .take()
        .map(crate::util::unix_micros)
        .transpose()?;

    let until_realtime = filter
        .until
        .take()
        .map(crate::util::unix_micros)
        .transpose()?;

    let after_cursor = match filter.after_cursor.take() {
        Some(s) => Some(parse_cursor(&s)?),
        None => None,
    };

    let limit = filter.limit;
    let max_bytes = filter.max_bytes;
    let max_message_bytes = filter.max_message_bytes;
    let parse_error = filter.parse_error;

    blocking::unblock(move || {
        query_sdjournal_sync(
            unit,
            since_realtime,
            until_realtime,
            after_cursor,
            limit,
            max_bytes,
            max_message_bytes,
            timeout,
            parse_error,
        )
    })
    .await
}

fn query_sdjournal_sync(
    unit: Option<String>,
    since_realtime: Option<u64>,
    until_realtime: Option<u64>,
    after_cursor: Option<sdjournal::Cursor>,
    limit: u32,
    max_bytes: u32,
    max_message_bytes: u32,
    timeout: Duration,
    parse_error: ParseErrorMode,
) -> Result<JournalResult> {
    let mut stats = JournalStats::default();
    let mut entries: Vec<JournalEntry> = Vec::new();
    let mut truncated = false;
    let mut skipped = 0u32;

    let deadline = Instant::now().checked_add(timeout);

    let journal = sdjournal::Journal::open_default().map_err(map_sdjournal_error)?;
    let mut q = journal.query();

    if let Some(unit) = &unit {
        q.or_group(|g| {
            g.match_exact("_SYSTEMD_UNIT", unit.as_bytes());
            g.match_exact("UNIT", unit.as_bytes());
            g.match_exact("OBJECT_SYSTEMD_UNIT", unit.as_bytes());
        });
    }
    if let Some(us) = since_realtime {
        q.since_realtime(us);
    }
    if let Some(us) = until_realtime {
        q.until_realtime(us);
    }
    if let Some(c) = after_cursor {
        q.after_cursor(c);
    }

    let want = usize::try_from(limit).unwrap_or(usize::MAX);
    let probe = want.saturating_add(1);
    q.limit(probe);

    let iter = q.iter().map_err(map_sdjournal_error)?;

    for item in iter {
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return Err(Error::Timeout {
                action: "sdjournal",
                timeout,
            });
        }

        stats.lines_read = stats.lines_read.saturating_add(1);

        let entry = match item {
            Ok(e) => e,
            Err(e) => match &parse_error {
                ParseErrorMode::FailFast => return Err(map_sdjournal_error(e)),
                ParseErrorMode::Skip { max_skipped } => {
                    stats.parse_errors = stats.parse_errors.saturating_add(1);
                    stats.skipped_lines = stats.skipped_lines.saturating_add(1);
                    skipped = skipped.saturating_add(1);
                    if skipped > *max_skipped {
                        return Err(map_sdjournal_error(e));
                    }
                    continue;
                }
            },
        };

        if entries.len() >= want {
            truncated = true;
            break;
        }

        let entry_bytes = estimate_entry_bytes(&entry);
        let next_bytes = stats.bytes_read.saturating_add(entry_bytes);
        if next_bytes > max_bytes {
            truncated = true;
            break;
        }
        stats.bytes_read = next_bytes;

        let timestamp = crate::util::system_time_from_unix_micros(entry.realtime_usec());
        let cursor = entry
            .cursor()
            .ok()
            .map(|c| c.to_string())
            .filter(|s| !s.is_empty());

        let (message, message_truncated) = match entry.get("MESSAGE") {
            Some(bytes) => {
                let max = usize::try_from(max_message_bytes).unwrap_or(0);
                let truncated = bytes.len() > max;
                let slice = if truncated { &bytes[..max] } else { bytes };
                (Some(String::from_utf8_lossy(slice).into_owned()), truncated)
            }
            None => (None, false),
        };

        let priority = entry
            .get("PRIORITY")
            .and_then(|b| std::str::from_utf8(b).ok())
            .and_then(|s| s.trim().parse::<u8>().ok());

        let unit = entry
            .get("_SYSTEMD_UNIT")
            .and_then(|b| std::str::from_utf8(b).ok())
            .and_then(non_empty_string);

        let pid = entry
            .get("_PID")
            .and_then(|b| std::str::from_utf8(b).ok())
            .and_then(|s| s.trim().parse::<u32>().ok());

        let mut fields = BTreeMap::new();
        for (k, v) in entry.iter_fields() {
            fields.insert(k.to_string(), v.to_vec());
        }

        entries.push(JournalEntry {
            timestamp,
            cursor,
            message,
            message_truncated,
            priority,
            unit,
            pid,
            fields,
        });
    }

    let next_cursor = entries.last().and_then(|e| e.cursor.clone());

    Ok(JournalResult {
        entries,
        next_cursor,
        truncated,
        stats,
    })
}

fn parse_cursor(input: &str) -> Result<sdjournal::Cursor> {
    sdjournal::Cursor::parse(input).map_err(|e| Error::invalid_input(format!("after_cursor: {e}")))
}

fn estimate_entry_bytes(entry: &sdjournal::EntryRef) -> u32 {
    let mut total = 0u32;
    for (k, v) in entry.iter_fields() {
        total = total.saturating_add(u32_from_usize(k.len()));
        total = total.saturating_add(u32_from_usize(v.len()));
    }
    total
}

fn u32_from_usize(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

fn non_empty_string(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn map_sdjournal_error(err: sdjournal::SdJournalError) -> Error {
    let detail = err.to_string();
    match &err {
        sdjournal::SdJournalError::PermissionDenied { .. } => Error::PermissionDenied {
            action: "read_journal",
            detail,
        },
        sdjournal::SdJournalError::Io { source, .. }
            if source.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            Error::PermissionDenied {
                action: "read_journal",
                detail,
            }
        }
        sdjournal::SdJournalError::NotFound | sdjournal::SdJournalError::Unsupported { .. } => {
            Error::BackendUnavailable {
                backend: "sdjournal",
                detail,
            }
        }
        sdjournal::SdJournalError::InvalidQuery { .. } => Error::InvalidInput { context: detail },
        _ => Error::IoError { context: detail },
    }
}
