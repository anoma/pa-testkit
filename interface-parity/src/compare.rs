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

    /// Removes the Rust items of `module` and of everything inside it: the
    /// module, items under its path, and impls whose type is under it.
    /// Returns how many keys it removed.
    pub fn remove_rust_module(&mut self, module: &str) -> usize {
        let (exact, inside) = (format!("rust {module} "), format!("rust {module}::"));
        let before = self.0.len();
        self.0
            .retain(|key, _| !key.starts_with(&exact) && !key.starts_with(&inside));
        before - self.0.len()
    }

    /// Removes one `value` from under `key`, and the key once it holds no
    /// value. Returns whether there was one to remove.
    pub fn remove(&mut self, key: &str, value: &str) -> bool {
        let Some(values) = self.0.get_mut(key) else {
            return false;
        };
        let Some(at) = values.iter().position(|v| v == value) else {
            return false;
        };
        values.remove(at);
        if values.is_empty() {
            self.0.remove(key);
        }
        true
    }

    /// The values under `key`.
    pub fn get(&self, key: &str) -> Option<&Vec<String>> {
        self.0.get(key)
    }

    /// Keeps the keys, with their values, for which `keep` holds.
    pub fn retain(&mut self, mut keep: impl FnMut(&str, &[String]) -> bool) {
        self.0.retain(|key, values| keep(key, values));
    }

    /// Each key with its values, in key order.
    pub fn entries(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.0.iter()
    }

    pub fn extend(&mut self, other: Surface) {
        for (key, mut values) in other.0 {
            let existing = self.0.entry(key).or_default();
            existing.append(&mut values);
            existing.sort();
        }
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
    /// How many items inside this one-sided container were folded into it
    /// ([`fold`]).
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
            inside: 0,
        })
        .collect()
}

/// The Rust item kinds that hold other items.
const CONTAINERS: &[&str] = &["mod", "struct", "enum", "union", "trait"];

/// A Rust key's path and whether it names an impl: `rust <path> <kind>`, or
/// `rust <self type> impl <trait>` with the self type's generics dropped.
fn rust_path(key: &str) -> Option<(&str, bool)> {
    let rest = key.strip_prefix("rust ")?;
    let (path, tail) = rest.split_once(' ')?;
    let is_impl = tail == "impl" || tail.starts_with("impl ");
    let path = if is_impl {
        path.split_once('<').map_or(path, |(path, _)| path)
    } else {
        path
    };
    Some((path, is_impl))
}

/// Folds each line into the outermost container of its pair that only the
/// same side has and that holds it: its members and the items under its path,
/// and the impls on it. Such an item is necessarily on that side only too, so
/// the container's line, which counts it, says all it would.
pub fn fold(lines: Vec<Line>) -> Vec<Line> {
    let containers: BTreeSet<(String, bool, String)> = lines
        .iter()
        .filter(|l| matches!(l.outcome(), Outcome::OnlyEvm | Outcome::OnlySolana))
        .filter_map(|l| {
            let (path, is_impl) = rust_path(&l.key)?;
            let kind = l.key.rsplit(' ').next()?;
            (!is_impl && CONTAINERS.contains(&kind)).then(|| {
                (
                    l.pair.clone(),
                    l.outcome() == Outcome::OnlyEvm,
                    path.to_owned(),
                )
            })
        })
        .collect();
    // The outermost container holding `line`, by its path.
    let holder = |line: &Line| -> Option<String> {
        let side = match line.outcome() {
            Outcome::OnlyEvm => true,
            Outcome::OnlySolana => false,
            _ => return None,
        };
        let (path, is_impl) = rust_path(&line.key)?;
        let segments: Vec<&str> = path.split("::").collect();
        // An impl is held by its self type; any other item only by a
        // container strictly above it.
        let deepest = if is_impl {
            segments.len()
        } else {
            segments.len() - 1
        };
        (1..=deepest)
            .map(|n| segments[..n].join("::"))
            .find(|prefix| containers.contains(&(line.pair.clone(), side, prefix.clone())))
    };
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut kept = vec![];
    for line in lines {
        match holder(&line) {
            Some(path) => *counts.entry((line.pair.clone(), path)).or_default() += 1,
            None => kept.push(line),
        }
    }
    for line in &mut kept {
        if let Some((path, false)) = rust_path(&line.key) {
            line.inside = counts
                .remove(&(line.pair.clone(), path.to_owned()))
                .unwrap_or(0);
        }
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

    /// The pair `E ↔ S`'s lines for `keys`, each only on the side named.
    fn one_sided(keys: &[(&str, Outcome)]) -> Vec<Line> {
        keys.iter()
            .map(|(key, side)| match side {
                Outcome::OnlyEvm => test_line(key, &["e"], &[]),
                Outcome::OnlySolana => test_line(key, &[], &["s"]),
                _ => test_line(key, &["e"], &["s"]),
            })
            .collect()
    }

    /// An item inside a container that only one side has is only on that
    /// side too, so it folds into the outermost such container's count: its
    /// members and the items under its path, and impls on it, generic or
    /// not. Items on the other side, items that differ, and a container
    /// whose name only starts the same stay.
    #[test]
    fn items_inside_a_one_sided_container_fold_into_its_count() {
        use Outcome::{Differs, OnlyEvm, OnlySolana};
        let lines = one_sided(&[
            ("rust crate::E enum", OnlySolana),
            ("rust crate::E::A member", OnlySolana),
            ("rust crate::E impl core::marker::Send", OnlySolana),
            ("rust crate::E::B member", OnlyEvm),
            ("rust crate::E::d fn", Differs),
            ("rust crate::Ex struct", OnlySolana),
            ("rust crate::G struct", OnlySolana),
            ("rust crate::G<T> impl core::clone::Clone", OnlySolana),
            ("rust crate::m mod", OnlySolana),
            ("rust crate::m::S struct", OnlySolana),
            ("rust crate::m::S::f fn", OnlySolana),
            ("rust crate::m::S impl core::marker::Sync", OnlySolana),
            ("fn settle", OnlySolana),
        ]);

        let folded = fold(lines);

        let got: Vec<(&str, usize)> = folded.iter().map(|l| (l.key.as_str(), l.inside)).collect();
        assert_eq!(
            got,
            vec![
                ("rust crate::E enum", 2),
                ("rust crate::E::B member", 0),
                ("rust crate::E::d fn", 0),
                ("rust crate::Ex struct", 0),
                ("rust crate::G struct", 1),
                ("rust crate::m mod", 3),
                ("fn settle", 0),
            ],
            "{folded:#?}"
        );
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
