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

# Check that each feature builds on its own, without warnings, as a consumer enabling only it
check-features:
    for feature in fixtures local e2e prove; do cargo clippy --no-default-features --features "$feature" -- -D warnings; done

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

# Rebuild the committed guest ELFs reproducibly (Docker)
elfs:
    ./scripts/update_elfs.sh

# The toolchain whose rustdoc JSON interface-parity reads (rust_api::NIGHTLY)
nightly := "nightly-2026-02-08"

# Install the toolchain interface-parity runs rustdoc with
install-nightly:
    rustup toolchain install {{ nightly }} --profile minimal

# Compare the pinned EVM and Solana repositories (interface-parity/pins.toml);
# the full report is target/interface-parity/report.md
interface-parity: install-nightly
    cargo test -p interface-parity --test evm_solana -- --ignored
