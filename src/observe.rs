use crate::{Diagnosis, DiagnosisOptions, Error, Result, UnitStatus};

use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;

use zbus::zvariant::OwnedValue;

const UNIT_INTERFACE: &str = "org.freedesktop.systemd1.Unit";

/// Options for observing unit failure events.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ObserveOptions {
    /// Whether to attempt a journald diagnosis snapshot when a failure is observed.
    pub include_diagnosis: bool,

    /// Options used for `Journal::diagnose_unit_failure` when `include_diagnosis=true`.
    pub diagnosis: DiagnosisOptions,
}

impl Default for ObserveOptions {
    fn default() -> Self {
        Self {
            include_diagnosis: true,
            diagnosis: DiagnosisOptions::default(),
        }
    }
}

/// Observe APIs (feature=`observe`).
#[derive(Clone, Debug)]
pub struct Observe {
    inner: Arc<crate::Inner>,
}

impl Observe {
    pub(crate) fn new(inner: Arc<crate::Inner>) -> Self {
        Self { inner }
    }

    /// Watch a unit and yield events when it transitions to `ActiveState=failed`.
    ///
    /// This uses D-Bus signal subscription (`PropertiesChanged`) and does not start/stop units.
    pub async fn watch_unit_failure(
        &self,
        unit: &str,
        opts: ObserveOptions,
    ) -> Result<UnitFailureWatcher> {
        let unit = crate::util::canonicalize_unit_name(unit)?;
        let unit_path = self.inner.bus.get_unit_path(&unit).await?;

        let conn = self.inner.bus.connection();

        let builder = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal);
        let builder = builder
            .sender(crate::bus::SYSTEMD_DESTINATION)
            .map_err(map_match_rule_error)?;
        let builder = builder
            .interface(crate::bus::DBUS_PROPERTIES_INTERFACE)
            .map_err(map_match_rule_error)?;
        let builder = builder
            .member("PropertiesChanged")
            .map_err(map_match_rule_error)?;
        let builder = builder
            .path(unit_path.as_str())
            .map_err(map_match_rule_error)?;
        let builder = builder
            .add_arg(UNIT_INTERFACE)
            .map_err(map_match_rule_error)?;
        let rule = builder.build();

        let stream = zbus::MessageStream::for_match_rule(rule, &conn, Some(16))
            .await
            .map_err(|e| Error::IoError {
                context: format!("observe subscribe failed: {e}"),
            })?;

        Ok(UnitFailureWatcher {
            inner: self.inner.clone(),
            unit,
            opts,
            stream,
        })
    }
}

/// Unit failure event observed via D-Bus.
#[derive(Debug)]
#[non_exhaustive]
pub struct UnitFailedEvent {
    pub unit: String,
    pub status: UnitStatus,
    pub diagnosis: Option<Diagnosis>,
    pub diagnosis_error: Option<Error>,
}

/// Watcher that yields `UnitFailedEvent` as the unit fails.
///
/// The watcher is driven by calling `next()` in a loop.
#[derive(Debug)]
pub struct UnitFailureWatcher {
    inner: Arc<crate::Inner>,
    unit: String,
    opts: ObserveOptions,
    stream: zbus::MessageStream,
}

impl UnitFailureWatcher {
    pub fn unit(&self) -> &str {
        &self.unit
    }

    pub async fn next(&mut self) -> Result<Option<UnitFailedEvent>> {
        loop {
            let Some(msg) = self.stream.next().await else {
                return Ok(None);
            };
            let msg = msg.map_err(|e| Error::IoError {
                context: format!("observe stream error: {e}"),
            })?;

            let failed = properties_changed_is_failed(msg)?;
            if !failed {
                continue;
            }

            let status = crate::units::Units::new(self.inner.clone())
                .get_status(&self.unit)
                .await?;

            let mut diagnosis = None;
            let mut diagnosis_error = None;
            if self.opts.include_diagnosis {
                match crate::journal::Journal::new(self.inner.clone())
                    .diagnose_unit_failure(&self.unit, self.opts.diagnosis.clone())
                    .await
                {
                    Ok(d) => diagnosis = Some(d),
                    Err(e) => diagnosis_error = Some(e),
                }
            }

            return Ok(Some(UnitFailedEvent {
                unit: self.unit.clone(),
                status,
                diagnosis,
                diagnosis_error,
            }));
        }
    }
}

fn properties_changed_is_failed(msg: zbus::Message) -> Result<bool> {
    let body = msg.body();
    let decoded: std::result::Result<(String, HashMap<String, OwnedValue>, Vec<String>), _> =
        body.deserialize();

    let (iface, changed, _invalidated) = decoded.map_err(|e| Error::DbusError {
        name: "SignalDecode".to_string(),
        message: e.to_string(),
    })?;

    if iface != UNIT_INTERFACE {
        return Ok(false);
    }

    let Some(v) = changed.get("ActiveState") else {
        return Ok(false);
    };
    let s = match <&str>::try_from(v) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };

    Ok(s == "failed")
}

fn map_match_rule_error(e: zbus::Error) -> Error {
    Error::IoError {
        context: format!("observe match rule error: {e}"),
    }
}
