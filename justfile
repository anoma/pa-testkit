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

# Test
test *args:
    cargo test {{ args }}

# Lint (clippy)
lint:
    cargo clippy --no-deps -- -Dwarnings
    cargo clippy --no-deps --tests -- -Dwarnings
