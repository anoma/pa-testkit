# Integration tests avoid local risc0 proving

## Context

Integration tests run in two environments (local, e2e) and are exercised on
GitHub CI, where compile and run time matter. risc0's expensive build step is
guest-circuit compilation (`build.rs` + `risc0-build`, which cross-compiles
RISC-V and pulls the risc0 toolchain). That cost is what usually makes risc0 CI
slow and cache-unfriendly.

## Decision

Integration tests never generate proofs locally. The **local** environment runs
resource logic and compliance natively via `constrain` (no zkVM); the **e2e**
environment submits to the remote proving queue (proving happens off-box). No
integration-test crate enables `risc0-zkvm/prove` (nor `cuda`) or depends on any
`arm_circuits/*/methods` crate. `anoma-rm-risc0` is pinned with
`default-features = false` and only host-side features (`transaction`,
`compliance_circuit`, and `aggregation` where the e2e payload is assembled).

This works because `anoma-rm-risc0` ships the circuits as **prebuilt, committed
ELF binaries** embedded via `include_bytes!` (`arm/src/constants.rs`,
`arm/src/aggregation/constants.rs`); it has no dependency on the guest `methods`
crates. So the dependency graph compiles only risc0 host crates — chunky on a
cold cache but ordinary, fully cacheable Rust (use `Swatinem/rust-cache`, keyed
on `Cargo.lock`), and triggers no RISC-V guest builds.

## Consequences

- The decisive guardrail is a constraint, not a code structure: enabling
  `prove`/`cuda` or pulling a `*-methods` crate anywhere in the graph silently
  reintroduces the guest toolchain and balloons CI. A future engineer wiring up
  "real" local proving would undo this — hence this record.
- Local tests validate constraint satisfaction, not real proofs. Real proof
  generation and on-chain verifier compatibility are covered only by e2e.
