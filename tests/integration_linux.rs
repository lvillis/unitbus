#![cfg(target_os = "linux")]

// Linux/systemd integration tests.
//
// These are ignored by default and are intended to be run on a real systemd host:
// - `UNITBUS_ITEST_UNIT`: a safe unit name to restart (e.g. "cron.service" in a test VM)
// - `UNITBUS_ITEST_DROPIN_UNIT`: a unit name to write drop-ins for (requires root/policy)

use std::future::Future;
use std::time::Duration;

use unitbus::{UnitBus, UnitStartMode};

fn block_on<T>(fut: impl Future<Output = T>) -> T {
    #[cfg(feature = "rt-async-io")]
    {
        smol::block_on(fut)
    }

    #[cfg(feature = "rt-tokio")]
    {
        let rt = tokio::runtime::Runtime::new().expect("init tokio runtime");
        rt.block_on(fut)
    }
}

fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => Some(v),
        _ => None,
    }
}

fn find_executable(candidates: &[&str]) -> Option<String> {
    for c in candidates {
        if std::path::Path::new(c).exists() {
            return Some((*c).to_string());
        }
    }
    None
}

#[test]
#[ignore]
fn manager_list_units_and_info_read_only() {
    block_on(async {
        let bus = UnitBus::connect_system().await?;

        let units = match bus.manager().list_units().await {
            Ok(v) => v,
            Err(unitbus::Error::PermissionDenied { .. }) => {
                eprintln!("permission denied; skipping manager list_units");
                return Ok(());
            }
            Err(unitbus::Error::BackendUnavailable { .. }) => {
                eprintln!("system bus/systemd unavailable; skipping manager list_units");
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        assert!(!units.is_empty(), "expected at least one unit");
        for u in units.iter().take(50) {
            assert!(!u.name.trim().is_empty(), "unit name must not be empty");
            assert!(
                u.unit_path.starts_with("/org/freedesktop/systemd1/unit/"),
                "unexpected unit path: {}",
                u.unit_path
            );

            if u.job_id.is_some() {
                assert!(
                    u.job_path.is_some(),
                    "job_path should exist when job_id exists"
                );
            }
        }

        let info = match bus.manager().info().await {
            Ok(i) => i,
            Err(unitbus::Error::PermissionDenied { .. }) => {
                eprintln!("permission denied; skipping manager info");
                return Ok(());
            }
            Err(unitbus::Error::BackendUnavailable { .. }) => {
                eprintln!("system bus/systemd unavailable; skipping manager info");
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        assert!(
            info.system_state.is_some() || info.version.is_some() || info.virtualization.is_some(),
            "expected at least one manager info field"
        );

        Ok::<(), unitbus::Error>(())
    })
    .unwrap();
}

#[test]
#[ignore]
fn manager_list_units_filtered_active_only_contains_active() {
    block_on(async {
        let bus = UnitBus::connect_system().await?;

        let units = match bus.manager().list_units_filtered(&["active"]).await {
            Ok(v) => v,
            Err(unitbus::Error::PermissionDenied { .. }) => {
                eprintln!("permission denied; skipping list_units_filtered");
                return Ok(());
            }
            Err(unitbus::Error::BackendUnavailable { .. }) => {
                eprintln!("system bus/systemd unavailable; skipping list_units_filtered");
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        for u in units {
            assert_eq!(
                u.active_state.as_str(),
                "active",
                "expected only active units"
            );
        }

        Ok::<(), unitbus::Error>(())
    })
    .unwrap();
}

#[test]
#[ignore]
fn can_read_unit_properties_by_path_read_only() {
    block_on(async {
        let bus = UnitBus::connect_system().await?;

        let units = match bus.manager().list_units().await {
            Ok(v) => v,
            Err(unitbus::Error::PermissionDenied { .. }) => {
                eprintln!("permission denied; skipping");
                return Ok(());
            }
            Err(unitbus::Error::BackendUnavailable { .. }) => {
                eprintln!("system bus/systemd unavailable; skipping");
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        let Some(u) = units.first() else {
            return Ok(());
        };

        let props = bus
            .units()
            .get_unit_properties_by_path(&u.unit_path)
            .await?;
        let id = props.get_opt_str("Id").unwrap_or("");
        assert!(!id.is_empty(), "expected Unit.Id to be present");

        // Only a best-effort check: non-service units return None.
        let _service_props = bus
            .units()
            .get_service_properties_by_path(&u.unit_path)
            .await?;

        Ok::<(), unitbus::Error>(())
    })
    .unwrap();
}

#[test]
#[ignore]
fn restart_and_wait() {
    let unit = match env("UNITBUS_ITEST_UNIT") {
        Some(u) => u,
        None => {
            eprintln!("set UNITBUS_ITEST_UNIT to a safe systemd unit to restart");
            return;
        }
    };

    block_on(async {
        let bus = UnitBus::connect_system().await?;
        let job = bus.units().restart(&unit, UnitStartMode::Replace).await?;
        let outcome = job.wait(Duration::from_secs(30)).await?;
        eprintln!("outcome={outcome:?}");
        Ok::<(), unitbus::Error>(())
    })
    .unwrap();
}

#[cfg(all(
    feature = "tasks",
    any(feature = "journal-cli", feature = "journal-sdjournal")
))]
#[test]
#[ignore]
fn run_task_echo_and_fetch_logs() {
    let echo = match find_executable(&["/bin/echo", "/usr/bin/echo"]) {
        Some(p) => p,
        None => {
            eprintln!("cannot find /bin/echo or /usr/bin/echo; skipping");
            return;
        }
    };

    block_on(async {
        let bus = UnitBus::connect_system().await?;

        let mut spec = unitbus::TaskSpec::default();
        spec.argv = vec![echo, "hello".to_string()];
        spec.timeout = Duration::from_secs(10);
        spec.name_hint = Some("itest".to_string());

        let task = bus.tasks().run(spec).await?;
        let res = task.wait(Duration::from_secs(30)).await?;

        assert_eq!(
            res.exit_status,
            Some(0),
            "expected successful exit; status={:?}",
            res.unit_status
        );

        let mut filter = unitbus::JournalFilter::default();
        filter.unit = Some(task.unit.clone());
        filter.limit = 50;
        filter.timeout = Some(Duration::from_secs(5));

        let logs = match bus.journal().query(filter).await {
            Ok(r) => r.entries,
            Err(unitbus::Error::PermissionDenied { .. }) => {
                eprintln!("journal permission denied; skipping log assertion");
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        let mut saw = false;
        for e in logs {
            if let Some(m) = e.message.as_deref()
                && m.contains("hello")
            {
                saw = true;
                break;
            }
        }
        assert!(saw, "expected at least one log line containing 'hello'");

        Ok::<(), unitbus::Error>(())
    })
    .unwrap();
}

#[cfg(all(
    feature = "tasks",
    any(feature = "journal-cli", feature = "journal-sdjournal")
))]
#[test]
#[ignore]
fn run_task_failure_can_diagnose() {
    let false_bin = match find_executable(&["/bin/false", "/usr/bin/false"]) {
        Some(p) => p,
        None => {
            eprintln!("cannot find /bin/false or /usr/bin/false; skipping");
            return;
        }
    };

    block_on(async {
        let bus = UnitBus::connect_system().await?;

        let mut spec = unitbus::TaskSpec::default();
        spec.argv = vec![false_bin];
        spec.timeout = Duration::from_secs(10);
        spec.name_hint = Some("itest-fail".to_string());

        let task = bus.tasks().run(spec).await?;
        let res = task.wait(Duration::from_secs(30)).await?;

        assert_ne!(
            res.exit_status,
            Some(0),
            "expected non-zero exit; status={:?}",
            res.unit_status
        );

        let diag = match bus
            .journal()
            .diagnose_unit_failure(&task.unit, Default::default())
            .await
        {
            Ok(d) => d,
            Err(unitbus::Error::PermissionDenied { .. }) => {
                eprintln!("journal permission denied; skipping diagnose assertion");
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        assert_eq!(diag.status.id, task.unit);
        assert!(diag.logs.len() <= 200);
        Ok::<(), unitbus::Error>(())
    })
    .unwrap();
}

#[cfg(feature = "config")]
#[test]
#[ignore]
fn dropin_apply_remove_idempotent() {
    let unit = match env("UNITBUS_ITEST_DROPIN_UNIT") {
        Some(u) => u,
        None => {
            eprintln!("set UNITBUS_ITEST_DROPIN_UNIT to a unit to write drop-ins for");
            return;
        }
    };

    block_on(async {
        let bus = UnitBus::connect_system().await?;

        let mut spec = unitbus::DropInSpec::default();
        spec.unit = unit.clone();
        spec.name = "unitbus-itest".to_string();
        spec.environment
            .insert("UNITBUS_ITEST".to_string(), "1".to_string());

        let r1 = match bus.config().apply_dropin(spec.clone()).await {
            Ok(r) => r,
            Err(unitbus::Error::PermissionDenied { .. }) => {
                eprintln!("drop-in write permission denied; skipping");
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        assert!(r1.requires_daemon_reload);

        let r2 = bus.config().apply_dropin(spec).await?;
        assert!(!r2.changed, "expected idempotent apply");

        let rm1 = bus.config().remove_dropin(&unit, "unitbus-itest").await?;
        assert!(rm1.requires_daemon_reload);

        let rm2 = bus.config().remove_dropin(&unit, "unitbus-itest").await?;
        assert!(!rm2.changed, "expected idempotent remove");

        Ok::<(), unitbus::Error>(())
    })
    .unwrap();
}

#[cfg(feature = "config")]
#[test]
#[ignore]
fn install_uninstall_service_unit_file() {
    let unit = match env("UNITBUS_ITEST_UNITFILE_UNIT") {
        Some(u) => u,
        None => {
            eprintln!(
                "set UNITBUS_ITEST_UNITFILE_UNIT to a unique service name (e.g. unitbus-itest.service)"
            );
            return;
        }
    };

    let exe = match find_executable(&["/bin/sleep", "/usr/bin/sleep", "/bin/true", "/usr/bin/true"])
    {
        Some(p) => p,
        None => {
            eprintln!("cannot find /bin/sleep or /bin/true; skipping");
            return;
        }
    };

    block_on(async {
        let bus = UnitBus::connect_system().await?;

        let argv = if exe.ends_with("/sleep") {
            vec![exe, "1".to_string()]
        } else {
            vec![exe]
        };

        let mut spec = unitbus::ServiceUnitSpec::default();
        spec.unit = unit.clone();
        spec.description = Some("unitbus integration test service unit".to_string());
        spec.service_type = Some(unitbus::ServiceType::Oneshot);
        spec.exec_start = argv;
        spec.wanted_by = vec!["multi-user.target".to_string()];

        let unit_name = spec.canonical_unit_name()?;

        let install = match bus
            .config()
            .install_service_unit(spec, Default::default())
            .await
        {
            Ok(r) => r,
            Err(unitbus::Error::PermissionDenied { .. }) => {
                eprintln!("permission denied; skipping install_service_unit");
                return Ok(());
            }
            Err(e) => {
                let _ = bus.config().remove_unit_file(&unit_name).await;
                let _ = bus.config().daemon_reload().await;
                return Err(e);
            }
        };

        let uninstall = match bus
            .config()
            .uninstall_unit(&install.unit, Default::default())
            .await
        {
            Ok(r) => r,
            Err(unitbus::Error::PermissionDenied { .. }) => {
                eprintln!(
                    "permission denied; skipping uninstall cleanup (manual cleanup may be required)"
                );
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        assert!(
            uninstall.removed.path_removed.contains(&install.unit),
            "expected removed file path to mention unit name"
        );

        Ok::<(), unitbus::Error>(())
    })
    .unwrap();
}
