use crate::{Error, Result};

#[cfg(feature = "journal-cli")]
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) fn canonicalize_unit_name(input: &str) -> Result<String> {
    validate_no_control("unit", input)?;
    let input = input.trim();
    if input.is_empty() {
        return Err(Error::invalid_input("unit must not be empty"));
    }
    if input.contains('/') || input.contains('\\') {
        return Err(Error::invalid_input(
            "unit must not contain path separators",
        ));
    }
    if input.contains("..") {
        return Err(Error::invalid_input("unit must not contain '..'"));
    }

    if input.contains('.') {
        return Ok(input.to_string());
    }
    Ok(format!("{input}.service"))
}

#[cfg(feature = "config")]
pub(crate) fn validate_dropin_name(input: &str) -> Result<()> {
    validate_no_control("drop-in name", input)?;
    let input = input.trim();
    if input.is_empty() {
        return Err(Error::invalid_input("drop-in name must not be empty"));
    }
    if input.contains('/') || input.contains('\\') {
        return Err(Error::invalid_input(
            "drop-in name must not contain path separators",
        ));
    }
    if input.contains("..") {
        return Err(Error::invalid_input("drop-in name must not contain '..'"));
    }
    if input.ends_with(".conf") {
        return Err(Error::invalid_input(
            "drop-in name must not include the .conf suffix",
        ));
    }
    Ok(())
}

#[cfg(any(feature = "config", feature = "tasks"))]
pub(crate) fn validate_env_key(input: &str) -> Result<()> {
    validate_no_control("env key", input)?;
    if input.is_empty() {
        return Err(Error::invalid_input("env key must not be empty"));
    }
    if input.contains('=') {
        return Err(Error::invalid_input("env key must not contain '='"));
    }
    Ok(())
}

pub(crate) fn validate_no_control(context: &'static str, input: &str) -> Result<()> {
    if input.contains('\0') {
        return Err(Error::invalid_input(format!(
            "{context} must not contain NUL"
        )));
    }
    if input.contains('\n') || input.contains('\r') {
        return Err(Error::invalid_input(format!(
            "{context} must not contain newlines"
        )));
    }
    if input.chars().any(|c| c.is_control()) {
        return Err(Error::invalid_input(format!(
            "{context} must not contain control characters"
        )));
    }
    Ok(())
}

#[cfg(feature = "journal-cli")]
pub(crate) fn unix_seconds(t: SystemTime) -> Result<i64> {
    let dur = t.duration_since(UNIX_EPOCH).map_err(|e| Error::IoError {
        context: format!("system time before unix epoch: {e}"),
    })?;
    i64::try_from(dur.as_secs()).map_err(|e| Error::IoError {
        context: format!("unix seconds overflow: {e}"),
    })
}

#[cfg(feature = "journal-cli")]
pub(crate) fn system_time_from_unix_micros(us: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_micros(us)
}

#[cfg(feature = "journal-cli")]
pub(crate) fn truncate_string_bytes(input: &str, max_bytes: usize) -> (String, bool) {
    if input.len() <= max_bytes {
        return (input.to_string(), false);
    }
    let mut end = max_bytes;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    (input[..end].to_string(), true)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn canonicalize_unit_appends_service_suffix() {
        let name = canonicalize_unit_name("nginx").expect("ok");
        assert_eq!(name, "nginx.service");
    }

    #[test]
    fn canonicalize_unit_keeps_existing_suffix() {
        let name = canonicalize_unit_name("nginx.timer").expect("ok");
        assert_eq!(name, "nginx.timer");
    }

    #[test]
    fn canonicalize_unit_rejects_control_chars() {
        let err = canonicalize_unit_name("nginx\n").expect_err("must fail");
        let Error::InvalidInput { .. } = err else {
            panic!("unexpected error: {err:?}");
        };
    }

    #[test]
    fn canonicalize_unit_rejects_path_separators() {
        let err = canonicalize_unit_name("a/b").expect_err("must fail");
        let Error::InvalidInput { .. } = err else {
            panic!("unexpected error: {err:?}");
        };
    }

    #[test]
    fn canonicalize_unit_rejects_dotdot() {
        let err = canonicalize_unit_name("../nginx").expect_err("must fail");
        let Error::InvalidInput { .. } = err else {
            panic!("unexpected error: {err:?}");
        };
    }
}
