use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, bail};
use serde::Deserialize;

use crate::compare::Line;

/// A reviewed difference: the exact values both sides publish under one key
/// of one pair, and why that difference is accepted.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Excuse {
    pub id: String,
    pub pair: String,
    pub key: String,
    pub evm: Vec<String>,
    pub solana: Vec<String>,
    pub reason: String,
}

impl Excuse {
    pub fn covers(&self, line: &Line) -> bool {
        self.pair == line.pair
            && self.key == line.key
            && self.evm == line.evm
            && self.solana == line.solana
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExcusesFile {
    #[serde(default)]
    excuse: Vec<Excuse>,
}

pub fn load(path: &Path) -> anyhow::Result<Vec<Excuse>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn parse(text: &str) -> anyhow::Result<Vec<Excuse>> {
    let mut excuses = toml::from_str::<ExcusesFile>(text)?.excuse;
    let mut ids = BTreeSet::new();
    for excuse in &mut excuses {
        if !ids.insert(excuse.id.clone()) {
            bail!("excuse id {:?} appears more than once", excuse.id);
        }
        excuse.evm.sort();
        excuse.solana.sort();
    }
    Ok(excuses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::Line;

    const TEXT: &str = r#"
        [[excuse]]
        id = "names"
        pair = "E ↔ S"
        key = "package name"
        evm = ["e"]
        solana = ["s"]
        reason = "Package names differ by chain."
    "#;

    fn line(evm: &[&str], solana: &[&str]) -> Line {
        Line {
            pair: "E ↔ S".into(),
            key: "package name".into(),
            evm: evm.iter().map(|s| s.to_string()).collect(),
            solana: solana.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn an_excuse_covers_only_its_exact_values() {
        let excuses = parse(TEXT).unwrap();
        assert!(excuses[0].covers(&line(&["e"], &["s"])));
        assert!(
            !excuses[0].covers(&line(&["e"], &["s2"])),
            "a changed Solana value must not stay covered"
        );
        assert!(
            !excuses[0].covers(&line(&["e2"], &["s"])),
            "a changed EVM value must not stay covered"
        );
    }

    #[test]
    fn values_are_sorted_on_load_and_duplicate_ids_are_rejected() {
        let unsorted = TEXT.replace(r#"evm = ["e"]"#, r#"evm = ["z", "e"]"#);
        assert_eq!(parse(&unsorted).unwrap()[0].evm, vec!["e", "z"]);
        let twice = format!("{TEXT}{TEXT}");
        let err = parse(&twice).unwrap_err().to_string();
        assert!(err.contains("names"), "{err}");
    }
}
