# Show commands before running (helps debug failures)
set shell := ["bash", "-euo", "pipefail", "-c"]

# Default recipe
default:
    @just --list

# Format
fmt *args:
    cargo fmt {{ args }}

# Check formatting
fmt-check:
    cargo fmt -- --check

# Build
build *args:
    cargo build {{ args }}
    cargo build --features e2e,abi_encoding {{ args }}

# Test
test *args:
    cargo test {{ args }}
    cargo test --features e2e {{ args }}

# Publish
publish *args:
    cargo publish {{ args }}

# Lint (clippy)
lint:
    cargo clippy --no-deps -- -Dwarnings
    cargo clippy --no-deps --tests -- -Dwarnings
    cargo clippy --no-deps --features e2e,abi_encoding -- -Dwarnings
    cargo clippy --no-deps --tests --features e2e -- -Dwarnings
    cargo clippy --no-deps --tests --features e2e,abi_encoding -- -Dwarnings
