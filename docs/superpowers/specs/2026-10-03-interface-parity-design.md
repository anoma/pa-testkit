# Interface parity between the EVM and Solana repositories

A test in pa-testkit lists every difference between what the EVM repositories publish and what the Solana repositories publish. It fails until each difference is either removed or excused by a reviewed entry that explains it. This makes anoma/dos-pm#89 ("Give the Solana repositories the same external interface as the EVM repositories") checkable by a machine, with every gap named.

## What the test reports

Every published item on either side ends in exactly one of five outcomes, and the report keeps them under separate headings:

| Outcome | Meaning | Fails the test |
|---|---|---|
| match | the item exists in both paired packages with identical values | no |
| excused by `<id>` | the item differs, and excuse `<id>` covers exactly this difference; its explanation is printed with it | no |
| only on EVM / only on Solana | the item exists in one paired package and not the other | yes |
| differs | the item exists in both, with different values; both values are printed | yes |
| stale excuse | an excuse entry matches no difference in this run | yes |

The test passes when there are no unexcused differences and no stale excuses. Excused differences are still printed, under their own heading, so a pass never hides them.

An example unexcused line (the item's exact rendering comes from `public-api`):

```
only on EVM  anoma-pa-evm-bindings ↔ anoma-pa-solana-client
  pub fn addresses::protocol_adapter_address(environment: Environment, chain: &alloy_chains::NamedChain) -> Option<alloy_primitives::Address>
```

## Inputs: three reviewed files

The test reads three files committed next to it in pa-testkit. Each is edited by hand and reviewed like code. Nothing else in the comparison is hand-written.

- **`pins.toml`** names each repository by URL and exact commit. The test compares those commits and nothing else. Comparing newer code means changing a pin in a commit, and the report for that commit shows every difference the change introduced.
- **`pairs.toml`** declares which EVM repository corresponds to which Solana repository, and which EVM package to which Solana package. Pairing is needed because items pair by literal path inside a package, and package names differ across chains (`anoma-pa-evm-bindings` vs `anoma-pa-solana-client`). A published package that appears in no pair is still reported, item by item, as only on its side. The report prints the pairs first.
- **`excuses.toml`** holds one entry per accepted difference: an id, the pair, the item path, the part that differs, the exact EVM value, the exact Solana value, and the explanation. An entry covers that exact difference only. If either side later changes the value, the entry stops matching, so the line fails again and the entry is reported as stale.

## What counts as a repository's external interface

The published packages are read from each repository's own manifests, with no list maintained here:

- every Cargo package whose manifest does not declare `publish = false`, found with `cargo metadata --no-deps` on every `Cargo.toml` in the repository;
- every npm package whose `package.json` has a `name` and does not declare `"private": true` (npm refuses to publish a package without a name).

This rule is provisional. The repositories do not set `publish = false` consistently, so some packages it counts as published are test or tooling crates. The report shows them, and a review that finds the rule wrong changes the rule.

For each published package, the test extracts:

1. **Package metadata:** name, version, features and their contents, and dependencies with their version requirements.
2. **Rust public API:** one item per line from the `public-api` crate, the library behind `cargo public-api`. It reads rustdoc's JSON output, which needs a pinned nightly toolchain. Functions, struct fields, enum variants, constants and trait impls are each their own item, at every public path, re-exports included. The extraction enables all features.
3. **TypeScript exports**, read with the TypeScript compiler API: each exported name with its declared type.
4. **Shipped files** other than Rust sources (the Rust API in item 2 already covers those). A file is shipped if `cargo package --list` or `npm pack --dry-run --json` lists it. Files pair by path within the package. JSON files such as `deployments.json` are compared key by key, so a difference names the JSON path; any other file is compared byte for byte.
5. **Tag schemes of the repository:** each remote tag is split into a prefix and a semver version (`bindings/v3.0.0` gives `bindings/v`). Paired repositories are compared by their sets of prefixes.

## Repositories at the first pin

| Side | Repository | Branch at the pin | Commit |
|---|---|---|---|
| EVM | anoma/pa-evm | `next` | `dbac05ae68776199ac904aae4376e886d11f199f` |
| EVM | anoma/anomapay-erc20-forwarder | `next` | `8e2d30f246d595bf6b91231f76d4c25b5c0d24bf` |
| EVM | anoma/forwarder-bases | `next` | `08f2f2e8b147fc6c9394031063664a6f3cd6a58c` |
| Solana | anoma/anoma-pa-solana-client | `main` | `ba957aa38246cc3906cc1dc1e9c5cacd36ac5f10` |
| Solana | anoma/solana-protocol-adapter | `anthony/arm-v2-port` | `866d11ed20bfec13d77ac880528206f2eaa5113d` |

The EVM side is V2, which lives on `next` in all three EVM repositories.

The first `pairs.toml`:

| EVM | Solana |
|---|---|
| repository anoma/pa-evm | repository anoma/solana-protocol-adapter |
| repository anoma/anomapay-erc20-forwarder | repository anoma/solana-protocol-adapter |
| package `anoma-pa-evm-bindings` | package `anoma-pa-solana-client` |
| package `anomapay-erc20-forwarder-bindings` | package `anoma-pa-solana-client` |

One Solana package or repository may appear in several pairs, and each pair is compared on its own. At the first pin, one Solana client and one Solana repository still serve both the adapter and the forwarder. anoma/forwarder-bases and its package `anoma-forwarder-bases-bindings` start unpaired, so all of their items are reported as only on EVM.

The same work splits the SPL token forwarder into its own repository, anoma/anomapay-spl-token-forwarder, with its program, its client crate and npm package, and its deployment record (anoma/dos-pm#90). After the split, a repin changes the forwarder pairs to anoma/anomapay-erc20-forwarder ↔ anoma/anomapay-spl-token-forwarder and `anomapay-erc20-forwarder-bindings` ↔ the new repository's client crate.

The first run will report tens of thousands of unexcused differences. At its pin, `anoma-pa-evm-bindings` alone has 47,087 public items, most of them generated contract bindings and the trait methods every type gets from blanket impls; `anoma-pa-solana-client` has 2,680. The Solana adapter repository declares no `publish = false`, so its test programs and `fixture-gen` count as published and are reported as only on Solana. Neither Solana repository has tags yet. The test reports all of these; deciding which become excuses and which become Solana changes is the review.

## Where it lives

Everything lives in pa-testkit, which holds every chain-agnostic test. pa-testkit becomes a Cargo workspace with two members: the existing `anoma-pa-testkit` crate, unchanged, and `interface-parity`, declared `publish = false`, which holds the comparison library, its own tests, the EVM-to-Solana comparison test and the three input files. Consumers of the testkit library never build the comparison's dependencies.

pa-testkit's CI runs only the tool's own tests on its fixture packages (see below). The EVM-to-Solana comparison test carries `#[ignore = "compares the pinned EVM and Solana repositories; run with just interface-parity"]`, so `cargo test` and CI skip it by that explicit filter, and the recipe `just interface-parity` runs it with `cargo test -p interface-parity -- --ignored`. It runs when the pins, pairs or excuses change and for each review.

The test needs network access to fetch the pinned commits, a pinned nightly toolchain for rustdoc JSON, and Node for the TypeScript extraction. The tool runs that nightly's `rustc` and `rustdoc` explicitly, because rustdoc JSON fails when the dependencies were compiled by another `rustc`. It writes the full report to `target/interface-parity/report.md` and prints the failing lines.

## Testing the comparison itself

Small fixture packages inside `interface-parity` (one EVM-like, one Solana-like, with known differences) test each outcome:

- an identical item gives a match;
- an item on one side only gives only on EVM or only on Solana;
- a changed signature, struct field, enum variant, JSON value or tag prefix gives differs, with both values;
- an excuse covering a difference gives excused, with its explanation;
- an excuse whose EVM value or Solana value no longer matches gives a stale excuse, and the difference fails again;
- a package with `publish = false` or `"private": true` is not extracted;
- a published package in no pair is reported item by item.

Each test asserts the exact report lines it expects.
