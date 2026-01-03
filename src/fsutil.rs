use crate::types::config::{ApplyReport, DropInSpec, RecommendedAction, RemoveReport};
use crate::{Error, Result, util};

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const ETC_SYSTEMD_SYSTEM: &str = "/etc/systemd/system";

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn render_dropin(spec: &DropInSpec) -> Result<String> {
    util::validate_no_control("unit", &spec.unit)?;
    util::validate_dropin_name(&spec.name)?;

    for (k, v) in &spec.environment {
        util::validate_env_key(k)?;
        util::validate_no_control("env value", v)?;
    }
    if let Some(v) = &spec.working_directory {
        util::validate_no_control("working_directory", v)?;
    }
    if let Some(v) = &spec.restart {
        util::validate_no_control("restart", v)?;
    }

    let mut out = String::new();
    out.push_str("# Managed by unitbus. DO NOT EDIT.\n");
    out.push_str("[Service]\n");

    for (k, v) in &spec.environment {
        let assignment = format!("{k}={v}");
        out.push_str("Environment=");
        out.push_str(&quote_systemd_value(&assignment));
        out.push('\n');
    }

    if let Some(dir) = &spec.working_directory {
        out.push_str("WorkingDirectory=");
        out.push_str(&quote_systemd_value(dir));
        out.push('\n');
    }

    if let Some(restart) = &spec.restart {
        out.push_str("Restart=");
        out.push_str(restart);
        out.push('\n');
    }

    if let Some(sec) = spec.timeout_start_sec {
        out.push_str("TimeoutStartSec=");
        out.push_str(&sec.to_string());
        out.push('\n');
    }

    if let Some(argv) = &spec.exec_start_override {
        if argv.is_empty() {
            return Err(Error::invalid_input(
                "exec_start_override must not be empty",
            ));
        }
        out.push_str("ExecStart=\n");
        out.push_str("ExecStart=");
        out.push_str(&render_exec(argv)?);
        out.push('\n');
    }

    Ok(out)
}

pub(crate) fn apply_dropin_file(unit: &str, name: &str, contents: String) -> Result<ApplyReport> {
    let path = dropin_path(unit, name);
    let dir = path
        .parent()
        .ok_or_else(|| Error::invalid_input("invalid drop-in path"))?;
    fs::create_dir_all(dir).map_err(|e| map_dropin_io("create drop-in directory", dir, e))?;

    let existing = match fs::read(&path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(map_dropin_io("read drop-in", &path, e)),
    };

    if let Some(existing) = existing
        && existing == contents.as_bytes()
    {
        return Ok(ApplyReport {
            changed: false,
            path_written: path.to_string_lossy().into_owned(),
            requires_daemon_reload: false,
            recommended_action: RecommendedAction::None,
        });
    }

    atomic_write(&path, contents.as_bytes())
        .map_err(|e| map_dropin_io("write drop-in", &path, e))?;

    Ok(ApplyReport {
        changed: true,
        path_written: path.to_string_lossy().into_owned(),
        requires_daemon_reload: true,
        recommended_action: RecommendedAction::DaemonReload,
    })
}

pub(crate) fn remove_dropin_file(unit: &str, name: &str) -> Result<RemoveReport> {
    let path = dropin_path(unit, name);
    match fs::remove_file(&path) {
        Ok(()) => Ok(RemoveReport {
            changed: true,
            path_removed: path.to_string_lossy().into_owned(),
            requires_daemon_reload: true,
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(RemoveReport {
            changed: false,
            path_removed: path.to_string_lossy().into_owned(),
            requires_daemon_reload: false,
        }),
        Err(e) => Err(map_dropin_io("remove drop-in", &path, e)),
    }
}

fn dropin_path(unit: &str, name: &str) -> PathBuf {
    Path::new(ETC_SYSTEMD_SYSTEM)
        .join(format!("{unit}.d"))
        .join(format!("{name}.conf"))
}

fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;

    let tmp_path = loop {
        let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = dir.join(format!(
            ".{}.tmp-{}-{}",
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("dropin.conf"),
            std::process::id(),
            n
        ));
        if !candidate.exists() {
            break candidate;
        }
    };

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)?;
    file.write_all(contents)?;
    file.sync_all()?;

    fs::rename(&tmp_path, path)?;
    fsync_dir(dir)?;
    Ok(())
}

fn map_dropin_io(context: &'static str, path: &Path, e: io::Error) -> Error {
    if e.kind() == io::ErrorKind::PermissionDenied {
        return Error::PermissionDenied {
            action: "write_dropins",
            detail: format!("{context} {}: {e}", path.to_string_lossy()),
        };
    }
    Error::IoError {
        context: format!("{context} {}: {e}", path.to_string_lossy()),
    }
}

#[cfg(unix)]
fn fsync_dir(dir: &Path) -> io::Result<()> {
    let f = fs::File::open(dir)?;
    f.sync_all()
}

#[cfg(not(unix))]
fn fsync_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}

fn quote_systemd_value(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn render_exec(argv: &[String]) -> Result<String> {
    for arg in argv {
        util::validate_no_control("exec argv", arg)?;
    }
    Ok(argv
        .iter()
        .map(|a| quote_exec_arg(a))
        .collect::<Vec<_>>()
        .join(" "))
}

fn quote_exec_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_string();
    }
    if arg
        .chars()
        .any(|c| c.is_whitespace() || c == '"' || c == '\\')
    {
        quote_systemd_value(arg)
    } else {
        arg.to_string()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn render_dropin_is_stable_and_sorted() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("B".to_string(), "2".to_string());
        env.insert("A".to_string(), "1".to_string());

        let spec = DropInSpec {
            unit: "nginx.service".to_string(),
            name: "unitbus".to_string(),
            environment: env,
            working_directory: Some("/srv/app".to_string()),
            restart: Some("always".to_string()),
            timeout_start_sec: Some(10),
            exec_start_override: None,
        };

        let rendered = render_dropin(&spec).expect("render ok");
        let idx_a = rendered.find("Environment=\"A=1\"").expect("A exists");
        let idx_b = rendered.find("Environment=\"B=2\"").expect("B exists");
        assert!(idx_a < idx_b);
        assert!(rendered.ends_with('\n'));
    }
}
