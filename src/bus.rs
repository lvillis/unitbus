use crate::{Error, Result, UnitBusOptions};

use std::collections::HashMap;
use std::time::Duration;

use zbus::zvariant::{OwnedObjectPath, OwnedValue};

pub(crate) const SYSTEMD_DESTINATION: &str = "org.freedesktop.systemd1";
pub(crate) const SYSTEMD_MANAGER_PATH: &str = "/org/freedesktop/systemd1";
pub(crate) const SYSTEMD_MANAGER_INTERFACE: &str = "org.freedesktop.systemd1.Manager";

pub(crate) const DBUS_PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";

const SYSTEMD_JOB_INTERFACE: &str = "org.freedesktop.systemd1.Job";

pub(crate) type ListUnitItem = (
    String,
    String,
    String,
    String,
    String,
    String,
    OwnedObjectPath,
    u32,
    String,
    OwnedObjectPath,
);

#[derive(Debug)]
pub(crate) struct Bus {
    conn: zbus::Connection,
    dbus_call_timeout: Duration,
}

impl Bus {
    #[cfg(feature = "observe")]
    pub(crate) fn connection(&self) -> zbus::Connection {
        self.conn.clone()
    }

    pub(crate) async fn connect_system(opts: &UnitBusOptions) -> Result<Self> {
        let dbus_call_timeout = opts.dbus_call_timeout;
        let conn = zbus::connection::Builder::system()
            .map_err(|e| Error::BackendUnavailable {
                backend: "system_bus",
                detail: e.to_string(),
            })?
            .method_timeout(dbus_call_timeout)
            .build()
            .await
            .map_err(|e| Error::BackendUnavailable {
                backend: "system_bus",
                detail: e.to_string(),
            })?;
        Ok(Self {
            conn,
            dbus_call_timeout,
        })
    }

    pub(crate) async fn manager_proxy(&self) -> Result<zbus::Proxy<'_>> {
        zbus::Proxy::new(
            &self.conn,
            SYSTEMD_DESTINATION,
            SYSTEMD_MANAGER_PATH,
            SYSTEMD_MANAGER_INTERFACE,
        )
        .await
        .map_err(map_zbus_error)
    }

    pub(crate) async fn get_unit_path(&self, unit: &str) -> Result<OwnedObjectPath> {
        let proxy = self.manager_proxy().await?;
        proxy
            .call("GetUnit", &(unit))
            .await
            .map_err(|e| map_zbus_method_error("get_unit", self.dbus_call_timeout, e, Some(unit)))
    }

    pub(crate) async fn start_unit(&self, unit: &str, mode: &str) -> Result<OwnedObjectPath> {
        let proxy = self.manager_proxy().await?;
        proxy
            .call("StartUnit", &(unit, mode))
            .await
            .map_err(|e| map_zbus_method_error("start_unit", self.dbus_call_timeout, e, Some(unit)))
    }

    pub(crate) async fn stop_unit(&self, unit: &str, mode: &str) -> Result<OwnedObjectPath> {
        let proxy = self.manager_proxy().await?;
        proxy
            .call("StopUnit", &(unit, mode))
            .await
            .map_err(|e| map_zbus_method_error("stop_unit", self.dbus_call_timeout, e, Some(unit)))
    }

    pub(crate) async fn restart_unit(&self, unit: &str, mode: &str) -> Result<OwnedObjectPath> {
        let proxy = self.manager_proxy().await?;
        proxy.call("RestartUnit", &(unit, mode)).await.map_err(|e| {
            map_zbus_method_error("restart_unit", self.dbus_call_timeout, e, Some(unit))
        })
    }

    pub(crate) async fn reload_unit(&self, unit: &str, mode: &str) -> Result<OwnedObjectPath> {
        let proxy = self.manager_proxy().await?;
        proxy.call("ReloadUnit", &(unit, mode)).await.map_err(|e| {
            map_zbus_method_error("reload_unit", self.dbus_call_timeout, e, Some(unit))
        })
    }

    pub(crate) async fn list_units(&self) -> Result<Vec<ListUnitItem>> {
        let proxy = self.manager_proxy().await?;
        proxy
            .call("ListUnits", &())
            .await
            .map_err(|e| map_zbus_method_error("list_units", self.dbus_call_timeout, e, None))
    }

    pub(crate) async fn list_units_filtered(&self, states: &[&str]) -> Result<Vec<ListUnitItem>> {
        let proxy = self.manager_proxy().await?;
        proxy
            .call("ListUnitsFiltered", &(states))
            .await
            .map_err(|e| {
                map_zbus_method_error("list_units_filtered", self.dbus_call_timeout, e, None)
            })
    }

    #[cfg(feature = "tasks")]
    pub(crate) async fn start_transient_unit(
        &self,
        name: &str,
        mode: &str,
        properties: Vec<(String, OwnedValue)>,
    ) -> Result<OwnedObjectPath> {
        let proxy = self.manager_proxy().await?;
        let aux: Vec<(String, Vec<(String, OwnedValue)>)> = Vec::new();
        proxy
            .call("StartTransientUnit", &(name, mode, properties, aux))
            .await
            .map_err(|e| map_zbus_method_error("run_task", self.dbus_call_timeout, e, Some(name)))
    }

    #[cfg(feature = "config")]
    pub(crate) async fn daemon_reload(&self) -> Result<()> {
        let proxy = self.manager_proxy().await?;
        proxy
            .call::<_, _, ()>("Reload", &())
            .await
            .map_err(|e| map_zbus_method_error("daemon_reload", self.dbus_call_timeout, e, None))
    }

    pub(crate) async fn get_all_properties(
        &self,
        object_path: &str,
        interface: &str,
    ) -> Result<HashMap<String, OwnedValue>> {
        let proxy = zbus::Proxy::new(
            &self.conn,
            SYSTEMD_DESTINATION,
            object_path,
            DBUS_PROPERTIES_INTERFACE,
        )
        .await
        .map_err(map_zbus_error)?;

        proxy.call("GetAll", &(interface)).await.map_err(|e| {
            map_zbus_method_error("get_all_properties", self.dbus_call_timeout, e, None)
        })
    }

    pub(crate) async fn job_exists(&self, job_path: &str) -> Result<bool> {
        match self
            .get_all_properties(job_path, SYSTEMD_JOB_INTERFACE)
            .await
        {
            Ok(_) => Ok(true),
            Err(Error::DbusError { name, .. }) if name.contains("UnknownObject") => Ok(false),
            Err(e) => Err(e),
        }
    }
}

fn map_zbus_method_error(
    action: &'static str,
    timeout: Duration,
    err: zbus::Error,
    unit: Option<&str>,
) -> Error {
    match &err {
        zbus::Error::MethodError(name, detail, _reply) => {
            let name = name.to_string();
            let message = detail.clone().unwrap_or_default();

            if (name.contains("NoSuchUnit") || name.contains("UnknownUnit"))
                && let Some(unit) = unit
            {
                return Error::UnitNotFound {
                    unit: unit.to_string(),
                };
            }

            if name.contains("AccessDenied")
                || name.contains("PermissionDenied")
                || name.contains("PolicyKit")
            {
                return Error::PermissionDenied {
                    action,
                    detail: format!("{name}: {message}"),
                };
            }

            Error::DbusError { name, message }
        }
        zbus::Error::InputOutput(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            Error::Timeout { action, timeout }
        }
        _ => map_zbus_error(err),
    }
}

fn map_zbus_error(err: zbus::Error) -> Error {
    match err {
        zbus::Error::MethodError(name, detail, _reply) => Error::DbusError {
            name: name.to_string(),
            message: detail.unwrap_or_default(),
        },
        zbus::Error::InputOutput(e) => Error::IoError {
            context: format!("dbus io error: {e}"),
        },
        other => Error::IoError {
            context: format!("dbus error: {other}"),
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::sync::Arc;

    fn dummy_msg() -> zbus::Message {
        zbus::Message::method_call("/org/freedesktop/systemd1", "Dummy")
            .expect("builder")
            .build(&())
            .expect("msg")
    }

    #[test]
    fn maps_no_such_unit_to_unit_not_found() {
        let name = zbus::names::OwnedErrorName::try_from("org.freedesktop.systemd1.NoSuchUnit")
            .expect("name");
        let err = zbus::Error::MethodError(name, Some("missing".to_string()), dummy_msg());

        let mapped = map_zbus_method_error(
            "start_unit",
            Duration::from_secs(5),
            err,
            Some("nginx.service"),
        );

        let Error::UnitNotFound { unit } = mapped else {
            panic!("unexpected error: {mapped:?}");
        };
        assert_eq!(unit, "nginx.service");
    }

    #[test]
    fn maps_access_denied_to_permission_denied() {
        let name = zbus::names::OwnedErrorName::try_from("org.freedesktop.DBus.Error.AccessDenied")
            .expect("name");
        let err = zbus::Error::MethodError(name, Some("no".to_string()), dummy_msg());

        let mapped = map_zbus_method_error(
            "stop_unit",
            Duration::from_secs(5),
            err,
            Some("dbus.service"),
        );

        let Error::PermissionDenied { action, .. } = mapped else {
            panic!("unexpected error: {mapped:?}");
        };
        assert_eq!(action, "stop_unit");
    }

    #[test]
    fn maps_io_timeout_to_timeout_variant() {
        let err = zbus::Error::InputOutput(Arc::new(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "timeout",
        )));

        let mapped = map_zbus_method_error("get_unit", Duration::from_secs(7), err, None);

        let Error::Timeout { action, timeout } = mapped else {
            panic!("unexpected error: {mapped:?}");
        };
        assert_eq!(action, "get_unit");
        assert_eq!(timeout, Duration::from_secs(7));
    }
}
