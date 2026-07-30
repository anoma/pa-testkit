# Fixture builders: one `build` per action kind

## Context

Each fixture kind (testkit `trivial`; downstream `wrap`/`transfer`/`unwrap`,
`generic_call`) exposes a builder that assembles the kind's resources into an
action and returns the provable `ActionWitnesses`. Because Rust has no default
arguments, every optional knob historically froze a new function name into the
API: `build`, `build_with_overrides`, `build_with_parts`,
`build_with_parts_and_path`, `build_with_parts_for_owner`. The ladder was the
Cartesian product of optional parameters × return richness, it grew back
independently per repo (only unwrap had `_for_owner`, only some kinds had
`_with_overrides`), and the repos drifted despite the homogeneity rule. The
`Parts` return type also leaked *inputs* back out (e.g. the acting `Keychain`),
because builders chose default identities internally and returning them was the
only way a test could learn who acted.

## Decision

Each fixture kind exposes exactly **one builder**:

```text
kind::{build, ActionData, Overrides}
```

- **One `build` per kind.** Optional knobs are parameters (e.g.
  `path: Option<MerklePath>` where a consumed resource may already be on chain)
  or `Overrides` fields (e.g. `owner: Option<Keychain>`) — **never new function
  names**. The only sanctioned sibling is a batch convenience (`build_many`)
  whose arity, not knobs, differs.
- **`ActionData` contains derived artifacts only**: the `ActionWitnesses` plus the
  consumed/created resources (their nonces and refs are derived during the
  build). Inputs — identities above all — are never returned; the deterministic
  `identities::alice()`/`bob()` are obtainable anywhere, and a test that acts as
  a non-default identity passes it via `Overrides`. `ActionData` resource field
  names state ephemerality (`consumed_ephemeral`, `created_persistent`, …), and
  the witnesses field is named after its type (`witnesses: ActionWitnesses`).
- **`Overrides` is the single home for optional deviations**, flat at the kind
  root (not under a `resource` submodule — it overrides action-level
  derivations like signatures too). Its named `invalid_*` constructors form the
  catalog of defeated checks for negative tests.
- The kind module re-exports this surface from its `mod.rs`; `action.rs` /
  `resource.rs` are private implementation files.

## Consequences

- Call sites pay one token per unused knob (`None`, `Overrides::default()`,
  `.witnesses`) — the price of never multiplying names. A new optional knob is a
  new `Overrides` field (non-breaking for `..Default::default()` users), not a
  new function.
- Homogeneity across repos becomes mechanical: every kind has the same three
  public names, so drift is visible at a glance.
- Tests state their actors explicitly (or accept the documented defaults)
  instead of laundering identities through builder outputs.
- Downstream repos consume this surface via git-rev pins; rolling out a builder
  change follows the push-then-bump order of
  [[0002-cross-repo-dependencies-and-deferred-publishing]].
