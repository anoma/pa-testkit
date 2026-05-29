# Protocol Adapter Test Harness

A lightweight, multi-backend test harness for Protocol Adapter integration and end-to-end testing.

This repository provides a backend-agnostic harness core and an EVM-specific harness. The same test logic can run in local integration-style setups or end-to-end flows against real deployments by injecting backend behavior through shared core traits.

Forwarder-specific harness extensions, action builders, and the integration/e2e tests that use them live alongside each forwarder contract:

- ERC20 forwarder: [anomapay-erc20-forwarder](https://github.com/anoma/anomapay-erc20-forwarder)
- Generic call forwarder: [generic-call-forwarder](https://github.com/anoma/generic-call-forwarder)

For a deeper walkthrough of crate responsibilities and data flow, see [ARCHITECTURE.md](./ARCHITECTURE.md).

## Workspace overview

- `crates/core` - shared traits, state container, witness types, test helpers
- `crates/evm` - EVM environment, setup/prover/execute paths, EVM state helpers

ERC-20 / Permit2 deploy helpers and forwarder-specific action builders now live in the forwarder repositories listed above.

## Quick start

```bash
cargo test --workspace
```

A trivial-action self-test lives at `crates/evm/tests/integration.rs` and exercises the local environment end-to-end. Its trivial-action fixtures live next to it under `crates/evm/tests/trivial_action/` (test-only — never compiled into the library). Forwarder-specific suites live in the forwarder repositories listed above.

## Using as a dependency

Forwarder repositories depend on this crate via a pinned git revision, e.g.:

```toml
[workspace.dependencies]
anoma-pa-testkit-core = { git = "https://github.com/anoma/pa-tests.git", rev = "..." }
anoma-pa-testkit-evm = { git = "https://github.com/anoma/pa-tests.git", rev = "..." }
```
