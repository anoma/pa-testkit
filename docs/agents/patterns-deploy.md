# Deploy Patterns

Scope: deploy/init helpers and state insertion conventions.

## Conventions

- For each deployable contract/binding module, prefer this shape:
  - `contract_instance(address, provider)` constructor helper.
  - `deploy_*` for deployment only.
  - `deploy_and_insert_*` for deployment + state insertion.
- `deploy_and_insert_*` should accept `&mut StateBuilder` and write typed keys through state helpers.
- Keep deploy helpers in binding/deploy modules.
- Add `.context(...)` at async boundaries (deploy, send, receipt, chain reads).
- Mirror this shape per backend.

## Existing Examples

- `crates/evm/src/pa.rs`: `deploy_protocol_adapter`, `deploy_and_insert_protocol_adapter`.
- `crates/evm/src/mock_risc0_bindings.rs`: `deploy_mock_risc0_stack`.
- ERC-20 deploy helpers (`deploy_weth`, `deploy_and_mint_weth`, `deploy_and_insert_weth`, matching ERC20Example helpers), the canonical Permit2 deployment utility, and forwarder-specific deploy helpers (e.g. `deploy_and_insert_erc20_forwarder`, `deploy_and_insert_generic_call_forwarder`) live in the forwarder repositories.

## Do / Do Not

- Do keep base env deployment minimal unless task explicitly needs extras.
- Do keep add-on deploy behavior in extension crates.
- Do not auto-wire optional contracts into base setup without request.
