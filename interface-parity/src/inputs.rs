use std::path::Path;

use anyhow::Context;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Evm,
    Solana,
}

/// A repository compared at exactly one commit.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub name: String,
    pub side: Side,
    pub url: String,
    pub commit: String,
    /// A file of `KEY=VALUE` lines, relative to the repository, that the
    /// repository's builds read into their environment (a Solana program's
    /// address, which it takes with `env!`).
    #[serde(default)]
    pub build_env: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinsFile {
    repository: Vec<Pin>,
}

/// One EVM repository or package and the Solana one compared with it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairEntry {
    pub evm: String,
    pub solana: String,
}

/// A file of a pinned repository.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoFile {
    pub repository: String,
    pub path: String,
}

/// An EVM contract's ABI and the Solana program's IDL compared with it
/// (`interface`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterfaceEntry {
    pub abi: RepoFile,
    pub idl: RepoFile,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pairs {
    #[serde(default)]
    pub repository: Vec<PairEntry>,
    #[serde(default)]
    pub package: Vec<PairEntry>,
    #[serde(default)]
    pub interface: Vec<InterfaceEntry>,
}

/// The `KEY=VALUE` lines of a build environment file; blank lines and `#`
/// comments are skipped.
pub fn load_env(path: &Path) -> anyhow::Result<Vec<(String, String)>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (key, value) = line
                .split_once('=')
                .with_context(|| format!("{}: {line:?} is not KEY=VALUE", path.display()))?;
            Ok((key.trim().to_owned(), value.trim().to_owned()))
        })
        .collect()
}

pub(crate) fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> anyhow::Result<T> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn load_pins(path: &Path) -> anyhow::Result<Vec<Pin>> {
    Ok(read::<PinsFile>(path)?.repository)
}
