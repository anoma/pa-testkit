# Integration Testkit Restructure — Execution Plan

Design rationale and glossary: see `CONTEXT.md` and `docs/adr/0001`, `docs/adr/0002`
(and `anomapay-erc20-forwarder/CONTEXT.md` for the app domain language).

All work lands on the **currently checked-out branches** — no new branches:

| Repo | Branch |
|---|---|
| anoma-pa-testkit | `feature/simplifications` |
| anoma-pa-evm | `next-bindings` |
| anomapay-erc20-forwarder | `feature/integration-tests` |
| anoma-generic-call-forwarder | `feature/integration-tests` |

arm-risc0 and the resource repos are upstream and untouched.

## Target shape

Two orthogonal axes: **proving backend** (risc0 → openVM/Jolt) varies
`Prover`/`Transaction` and lives in the testkit; **target chain** (EVM → Solana)
varies `ProtocolAdapter`/execution/state and lives in the protocol-adapter repos.
An `Environment` binds one `(backend, chain)` pair and (for e2e) one queue.

```
anoma-pa-testkit                       (ONE crate; crates/evm deleted)
  traits · State · identities · witness+constrain · generic helpers
  LocalProver [local] · QueueProver(typed params) [e2e]
  risc0 Transaction newtype · trivial-action fixtures [fixtures]
  tests/ = chain-free constrain/prover tests

anoma-pa-evm/anoma-pa-evm-integration-test     (was anoma-pa-testkit-evm)
anomapay-erc20-forwarder/anomapay-erc20-forwarder-integration-test   (absorbs 4 harness crates)
anoma-generic-call-forwarder/anoma-generic-call-forwarder-integration-test
```

Deps: git-rev in committed manifests, `[patch]`-to-sibling-path for local dev.
`pa-evm-it` = normal dep; `erc20-it → gc-it` = dev dep. All `publish = false` for now.

## Slices (each ends green: `cargo build` + touched repo's local tests)

- **0. Dev wiring** — `[patch]` git→sibling-path in each downstream root.
- **1. testkit** — rename `-core` → `anoma-pa-testkit`; absorb provers (typed-param
  `QueueProver`, no env), risc0 `Transaction` newtype, trivial fixtures (exposed,
  `fixtures` feature); shrink `crates/evm` to EVM-only consuming the merged crate.
- **2. pa-evm** — move EVM-only `crates/evm` → `anoma-pa-evm-integration-test`;
  e2e drops `deploy_fresh_pa`/param-read, reads addresses from bindings per chain,
  parameterizes chain+queue, builds `QueueProver` from typed params; mock-verifier
  bindings + full prove+execute self-tests move here; delete `crates/evm` from testkit.
- **3. erc20-forwarder** — consolidate 4 harness crates + integration-tests into
  one `anomapay-erc20-forwarder-integration-test` (modules deploy/state/actions);
  AnomaPay ERC20 naming; pa-evm-it as normal dep; drop `anoma-pa-testkit-*` names.
- **4. generic-call** — consolidate into `anoma-generic-call-forwarder-integration-test`;
  erc20-it as **dev**-dep replacing the relative-path deps.
- **5. guardrails & docs** — verify no `prove`/`cuda`/`*-methods` anywhere (ADR-0001);
  add `Swatinem/rust-cache`; document `[patch]` workflow; refresh ARCHITECTURE.md.

## Out of scope (don't touch unless asked)
`State` `unsafe` transmute; dropping `Prover::Transaction`; arm-risc0/resource repos.
