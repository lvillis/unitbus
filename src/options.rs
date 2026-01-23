use std::time::Duration;

/// Configuration options for `UnitBus`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct UnitBusOptions {
    /// D-Bus method call timeout.
    pub dbus_call_timeout: Duration,

    /// Default timeout for journald queries when not specified in the filter.
    pub journal_default_timeout: Duration,

    /// Initial polling interval for job wait fallback.
    pub job_poll_initial: Duration,

    /// Maximum polling interval for job wait fallback.
    pub job_poll_max: Duration,

    /// Base directory for systemd unit files and drop-ins (feature=`config`).
    ///
    /// Default: `/etc/systemd/system`.
    pub systemd_system_dir: String,
}

impl Default for UnitBusOptions {
    fn default() -> Self {
        Self {
            dbus_call_timeout: Duration::from_secs(5),
            journal_default_timeout: Duration::from_secs(10),
            job_poll_initial: Duration::from_millis(200),
            job_poll_max: Duration::from_secs(2),
            systemd_system_dir: "/etc/systemd/system".to_string(),
        }
    }
}
