set shell := ["bash", "-euo", "pipefail", "-c"]

patch:
    cargo release patch --no-publish --execute

publish:
    cargo publish

ci:
    cargo fmt --all --check
    cargo clippy --all-targets --no-default-features --features rt-async-io,journal-sdjournal,journal-cli,config,tasks,observe,blocking,tracing -- -D warnings
    cargo clippy --all-targets --no-default-features --features rt-tokio,journal-sdjournal,journal-cli,config,tasks,observe,blocking,tracing -- -D warnings
    cargo nextest run
    cargo nextest run --no-default-features --features rt-tokio
    cargo nextest run --no-default-features --features rt-async-io,journal-cli
    cargo nextest run --no-default-features --features rt-async-io,config
    cargo nextest run --no-default-features --features rt-async-io,tasks
    cargo test --doc
    cargo test --doc --no-default-features --features rt-async-io
    cargo test --doc --no-default-features --features rt-tokio
    cargo doc --no-deps
    cargo doc --no-deps --no-default-features --features rt-async-io,journal-sdjournal,journal-cli,config,tasks,observe,blocking,tracing
    cargo doc --no-deps --no-default-features --features rt-tokio,journal-sdjournal,journal-cli,config,tasks,observe,blocking,tracing
    cargo package --allow-dirty
