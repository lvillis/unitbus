use crate::Result;

use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Journal {
    inner: Arc<crate::Inner>,
}

impl Journal {
    pub(crate) fn new(inner: Arc<crate::Inner>) -> Self {
        Self { inner }
    }

    /// Query journald logs using the configured backend (default: `journalctl --output=json`).
    ///
    /// The result is always bounded by `filter.limit` and `filter.max_bytes`. When limits are hit,
    /// `JournalResult.truncated` is set to `true`.
    pub async fn query(
        &self,
        filter: crate::types::journal::JournalFilter,
    ) -> Result<crate::types::journal::JournalResult> {
        #[cfg(feature = "journal-cli")]
        {
            return crate::journal::cli::query_journalctl(&self.inner.opts, filter).await;
        }

        #[cfg(not(feature = "journal-cli"))]
        {
            let _ = filter;
            return Err(crate::Error::BackendUnavailable {
                backend: "journalctl",
                detail: "feature journal-cli is disabled".to_string(),
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
