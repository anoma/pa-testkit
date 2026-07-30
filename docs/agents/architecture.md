# Architecture

Scope: crate responsibilities and data flow.

## Workspace Layout

- `crates/core`: generic traits, state store, witness types, helpers (`prove_actions`, `execute_tx`, `commitment_root`).
- `crates/evm`: EVM harness implementation, state helpers, PA/mock Risc0 deploy and integration/e2e envs. Hosts the trivial-action self-test under `tests/`, with its fixtures under `tests/trivial_action/`.

ERC-20 / Permit2 deploy helpers and forwarder-specific harness extensions and integration tests live in their respective forwarder repositories (`anomapay-erc20-forwarder`, `generic-call-forwarder`) and depend on this workspace via a pinned git revision.

## Core Flow

- Setup builds concrete env and populates typed `State` keys.
- Tests construct witnesses (via the trivial-action fixtures under `crates/evm/tests/trivial_action/`, or a forwarder-specific action builder in a downstream repo).
- `prove_actions` delegates to env prover.
- `execute_tx` delegates to protocol adapter execution.
- Successful execution updates commitment tree; tests assert roots and failures.

## Design Boundaries

- Setup code may touch concrete env fields directly.
- Test execution code should stay generic over `impl Environment`.
- State key namespaces are backend-scoped (`evm.*`, `solana.*`, etc.); keep core patterns backend-agnostic.
- Keep state access via typed state helper modules, not ad hoc string keys in tests.
- Keep feature-gated modules aligned with crate features.
