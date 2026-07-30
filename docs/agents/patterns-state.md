# State Patterns

Scope: define and access harness state.

## Conventions

- Keep key constants in `state/keys.rs` or module-local constants when specific.
- Use backend-scoped key namespaces (for example, `evm.*`, `solana.*`).
- Keep `core` state conventions backend-agnostic.
- Expose typed helpers in `state/*` modules:
  - `insert_*(&mut StateBuilder, ...)`
  - `*_in_state(&State) -> anyhow::Result<T>` (or extra args only when truly required)
  - `*<E: Environment>(&E) -> anyhow::Result<T>` (or extra args only when truly required)
- Add context-rich errors.

## Existing Examples

- `crates/evm/src/state/pa.rs`: `KEY_PA_ADDRESS = "evm.pa.address"`.
- `crates/evm/src/state/actors.rs`: `KEY_DEFAULT_SIGNER = "evm.actor.default_signer"`.
- `crates/evm/src/state/keys.rs`: `KEY_CHAIN_ID = "evm.chain.id"`, `KEY_CHAIN_NAME = "evm.chain.name"`.
- `crates/evm/src/state/chains.rs`: `insert_chain`, `chain_id`, `chain_name` typed helpers.
- ERC-20 token-address keys (`evm.erc20.addr.<symbol>`) and forwarder-specific keys (e.g. `evm.forwarder.erc20.v1`, `evm.forwarder.generic-call.v1`) live in the forwarder repositories that own those crates.

## Do / Do Not

- Do centralize key formats behind helper functions.
- Do test key format and insert/get behavior.
- Do not hardcode key strings across unrelated modules.
- Do not bypass typed helpers from test logic when helpers already exist.
