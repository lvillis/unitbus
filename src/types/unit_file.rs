use crate::{Error, Result, util};

use std::collections::BTreeMap;

/// systemd service `Type=...`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum ServiceType {
    /// Default. Usually means the process started by `ExecStart` is the main service process.
    #[default]
    Simple,
    Exec,
    Forking,
    Oneshot,
    Notify,
    Idle,
    Other(String),
}

impl ServiceType {
    pub fn as_str(&self) -> &str {
        match self {
            ServiceType::Simple => "simple",
            ServiceType::Exec => "exec",
            ServiceType::Forking => "forking",
            ServiceType::Oneshot => "oneshot",
            ServiceType::Notify => "notify",
            ServiceType::Idle => "idle",
            ServiceType::Other(s) => s.as_str(),
        }
    }
}

/// Specification for generating a systemd service unit file.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct ServiceUnitSpec {
    /// Unit name (shorthand names will be canonicalized to `<name>.service`).
    pub unit: String,

    /// Optional `Description=...`.
    pub description: Option<String>,

    /// Optional `After=...` entries.
    pub after: Vec<String>,
    /// Optional `Wants=...` entries.
    pub wants: Vec<String>,
    /// Optional `Requires=...` entries.
    pub requires: Vec<String>,

    /// Optional service `Type=...` (defaults to systemd's default when omitted).
    pub service_type: Option<ServiceType>,

    /// Required `ExecStart=...` argv.
    pub exec_start: Vec<String>,
    /// Optional `ExecStartPre=...` argv list.
    pub exec_start_pre: Vec<Vec<String>>,
    /// Optional `ExecStartPost=...` argv list.
    pub exec_start_post: Vec<Vec<String>>,

    /// Optional `WorkingDirectory=...`.
    pub working_directory: Option<String>,

    /// Optional `User=...`.
    pub user: Option<String>,
    /// Optional `Group=...`.
    pub group: Option<String>,

    /// Environment variables rendered as `Environment="K=V"`.
    pub environment: BTreeMap<String, String>,

    /// Optional `Restart=...` (raw string, validated for control chars).
    pub restart: Option<String>,
    /// Optional `RestartSec=...` seconds.
    pub restart_sec: Option<u32>,
    /// Optional `TimeoutStartSec=...` seconds.
    pub timeout_start_sec: Option<u32>,
    /// Optional `TimeoutStopSec=...` seconds.
    pub timeout_stop_sec: Option<u32>,

    /// Optional `StandardOutput=...`.
    pub standard_output: Option<String>,
    /// Optional `StandardError=...`.
    pub standard_error: Option<String>,

    /// Optional `[Install] WantedBy=...` entries.
    pub wanted_by: Vec<String>,
    /// Optional `[Install] RequiredBy=...` entries.
    pub required_by: Vec<String>,
    /// Optional `[Install] Alias=...` entries.
    pub alias: Vec<String>,

    /// Extra raw lines appended under `[Unit]` (escape hatch).
    pub extra_unit: Vec<String>,
    /// Extra raw lines appended under `[Service]` (escape hatch).
    pub extra_service: Vec<String>,
    /// Extra raw lines appended under `[Install]` (escape hatch).
    pub extra_install: Vec<String>,
}

/// Report for writing a unit file.
#[cfg(feature = "config")]
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct UnitFileWriteReport {
    /// Whether the file content changed.
    pub changed: bool,
    /// Path written (or existing path when unchanged).
    pub path_written: String,
    /// Whether a daemon reload is required for systemd to pick up the change.
    pub requires_daemon_reload: bool,
}

/// Report for removing a unit file.
#[cfg(feature = "config")]
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct UnitFileRemoveReport {
    /// Whether the file was removed.
    pub changed: bool,
    /// Path removed (or expected path when unchanged).
    pub path_removed: String,
    /// Whether a daemon reload is required for systemd to pick up the change.
    pub requires_daemon_reload: bool,
}

/// A single unit file change entry returned by systemd.
#[cfg(feature = "config")]
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct UnitFileChange {
    pub kind: String,
    pub path: String,
    pub source: Option<String>,
}

#[cfg(feature = "config")]
impl UnitFileChange {
    pub(crate) fn from_dbus(item: (String, String, String)) -> Self {
        let (kind, path, source) = item;
        let source = if source.is_empty() {
            None
        } else {
            Some(source)
        };
        Self { kind, path, source }
    }
}

/// Options for enabling a unit file via D-Bus (`EnableUnitFiles`).
#[cfg(feature = "config")]
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct UnitFileEnableOptions {
    /// Enable only for the current boot (runtime).
    pub runtime: bool,
    /// Overwrite existing symlinks.
    pub force: bool,
}

/// Options for disabling a unit file via D-Bus (`DisableUnitFiles`).
#[cfg(feature = "config")]
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct UnitFileDisableOptions {
    /// Disable only for the current boot (runtime).
    pub runtime: bool,
}

/// Report returned by enabling unit files.
#[cfg(feature = "config")]
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct UnitFileEnableReport {
    pub carries_install_info: bool,
    pub changes: Vec<UnitFileChange>,
}

/// Report returned by disabling unit files.
#[cfg(feature = "config")]
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct UnitFileDisableReport {
    pub changes: Vec<UnitFileChange>,
}

/// Options for installing a service unit file (write + optional daemon-reload + optional enable).
#[cfg(feature = "config")]
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ServiceUnitInstallOptions {
    /// Whether to call `config().daemon_reload()` after writing (recommended).
    pub daemon_reload: bool,
    /// Whether to enable the unit (`EnableUnitFiles`).
    pub enable: bool,
    pub enable_options: UnitFileEnableOptions,
}

#[cfg(feature = "config")]
impl Default for ServiceUnitInstallOptions {
    fn default() -> Self {
        Self {
            daemon_reload: true,
            enable: true,
            enable_options: UnitFileEnableOptions::default(),
        }
    }
}

/// Report returned by `install_service_unit`.
#[cfg(feature = "config")]
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ServiceUnitInstallReport {
    pub unit: String,
    pub wrote: UnitFileWriteReport,
    pub daemon_reload_performed: bool,
    pub enabled: Option<UnitFileEnableReport>,
}

/// Options for uninstalling a unit file (optional disable + remove + optional daemon-reload).
#[cfg(feature = "config")]
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct UnitUninstallOptions {
    /// Whether to disable the unit (`DisableUnitFiles`) before removing.
    pub disable: bool,
    pub disable_options: UnitFileDisableOptions,
    /// Whether to call `config().daemon_reload()` after removal (recommended).
    pub daemon_reload: bool,
}

#[cfg(feature = "config")]
impl Default for UnitUninstallOptions {
    fn default() -> Self {
        Self {
            disable: true,
            disable_options: UnitFileDisableOptions::default(),
            daemon_reload: true,
        }
    }
}

/// Report returned by `uninstall_unit`.
#[cfg(feature = "config")]
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct UnitUninstallReport {
    pub unit: String,
    pub disabled: Option<UnitFileDisableReport>,
    pub removed: UnitFileRemoveReport,
    pub daemon_reload_performed: bool,
}

impl ServiceUnitSpec {
    /// Canonicalize and validate the unit name.
    pub fn canonical_unit_name(&self) -> Result<String> {
        let unit = util::canonicalize_unit_name(&self.unit)?;
        if !unit.ends_with(".service") {
            return Err(Error::invalid_input("service unit must end with .service"));
        }
        Ok(unit)
    }

    /// Render the unit file content.
    pub fn render(&self) -> Result<String> {
        let unit_name = self.canonical_unit_name()?;

        let description = normalize_opt_line("description", self.description.as_deref())?;

        let after = normalize_unit_list("after", &self.after)?;
        let wants = normalize_unit_list("wants", &self.wants)?;
        let requires = normalize_unit_list("requires", &self.requires)?;

        let wanted_by = normalize_unit_list("wanted_by", &self.wanted_by)?;
        let required_by = normalize_unit_list("required_by", &self.required_by)?;
        let alias = normalize_unit_list("alias", &self.alias)?;

        let exec_start = normalize_argv("exec_start", &self.exec_start)?;
        let exec_start_pre = normalize_argv_list("exec_start_pre", &self.exec_start_pre)?;
        let exec_start_post = normalize_argv_list("exec_start_post", &self.exec_start_post)?;

        for (k, v) in &self.environment {
            util::validate_env_key(k)?;
            util::validate_no_control("env value", v)?;
        }

        let working_directory =
            normalize_opt_line("working_directory", self.working_directory.as_deref())?;
        let user = normalize_opt_line("user", self.user.as_deref())?;
        let group = normalize_opt_line("group", self.group.as_deref())?;

        let restart = normalize_opt_line("restart", self.restart.as_deref())?;
        let standard_output =
            normalize_opt_line("standard_output", self.standard_output.as_deref())?;
        let standard_error = normalize_opt_line("standard_error", self.standard_error.as_deref())?;

        for line in self.extra_unit.iter() {
            validate_raw_line("extra_unit", line)?;
        }
        for line in self.extra_service.iter() {
            validate_raw_line("extra_service", line)?;
        }
        for line in self.extra_install.iter() {
            validate_raw_line("extra_install", line)?;
        }

        let mut out = String::new();
        out.push_str("# Managed by unitbus. DO NOT EDIT.\n");
        out.push_str(&format!("# Unit: {unit_name}\n"));
        out.push_str("[Unit]\n");

        if let Some(desc) = description {
            out.push_str("Description=");
            out.push_str(&desc);
            out.push('\n');
        }
        if !after.is_empty() {
            out.push_str("After=");
            out.push_str(&after.join(" "));
            out.push('\n');
        }
        if !wants.is_empty() {
            out.push_str("Wants=");
            out.push_str(&wants.join(" "));
            out.push('\n');
        }
        if !requires.is_empty() {
            out.push_str("Requires=");
            out.push_str(&requires.join(" "));
            out.push('\n');
        }
        for line in self
            .extra_unit
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            out.push_str(line);
            out.push('\n');
        }

        out.push_str("\n[Service]\n");

        if let Some(t) = &self.service_type {
            let t = t.as_str();
            util::validate_no_control("service_type", t)?;
            if t.trim().is_empty() {
                return Err(Error::invalid_input("service_type must not be empty"));
            }
            out.push_str("Type=");
            out.push_str(t);
            out.push('\n');
        }

        for argv in exec_start_pre {
            out.push_str("ExecStartPre=");
            out.push_str(&util::render_systemd_exec(&argv)?);
            out.push('\n');
        }

        out.push_str("ExecStart=");
        out.push_str(&util::render_systemd_exec(&exec_start)?);
        out.push('\n');

        for argv in exec_start_post {
            out.push_str("ExecStartPost=");
            out.push_str(&util::render_systemd_exec(&argv)?);
            out.push('\n');
        }

        if let Some(dir) = working_directory {
            out.push_str("WorkingDirectory=");
            out.push_str(&util::quote_systemd_value(&dir));
            out.push('\n');
        }
        if let Some(u) = user {
            out.push_str("User=");
            out.push_str(&u);
            out.push('\n');
        }
        if let Some(g) = group {
            out.push_str("Group=");
            out.push_str(&g);
            out.push('\n');
        }

        for (k, v) in &self.environment {
            let assignment = format!("{k}={v}");
            out.push_str("Environment=");
            out.push_str(&util::quote_systemd_value(&assignment));
            out.push('\n');
        }

        if let Some(r) = restart {
            out.push_str("Restart=");
            out.push_str(&r);
            out.push('\n');
        }
        if let Some(sec) = self.restart_sec {
            out.push_str("RestartSec=");
            out.push_str(&sec.to_string());
            out.push('\n');
        }
        if let Some(sec) = self.timeout_start_sec {
            out.push_str("TimeoutStartSec=");
            out.push_str(&sec.to_string());
            out.push('\n');
        }
        if let Some(sec) = self.timeout_stop_sec {
            out.push_str("TimeoutStopSec=");
            out.push_str(&sec.to_string());
            out.push('\n');
        }

        if let Some(v) = standard_output {
            out.push_str("StandardOutput=");
            out.push_str(&v);
            out.push('\n');
        }
        if let Some(v) = standard_error {
            out.push_str("StandardError=");
            out.push_str(&v);
            out.push('\n');
        }

        for line in self
            .extra_service
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            out.push_str(line);
            out.push('\n');
        }

        let has_install = !wanted_by.is_empty()
            || !required_by.is_empty()
            || !alias.is_empty()
            || self.extra_install.iter().any(|s| !s.trim().is_empty());
        if has_install {
            out.push_str("\n[Install]\n");
            if !wanted_by.is_empty() {
                out.push_str("WantedBy=");
                out.push_str(&wanted_by.join(" "));
                out.push('\n');
            }
            if !required_by.is_empty() {
                out.push_str("RequiredBy=");
                out.push_str(&required_by.join(" "));
                out.push('\n');
            }
            if !alias.is_empty() {
                out.push_str("Alias=");
                out.push_str(&alias.join(" "));
                out.push('\n');
            }
            for line in self
                .extra_install
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                out.push_str(line);
                out.push('\n');
            }
        }

        Ok(out)
    }
}

fn normalize_opt_line(context: &'static str, input: Option<&str>) -> Result<Option<String>> {
    let Some(s) = input else {
        return Ok(None);
    };
    util::validate_no_control(context, s)?;
    let s = s.trim();
    if s.is_empty() {
        Ok(None)
    } else {
        Ok(Some(s.to_string()))
    }
}

fn normalize_unit_list(context: &'static str, input: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::<String>::new();
    for item in input {
        util::validate_no_control(context, item)?;
        let s = item.trim();
        if s.is_empty() {
            return Err(Error::invalid_input(format!(
                "{context} must not contain empty items"
            )));
        }
        out.push(s.to_string());
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn normalize_argv(context: &'static str, argv: &[String]) -> Result<Vec<String>> {
    if argv.is_empty() {
        return Err(Error::invalid_input(format!("{context} must not be empty")));
    }
    for arg in argv {
        util::validate_no_control("exec argv", arg)
            .map_err(|e| Error::invalid_input(format!("{context}: {e}")))?;
    }
    Ok(argv.to_vec())
}

fn normalize_argv_list(context: &'static str, list: &[Vec<String>]) -> Result<Vec<Vec<String>>> {
    let mut out = Vec::<Vec<String>>::new();
    for argv in list {
        out.push(normalize_argv(context, argv)?);
    }
    Ok(out)
}

fn validate_raw_line(context: &'static str, line: &str) -> Result<()> {
    util::validate_no_control(context, line)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn render_is_stable_and_sorted() {
        let mut env = BTreeMap::new();
        env.insert("B".to_string(), "2".to_string());
        env.insert("A".to_string(), "1".to_string());

        let spec = ServiceUnitSpec {
            unit: "demo".to_string(),
            description: Some("Demo service".to_string()),
            after: vec!["network-online.target".to_string()],
            wants: vec!["network-online.target".to_string()],
            requires: vec![],
            service_type: Some(ServiceType::Simple),
            exec_start: vec!["/usr/bin/demo".to_string(), "--flag".to_string()],
            exec_start_pre: vec![],
            exec_start_post: vec![],
            working_directory: Some("/srv/demo".to_string()),
            user: Some("demo".to_string()),
            group: Some("demo".to_string()),
            environment: env,
            restart: Some("always".to_string()),
            restart_sec: Some(3),
            timeout_start_sec: Some(10),
            timeout_stop_sec: Some(5),
            standard_output: Some("journal".to_string()),
            standard_error: Some("journal".to_string()),
            wanted_by: vec!["multi-user.target".to_string()],
            required_by: vec![],
            alias: vec![],
            extra_unit: vec![],
            extra_service: vec![],
            extra_install: vec![],
        };

        let rendered = spec.render().expect("render ok");
        assert!(rendered.contains("[Unit]\n"));
        assert!(rendered.contains("[Service]\n"));
        assert!(rendered.contains("[Install]\n"));

        let idx_a = rendered.find("Environment=\"A=1\"").expect("A exists");
        let idx_b = rendered.find("Environment=\"B=2\"").expect("B exists");
        assert!(idx_a < idx_b);
        assert!(rendered.ends_with('\n'));
    }

    #[test]
    fn canonical_unit_name_requires_service_suffix() {
        let spec = ServiceUnitSpec {
            unit: "x.socket".to_string(),
            exec_start: vec!["/bin/true".to_string()],
            ..Default::default()
        };
        let err = spec.canonical_unit_name().expect_err("must fail");
        let Error::InvalidInput { .. } = err else {
            panic!("unexpected error: {err:?}");
        };
    }

    #[test]
    fn render_omits_install_section_when_empty() {
        let spec = ServiceUnitSpec {
            unit: "demo".to_string(),
            exec_start: vec!["/bin/true".to_string()],
            ..Default::default()
        };

        let rendered = spec.render().expect("render ok");
        assert!(rendered.contains("[Unit]\n"));
        assert!(rendered.contains("[Service]\n"));
        assert!(!rendered.contains("[Install]\n"));
    }

    #[test]
    fn render_quotes_exec_args_with_spaces() {
        let spec = ServiceUnitSpec {
            unit: "demo".to_string(),
            exec_start: vec!["/bin/echo".to_string(), "hello world".to_string()],
            ..Default::default()
        };

        let rendered = spec.render().expect("render ok");
        assert!(
            rendered.contains("ExecStart=/bin/echo \"hello world\""),
            "rendered={rendered}"
        );
    }
}
