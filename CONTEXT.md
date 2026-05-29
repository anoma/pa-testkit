# Anoma Testkit

The reusable test core for Anoma integration testing, built on the risc0 proving
stack. All target-chain-specific (EVM, future Solana) testing code lives in the
respective protocol-adapter repos.

Two orthogonal axes of variation shape the design:

- **Proving backend** (risc0 today; possibly openVM/Jolt later) — varies the
  `Prover` and the `Transaction`. Lives in the testkit.
- **Target chain** (EVM today; Solana later) — varies the `ProtocolAdapter`,
  on-chain execution, and state. Lives in the protocol-adapter repos.

An `Environment` binds one (backend, chain) pair. The `type Transaction`
associated seam keeps the backend swappable in principle, but the testkit is not
yet fully backend-agnostic: its witness types and ARM value vocabulary are
risc0-bound. Full openVM/Jolt support is a deliberately deferred future effort.

## Language

**Testkit**:
The backend-agnostic test core. Owns the risc0 proving path and the traits that
chain-specific harnesses implement. Knows nothing about EVM or Solana.
_Avoid_: framework, library (when referring to this repo specifically)

**Environment**:
The trait a chain-specific harness implements to wire together a prover, a
protocol adapter, and test state. A concrete environment is one (chain, mode)
pairing — e.g. EVM-local or EVM-e2e.
_Avoid_: harness (an environment is assembled by a harness, it is not the harness)

**Proving backend**:
The zkVM stack that produces ARM proofs — risc0 today, possibly openVM or Jolt
later. The backend determines the concrete `Transaction` type and `Prover`
implementations, all of which live in the testkit.

**Prover**:
The component that turns action witnesses into a proven ARM transaction. Two
risc0 variants live in the testkit and are agnostic to the target chain: a local
prover (runs circuits via `constrain`, emits mock seals, no real proving) and a
queue prover (submits to the real remote proving queue).
_Avoid_: proof generator

**ARM transaction**:
The transaction produced by a prover, carrying the proofs. In the testkit it is
a thin newtype over the proving backend's transaction (e.g. risc0's), existing
so the testkit can implement the `Transaction` trait on a foreign type. Each
target chain's `ProtocolAdapter` converts it into chain-specific calldata at
execution time. The newtype is a proving-backend artifact and lives in the
testkit, never in a protocol-adapter repo.
_Avoid_: tx (in prose), EVM transaction (that is the post-conversion artifact)

**Protocol adapter**:
The on-chain contract that verifies and executes ARM transactions on a target
chain. Each chain has its own (the EVM PA, the future Solana PA). In tests it is
represented by a chain-specific `ProtocolAdapter` trait implementation.
_Avoid_: PA contract (use "protocol adapter"), verifier

**Action kind**:
A fixture module that builds one kind of action (`trivial` here; `wrap` /
`transfer` / `unwrap` and `generic_call` downstream). Its public surface is
exactly `build` (plus a batch `build_many` where warranted), `ActionData`, and
`Overrides` — one builder per kind, optional knobs as parameters or `Overrides`
fields, never new function names (ADR-0003).
_Avoid_: build variants (`build_with_*` suffixes)

**ActionData**:
The derived data of a build: the action witnesses (`witnesses:
ActionWitnesses`) plus the consumed and created resources, resource field names
stating ephemerality (`consumed_ephemeral`, `created_persistent`, …). Never
contains inputs — identities are passed in (or defaulted) via `Overrides`, not
returned.
_Avoid_: Parts, Built (older names; Parts also leaked input keychains)

**Overrides**:
A kind's optional deviations from its defaults — resource-field and
action-derivation knobs alike, including the acting identity. Named `invalid_*`
constructors catalog the deliberately-broken variants for negative tests, one
per defeated check.

**Provisioning helper**:
A reusable building block, owned and exposed by the integration-test crate of
the repo that owns a contract, that makes one contract available to a test —
either by deploying it (local) or by reading its address from `bindings` (e2e).
Dependent apps call these; they are never duplicated.

**Scenario setup**:
A test-local function (lives under `tests/common/`) that composes provisioning
helpers into the full world a particular test needs. Written once per
environment — i.e. deliberately duplicated across local/e2e and never shared
with dependent apps, since scenarios are specific to the test that needs them.
_Avoid_: fixture (reserved for static test data)

**Local environment**:
Test mode running against a fresh local chain with nothing deployed; the harness
deploys everything from scratch and uses the local prover (no real proofs).
Optimized for fast, self-contained test runs. Accepts a chain id so chain-id-
sensitive logic (e.g. EIP-712 / Permit2 signing) can be exercised.

**E2e environment**:
Test mode running against an Anvil fork of a real chain on which the contracts
are already deployed; the harness reads their addresses + chain IDs from the
`bindings` crates and deploys nothing, and uses the queue prover for real proof
generation. Forking isolates the test so real on-chain state is never mutated.
_Avoid_: end-to-end (spell as "e2e" for the environment name)

**Chain / queue selection**:
An `Environment` targets exactly one chain and (for e2e) one proving queue, both
chosen per run. The chain selector resolves an RPC/fork target and per-chain
deployed addresses (from `bindings`); the queue selector constructs a
`QueueProver` from typed params. The testkit hardcodes neither — selection and
config live in the protocol-adapter repo. A test fans out across chains/queues by
adding `rstest` cases over the same generic body. Simultaneous multi-chain (one
`Environment`, several chains at once, for cross-chain scenarios) is out of scope
for now.
