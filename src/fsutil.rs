use crate::types::config::{ApplyReport, DropInSpec, RecommendedAction, RemoveReport};
use crate::types::unit_file::{UnitFileRemoveReport, UnitFileWriteReport};
use crate::{Error, Result, util};

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
        out.push_str(&util::quote_systemd_value(&assignment));
        out.push('\n');
    }

    if let Some(dir) = &spec.working_directory {
        out.push_str("WorkingDirectory=");
        out.push_str(&util::quote_systemd_value(dir));
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
        out.push_str(&util::render_systemd_exec(argv)?);
        out.push('\n');
    }

    Ok(out)
}

pub(crate) fn apply_dropin_file(
    systemd_system_dir: &Path,
    unit: &str,
    name: &str,
    contents: String,
) -> Result<ApplyReport> {
    let path = dropin_path(systemd_system_dir, unit, name);
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

pub(crate) fn remove_dropin_file(
    systemd_system_dir: &Path,
    unit: &str,
    name: &str,
) -> Result<RemoveReport> {
    let path = dropin_path(systemd_system_dir, unit, name);
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

pub(crate) fn apply_unit_file(
    systemd_system_dir: &Path,
    unit: &str,
    contents: String,
) -> Result<UnitFileWriteReport> {
    validate_unit_file_name(unit)?;

    let path = unit_file_path(systemd_system_dir, unit);
    let dir = path
        .parent()
        .ok_or_else(|| Error::invalid_input("invalid unit file path"))?;

    fs::create_dir_all(dir).map_err(|e| map_unitfile_io("create unit directory", dir, e))?;

    let existing = match fs::read(&path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(map_unitfile_io("read unit file", &path, e)),
    };

    if let Some(existing) = existing
        && existing == contents.as_bytes()
    {
        return Ok(UnitFileWriteReport {
            changed: false,
            path_written: path.to_string_lossy().into_owned(),
            requires_daemon_reload: false,
        });
    }

    atomic_write(&path, contents.as_bytes())
        .map_err(|e| map_unitfile_io("write unit file", &path, e))?;

    Ok(UnitFileWriteReport {
        changed: true,
        path_written: path.to_string_lossy().into_owned(),
        requires_daemon_reload: true,
    })
}

pub(crate) fn remove_unit_file(
    systemd_system_dir: &Path,
    unit: &str,
) -> Result<UnitFileRemoveReport> {
    validate_unit_file_name(unit)?;

    let path = unit_file_path(systemd_system_dir, unit);
    match fs::remove_file(&path) {
        Ok(()) => Ok(UnitFileRemoveReport {
            changed: true,
            path_removed: path.to_string_lossy().into_owned(),
            requires_daemon_reload: true,
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(UnitFileRemoveReport {
            changed: false,
            path_removed: path.to_string_lossy().into_owned(),
            requires_daemon_reload: false,
        }),
        Err(e) => Err(map_unitfile_io("remove unit file", &path, e)),
    }
}

fn dropin_path(systemd_system_dir: &Path, unit: &str, name: &str) -> PathBuf {
    systemd_system_dir
        .join(format!("{unit}.d"))
        .join(format!("{name}.conf"))
}

fn unit_file_path(systemd_system_dir: &Path, unit: &str) -> PathBuf {
    systemd_system_dir.join(unit)
}

fn validate_unit_file_name(unit: &str) -> Result<()> {
    util::validate_no_control("unit", unit)?;
    let unit = unit.trim();
    if unit.is_empty() {
        return Err(Error::invalid_input("unit must not be empty"));
    }
    if unit.contains('/') || unit.contains('\\') {
        return Err(Error::invalid_input(
            "unit must not contain path separators",
        ));
    }
    if unit.contains("..") {
        return Err(Error::invalid_input("unit must not contain '..'"));
    }
    Ok(())
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

fn map_unitfile_io(context: &'static str, path: &Path, e: io::Error) -> Error {
    if e.kind() == io::ErrorKind::PermissionDenied {
        return Error::PermissionDenied {
            action: "write_unit_files",
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(name: &str) -> PathBuf {
        let n = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut dir = std::env::temp_dir();
        dir.push(format!("unitbus-{name}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

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

    #[test]
    fn apply_and_remove_unit_file_is_idempotent() {
        let dir = temp_dir("unitfile");
        let unit = "unitbus-test.service";

        let r1 = apply_unit_file(&dir, unit, "a\n".to_string()).expect("write ok");
        assert!(r1.changed);
        assert!(r1.requires_daemon_reload);

        let r2 = apply_unit_file(&dir, unit, "a\n".to_string()).expect("write ok");
        assert!(!r2.changed);
        assert!(!r2.requires_daemon_reload);

        let r3 = apply_unit_file(&dir, unit, "b\n".to_string()).expect("write ok");
        assert!(r3.changed);
        assert!(r3.requires_daemon_reload);

        let rm1 = remove_unit_file(&dir, unit).expect("remove ok");
        assert!(rm1.changed);
        assert!(rm1.requires_daemon_reload);

        let rm2 = remove_unit_file(&dir, unit).expect("remove ok");
        assert!(!rm2.changed);
        assert!(!rm2.requires_daemon_reload);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_and_remove_dropin_is_idempotent() {
        let dir = temp_dir("dropin");
        let unit = "unitbus-test.service";
        let name = "demo";
        let contents = "[Service]\nEnvironment=\"A=1\"\n".to_string();

        let r1 = apply_dropin_file(&dir, unit, name, contents.clone()).expect("apply ok");
        assert!(r1.changed);
        assert!(r1.requires_daemon_reload);

        let r2 = apply_dropin_file(&dir, unit, name, contents).expect("apply ok");
        assert!(!r2.changed);
        assert!(!r2.requires_daemon_reload);

        let rm1 = remove_dropin_file(&dir, unit, name).expect("remove ok");
        assert!(rm1.changed);
        assert!(rm1.requires_daemon_reload);

        let rm2 = remove_dropin_file(&dir, unit, name).expect("remove ok");
        assert!(!rm2.changed);
        assert!(!rm2.requires_daemon_reload);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
