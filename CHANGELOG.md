# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog (https://keepachangelog.com/en/1.1.0/)
and this project aims to follow Semantic Versioning (https://semver.org/).

## [Unreleased]

## [0.1.0]

### Added

- Async-first Rust SDK for controlling systemd units via D-Bus (start/stop/restart/reload + wait for job outcome).
- Structured journald log access via `journalctl --output=json` (bounded reads, error classification).
- Failure diagnosis helper (unit status + recent log slice).
- Optional features:
  - `tasks`: run one-shot transient tasks (`StartTransientUnit`) and get exit status.
  - `config`: manage systemd drop-in files.
  - `observe`: watch unit failures via D-Bus signals.
  - `blocking`: synchronous wrappers.
  - `tracing`: instrumentation via `tracing`.

