use std::fmt;

/// systemd `StartUnit`/`StopUnit` mode.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum UnitStartMode {
    #[default]
    Replace,
    Fail,
    Isolate,
    IgnoreDependencies,
    IgnoreRequirements,
    Other(String),
}

impl UnitStartMode {
    pub(crate) fn as_dbus_str(&self) -> &str {
        match self {
            UnitStartMode::Replace => "replace",
            UnitStartMode::Fail => "fail",
            UnitStartMode::Isolate => "isolate",
            UnitStartMode::IgnoreDependencies => "ignore-dependencies",
            UnitStartMode::IgnoreRequirements => "ignore-requirements",
            UnitStartMode::Other(s) => s.as_str(),
        }
    }
}

/// systemd `Unit.LoadState`.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum LoadState {
    Loaded,
    NotFound,
    Error,
    Masked,
    Stub,
    Merged,
    Generated,
    Transient,
    BadSetting,
    Unknown(String),
}

impl LoadState {
    pub(crate) fn parse(s: &str) -> Self {
        match s {
            "loaded" => LoadState::Loaded,
            "not-found" => LoadState::NotFound,
            "error" => LoadState::Error,
            "masked" => LoadState::Masked,
            "stub" => LoadState::Stub,
            "merged" => LoadState::Merged,
            "generated" => LoadState::Generated,
            "transient" => LoadState::Transient,
            "bad-setting" => LoadState::BadSetting,
            other => LoadState::Unknown(other.to_string()),
        }
    }
}

/// systemd `Unit.ActiveState`.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ActiveState {
    Active,
    Reloading,
    Inactive,
    Failed,
    Activating,
    Deactivating,
    Maintenance,
    Unknown(String),
}

impl ActiveState {
    pub(crate) fn parse(s: &str) -> Self {
        match s {
            "active" => ActiveState::Active,
            "reloading" => ActiveState::Reloading,
            "inactive" => ActiveState::Inactive,
            "failed" => ActiveState::Failed,
            "activating" => ActiveState::Activating,
            "deactivating" => ActiveState::Deactivating,
            "maintenance" => ActiveState::Maintenance,
            other => ActiveState::Unknown(other.to_string()),
        }
    }
}

/// Snapshot of relevant systemd unit/service properties.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct UnitStatus {
    pub id: String,
    pub description: Option<String>,
    pub load_state: LoadState,
    pub active_state: ActiveState,
    pub sub_state: Option<String>,
    pub result: Option<String>,
    pub fragment_path: Option<String>,
    pub main_pid: Option<u32>,
    pub exec_main_code: Option<i32>,
    pub exec_main_status: Option<i32>,
    pub n_restarts: Option<u32>,
}

/// Handle for a systemd job.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct JobHandle {
    pub unit: String,
    pub job_path: String,

    #[doc(hidden)]
    pub(crate) inner: crate::units::JobInner,
}

impl fmt::Display for JobHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} @ {}", self.unit, self.job_path)
    }
}

/// A best-effort classification of why a job failed.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FailureHint {
    NotLoaded {
        load_state: LoadState,
    },
    ExecMainFailed {
        exec_main_code: i32,
        exec_main_status: i32,
    },
    UnitFailed {
        result: Option<String>,
    },
    JobFailed {
        result: String,
    },
    UnexpectedState {
        active_state: ActiveState,
        sub_state: Option<String>,
    },
    Unknown,
}

/// Normalized outcome for a job wait.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum JobOutcome {
    Success {
        unit_status: UnitStatus,
    },
    Failed {
        unit_status: UnitStatus,
        reason: FailureHint,
    },
    Canceled {
        unit_status: UnitStatus,
    },
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn unit_start_mode_maps_to_dbus_string() {
        assert_eq!(UnitStartMode::Replace.as_dbus_str(), "replace");
        assert_eq!(UnitStartMode::Fail.as_dbus_str(), "fail");
        assert_eq!(UnitStartMode::Isolate.as_dbus_str(), "isolate");
        assert_eq!(
            UnitStartMode::IgnoreDependencies.as_dbus_str(),
            "ignore-dependencies"
        );
        assert_eq!(
            UnitStartMode::IgnoreRequirements.as_dbus_str(),
            "ignore-requirements"
        );
        assert_eq!(
            UnitStartMode::Other("custom".to_string()).as_dbus_str(),
            "custom"
        );
    }

    #[test]
    fn load_state_parses_known_and_unknown_values() {
        assert_eq!(LoadState::parse("loaded"), LoadState::Loaded);
        assert_eq!(LoadState::parse("not-found"), LoadState::NotFound);
        assert_eq!(
            LoadState::parse("wat"),
            LoadState::Unknown("wat".to_string())
        );
    }

    #[test]
    fn active_state_parses_known_and_unknown_values() {
        assert_eq!(ActiveState::parse("active"), ActiveState::Active);
        assert_eq!(ActiveState::parse("failed"), ActiveState::Failed);
        assert_eq!(
            ActiveState::parse("wat"),
            ActiveState::Unknown("wat".to_string())
        );
    }
}
