use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Everything one package or repository publishes: each item's key with the
/// values found under it, kept sorted so two surfaces compare as multisets,
/// and the key of the item each belongs to: an enum's variant its enum, a
/// method its impl, an impl its type, a module's item the module, an ABI
/// entry its `forge bind` module, a JSON value its file. A key can carry
/// several values, e.g. a method several impls give a type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Surface {
    items: BTreeMap<String, Vec<String>>,
    parents: BTreeMap<String, String>,
}

impl Surface {
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let values = self.items.entry(key.into()).or_default();
        values.push(value.into());
        values.sort();
    }

    /// Records that the item under `key` belongs to the one under `parent`.
    pub fn set_parent(&mut self, key: impl Into<String>, parent: impl Into<String>) {
        self.parents.insert(key.into(), parent.into());
    }

    /// The key of the item the one under `key` belongs to.
    pub fn parent(&self, key: &str) -> Option<&str> {
        self.parents.get(key).map(String::as_str)
    }

    /// Removes the Rust items of `module` and of everything inside it: the
    /// module, items under its path, and impls whose type is under it.
    /// Returns how many keys it removed.
    pub fn remove_rust_module(&mut self, module: &str) -> usize {
        let (exact, inside) = (format!("rust {module} "), format!("rust {module}::"));
        let outside = |key: &String| !key.starts_with(&exact) && !key.starts_with(&inside);
        let before = self.items.len();
        self.items.retain(|key, _| outside(key));
        self.parents.retain(|key, _| outside(key));
        before - self.items.len()
    }

    /// Removes one `value` from under `key`, and the key once it holds no
    /// value. Returns whether there was one to remove.
    pub fn remove(&mut self, key: &str, value: &str) -> bool {
        let Some(values) = self.items.get_mut(key) else {
            return false;
        };
        let Some(at) = values.iter().position(|v| v == value) else {
            return false;
        };
        values.remove(at);
        if values.is_empty() {
            self.items.remove(key);
            self.parents.remove(key);
        }
        true
    }

    /// Each key with its values, in key order.
    pub fn entries(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.items.iter()
    }

    pub fn extend(&mut self, other: Surface) {
        for (key, mut values) in other.items {
            let existing = self.items.entry(key).or_default();
            existing.append(&mut values);
            existing.sort();
        }
        self.parents.extend(other.parents);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// The key of the item this one belongs to, on either side.
    pub parent: Option<String>,
    /// How many items this line stands for: those folded into it
    /// ([`fold`]), or everything a repository or package in no pair
    /// publishes.
    pub inside: usize,
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

    /// Whether the line is on one side only.
    fn one_sided(&self) -> bool {
        matches!(self.outcome(), Outcome::OnlyEvm | Outcome::OnlySolana)
    }
}

/// One line per key found on either side.
pub fn compare(pair: &str, evm: &Surface, solana: &Surface) -> Vec<Line> {
    let keys: BTreeSet<&String> = evm.items.keys().chain(solana.items.keys()).collect();
    keys.into_iter()
        .map(|key| Line {
            pair: pair.to_owned(),
            key: key.clone(),
            evm: evm.items.get(key).cloned().unwrap_or_default(),
            solana: solana.items.get(key).cloned().unwrap_or_default(),
            parent: evm.parent(key).or(solana.parent(key)).map(str::to_owned),
            inside: 0,
        })
        .collect()
}

/// Folds each one-sided line into its outermost ancestor that the same side
/// alone has, reached through ancestors that the same side alone has: an
/// item that belongs to something only one side publishes is on that side
/// only too, so the ancestor's line, which counts it, says all it would.
pub fn fold(lines: Vec<Line>) -> Vec<Line> {
    let by_key: HashMap<(&str, &str), &Line> = lines
        .iter()
        .map(|l| ((l.pair.as_str(), l.key.as_str()), l))
        .collect();
    let holder = |line: &Line| -> Option<String> {
        if !line.one_sided() {
            return None;
        }
        let mut holder = None;
        let mut parent = line.parent.as_deref();
        while let Some(key) = parent {
            match by_key.get(&(line.pair.as_str(), key)) {
                Some(p) if p.outcome() == line.outcome() => {
                    holder = Some(key.to_owned());
                    parent = p.parent.as_deref();
                }
                _ => break,
            }
        }
        holder
    };
    let holders: Vec<Option<String>> = lines.iter().map(holder).collect();
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    let mut kept = vec![];
    for (line, holder) in lines.into_iter().zip(holders) {
        match holder {
            Some(key) => *counts.entry((line.pair.clone(), key)).or_default() += 1,
            None => kept.push(line),
        }
    }
    for line in &mut kept {
        line.inside += counts
            .remove(&(line.pair.clone(), line.key.clone()))
            .unwrap_or(0);
    }
    kept
}

/// A line of the pair `E ↔ S`, for tests.
#[cfg(test)]
pub(crate) fn test_line(key: &str, evm: &[&str], solana: &[&str]) -> Line {
    Line {
        pair: "E ↔ S".into(),
        key: key.into(),
        evm: evm.iter().map(|s| s.to_string()).collect(),
        solana: solana.iter().map(|s| s.to_string()).collect(),
        parent: None,
        inside: 0,
    }
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

    /// The pair `E ↔ S`'s line for `key` with the outcome `outcome`, and the
    /// key of the item it belongs to.
    fn line_of(key: &str, outcome: Outcome, parent: Option<&str>) -> Line {
        let mut line = match outcome {
            Outcome::OnlyEvm => test_line(key, &["e"], &[]),
            Outcome::OnlySolana => test_line(key, &[], &["s"]),
            Outcome::Differs => test_line(key, &["e"], &["s"]),
            Outcome::Match => test_line(key, &["x"], &["x"]),
        };
        line.parent = parent.map(str::to_owned);
        line
    }

    /// A one-sided line folds into its outermost ancestor that the same side
    /// alone has, and that line counts it; the chain stops at an ancestor on
    /// both sides or on the other side. Lines on both sides never fold, and
    /// a count goes to its ancestor's own line, not to another line of the
    /// same path.
    #[test]
    fn a_one_sided_line_folds_into_its_outermost_one_sided_ancestor() {
        use Outcome::{Differs, Match, OnlyEvm, OnlySolana};
        let lines = vec![
            line_of("rust crate::m mod", OnlySolana, None),
            line_of(
                "rust crate::m::S struct",
                OnlySolana,
                Some("rust crate::m mod"),
            ),
            line_of(
                "rust crate::m::S impl",
                OnlySolana,
                Some("rust crate::m::S struct"),
            ),
            line_of(
                "rust crate::m::S::f fn",
                OnlySolana,
                Some("rust crate::m::S impl"),
            ),
            line_of("rust crate::E enum", OnlySolana, Some("rust crate mod")),
            line_of("rust crate::E fn", OnlySolana, Some("rust crate mod")),
            line_of(
                "rust crate::E::A member",
                OnlySolana,
                Some("rust crate::E enum"),
            ),
            line_of(
                "rust crate::E::B member",
                OnlyEvm,
                Some("rust crate::E enum"),
            ),
            line_of("rust crate::E::d fn", Differs, Some("rust crate::E enum")),
            line_of("rust crate mod", Match, None),
            line_of("rust crate::T trait", Match, Some("rust crate mod")),
            line_of(
                "rust crate::T::g fn",
                OnlySolana,
                Some("rust crate::T trait"),
            ),
        ];

        let folded = fold(lines);

        let got: Vec<(&str, usize)> = folded.iter().map(|l| (l.key.as_str(), l.inside)).collect();
        assert_eq!(
            got,
            vec![
                ("rust crate::m mod", 3),
                ("rust crate::E enum", 1),
                ("rust crate::E fn", 0),
                ("rust crate::E::B member", 0),
                ("rust crate::E::d fn", 0),
                ("rust crate mod", 0),
                ("rust crate::T trait", 0),
                ("rust crate::T::g fn", 0),
            ],
            "{folded:#?}"
        );
    }

    #[test]
    fn a_line_carries_the_parent_its_side_records() {
        let mut evm = surface(&[("a::x", "1"), ("a", "1")]);
        evm.set_parent("a::x", "a");
        let solana = surface(&[("a", "1")]);
        let lines = compare("p", &evm, &solana);
        let x = lines.iter().find(|l| l.key == "a::x").unwrap();
        assert_eq!(x.parent.as_deref(), Some("a"), "{x:#?}");
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
