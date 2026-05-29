# Architecture

This repository is the **risc0 testkit**: the reusable, backend-agnostic core for
Anoma integration testing. It owns the risc0 proving path and the traits that
each target chain's harness implements. It knows nothing about EVM or Solana —
all chain-specific testing code lives in the protocol-adapter repos.

See `CONTEXT.md` for the glossary, `docs/adr/` for decisions, and
`docs/RESTRUCTURE-PLAN.md` for the migration that produced this shape.

## Two axes of variation

- **Proving backend** (risc0 today; possibly openVM/Jolt later) — varies the
  `Prover` and the `Transaction`. Lives here.
- **Target chain** (EVM today; Solana later) — varies the `ProtocolAdapter`,
  on-chain execution, and state. Lives in the protocol-adapter repos.

An `Environment` binds one (backend, chain) pair, and for e2e one proving queue.

## Crate

A single crate `anoma-pa-testkit` at the repo root — no workspace, no `crates/`
nesting.

- `environment` — backend-agnostic traits: `Environment`, `Prover`,
  `ProtocolAdapter`, `Transaction`, `CommitmentTree`, plus the typed `State` /
  `StateBuilder` container.
- `witness` — `ActionWitnesses`, `LogicWitness`, and `constrain_action` (native
  constraint checking, no zkVM).
- `transaction` — the risc0 `Transaction` newtype over `arm::Transaction`, the
  orphan-rule seam that lets the testkit implement the `Transaction` trait.
- `prover` — the two chain-agnostic provers:
  - `LocalProver` (`feature = "local"`): runs `constrain` and emits mock Groth16
    seals. No real proving — fast and offline.
  - `QueueProver` (`feature = "e2e"`): submits to the real remote proving queue.
    Built from typed params (`new(base_url, auth_token)`); reads no environment.
- `assert` — negative-test assertion helpers (`Needle`,
  `expect_integration_panic`), shared by every integration-test crate. The
  proof-tamper counterpart lives on `Transaction` (`tamper_first_logic_seal`).
- `fixtures` (`feature = "fixtures"`): the trivial action kind — one `build`
  (plus batch `build_many`) returning `ActionData`, with `Overrides` for negative
  tests (ADR-0003). App- and chain-agnostic, exposed for reuse by every
  integration-test crate.
- `identities` — well-known test signing keys.
- `mocks` (`feature = "mocks"`): `mockall` doubles of the core traits.

Generic helpers `prove_actions`, `execute_tx`, `commitment_root` live at the
crate root.

## Downstream layout

Each protocol-adapter / forwarder repo owns one `integration-test` crate next to
its `bindings`, implementing the core traits (or composing helpers) for its
contracts:

- `anoma-pa-evm` → `anoma-pa-evm-integration-test`: the EVM `Environment`
  (local + e2e), `ProtocolAdapter`, ARM→EVM conversion, EVM state, PA deploy
  (local) / lookup-from-bindings per chain (e2e), mock verifier bindings.
- `anomapay-erc20-forwarder` → `anomapay-erc20-forwarder-integration-test`:
  ERC20/Permit2/forwarder deploy + state helpers and the AnomaPay ERC20
  wrap/transfer/unwrap action builders.
- `anoma-generic-call-forwarder` →
  `anoma-generic-call-forwarder-integration-test`: generic-call deploy + state
  and action builders; composes the AnomaPay ERC20 app in its setups.

## Data flow

1. A chain harness constructs an `Environment` and populates backend state.
2. Tests build action witnesses (trivial fixtures here, or app-specific builders
   downstream).
3. `prove_actions` delegates to the backend `Prover`, yielding an
   `arm::Transaction`.
4. `execute_tx` delegates to the chain `ProtocolAdapter`, which converts the ARM
   transaction to chain calldata and executes it.
5. Successful execution updates the commitment tree; tests assert roots and
   error paths.

## CI / proving guardrail

Integration tests never prove locally (local = `constrain`; e2e = remote queue).
No crate enables `risc0-zkvm/prove` or `cuda`, or depends on a guest `*-methods`
crate — see `docs/adr/0001`. This keeps CI off the risc0 guest toolchain.
