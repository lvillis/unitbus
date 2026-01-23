use crate::{Error, ManagerInfo, Properties, Result, UnitListEntry, util};

use std::sync::Arc;

/// systemd `Manager` APIs.
#[derive(Clone, Debug)]
pub struct Manager {
    inner: Arc<crate::Inner>,
}

impl Manager {
    pub(crate) fn new(inner: Arc<crate::Inner>) -> Self {
        Self { inner }
    }

    /// List all units currently known to systemd.
    pub async fn list_units(&self) -> Result<Vec<UnitListEntry>> {
        let items = self.inner.bus.list_units().await?;
        Ok(items.into_iter().map(UnitListEntry::from_dbus).collect())
    }

    /// List units filtered by one or more states (e.g. `"active"`, `"failed"`, `"loaded"`).
    ///
    /// This uses `Manager.ListUnitsFiltered` when available, and falls back to `ListUnits` with
    /// in-process filtering on older systemd versions.
    pub async fn list_units_filtered(&self, states: &[&str]) -> Result<Vec<UnitListEntry>> {
        if states.is_empty() {
            return Err(Error::invalid_input("states must not be empty"));
        }
        for s in states {
            util::validate_no_control("unit state filter", s)?;
        }

        match self.inner.bus.list_units_filtered(states).await {
            Ok(items) => Ok(items.into_iter().map(UnitListEntry::from_dbus).collect()),
            Err(Error::DbusError { name, .. })
                if name.contains("UnknownMethod")
                    || name.contains("UnknownMember")
                    || name.contains("UnknownInterface") =>
            {
                let all = self.list_units().await?;
                Ok(all
                    .into_iter()
                    .filter(|u| {
                        states
                            .iter()
                            .any(|s| u.load_state.as_str() == *s || u.active_state.as_str() == *s)
                    })
                    .collect())
            }
            Err(e) => Err(e),
        }
    }

    /// Fetch a snapshot of manager/global properties.
    pub async fn properties(&self) -> Result<Properties> {
        let props = self
            .inner
            .bus
            .get_all_properties(
                crate::bus::SYSTEMD_MANAGER_PATH,
                crate::bus::SYSTEMD_MANAGER_INTERFACE,
            )
            .await?;
        Ok(Properties::from_dbus(props))
    }

    /// Fetch common manager/global information.
    pub async fn info(&self) -> Result<ManagerInfo> {
        let props = self.properties().await?;
        Ok(ManagerInfo {
            system_state: props.get_opt_string("SystemState"),
            version: props.get_opt_string("Version"),
            virtualization: props.get_opt_string("Virtualization"),
        })
    }
}
