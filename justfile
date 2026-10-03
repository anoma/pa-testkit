# Show commands before running (helps debug failures)
set shell := ["bash", "-euo", "pipefail", "-c"]

# Default recipe
default:
    @just --list

# Format
fmt *args:
    cargo fmt --all {{ args }}

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Build
build *args:
    cargo build --workspace {{ args }}

# Test
test *args:
    cargo test --workspace {{ args }}

# Publish
publish *args:
    cargo publish {{ args }}

# Lint (clippy)
lint:
    cargo clippy --workspace --no-deps -- -Dwarnings
    cargo clippy --workspace --no-deps --tests -- -Dwarnings

# Compare the pinned EVM and Solana repositories (interface-parity/pins.toml);
# the full report is target/interface-parity/report.md
interface-parity:
    rustup toolchain install nightly-2026-02-08 --profile minimal
    cargo test -p interface-parity --test evm_solana -- --ignored
