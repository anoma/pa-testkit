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

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pairs {
    #[serde(default)]
    pub repository: Vec<PairEntry>,
    #[serde(default)]
    pub package: Vec<PairEntry>,
}

pub(crate) fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> anyhow::Result<T> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn load_pins(path: &Path) -> anyhow::Result<Vec<Pin>> {
    Ok(read::<PinsFile>(path)?.repository)
}
