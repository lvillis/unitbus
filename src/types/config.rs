use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RecommendedAction {
    None,
    DaemonReload,
    RestartUnit,
}

/// Specification for generating/applying a systemd drop-in (feature=`config`).
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct DropInSpec {
    /// Target unit name (shorthand names will be canonicalized).
    pub unit: String,
    /// Drop-in name (without `.conf` suffix).
    pub name: String,
    /// Environment variables to set (rendered as `Environment="K=V"`).
    pub environment: BTreeMap<String, String>,
    /// Optional `WorkingDirectory=...`.
    pub working_directory: Option<String>,
    /// Optional `Restart=...`.
    pub restart: Option<String>,
    /// Optional `TimeoutStartSec=...`.
    pub timeout_start_sec: Option<u32>,
    /// Optional `ExecStart` override (reset + set).
    pub exec_start_override: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ApplyReport {
    /// Whether the file content changed.
    pub changed: bool,
    /// Path written (or existing path when unchanged).
    pub path_written: String,
    /// Whether a daemon reload is required for systemd to pick up the change.
    pub requires_daemon_reload: bool,
    /// Recommended next action for callers.
    pub recommended_action: RecommendedAction,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct RemoveReport {
    /// Whether the file was removed.
    pub changed: bool,
    /// Path removed (or expected path when unchanged).
    pub path_removed: String,
    /// Whether a daemon reload is required for systemd to pick up the change.
    pub requires_daemon_reload: bool,
}
