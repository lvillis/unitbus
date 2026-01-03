# unitbus

[![crates.io](https://img.shields.io/crates/v/unitbus.svg)](https://crates.io/crates/unitbus)
[![docs.rs](https://docs.rs/unitbus/badge.svg)](https://docs.rs/unitbus)
[![CI](https://github.com/lvillis/unitbus/actions/workflows/ci.yml/badge.svg)](https://github.com/lvillis/unitbus/actions/workflows/ci.yml)

[中文版本](README.zh-CN.md)

Rust SDK for **Linux systemd**: control **units/jobs** over the **system D-Bus** (systemctl-like), run **transient one-shot tasks**, and query **journald** logs via `journalctl --output=json`.

Runtime is Linux-only (systemd + system bus required). The crate is designed to compile on other
platforms, but most operations will fail with `Error::BackendUnavailable`.

## Use cases

- CD/agent: restart a service and wait for a clear outcome (success/failed/timeout)
- Troubleshooting: get unit status + a bounded slice of recent logs on failure
- Deployment tasks: run one-shot commands as transient units and collect exit status

## Requirements

- systemd on the system bus (`org.freedesktop.systemd1`)
- journald access via `journalctl` (default backend, feature=`journal-cli`)
- Permissions:
  - Unit control typically requires root or PolicyKit authorization.
  - Reading logs via `journalctl` may require root or `systemd-journal` group membership.

## Features

- Default: `journal-cli` (journald via `journalctl`)
- Optional: `config` (drop-in management)
- Optional: `tasks` (transient tasks via `StartTransientUnit`)
- Optional: `tracing` (instrumentation via `tracing`)
- Optional: `observe` (watch unit failures via D-Bus signals)
- Optional: `blocking` (synchronous wrappers, powered by `async_io::block_on`)

## Installation

```toml
[dependencies]
unitbus = "0.1.0"
```

## Quick start

```rust
use unitbus::{UnitBus, UnitStartMode};

async fn restart_nginx() -> Result<(), unitbus::Error> {
    let bus = UnitBus::connect_system().await?;
    let job = bus.units().restart("nginx", UnitStartMode::Replace).await?;
    let outcome = job.wait(std::time::Duration::from_secs(30)).await?;
    println!("{outcome:?}");
    Ok(())
}
```

## Examples

- `examples/restart_and_wait.rs`
- `examples/fetch_recent_logs.rs`
- `examples/diagnose_on_failure.rs`
- `examples/run_transient_task.rs` (requires `--features tasks`)
- `examples/observe_unit_failure.rs` (requires `--features observe`)
- `examples/blocking_restart_and_wait.rs` (requires `--features blocking`)
