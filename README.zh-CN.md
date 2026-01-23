# unitbus

[![crates.io](https://img.shields.io/crates/v/unitbus.svg)](https://crates.io/crates/unitbus)
[![docs.rs](https://docs.rs/unitbus/badge.svg)](https://docs.rs/unitbus)
[![CI](https://github.com/lvillis/unitbus/actions/workflows/ci.yml/badge.svg)](https://github.com/lvillis/unitbus/actions/workflows/ci.yml)

面向 **Linux systemd** 的 Rust SDK：通过 **system D-Bus** 以类似 `systemctl` 的方式控制 **unit/job**，执行 **transient oneshot 任务**，并结构化读取 **journald** 日志（默认纯 Rust 后端）。

运行时仅支持 Linux（需要 systemd + system bus）。本 crate 设计为可在其他平台编译，但大多数操作会返回 `Error::BackendUnavailable`。

## 适用场景

- CD/Agent：restart 某个 service，并在超时内拿到明确结果（成功/失败/超时）
- 故障诊断：失败时获取 unit 状态 + 最近关键日志切片（有上限，避免拉爆日志）
- 部署任务：用 transient unit 运行一次性命令并拿到 exit status
- Exporter/监控：枚举全部 unit，并按类型读取 properties（Unit/Service/Socket/Timer）用于指标采集

## 环境要求

- system bus 上存在 systemd（`org.freedesktop.systemd1`）
- async runtime 后端（二选一）：
  - 默认：`rt-async-io`（不依赖 `tokio`）
  - 可选：`rt-tokio`（tokio 后端）
- journald 后端：
  - 默认：纯 Rust 读取 journal 文件（feature=`journal-sdjournal`）
  - 可选：`journalctl` JSON 后端（feature=`journal-cli`）
- 权限：
  - 控制 unit（start/stop/restart/reload）通常需要 root 或 PolicyKit 授权
  - 读取日志可能需要 root 或加入 `systemd-journal` 组

## Features

- 默认运行时：`rt-async-io`
- 可选运行时：`rt-tokio`（与 `rt-async-io` 互斥）
- 默认：`journal-sdjournal`（纯 Rust journald 后端，不依赖 `journalctl` 子进程）
- 可选：`journal-cli`（通过 `journalctl --output=json` 读取 journald）
- 可选：`config`（drop-in 配置管理）
- 可选：`tasks`（通过 `StartTransientUnit` 执行 transient task）
- 可选：`tracing`（通过 `tracing` 增强可观测性）
- 可选：`observe`（通过 D-Bus 信号观察 unit 失败事件）
- 可选：`blocking`（同步封装，由所选 runtime 驱动）

## 安装

```toml
[dependencies]
unitbus = "0.1"
```

使用 `journalctl` JSON 后端：

```toml
[dependencies]
unitbus = { version = "0.1", default-features = false, features = ["rt-async-io", "journal-cli"] }
```

tokio 项目推荐：

```toml
[dependencies]
unitbus = { version = "0.1", default-features = false, features = ["rt-tokio", "journal-sdjournal"] }
```

## 快速开始

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

## 示例

- `examples/restart_and_wait.rs`
- `examples/fetch_recent_logs.rs`
- `examples/diagnose_on_failure.rs`
- `examples/list_units_and_properties.rs`
- `examples/run_transient_task.rs`（需要 `--features tasks`）
- `examples/observe_unit_failure.rs`（需要 `--features observe`）
- `examples/blocking_restart_and_wait.rs`（需要 `--features blocking`）

## 可选：Linux/systemd 集成测试

这些测试默认是 `#[ignore]`，用于在真实 systemd 环境上验收：

```bash
UNITBUS_ITEST_UNIT=dbus.service cargo test --no-default-features --features rt-async-io,journal-sdjournal,tasks,config,observe,blocking,tracing --test integration_linux -- --ignored
```
