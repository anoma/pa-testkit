# Cross-repo dependencies and deferred publishing

## Context

The testkit and the per-repo integration-test crates form a layered cross-repo
graph (`anoma-pa-testkit ← anoma-pa-evm-integration-test ← *-forwarder-integration-test`).
They are co-developed as sibling checkouts, but each repo has its own CI and must
build standalone. Separately, these crates *could* be published to crates.io for
external consumers — but publishing is a one-way door: crate names are claimed
permanently, versions are immutable, and external consumers impose a semver
contract on a still-churning API.

## Decision

Cross-repo dependencies are pinned by **git rev** in committed manifests, and
local cross-repo iteration uses a Cargo `[patch]` that redirects those git deps
to sibling **paths**. Path deps never appear in a committed dependency line —
only inside `[patch]` — so the repos stay self-contained and CI hermetic.

Publishing is **deferred**. All integration-test crates are `publish = false`;
the testkit is publish-ready but not yet published. Manifests are nonetheless
kept publish-ready (real `version`/`license`/`description`/`repository`, no
path-only deps in dependency lines) so the option stays open at zero cost.
Publishing happens later, deliberately, per-crate, only when there is real
external demand and the crate's API has stabilized — in topological order
(`anoma-pa-testkit` first; the `*-bindings` crates are already published). The swap
from `git+rev` to a crates.io `version` is then a one-line change per dep.

## Consequences

- Deferral costs nothing for in-workspace development (git-rev + `[patch]` serve
  the whole sibling graph identically whether or not anything is published) and
  avoids prematurely freezing crate names and APIs.
- When publishing does happen, normal/build deps of a published crate must
  already be on crates.io; **dev-dependencies may stay git/path** (Cargo strips
  versionless dev-deps on publish), which is why the layering keeps
  `anoma-pa-evm-integration-test` a normal dep but the generic-call → erc20
  dependency a dev-dep. See [[0001-integration-tests-avoid-local-risc0-proving]].
