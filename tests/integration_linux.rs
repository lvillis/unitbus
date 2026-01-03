#![cfg(target_os = "linux")]

// Linux/systemd integration tests.
//
// These are ignored by default and are intended to be run on a real systemd host:
// - `UNITBUS_ITEST_UNIT`: a safe unit name to restart (e.g. "cron.service" in a test VM)
// - `UNITBUS_ITEST_DROPIN_UNIT`: a unit name to write drop-ins for (requires root/policy)

use std::time::Duration;

use unitbus::{UnitBus, UnitStartMode};

fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => Some(v),
        _ => None,
    }
}

#[cfg(all(
    feature = "tasks",
    any(feature = "journal-cli", feature = "journal-sdjournal")
))]
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
fn restart_and_wait() {
    let unit = match env("UNITBUS_ITEST_UNIT") {
        Some(u) => u,
        None => {
            eprintln!("set UNITBUS_ITEST_UNIT to a safe systemd unit to restart");
            return;
        }
    };

    smol::block_on(async {
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

    smol::block_on(async {
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

    smol::block_on(async {
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

    smol::block_on(async {
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
