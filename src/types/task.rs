use std::collections::BTreeMap;

/// Specification for running a transient task (feature=`tasks`).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct TaskSpec {
    /// Process argv (must be non-empty; executed without a shell).
    pub argv: Vec<String>,
    /// Environment variables (keys must not contain `=` or control characters).
    pub env: BTreeMap<String, String>,
    /// Working directory for the transient unit.
    pub workdir: Option<String>,
    /// Task execution timeout (also applied as `TimeoutStartUSec` in systemd).
    pub timeout: std::time::Duration,
    /// Optional hint included in the generated transient unit name (sanitized).
    pub name_hint: Option<String>,
}

impl Default for TaskSpec {
    fn default() -> Self {
        Self {
            argv: Vec::new(),
            env: BTreeMap::new(),
            workdir: None,
            timeout: std::time::Duration::from_secs(0),
            name_hint: None,
        }
    }
}

/// Handle for a transient task.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct TaskHandle {
    /// Transient unit name (e.g. `unitbus-<ts>-<nonce>.service`).
    pub unit: String,
    /// D-Bus job object path returned by `StartTransientUnit`.
    pub job_path: String,

    #[doc(hidden)]
    pub(crate) inner: crate::units::JobInner,
}

/// Result of a transient task.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct TaskResult {
    /// Final unit status snapshot.
    pub unit_status: crate::types::unit::UnitStatus,
    /// Exit status when available (`ExecMainCode == CLD_EXITED`).
    pub exit_status: Option<i32>,
    /// Signal number when available (`ExecMainCode == CLD_KILLED/CLD_DUMPED`).
    pub signal: Option<i32>,
}
