//! unitbus is a Rust SDK for Linux systemd: control units/jobs via the system D-Bus (systemctl-like),
//! run one-shot transient tasks, and query journald logs (default: pure Rust backend).
//!
//! It is designed as a control-plane foundation for traditional deployments (non-Kubernetes) and
//! CD/agent tooling.
//!
//! Runtime is Linux-only (systemd + system bus required). The public API is designed to compile on
//! other platforms but will return `Error::BackendUnavailable` where appropriate.
//!
//! ## Quick start
//! ```no_run
//! use unitbus::{UnitBus, UnitStartMode};
//!
//! async fn restart_nginx() -> Result<(), unitbus::Error> {
//!     let bus = UnitBus::connect_system().await?;
//!     let job = bus.units().restart("nginx", UnitStartMode::Replace).await?;
//!     let outcome = job.wait(std::time::Duration::from_secs(30)).await?;
//!     println!("{outcome:?}");
//!     Ok(())
//! }
//! ```
//!
//! ## Unit name rules
//! - You can pass either a full unit name (e.g. `"nginx.service"`) or a shorthand (e.g. `"nginx"`).
//! - Shorthand names are canonicalized to `"<name>.service"`.
//! - Names containing path separators or control characters are rejected as `Error::InvalidInput`.
//!
//! ## Journald limits
//! The journald backend enforces bounded results:
//! - `limit` (default: 200)
//! - `max_bytes` (default: 1 MiB)
//! - `max_message_bytes` (default: 16 KiB)
//!
//! Default backend: pure Rust journal reader (feature=`journal-sdjournal`).
//! Alternative backend: `journalctl --output=json` (feature=`journal-cli`).
//!
//! When limits are exceeded, the returned `JournalResult.truncated` is set to `true`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]
#![deny(clippy::dbg_macro)]

#[cfg(all(feature = "rt-async-io", feature = "rt-tokio"))]
compile_error!("features `rt-async-io` and `rt-tokio` are mutually exclusive; enable exactly one.");

#[cfg(not(any(feature = "rt-async-io", feature = "rt-tokio")))]
compile_error!(
    "missing runtime feature: enable one of `rt-async-io` or `rt-tokio` (default enables `rt-async-io`)."
);

#[cfg(feature = "blocking")]
mod blocking_api;
mod bus;
mod capabilities;
mod error;
#[cfg(feature = "config")]
mod fsutil;
mod journal;
#[cfg(feature = "observe")]
mod observe;
mod options;
mod runtime;
mod types;
mod units;
mod util;

#[cfg(feature = "config")]
pub use crate::types::config::{ApplyReport, DropInSpec, RecommendedAction, RemoveReport};
pub use crate::types::journal::{
    Diagnosis, DiagnosisOptions, JournalCursor, JournalEntry, JournalFilter, JournalResult,
    JournalStats, ParseErrorMode,
};
#[cfg(feature = "tasks")]
pub use crate::types::task::{TaskHandle, TaskResult, TaskSpec};
pub use crate::types::unit::{
    ActiveState, FailureHint, JobHandle, JobOutcome, LoadState, UnitStartMode, UnitStatus,
};

pub use crate::capabilities::Capabilities;
pub use crate::error::{Error, Result};
pub use crate::options::UnitBusOptions;

#[cfg(feature = "blocking")]
pub use crate::blocking_api::{BlockingJobHandle, BlockingJournal, BlockingUnitBus, BlockingUnits};

#[cfg(all(feature = "blocking", feature = "tasks"))]
pub use crate::blocking_api::{BlockingTaskHandle, BlockingTasks};

#[cfg(all(feature = "blocking", feature = "config"))]
pub use crate::blocking_api::BlockingConfig;

pub use crate::journal::Journal;
#[cfg(feature = "observe")]
pub use crate::observe::{Observe, ObserveOptions, UnitFailedEvent, UnitFailureWatcher};
#[cfg(feature = "config")]
pub use crate::units::Config;
#[cfg(feature = "tasks")]
pub use crate::units::Tasks;
pub use crate::units::Units;

use std::sync::Arc;

/// Primary entrypoint for interacting with systemd and journald.
#[derive(Clone, Debug)]
pub struct UnitBus {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    opts: UnitBusOptions,
    bus: bus::Bus,
}

impl UnitBus {
    /// Connect to the system D-Bus.
    pub async fn connect_system() -> Result<Self> {
        Self::connect_system_with(UnitBusOptions::default()).await
    }

    /// Connect to the system D-Bus with custom options (timeouts, polling).
    pub async fn connect_system_with(opts: UnitBusOptions) -> Result<Self> {
        let bus = bus::Bus::connect_system(&opts).await?;
        Ok(Self {
            inner: Arc::new(Inner { opts, bus }),
        })
    }

    /// Probe environment capabilities conservatively.
    pub async fn capabilities(&self) -> Capabilities {
        capabilities::probe(self).await
    }

    /// Access unit/job control APIs.
    pub fn units(&self) -> Units {
        Units::new(self.inner.clone())
    }

    /// Access journald APIs.
    pub fn journal(&self) -> Journal {
        Journal::new(self.inner.clone())
    }

    /// Access observe APIs (feature=`observe`).
    #[cfg(feature = "observe")]
    pub fn observe(&self) -> Observe {
        Observe::new(self.inner.clone())
    }

    /// Access transient task APIs (feature=`tasks`).
    #[cfg(feature = "tasks")]
    pub fn tasks(&self) -> Tasks {
        Tasks::new(self.inner.clone())
    }

    /// Access drop-in config APIs (feature=`config`).
    #[cfg(feature = "config")]
    pub fn config(&self) -> Config {
        Config::new(self.inner.clone())
    }
}
