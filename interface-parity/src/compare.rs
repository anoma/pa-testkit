use std::collections::{BTreeMap, BTreeSet};

/// Everything one package or repository publishes: each item's key with the
/// values found under it, kept sorted so two surfaces compare as multisets.
/// A key can carry several values, e.g. a method several impls give a type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Surface(BTreeMap<String, Vec<String>>);

impl Surface {
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let values = self.0.entry(key.into()).or_default();
        values.push(value.into());
        values.sort();
    }

    pub fn extend(&mut self, other: Surface) {
        for (key, values) in other.0 {
            for value in values {
                self.insert(key.clone(), value);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Outcome {
    Match,
    OnlyEvm,
    OnlySolana,
    Differs,
}

/// One key of a pair: the values each side publishes under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub pair: String,
    pub key: String,
    pub evm: Vec<String>,
    pub solana: Vec<String>,
}

impl Line {
    pub fn outcome(&self) -> Outcome {
        match (self.evm.is_empty(), self.solana.is_empty()) {
            (false, true) => Outcome::OnlyEvm,
            (true, false) => Outcome::OnlySolana,
            _ if self.evm == self.solana => Outcome::Match,
            _ => Outcome::Differs,
        }
    }
}

/// One line per key found on either side.
pub fn compare(pair: &str, evm: &Surface, solana: &Surface) -> Vec<Line> {
    let keys: BTreeSet<&String> = evm.0.keys().chain(solana.0.keys()).collect();
    keys.into_iter()
        .map(|key| Line {
            pair: pair.to_owned(),
            key: key.clone(),
            evm: evm.0.get(key).cloned().unwrap_or_default(),
            solana: solana.0.get(key).cloned().unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface(items: &[(&str, &str)]) -> Surface {
        let mut s = Surface::default();
        for (k, v) in items {
            s.insert(*k, *v);
        }
        s
    }

    #[test]
    fn each_key_of_either_side_gets_one_line_with_its_outcome() {
        let evm = surface(&[("a", "1"), ("b", "1"), ("d", "x")]);
        let solana = surface(&[("a", "1"), ("b", "2"), ("c", "1")]);
        let lines = compare("E ↔ S", &evm, &solana);
        let got: Vec<(&str, Outcome)> = lines
            .iter()
            .map(|l| (l.key.as_str(), l.outcome()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("a", Outcome::Match),
                ("b", Outcome::Differs),
                ("c", Outcome::OnlySolana),
                ("d", Outcome::OnlyEvm),
            ],
            "lines: {lines:#?}"
        );
        assert_eq!(lines[1].evm, vec!["1"]);
        assert_eq!(lines[1].solana, vec!["2"]);
    }

    #[test]
    fn values_under_one_key_compare_as_a_sorted_multiset() {
        let evm = surface(&[("k", "y"), ("k", "x"), ("k", "x")]);
        let same = surface(&[("k", "x"), ("k", "y"), ("k", "x")]);
        let fewer = surface(&[("k", "x"), ("k", "y")]);
        assert_eq!(compare("p", &evm, &same)[0].outcome(), Outcome::Match);
        let line = &compare("p", &evm, &fewer)[0];
        assert_eq!(line.outcome(), Outcome::Differs, "{line:#?}");
        assert_eq!(line.evm, vec!["x", "x", "y"]);
    }
}
