use crate::Result;
#[cfg(any(feature = "journal-cli", feature = "journal-sdjournal", test))]
use crate::types::journal::{JournalEntry, JournalFilter, JournalResult, JournalStats};

use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Journal {
    inner: Arc<crate::Inner>,
}

#[cfg(any(feature = "journal-cli", feature = "journal-sdjournal", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JournalQueryMode {
    RecentTail,
    AfterCursor,
}

#[cfg(any(feature = "journal-cli", feature = "journal-sdjournal", test))]
impl JournalQueryMode {
    fn from_filter(filter: &JournalFilter) -> Self {
        if filter.after_cursor.is_some() {
            Self::AfterCursor
        } else {
            Self::RecentTail
        }
    }

    fn collect_newest_first(self) -> bool {
        matches!(self, Self::RecentTail)
    }
}

#[cfg(any(feature = "journal-cli", feature = "journal-sdjournal", test))]
fn validate_query_filter(filter: &JournalFilter) -> Result<()> {
    if filter.limit == 0 {
        return Err(crate::Error::invalid_input("journal limit must be > 0"));
    }
    if filter.max_bytes == 0 {
        return Err(crate::Error::invalid_input("journal max_bytes must be > 0"));
    }
    if filter.max_message_bytes == 0 {
        return Err(crate::Error::invalid_input(
            "journal max_message_bytes must be > 0",
        ));
    }
    if let Some(cursor) = &filter.after_cursor {
        crate::util::validate_no_control("cursor", cursor)?;
    }
    Ok(())
}

#[cfg(any(feature = "journal-cli", feature = "journal-sdjournal", test))]
fn finalize_query_result(
    mut entries: Vec<JournalEntry>,
    truncated: bool,
    stats: JournalStats,
    mode: JournalQueryMode,
) -> JournalResult {
    if mode.collect_newest_first() {
        // Internally tail newest-first so bounded queries prefer the most recent logs,
        // then restore chronological order for callers.
        entries.reverse();
    }

    let next_cursor = entries.last().and_then(|entry| entry.cursor.clone());

    JournalResult {
        entries,
        next_cursor,
        truncated,
        stats,
    }
}

impl Journal {
    pub(crate) fn new(inner: Arc<crate::Inner>) -> Self {
        Self { inner }
    }

    /// Query journald logs using the configured backend.
    ///
    /// Default backend: `sdjournal` (feature=`journal-sdjournal`).
    /// Alternative backend: `journalctl --output=json` (feature=`journal-cli`).
    ///
    /// The result is always bounded by `filter.limit` and `filter.max_bytes`. When limits are hit,
    /// `JournalResult.truncated` is set to `true`.
    ///
    /// Without `filter.after_cursor`, bounded queries return the most recent matching entries in
    /// chronological order.
    pub async fn query(
        &self,
        filter: crate::types::journal::JournalFilter,
    ) -> Result<crate::types::journal::JournalResult> {
        #[cfg(feature = "journal-cli")]
        {
            return crate::journal::cli::query_journalctl(&self.inner.opts, filter).await;
        }

        #[cfg(all(not(feature = "journal-cli"), feature = "journal-sdjournal"))]
        {
            return crate::journal::sdjournal::query_sdjournal(&self.inner.opts, filter).await;
        }

        #[cfg(all(not(feature = "journal-cli"), not(feature = "journal-sdjournal")))]
        {
            let _ = filter;
            return Err(crate::Error::BackendUnavailable {
                backend: "journald",
                detail: "no journald backend enabled (enable journal-cli or journal-sdjournal)"
                    .to_string(),
            });
        }
    }

    /// Convenience helper that fetches a status snapshot and a bounded log slice around "now".
    ///
    /// The default time window is `now - 30s` to `now + 10s` (see `DiagnosisOptions::default`).
    pub async fn diagnose_unit_failure(
        &self,
        unit: &str,
        opts: crate::types::journal::DiagnosisOptions,
    ) -> Result<crate::types::journal::Diagnosis> {
        let unit = crate::util::canonicalize_unit_name(unit)?;

        #[cfg(feature = "tracing")]
        tracing::info!(
            unit = %unit,
            limit = opts.limit,
            max_bytes = opts.max_bytes,
            max_message_bytes = opts.max_message_bytes,
            "diagnose_unit_failure"
        );

        let status = crate::units::Units::new(self.inner.clone())
            .get_status(&unit)
            .await?;

        let now = std::time::SystemTime::now();
        let since = match now.checked_sub(opts.window_before) {
            Some(t) => t,
            None => std::time::UNIX_EPOCH,
        };
        let until = now.checked_add(opts.window_after);

        let filter = crate::types::journal::JournalFilter {
            unit: Some(unit),
            since: Some(since),
            until,
            after_cursor: None,
            limit: opts.limit,
            max_bytes: opts.max_bytes,
            max_message_bytes: opts.max_message_bytes,
            timeout: opts.timeout,
            parse_error: opts.parse_error,
        };

        let res = self.query(filter).await?;
        Ok(crate::types::journal::Diagnosis {
            status,
            logs: res.entries,
            truncated: res.truncated,
        })
    }
}

#[cfg(feature = "journal-cli")]
mod cli;

#[cfg(all(feature = "journal-sdjournal", not(feature = "journal-cli")))]
mod sdjournal;

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn entry_with_cursor(cursor: &str) -> JournalEntry {
        JournalEntry {
            timestamp: std::time::UNIX_EPOCH,
            cursor: Some(cursor.to_string()),
            message: None,
            message_truncated: false,
            priority: None,
            unit: None,
            pid: None,
            fields: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn validate_query_filter_rejects_zero_limits() {
        let err = validate_query_filter(&JournalFilter {
            limit: 0,
            ..JournalFilter::default()
        })
        .expect_err("limit=0 must fail");
        assert!(matches!(err, crate::Error::InvalidInput { .. }));

        let err = validate_query_filter(&JournalFilter {
            max_bytes: 0,
            ..JournalFilter::default()
        })
        .expect_err("max_bytes=0 must fail");
        assert!(matches!(err, crate::Error::InvalidInput { .. }));

        let err = validate_query_filter(&JournalFilter {
            max_message_bytes: 0,
            ..JournalFilter::default()
        })
        .expect_err("max_message_bytes=0 must fail");
        assert!(matches!(err, crate::Error::InvalidInput { .. }));
    }

    #[test]
    fn query_mode_depends_on_cursor_presence() {
        assert_eq!(
            JournalQueryMode::from_filter(&JournalFilter::default()),
            JournalQueryMode::RecentTail
        );

        assert_eq!(
            JournalQueryMode::from_filter(&JournalFilter {
                after_cursor: Some("cursor-1".to_string()),
                ..JournalFilter::default()
            }),
            JournalQueryMode::AfterCursor
        );
    }

    #[test]
    fn recent_tail_results_are_restored_to_chronological_order() {
        let result = finalize_query_result(
            vec![entry_with_cursor("cursor-3"), entry_with_cursor("cursor-2")],
            true,
            JournalStats::default(),
            JournalQueryMode::RecentTail,
        );

        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].cursor.as_deref(), Some("cursor-2"));
        assert_eq!(result.entries[1].cursor.as_deref(), Some("cursor-3"));
        assert_eq!(result.next_cursor.as_deref(), Some("cursor-3"));
        assert!(result.truncated);
    }

    #[test]
    fn after_cursor_results_keep_streaming_order() {
        let result = finalize_query_result(
            vec![entry_with_cursor("cursor-2"), entry_with_cursor("cursor-3")],
            false,
            JournalStats::default(),
            JournalQueryMode::AfterCursor,
        );

        assert_eq!(result.entries[0].cursor.as_deref(), Some("cursor-2"));
        assert_eq!(result.entries[1].cursor.as_deref(), Some("cursor-3"));
        assert_eq!(result.next_cursor.as_deref(), Some("cursor-3"));
        assert!(!result.truncated);
    }
}
