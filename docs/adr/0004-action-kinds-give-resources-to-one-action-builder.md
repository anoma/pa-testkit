# Action kinds give resources to one action builder

Supersedes [[0003-fixture-builders-one-build-per-action-kind]].

## Context

Since arm-risc0 2.0.0-rc.8, an action holds one list of resources, each consumed or created. The protocol adapter processes the resources in the order of this list, so the list also sets the order of the forwarder calls. The nonce of a created resource derives from its index among the created resources and from the consumed nullifiers, both in the order of the list. arm's `Action` pairs logic proof `i` with resource `i` and checks its tag and verifying key.

Under ADR-0003, each action kind had its own `build`, which put only the resources of its kind into an action. A test could not put the resources of two kinds into one action, for example the wrap and the generic call of a swap. Each kind repeated the nonce derivation, the action tree and the compliance witness, and the trivial kind needed an order knob (`Overrides::resource_order`) to test the order.

## Decision

- `fixtures::action::build(resources)` is the only function that builds an action. It takes `ActionResource`s in the order of the action, derives the created nonces and the action tree root, and returns the `ActionWitnesses`.
- An `ActionResource` is one resource of the action and the function that builds its logic witness from the resource and the action tree root. Its constructors are `ActionResource::consumed(resource, nf_key, merkle_path, logic)` and `ActionResource::created(resource, logic)`.
- A created action resource takes a `ResourceToCreate`, which has all fields of a resource except the nonce. `build` derives the nonce, so a caller can neither set a nonce that `build` replaces nor compute a commitment from it.
- An action kind exposes `resources(..)`, which gives its action resources, and `Overrides`. Optional knobs are parameters or `Overrides` fields, never new function names. The named `invalid_*` constructors of `Overrides` are the catalog of defeated checks for negative tests.
- A test sets the order of the resources with the order of the action resources. No kind has an order knob.
- `ActionWitnesses` pairs logic witness `i` with resource `i` of the compliance witness, as arm's `Action` does, and both provers check each position. `ActionWitnesses::resources()` returns the resources with their derived nonces. Inputs, such as identities, are never returned.
- The provers borrow the actions, so a test can read the resources of an action after it proves the action.
- The testkit implements `LogicWitness` for each arm-risc0 `LogicProver` whose witness is a `LogicCircuit`. A kind passes the logic prover of its circuit and needs no adapter type.

## Consequences

- A test builds and proves an action in two steps: `let wrap = action::build(wrap::resources(..)?)?;`, then `prove_actions(&env, [&wrap])`. `prove_actions` takes any iterator of action witnesses, so a batch passes as `&actions`. A batch of actions is a map over seeds; `build_many` is gone.
- A test takes a resource by its position, for example `wrap.resources()[1]`. No type names the resources by side and ephemerality.
- Logic witnesses out of the order of the resources fail in the provers, as they fail in arm.
- A test that needs a created nonce other than the derived one must build the compliance witness itself. arm-risc0 tests that the compliance circuit rejects such a nonce.
- The downstream kinds (`wrap`, `transfer`, `unwrap`, `generic_call`) move to this surface after this testkit release, in the order of [[0002-cross-repo-dependencies-and-deferred-publishing]].
