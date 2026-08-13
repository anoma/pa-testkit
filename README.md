# Anoma Protocol Adapter Testkit

The backend-agnostic risc0 test core for Anoma protocol-adapter integration
testing: the `Environment` / `Prover` / `Transaction` traits, the trivial
action fixtures, the local mock prover, and the remote-queue prover for e2e
runs. It knows nothing about any target chain — chain-specific harnesses live
in the protocol-adapter repos and implement the traits from here:

- EVM: [anoma-pa-evm](https://github.com/anoma/pa-evm)
  (`anoma-pa-evm-integration-test`)
- Forwarder-specific extensions, action builders, and their test suites:
  [anomapay-erc20-forwarder](https://github.com/anoma/anomapay-erc20-forwarder),
  [generic-call-forwarder](https://github.com/anoma/generic-call-forwarder)

See [ARCHITECTURE.md](./ARCHITECTURE.md) for module responsibilities and data
flow, [CONTEXT.md](./CONTEXT.md) for the glossary, and [docs/adr/](./docs/adr)
for decisions.

## Layout

A single flat crate, `anoma-pa-testkit` — no workspace. Feature-gated parts:

- `fixtures` (default) — the trivial action kind and the test identities
- `local` (default) — `LocalProver`: native `constrain` plus mock Groth16
  seals, no real proving
- `e2e` — `QueueProver`: submits witnesses to the remote proving queue
- `abi_encoding` — the EVM-ABI aggregation journal encoding (what the EVM
  protocol adapter reconstructs); off by default
- `mocks` — `mockall` doubles of the core traits

## Quick start

```bash
cargo test
```

The trivial-action smoke tests live in `tests/trivial_action.rs`. Chain- and
forwarder-specific suites live in the repos listed above.

## Using as a dependency

Downstream repos pin a git revision so single-repo CI resolves
([ADR-0002](./docs/adr/0002-cross-repo-dependencies-and-deferred-publishing.md)):

```toml
[workspace.dependencies]
anoma-pa-testkit = { git = "https://github.com/anoma/pa-testkit", rev = "..." }
```
