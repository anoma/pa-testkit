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

- **`pins.toml`** names each repository by URL and exact commit. The test compares those commits and nothing else. Comparing newer code means changing a pin in a commit, and the report for that commit shows every difference the change introduced. A pin may name a `build_env` file in its repository, the `KEY=VALUE` lines the repository's builds read into their environment; the Solana programs take their addresses with `env!`, so their rustdoc is built with them.
- **`pairs.toml`** declares which EVM repository corresponds to which Solana repository, and which EVM package to which Solana package. Pairing is needed because items pair by literal path inside a package, and package names differ across chains (`anoma-pa-evm-bindings` vs `anoma-pa-solana-client`). A published package that appears in no pair is still reported, item by item, as only on its side. It also declares each `interface` pair: an EVM contract's ABI and the Solana program's Anchor IDL, each a file in a pinned repository (see below). The report prints the pairs first.
- **`excuses.toml`** holds one entry per accepted difference: an id, the pair, the item's key, the exact EVM values, the exact Solana values, and the reason it is accepted. An entry covers that exact difference only. If either side later changes the value, the entry stops matching, so the line fails again and the entry is reported as stale.

## What counts as a repository's external interface

The published packages are read from each repository's own manifests, with no list maintained here:

- every Cargo package whose manifest does not declare `publish = false`, found with `cargo metadata --no-deps` on every `Cargo.toml` in the repository;
- every npm package whose `package.json` has a `name` and does not declare `"private": true` (npm refuses to publish a package without a name).

This rule is provisional. The repositories do not set `publish = false` consistently, so some packages it counts as published are test or tooling crates. The report shows them, and a review that finds the rule wrong changes the rule.

For each published package, the test extracts:

1. **Package metadata:** name, version, features and their contents, and dependencies with their version requirements.
2. **Rust public API** of the package's library target (a package with no library target exposes no Rust items): one item per line from the `public-api` crate, the library behind `cargo public-api`, reading rustdoc's JSON output built with all features enabled by the pinned toolchain `nightly-2026-02-08`, which `just install-nightly` installs; a missing toolchain is an error, never an implicit install. Functions, struct fields, enum variants, constants and trait impls, including the blanket impls a type gets from its dependencies, are each their own item, at every public path, re-exports included. An item's key is its kind and its path with the crate's own name replaced by `crate`; a trait impl's key is its whole rendering. Its value is its whole rendering, with the crate's own name replaced by `crate` wherever it starts a path.
3. **TypeScript exports** of the package's types entry, the file TypeScript resolves an import of the package to (through its `exports` map by its own name when it has one, else as its directory), after `npm ci` and `npm publish --dry-run` have built the package as publishing would: each export with its type, and each property of an exported interface, class or enum as its own item, read with the package's own TypeScript compiler. Absolute paths in rendered types are rewritten relative to the package directory.
4. **Shipped files** other than source files, whose content items 2 and 3 already cover: `.rs` files for a crate; `.ts`, `.js`, `.mjs`, `.cjs` and `.map` files for an npm package. Files are read from the archive publishing would upload: the `.crate` that `cargo package --no-verify` builds, or the tarball `npm pack` builds after `npm publish --dry-run` has run the package's build scripts. Cargo's normalized `Cargo.toml`, `Cargo.toml.orig`, `Cargo.lock` and `.cargo_vcs_info.json` are compared like any other file. Files pair by path within the package. JSON files such as `deployments.json` are compared key by key, so a difference names the JSON path; any other file is compared byte for byte.
5. **Program interfaces** of each `interface` pair: the EVM contract's JSON ABI (a `.json` file, or the one `forge bind` embeds in the bindings it generates) and the Solana program's Anchor IDL, as each chain's client package ships them. Rust items cannot pair a contract's generated bindings with a hand-written client, so the contract and the program compare in one vocabulary instead: `fn` (a function or instruction with its arguments and return), `event`, `error`, `type` (a struct or enum the others use) and `account` (a Solana account layout, which has no EVM counterpart). Names compare in snake_case where the languages' conventions differ, events without the `Event` suffix the Solana programs give theirs, and types in one notation (`bytes32` for Solidity's `bytes32` and Anchor's `[u8; 32]`, `address` for Solidity's `address` and Anchor's `pubkey`; an Anchor type alias reads as the type it stands for, so a field of an alias of `u256` matches a `uint256`). A Solana instruction's accounts, an EVM event field's `indexed` flag and an EVM error's fields have no counterpart on the other chain and are left out.
6. **Tag schemes of the repository:** each remote tag is split into a prefix and a semver version (`bindings/v3.0.0` gives `bindings/v`). Paired repositories are compared by their sets of prefixes.

A package that fails to extract, for example because it does not compile with all features enabled, is reported as a failing line carrying the error output, and every other package is still compared. At the first pin this happens to four packages of anoma/solana-protocol-adapter: `protocol-adapter`, `spl-token-forwarder` and `test-forwarder`, whose `cpi` feature does not compile, and `passthrough-logic-methods`, whose build script builds a RISC0 guest and panics.

## Repositories at the first pin

The pinned repositories and commits are in `interface-parity/pins.toml`, and the pairs in `interface-parity/pairs.toml`. The EVM side is V2, which lives on `next` in all three EVM repositories (anoma/pa-evm, anoma/anomapay-erc20-forwarder and anoma/forwarder-bases); the Solana side is anoma/anoma-pa-solana-client and anoma/solana-protocol-adapter.

One Solana package or repository may appear in several pairs, and each pair is compared on its own. At the first pin, one Solana client and one Solana repository still serve both the adapter and the forwarder. anoma/forwarder-bases and its package `anoma-forwarder-bases-bindings` start unpaired, so all of their items are reported as only on EVM.

The same work splits the SPL token forwarder into its own repository, anoma/anomapay-spl-token-forwarder, with its program, its client crate and npm package, and its deployment record (anoma/dos-pm#90). After the split, a repin changes the forwarder pairs to anoma/anomapay-erc20-forwarder ↔ anoma/anomapay-spl-token-forwarder and `anomapay-erc20-forwarder-bindings` ↔ the new repository's client crate.

The first run will report tens of thousands of unexcused differences. At its pin, `anoma-pa-evm-bindings` alone has 47,087 public items, most of them generated contract bindings and the trait methods every type gets from blanket impls; `anoma-pa-solana-client` has 2,680. The Solana adapter repository declares no `publish = false`, so its test programs and `fixture-gen` count as published and are reported as only on Solana. Neither Solana repository has tags yet. The test reports all of these; deciding which become excuses and which become Solana changes is the review.

## Where it lives

Everything lives in pa-testkit, which holds every chain-agnostic test. pa-testkit becomes a Cargo workspace with two members: the existing `anoma-pa-testkit` crate, unchanged, and `interface-parity`, declared `publish = false`, which holds the comparison library, its own tests, the EVM-to-Solana comparison test and the three input files. Consumers of the testkit library never build the comparison's dependencies.

pa-testkit's CI runs only the tool's own tests on its fixture packages (see below). The EVM-to-Solana comparison test carries `#[ignore = "compares the pinned EVM and Solana repositories; run with just interface-parity"]`, so `cargo test` and CI skip it by that explicit filter, and the recipe `just interface-parity` runs it with `cargo test -p interface-parity --test evm_solana -- --ignored`. It runs when the pins, pairs or excuses change and for each review.

The test needs network access to fetch the pinned commits, a pinned nightly toolchain for rustdoc JSON, and Node for the TypeScript extraction. The tool runs that nightly's `rustc` and `rustdoc` explicitly, because rustdoc JSON fails when the dependencies were compiled by another `rustc`. It writes the full report to `target/interface-parity/report.md` and prints the failing lines.

## Testing the comparison itself

Small fixture repositories inside `interface-parity` (one EVM-like and one Solana-like with known differences, and one whose package does not compile) test each outcome, each asserting the outcome and the values of the lines it checks.
