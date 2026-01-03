use crate::{
    Capabilities, Diagnosis, DiagnosisOptions, JobHandle, JobOutcome, Journal, JournalFilter,
    JournalResult, Result, UnitBus, UnitBusOptions, UnitStartMode, UnitStatus, Units,
};

use std::time::Duration;

/// Blocking wrapper for `UnitBus` (feature=`blocking`).
///
/// This is a convenience API for environments where a synchronous interface is preferred.
/// Internally it uses the selected runtime (`rt-async-io` or `rt-tokio`) to drive the async
/// implementation.
#[derive(Clone, Debug)]
pub struct BlockingUnitBus {
    inner: UnitBus,
}

impl BlockingUnitBus {
    /// Connect to the system D-Bus (blocking).
    pub fn connect_system() -> Result<Self> {
        let inner = crate::runtime::block_on_result(UnitBus::connect_system())?;
        Ok(Self { inner })
    }

    /// Connect to the system D-Bus with custom options (blocking).
    pub fn connect_system_with(opts: UnitBusOptions) -> Result<Self> {
        let inner = crate::runtime::block_on_result(UnitBus::connect_system_with(opts))?;
        Ok(Self { inner })
    }

    /// Probe environment capabilities conservatively (blocking).
    pub fn capabilities(&self) -> Result<Capabilities> {
        crate::runtime::block_on_result(async { Ok(self.inner.capabilities().await) })
    }

    /// Access unit/job control APIs (blocking wrappers).
    pub fn units(&self) -> BlockingUnits {
        BlockingUnits {
            inner: self.inner.units(),
        }
    }

    /// Access journald APIs (blocking wrappers).
    pub fn journal(&self) -> BlockingJournal {
        BlockingJournal {
            inner: self.inner.journal(),
        }
    }

    /// Access transient task APIs (blocking wrappers).
    #[cfg(feature = "tasks")]
    pub fn tasks(&self) -> BlockingTasks {
        BlockingTasks {
            inner: self.inner.tasks(),
        }
    }

    /// Access drop-in config APIs (blocking wrappers).
    #[cfg(feature = "config")]
    pub fn config(&self) -> BlockingConfig {
        BlockingConfig {
            inner: self.inner.config(),
        }
    }
}

/// Blocking wrapper for `Units`.
#[derive(Clone, Debug)]
pub struct BlockingUnits {
    inner: Units,
}

impl BlockingUnits {
    pub fn get_status(&self, unit: &str) -> Result<UnitStatus> {
        crate::runtime::block_on_result(self.inner.get_status(unit))
    }

    pub fn start(&self, unit: &str, mode: UnitStartMode) -> Result<BlockingJobHandle> {
        let job = crate::runtime::block_on_result(self.inner.start(unit, mode))?;
        Ok(BlockingJobHandle { inner: job })
    }

    pub fn stop(&self, unit: &str, mode: UnitStartMode) -> Result<BlockingJobHandle> {
        let job = crate::runtime::block_on_result(self.inner.stop(unit, mode))?;
        Ok(BlockingJobHandle { inner: job })
    }

    pub fn restart(&self, unit: &str, mode: UnitStartMode) -> Result<BlockingJobHandle> {
        let job = crate::runtime::block_on_result(self.inner.restart(unit, mode))?;
        Ok(BlockingJobHandle { inner: job })
    }

    pub fn reload(&self, unit: &str, mode: UnitStartMode) -> Result<BlockingJobHandle> {
        let job = crate::runtime::block_on_result(self.inner.reload(unit, mode))?;
        Ok(BlockingJobHandle { inner: job })
    }
}

/// Blocking wrapper for `JobHandle`.
#[derive(Clone, Debug)]
pub struct BlockingJobHandle {
    inner: JobHandle,
}

impl BlockingJobHandle {
    pub fn unit(&self) -> &str {
        &self.inner.unit
    }

    pub fn job_path(&self) -> &str {
        &self.inner.job_path
    }

    pub fn wait(&self, timeout: Duration) -> Result<JobOutcome> {
        crate::runtime::block_on_result(self.inner.wait(timeout))
    }
}

/// Blocking wrapper for `Journal`.
#[derive(Clone, Debug)]
pub struct BlockingJournal {
    inner: Journal,
}

impl BlockingJournal {
    pub fn query(&self, filter: JournalFilter) -> Result<JournalResult> {
        crate::runtime::block_on_result(self.inner.query(filter))
    }

    pub fn diagnose_unit_failure(&self, unit: &str, opts: DiagnosisOptions) -> Result<Diagnosis> {
        crate::runtime::block_on_result(self.inner.diagnose_unit_failure(unit, opts))
    }
}

/// Blocking wrapper for `Tasks` (feature=`tasks`).
#[cfg(feature = "tasks")]
#[derive(Clone, Debug)]
pub struct BlockingTasks {
    inner: crate::Tasks,
}

#[cfg(feature = "tasks")]
impl BlockingTasks {
    pub fn run(&self, spec: crate::TaskSpec) -> Result<BlockingTaskHandle> {
        let handle = crate::runtime::block_on_result(self.inner.run(spec))?;
        Ok(BlockingTaskHandle { inner: handle })
    }
}

/// Blocking wrapper for `TaskHandle` (feature=`tasks`).
#[cfg(feature = "tasks")]
#[derive(Clone, Debug)]
pub struct BlockingTaskHandle {
    inner: crate::TaskHandle,
}

#[cfg(feature = "tasks")]
impl BlockingTaskHandle {
    pub fn unit(&self) -> &str {
        &self.inner.unit
    }

    pub fn job_path(&self) -> &str {
        &self.inner.job_path
    }

    pub fn wait(&self, timeout: Duration) -> Result<crate::TaskResult> {
        crate::runtime::block_on_result(self.inner.wait(timeout))
    }
}

/// Blocking wrapper for `Config` (feature=`config`).
#[cfg(feature = "config")]
#[derive(Clone, Debug)]
pub struct BlockingConfig {
    inner: crate::Config,
}

#[cfg(feature = "config")]
impl BlockingConfig {
    pub fn apply_dropin(&self, spec: crate::DropInSpec) -> Result<crate::ApplyReport> {
        crate::runtime::block_on_result(self.inner.apply_dropin(spec))
    }

    pub fn remove_dropin(&self, unit: &str, name: &str) -> Result<crate::RemoveReport> {
        crate::runtime::block_on_result(self.inner.remove_dropin(unit, name))
    }

    pub fn daemon_reload(&self) -> Result<()> {
        crate::runtime::block_on_result(self.inner.daemon_reload())
    }
}
