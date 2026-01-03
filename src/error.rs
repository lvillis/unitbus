use std::time::Duration;

/// Crate-wide result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Error returned by unitbus APIs.
///
/// This error model is designed to be:
/// - **Classifiable** (callers can branch on variants),
/// - **Diagnosable** (includes context like `unit`, `action`, `backend`),
/// - **Bounded** (error snippets are truncated to avoid unbounded memory/log growth).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Input validation failure (e.g. invalid unit name, invalid env key).
    #[error("invalid input: {context}")]
    InvalidInput { context: String },

    /// The current process is not allowed to perform an action (D-Bus policy, filesystem perms,
    /// journald access, etc).
    #[error("permission denied for {action}: {detail}")]
    PermissionDenied {
        action: &'static str,
        detail: String,
    },

    /// The requested unit does not exist (best-effort mapping from D-Bus error names).
    #[error("unit not found: {unit}")]
    UnitNotFound { unit: String },

    /// Timed out while waiting for a systemd job to complete.
    #[error("job timeout for {unit}: {timeout:?}")]
    JobTimeout { unit: String, timeout: Duration },

    /// Timed out while performing an external operation (D-Bus call, `journalctl`, etc).
    #[error("timeout for {action}: {timeout:?}")]
    Timeout {
        action: &'static str,
        timeout: Duration,
    },

    /// A backend is unavailable in the current environment (missing binary, missing D-Bus, feature
    /// disabled, unsupported journald option, etc).
    #[error("backend unavailable ({backend}): {detail}")]
    BackendUnavailable {
        backend: &'static str,
        detail: String,
    },

    /// Raw D-Bus error that did not match a more specific classification.
    #[error("dbus error {name}: {message}")]
    DbusError { name: String, message: String },

    /// Generic I/O or runtime error with context.
    #[error("io error: {context}")]
    IoError { context: String },

    /// Failed to parse an external payload (e.g. a `journalctl --output=json` line).
    ///
    /// `sample` is truncated to avoid unbounded output.
    #[error("parse error: {context}; sample={sample}")]
    ParseError { context: String, sample: String },

    /// A subprocess failed (non-zero exit or other failure mode).
    ///
    /// `stderr` is truncated to avoid unbounded output.
    #[error("process error: {command} (exit={exit_code:?}): {stderr}")]
    ProcessError {
        command: String,
        exit_code: Option<i32>,
        stderr: String,
    },
}

impl Error {
    pub(crate) fn invalid_input(context: impl Into<String>) -> Self {
        Self::InvalidInput {
            context: context.into(),
        }
    }

    #[cfg(feature = "journal-cli")]
    pub(crate) fn parse_error(context: impl Into<String>, sample: impl AsRef<str>) -> Self {
        Self::ParseError {
            context: context.into(),
            sample: truncate_for_error(sample.as_ref(), 512).into_owned(),
        }
    }

    #[cfg(feature = "journal-cli")]
    pub(crate) fn process_error(
        command: impl Into<String>,
        exit_code: Option<i32>,
        stderr: impl AsRef<str>,
    ) -> Self {
        Self::ProcessError {
            command: command.into(),
            exit_code,
            stderr: truncate_for_error(stderr.as_ref(), 8 * 1024).into_owned(),
        }
    }
}

#[cfg(feature = "journal-cli")]
fn truncate_for_error(input: &str, max_bytes: usize) -> std::borrow::Cow<'_, str> {
    if input.len() <= max_bytes {
        return std::borrow::Cow::Borrowed(input);
    }
    let mut end = max_bytes;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    std::borrow::Cow::Owned(input[..end].to_string())
}
