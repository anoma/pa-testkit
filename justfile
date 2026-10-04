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

# Check that each feature builds on its own, as a consumer enabling only it
check-features:
    for feature in fixtures local e2e mocks abi_encoding; do cargo check --no-default-features --features "$feature"; done

# Test
test *args:
    cargo test {{ args }}

# Publish
publish *args:
    cargo publish {{ args }}

# Lint (clippy)
lint:
    cargo clippy --no-deps -- -Dwarnings
    cargo clippy --no-deps --tests -- -Dwarnings

# Rebuild the committed guest ELFs reproducibly (Docker)
elfs:
    ./scripts/update_elfs.sh
