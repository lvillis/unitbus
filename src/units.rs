use crate::{
    ActiveState, Error, FailureHint, JobHandle, JobOutcome, LoadState, Result, UnitStartMode,
    UnitStatus, util,
};

use futures_util::StreamExt;
use std::{collections::HashMap, sync::Arc, time::Duration};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const SYSTEMD_UNIT_INTERFACE: &str = "org.freedesktop.systemd1.Unit";
const SYSTEMD_SERVICE_INTERFACE: &str = "org.freedesktop.systemd1.Service";

#[cfg(feature = "tasks")]
static TRANSIENT_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Clone, Debug)]
/// Unit/job control APIs.
pub struct Units {
    inner: Arc<crate::Inner>,
}

impl Units {
    pub(crate) fn new(inner: Arc<crate::Inner>) -> Self {
        Self { inner }
    }

    /// Fetch a snapshot of unit status via D-Bus.
    ///
    /// `unit` is canonicalized (e.g. `"nginx"` becomes `"nginx.service"`).
    pub async fn get_status(&self, unit: &str) -> Result<UnitStatus> {
        let unit = util::canonicalize_unit_name(unit)?;
        let unit_path = self.inner.bus.get_unit_path(&unit).await?;
        unit_status_from_paths(&self.inner.bus, &unit, &unit_path).await
    }

    /// Start a unit and return a job handle.
    pub async fn start(&self, unit: &str, mode: UnitStartMode) -> Result<JobHandle> {
        self.start_like(JobKind::Start, "start", unit, mode).await
    }

    /// Stop a unit and return a job handle.
    pub async fn stop(&self, unit: &str, mode: UnitStartMode) -> Result<JobHandle> {
        self.start_like(JobKind::Stop, "stop", unit, mode).await
    }

    /// Restart a unit and return a job handle.
    pub async fn restart(&self, unit: &str, mode: UnitStartMode) -> Result<JobHandle> {
        self.start_like(JobKind::Restart, "restart", unit, mode)
            .await
    }

    /// Reload a unit and return a job handle.
    pub async fn reload(&self, unit: &str, mode: UnitStartMode) -> Result<JobHandle> {
        self.start_like(JobKind::Reload, "reload", unit, mode).await
    }

    async fn start_like(
        &self,
        kind: JobKind,
        _action: &'static str,
        unit: &str,
        mode: UnitStartMode,
    ) -> Result<JobHandle> {
        let unit = util::canonicalize_unit_name(unit)?;
        let mode_str = mode.as_dbus_str();

        #[cfg(feature = "tracing")]
        tracing::info!(%unit, %mode_str, %_action, "systemd unit request");

        let job_path = match kind {
            JobKind::Start => self.inner.bus.start_unit(&unit, mode_str).await?,
            JobKind::Stop => self.inner.bus.stop_unit(&unit, mode_str).await?,
            JobKind::Restart => self.inner.bus.restart_unit(&unit, mode_str).await?,
            JobKind::Reload => self.inner.bus.reload_unit(&unit, mode_str).await?,
        };

        Ok(JobHandle {
            unit,
            job_path: job_path.to_string(),
            inner: JobInner {
                root: self.inner.clone(),
                kind,
            },
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) enum JobKind {
    Start,
    Stop,
    Restart,
    Reload,
}

#[derive(Clone, Debug)]
pub(crate) struct JobInner {
    pub(crate) root: Arc<crate::Inner>,
    pub(crate) kind: JobKind,
}

impl JobHandle {
    /// Wait for the job to complete or return `Error::JobTimeout`.
    ///
    /// Implementation prefers `JobRemoved` signals, with a bounded polling fallback.
    pub async fn wait(&self, timeout: Duration) -> Result<JobOutcome> {
        if timeout == Duration::from_secs(0) {
            return Err(Error::invalid_input("timeout must be > 0"));
        }
        self.inner
            .wait_job(&self.unit, &self.job_path, timeout)
            .await
    }
}

impl JobInner {
    async fn wait_job(&self, unit: &str, job_path: &str, timeout: Duration) -> Result<JobOutcome> {
        #[cfg(feature = "tracing")]
        tracing::debug!(%unit, %job_path, ?timeout, "wait_job start");

        let manager = self.root.bus.manager_proxy().await?;

        let mut signals = match manager.receive_signal("JobRemoved").await {
            Ok(s) => Some(futures_util::StreamExt::fuse(s)),
            Err(_) => None,
        };

        let mut job_result: Option<String> = None;

        let mut jitter = poll_jitter_seed(job_path);
        let mut poll_interval = apply_jitter(
            self.root.opts.job_poll_initial,
            self.root.opts.job_poll_max,
            &mut jitter,
        );
        let mut poll_timer = futures_util::FutureExt::fuse(async_io::Timer::after(poll_interval));
        let mut deadline = futures_util::FutureExt::fuse(async_io::Timer::after(timeout));

        if !self.root.bus.job_exists(job_path).await? {
            let status = Units::new(self.root.clone()).get_status(unit).await?;
            return Ok(infer_outcome(&self.kind, &status, None));
        }

        loop {
            if let Some(sig) = &mut signals {
                futures_util::select! {
                    _ = deadline => {
                        return Err(Error::JobTimeout { unit: unit.to_string(), timeout });
                    }
                    _ = poll_timer => {
                        if !self.root.bus.job_exists(job_path).await? {
                            break;
                        }
                        poll_interval = next_poll_interval(poll_interval, self.root.opts.job_poll_max, &mut jitter);
                        poll_timer =
                            futures_util::FutureExt::fuse(async_io::Timer::after(poll_interval));
                    }
                    msg = sig.next() => {
                        let Some(msg) = msg else {
                            signals = None;
                            continue;
                        };
                        if let Some(result) = decode_job_removed(job_path, msg)? {
                            job_result = Some(result);
                            break;
                        }
                    }
                }
            } else {
                futures_util::select! {
                    _ = deadline => {
                        return Err(Error::JobTimeout { unit: unit.to_string(), timeout });
                    }
                    _ = poll_timer => {
                        if !self.root.bus.job_exists(job_path).await? {
                            break;
                        }
                        poll_interval = next_poll_interval(poll_interval, self.root.opts.job_poll_max, &mut jitter);
                        poll_timer =
                            futures_util::FutureExt::fuse(async_io::Timer::after(poll_interval));
                    }
                }
            }
        }

        let status = Units::new(self.root.clone()).get_status(unit).await?;

        #[cfg(feature = "tracing")]
        tracing::debug!(%unit, %job_path, job_result = job_result.as_deref().unwrap_or(""), "wait_job done");

        Ok(infer_outcome(&self.kind, &status, job_result.as_deref()))
    }
}

fn next_poll_interval(current: Duration, max: Duration, seed: &mut u64) -> Duration {
    let doubled = current.saturating_mul(2);
    let base = if doubled > max { max } else { doubled };
    apply_jitter(base, max, seed)
}

fn apply_jitter(base: Duration, max: Duration, seed: &mut u64) -> Duration {
    if base >= max {
        return base;
    }

    let base_us = duration_to_micros_saturating(base);
    let max_us = duration_to_micros_saturating(max);

    let amplitude = base_us / 10;
    if amplitude == 0 {
        return base;
    }

    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    let jitter = *seed % amplitude.saturating_add(1);

    let us = std::cmp::min(base_us.saturating_add(jitter), max_us);
    Duration::from_micros(us)
}

fn duration_to_micros_saturating(d: Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

fn poll_jitter_seed(job_path: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for b in job_path.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }

    let now = std::time::SystemTime::now();
    let nanos = match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => u64::from(d.subsec_nanos()),
        Err(_) => 0,
    };

    hash ^ nanos ^ u64::from(std::process::id())
}

fn decode_job_removed(job_path: &str, msg: zbus::Message) -> Result<Option<String>> {
    let body = msg.body();
    let decoded: std::result::Result<(u32, OwnedObjectPath, String, String), _> =
        body.deserialize();
    let (_id, job, _unit, result) = decoded.map_err(|e| Error::DbusError {
        name: "SignalDecode".to_string(),
        message: e.to_string(),
    })?;

    if job.as_str() == job_path {
        return Ok(Some(result));
    }
    Ok(None)
}

fn infer_outcome(kind: &JobKind, status: &UnitStatus, job_result: Option<&str>) -> JobOutcome {
    if status.load_state != LoadState::Loaded {
        return JobOutcome::Failed {
            unit_status: status.clone(),
            reason: FailureHint::NotLoaded {
                load_state: status.load_state.clone(),
            },
        };
    }

    if let Some("canceled") = job_result
        && status.active_state != ActiveState::Active
    {
        return JobOutcome::Canceled {
            unit_status: status.clone(),
        };
    }

    if status.active_state == ActiveState::Failed {
        if let (Some(exec_main_code), Some(exec_main_status)) =
            (status.exec_main_code, status.exec_main_status)
        {
            return JobOutcome::Failed {
                unit_status: status.clone(),
                reason: FailureHint::ExecMainFailed {
                    exec_main_code,
                    exec_main_status,
                },
            };
        }
        return JobOutcome::Failed {
            unit_status: status.clone(),
            reason: FailureHint::UnitFailed {
                result: status.result.clone(),
            },
        };
    }

    if let Some(result) = job_result
        && result != "done"
    {
        return JobOutcome::Failed {
            unit_status: status.clone(),
            reason: FailureHint::JobFailed {
                result: result.to_string(),
            },
        };
    }

    let ok = match kind {
        JobKind::Start | JobKind::Restart => status.active_state == ActiveState::Active,
        JobKind::Stop => status.active_state == ActiveState::Inactive,
        JobKind::Reload => matches!(
            status.active_state,
            ActiveState::Active | ActiveState::Reloading
        ),
    };

    if ok {
        JobOutcome::Success {
            unit_status: status.clone(),
        }
    } else {
        JobOutcome::Failed {
            unit_status: status.clone(),
            reason: FailureHint::UnexpectedState {
                active_state: status.active_state.clone(),
                sub_state: status.sub_state.clone(),
            },
        }
    }
}

async fn unit_status_from_paths(
    bus: &crate::bus::Bus,
    unit: &str,
    unit_path: &OwnedObjectPath,
) -> Result<UnitStatus> {
    let unit_props = bus
        .get_all_properties(unit_path.as_str(), SYSTEMD_UNIT_INTERFACE)
        .await?;

    let service_props = match bus
        .get_all_properties(unit_path.as_str(), SYSTEMD_SERVICE_INTERFACE)
        .await
    {
        Ok(props) => Some(props),
        Err(Error::DbusError { name, .. }) if name.contains("UnknownInterface") => None,
        Err(e) => return Err(e),
    };

    Ok(UnitStatus {
        id: get_string(&unit_props, "Id").unwrap_or_else(|| unit.to_string()),
        description: get_opt_string(&unit_props, "Description"),
        load_state: get_string(&unit_props, "LoadState")
            .map(|v| LoadState::parse(&v))
            .unwrap_or_else(|| LoadState::Unknown("missing".to_string())),
        active_state: get_string(&unit_props, "ActiveState")
            .map(|v| ActiveState::parse(&v))
            .unwrap_or_else(|| ActiveState::Unknown("missing".to_string())),
        sub_state: get_opt_string(&unit_props, "SubState"),
        result: get_opt_string(&unit_props, "Result"),
        fragment_path: get_opt_string(&unit_props, "FragmentPath"),
        main_pid: service_props.as_ref().and_then(|m| get_u32(m, "MainPID")),
        exec_main_code: service_props
            .as_ref()
            .and_then(|m| get_i32(m, "ExecMainCode")),
        exec_main_status: service_props
            .as_ref()
            .and_then(|m| get_i32(m, "ExecMainStatus")),
        n_restarts: service_props.as_ref().and_then(|m| get_u32(m, "NRestarts")),
    })
}

fn get_string(map: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(|v| <&str>::try_from(v).ok())
        .map(|s| s.to_string())
}

fn get_opt_string(map: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    let s = get_string(map, key)?;
    if s.is_empty() { None } else { Some(s) }
}

fn get_u32(map: &HashMap<String, OwnedValue>, key: &str) -> Option<u32> {
    map.get(key).and_then(|v| u32::try_from(v).ok())
}

fn get_i32(map: &HashMap<String, OwnedValue>, key: &str) -> Option<i32> {
    map.get(key).and_then(|v| i32::try_from(v).ok())
}

#[cfg(feature = "config")]
#[derive(Clone, Debug)]
/// Drop-in configuration management (feature=`config`).
pub struct Config {
    inner: Arc<crate::Inner>,
}

#[cfg(feature = "config")]
impl Config {
    pub(crate) fn new(inner: Arc<crate::Inner>) -> Self {
        Self { inner }
    }

    /// Apply a drop-in file under `/etc/systemd/system/<unit>.d/<name>.conf`.
    pub async fn apply_dropin(
        &self,
        mut spec: crate::types::config::DropInSpec,
    ) -> Result<crate::types::config::ApplyReport> {
        spec.unit = util::canonicalize_unit_name(&spec.unit)?;
        util::validate_dropin_name(&spec.name)?;
        for key in spec.environment.keys() {
            util::validate_env_key(key)?;
        }

        #[cfg(feature = "tracing")]
        tracing::info!(
            unit = %spec.unit,
            name = %spec.name,
            env_keys = spec.environment.len(),
            has_workdir = spec.working_directory.is_some(),
            has_exec_override = spec.exec_start_override.is_some(),
            "apply_dropin"
        );

        let unit = spec.unit.clone();
        let name = spec.name.clone();
        let contents = crate::fsutil::render_dropin(&spec)?;
        let report =
            blocking::unblock(move || crate::fsutil::apply_dropin_file(&unit, &name, contents))
                .await?;

        #[cfg(feature = "tracing")]
        tracing::info!(
            unit = %spec.unit,
            name = %spec.name,
            changed = report.changed,
            requires_daemon_reload = report.requires_daemon_reload,
            "apply_dropin done"
        );

        Ok(report)
    }

    /// Remove a drop-in file under `/etc/systemd/system/<unit>.d/<name>.conf`.
    pub async fn remove_dropin(
        &self,
        unit: &str,
        name: &str,
    ) -> Result<crate::types::config::RemoveReport> {
        let unit = util::canonicalize_unit_name(unit)?;
        util::validate_dropin_name(name)?;

        #[cfg(feature = "tracing")]
        tracing::info!(unit = %unit, name = %name, "remove_dropin");

        let unit2 = unit.clone();
        let name2 = name.to_string();
        let report =
            blocking::unblock(move || crate::fsutil::remove_dropin_file(&unit2, &name2)).await?;

        #[cfg(feature = "tracing")]
        tracing::info!(
            unit = %unit,
            name = %name,
            changed = report.changed,
            requires_daemon_reload = report.requires_daemon_reload,
            "remove_dropin done"
        );

        Ok(report)
    }

    /// Reload systemd manager configuration (`org.freedesktop.systemd1.Manager.Reload`).
    pub async fn daemon_reload(&self) -> Result<()> {
        #[cfg(feature = "tracing")]
        tracing::info!("daemon_reload");
        self.inner.bus.daemon_reload().await
    }
}

#[cfg(feature = "tasks")]
#[derive(Clone, Debug)]
/// Transient task execution (feature=`tasks`).
pub struct Tasks {
    inner: Arc<crate::Inner>,
}

#[cfg(feature = "tasks")]
impl Tasks {
    pub(crate) fn new(inner: Arc<crate::Inner>) -> Self {
        Self { inner }
    }

    /// Run a one-shot transient task using `StartTransientUnit`.
    ///
    /// The transient unit is configured as `Type=oneshot`, without a shell, and routes stdout/stderr
    /// to journald.
    pub async fn run(
        &self,
        spec: crate::types::task::TaskSpec,
    ) -> Result<crate::types::task::TaskHandle> {
        if spec.argv.is_empty() {
            return Err(Error::invalid_input("argv must not be empty"));
        }
        for arg in &spec.argv {
            util::validate_no_control("argv", arg)?;
        }
        if spec.argv[0].trim().is_empty() {
            return Err(Error::invalid_input("argv[0] must not be empty"));
        }
        for (k, v) in &spec.env {
            util::validate_env_key(k)?;
            util::validate_no_control("env value", v)?;
        }
        if let Some(workdir) = &spec.workdir {
            util::validate_no_control("workdir", workdir)?;
        }
        if let Some(name_hint) = &spec.name_hint {
            util::validate_no_control("name_hint", name_hint)?;
        }
        if spec.timeout == Duration::from_secs(0) {
            return Err(Error::invalid_input("timeout must be > 0"));
        }

        let unit = transient_unit_name(spec.name_hint.as_deref());

        #[cfg(feature = "tracing")]
        tracing::info!(
            unit = %unit,
            argv0 = %spec.argv[0],
            argc = spec.argv.len(),
            env_keys = spec.env.len(),
            has_workdir = spec.workdir.is_some(),
            timeout_us = duration_to_micros(spec.timeout),
            "run_task"
        );

        let mut props: Vec<(String, OwnedValue)> = Vec::new();
        props.push(("Type".to_string(), owned_value("Type", "oneshot")?));

        let argv0 = spec.argv[0].clone();
        let exec = vec![(argv0, spec.argv.clone(), false)];
        props.push(("ExecStart".to_string(), owned_value("ExecStart", exec)?));

        if let Some(workdir) = spec.workdir {
            props.push((
                "WorkingDirectory".to_string(),
                owned_value("WorkingDirectory", workdir)?,
            ));
        }

        if !spec.env.is_empty() {
            let env = spec
                .env
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>();
            props.push(("Environment".to_string(), owned_value("Environment", env)?));
        }

        let timeout_us = duration_to_micros(spec.timeout);
        props.push((
            "TimeoutStartUSec".to_string(),
            owned_value("TimeoutStartUSec", timeout_us)?,
        ));

        props.push((
            "StandardOutput".to_string(),
            owned_value("StandardOutput", "journal")?,
        ));
        props.push((
            "StandardError".to_string(),
            owned_value("StandardError", "journal")?,
        ));

        let job_path = self
            .inner
            .bus
            .start_transient_unit(&unit, UnitStartMode::Replace.as_dbus_str(), props)
            .await?;

        #[cfg(feature = "tracing")]
        tracing::info!(unit = %unit, job_path = %job_path.as_str(), "run_task started");

        Ok(crate::types::task::TaskHandle {
            unit,
            job_path: job_path.to_string(),
            inner: JobInner {
                root: self.inner.clone(),
                kind: JobKind::Start,
            },
        })
    }
}

#[cfg(feature = "tasks")]
impl crate::types::task::TaskHandle {
    /// Wait for the transient task to finish and return `TaskResult`.
    ///
    /// This method always returns the final `UnitStatus` plus best-effort exit information.
    pub async fn wait(&self, timeout: Duration) -> Result<crate::types::task::TaskResult> {
        if timeout == Duration::from_secs(0) {
            return Err(Error::invalid_input("timeout must be > 0"));
        }

        let outcome = self
            .inner
            .wait_job(&self.unit, &self.job_path, timeout)
            .await?;
        let unit_status = match outcome {
            JobOutcome::Success { unit_status }
            | JobOutcome::Failed { unit_status, .. }
            | JobOutcome::Canceled { unit_status } => unit_status,
        };

        let (exit_status, signal) = decode_exit_status(&unit_status);
        Ok(crate::types::task::TaskResult {
            unit_status,
            exit_status,
            signal,
        })
    }
}

#[cfg(feature = "tasks")]
fn decode_exit_status(status: &UnitStatus) -> (Option<i32>, Option<i32>) {
    const CLD_EXITED: i32 = 1;
    const CLD_KILLED: i32 = 2;
    const CLD_DUMPED: i32 = 3;

    let (Some(code), Some(value)) = (status.exec_main_code, status.exec_main_status) else {
        return (None, None);
    };

    match code {
        CLD_EXITED => (Some(value), None),
        CLD_KILLED | CLD_DUMPED => (None, Some(value)),
        _ => (None, None),
    }
}

#[cfg(feature = "tasks")]
fn duration_to_micros(d: Duration) -> u64 {
    let us = d.as_micros();
    u64::try_from(us).unwrap_or(u64::MAX)
}

#[cfg(feature = "tasks")]
fn owned_value<T>(context: &'static str, v: T) -> Result<OwnedValue>
where
    zbus::zvariant::Value<'static>: From<T>,
{
    let value: zbus::zvariant::Value<'static> = zbus::zvariant::Value::from(v);
    OwnedValue::try_from(value).map_err(|e| Error::IoError {
        context: format!("encode transient unit property {context}: {e}"),
    })
}

#[cfg(feature = "tasks")]
fn transient_unit_name(name_hint: Option<&str>) -> String {
    let now = std::time::SystemTime::now();
    let ts = match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    };

    let n = TRANSIENT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = u64::from(std::process::id());
    let nonce = (ts ^ n ^ pid).wrapping_mul(0x9E37_79B9_7F4A_7C15);

    let hint = name_hint.and_then(sanitize_unit_name_hint);
    match hint {
        Some(h) => format!("unitbus-{h}-{ts}-{nonce:016x}.service"),
        None => format!("unitbus-{ts}-{nonce:016x}.service"),
    }
}

#[cfg(feature = "tasks")]
fn sanitize_unit_name_hint(input: &str) -> Option<String> {
    let mut out = String::new();
    for c in input.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
        if out.len() >= 32 {
            break;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn status(load: LoadState, active: ActiveState) -> UnitStatus {
        UnitStatus {
            id: "x.service".to_string(),
            description: None,
            load_state: load,
            active_state: active,
            sub_state: None,
            result: None,
            fragment_path: None,
            main_pid: None,
            exec_main_code: None,
            exec_main_status: None,
            n_restarts: None,
        }
    }

    #[test]
    fn infer_outcome_not_loaded() {
        let s = status(LoadState::NotFound, ActiveState::Inactive);
        let out = infer_outcome(&JobKind::Start, &s, Some("done"));

        let JobOutcome::Failed { reason, .. } = out else {
            panic!("unexpected outcome: {out:?}");
        };
        let FailureHint::NotLoaded { load_state } = reason else {
            panic!("unexpected reason: {reason:?}");
        };
        assert_eq!(load_state, LoadState::NotFound);
    }

    #[test]
    fn infer_outcome_exec_main_failed() {
        let mut s = status(LoadState::Loaded, ActiveState::Failed);
        s.exec_main_code = Some(1);
        s.exec_main_status = Some(42);

        let out = infer_outcome(&JobKind::Start, &s, Some("done"));

        let JobOutcome::Failed { reason, .. } = out else {
            panic!("unexpected outcome: {out:?}");
        };
        let FailureHint::ExecMainFailed {
            exec_main_code,
            exec_main_status,
        } = reason
        else {
            panic!("unexpected reason: {reason:?}");
        };
        assert_eq!(exec_main_code, 1);
        assert_eq!(exec_main_status, 42);
    }

    #[test]
    fn infer_outcome_unit_failed_without_exec_info() {
        let mut s = status(LoadState::Loaded, ActiveState::Failed);
        s.result = Some("exit-code".to_string());

        let out = infer_outcome(&JobKind::Start, &s, Some("done"));

        let JobOutcome::Failed { reason, .. } = out else {
            panic!("unexpected outcome: {out:?}");
        };
        let FailureHint::UnitFailed { result } = reason else {
            panic!("unexpected reason: {reason:?}");
        };
        assert_eq!(result.as_deref(), Some("exit-code"));
    }

    #[test]
    fn infer_outcome_canceled_when_not_active() {
        let s = status(LoadState::Loaded, ActiveState::Inactive);
        let out = infer_outcome(&JobKind::Start, &s, Some("canceled"));

        let JobOutcome::Canceled { .. } = out else {
            panic!("unexpected outcome: {out:?}");
        };
    }

    #[test]
    fn infer_outcome_job_failed_when_result_not_done() {
        let s = status(LoadState::Loaded, ActiveState::Inactive);
        let out = infer_outcome(&JobKind::Start, &s, Some("dependency"));

        let JobOutcome::Failed { reason, .. } = out else {
            panic!("unexpected outcome: {out:?}");
        };
        let FailureHint::JobFailed { result } = reason else {
            panic!("unexpected reason: {reason:?}");
        };
        assert_eq!(result, "dependency");
    }

    #[test]
    fn infer_outcome_stop_success_when_inactive() {
        let s = status(LoadState::Loaded, ActiveState::Inactive);
        let out = infer_outcome(&JobKind::Stop, &s, Some("done"));

        let JobOutcome::Success { .. } = out else {
            panic!("unexpected outcome: {out:?}");
        };
    }

    #[cfg(feature = "tasks")]
    #[test]
    fn decode_exit_status_follows_prd_rules() {
        let mut s = status(LoadState::Loaded, ActiveState::Inactive);
        s.exec_main_code = Some(1);
        s.exec_main_status = Some(7);
        assert_eq!(decode_exit_status(&s), (Some(7), None));

        s.exec_main_code = Some(2);
        s.exec_main_status = Some(9);
        assert_eq!(decode_exit_status(&s), (None, Some(9)));

        s.exec_main_code = Some(999);
        assert_eq!(decode_exit_status(&s), (None, None));
    }
}
