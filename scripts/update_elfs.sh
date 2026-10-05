#!/usr/bin/env bash
# Rebuilds the pass-through logic guest (circuits/passthrough-logic)
# reproducibly with cargo risczero build, which builds in Docker, and copies it
# to elfs/. The build prints its image id, which PASSTHROUGH_LOGIC_VK
# (src/fixtures/passthrough/logic.rs) must name; the fixture's test checks
# that it does.
set -euo pipefail

# The tool and builder image the committed guest was built with: another
# builder's toolchain produces another binary, so another image id.
CARGO_RISCZERO_VERSION="3.0.5"
export RISC0_DOCKER_CONTAINER_TAG="r0.1.88.0"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if [[ "$(cargo risczero --version)" != "cargo-risczero $CARGO_RISCZERO_VERSION" ]]; then
  echo "❌ The guest is built with cargo-risczero ${CARGO_RISCZERO_VERSION}; cargo risczero is $(cargo risczero --version)." >&2
  echo "   Install it with: rzup install cargo-risczero ${CARGO_RISCZERO_VERSION}" >&2
  exit 1
fi

cargo risczero build --manifest-path circuits/passthrough-logic/Cargo.toml
cp circuits/passthrough-logic/target/riscv32im-risc0-zkvm-elf/docker/passthrough-logic-guest.bin \
  elfs/passthrough-logic-guest.bin
