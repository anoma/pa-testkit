use std::collections::HashMap;
use std::fmt::Write;

use crate::compare::{Line, Outcome};
use crate::excuses::Excuse;

/// A package or repository whose surface could not be extracted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub subject: String,
    pub error: String,
}

#[derive(Debug)]
pub struct Report {
    pub pairs: Vec<String>,
    pub failures: Vec<Failure>,
    pub unexcused: Vec<Line>,
    pub stale: Vec<Excuse>,
    pub excused: Vec<(Line, Excuse)>,
    pub matches: Vec<Line>,
}

impl Report {
    pub fn build(
        pairs: Vec<String>,
        failures: Vec<Failure>,
        lines: Vec<Line>,
        excuses: Vec<Excuse>,
    ) -> Report {
        // Excuses are unique per pair and key, so one lookup finds the only
        // excuse that could cover a line.
        let by_line: HashMap<(&str, &str), usize> = excuses
            .iter()
            .enumerate()
            .map(|(i, e)| ((e.pair.as_str(), e.key.as_str()), i))
            .collect();
        let mut used = vec![false; excuses.len()];
        let (mut unexcused, mut excused, mut matches) = (vec![], vec![], vec![]);
        for line in lines {
            let covering = by_line
                .get(&(line.pair.as_str(), line.key.as_str()))
                .filter(|&&i| excuses[i].covers(&line))
                .copied();
            if line.outcome() == Outcome::Match {
                matches.push(line);
            } else if let Some(i) = covering {
                used[i] = true;
                excused.push((line, excuses[i].clone()));
            } else {
                unexcused.push(line);
            }
        }
        let stale = excuses
            .into_iter()
            .zip(used)
            .filter(|(_, used)| !used)
            .map(|(e, _)| e)
            .collect();
        Report {
            pairs,
            failures,
            unexcused,
            stale,
            excused,
            matches,
        }
    }

    pub fn passes(&self) -> bool {
        self.failures.is_empty() && self.unexcused.is_empty() && self.stale.is_empty()
    }

    /// The sections that make the test fail.
    pub fn failing_text(&self) -> String {
        let mut out = String::new();
        write_failures(&mut out, &self.failures);
        write_lines(
            &mut out,
            "Unexcused differences",
            self.unexcused.iter().map(|l| (l, None)),
        );
        write_stale(&mut out, &self.stale);
        out
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::from("# Interface parity report\n\n## Pairs\n\n");
        for pair in &self.pairs {
            writeln!(out, "- {pair}").unwrap();
        }
        out.push('\n');
        out.push_str(&self.failing_text());
        write_lines(
            &mut out,
            "Excused",
            self.excused.iter().map(|(l, e)| (l, Some(e))),
        );
        write_lines(&mut out, "Matches", self.matches.iter().map(|l| (l, None)));
        out
    }
}

fn write_failures(out: &mut String, failures: &[Failure]) {
    writeln!(out, "## Extraction failures ({})\n", failures.len()).unwrap();
    for f in failures {
        writeln!(
            out,
            "### {}\n\n```\n{}\n```\n",
            f.subject,
            f.error.trim_end()
        )
        .unwrap();
    }
}

fn write_stale(out: &mut String, stale: &[Excuse]) {
    writeln!(out, "## Stale excuses ({})\n", stale.len()).unwrap();
    for e in stale {
        writeln!(
            out,
            "- `{}` ({}, key `{}`): {}",
            e.id, e.pair, e.key, e.reason
        )
        .unwrap();
    }
    out.push('\n');
}

fn label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Match => "match",
        Outcome::OnlyEvm => "only on EVM",
        Outcome::OnlySolana => "only on Solana",
        Outcome::Differs => "differs",
    }
}

fn write_lines<'a>(
    out: &mut String,
    heading: &str,
    lines: impl Iterator<Item = (&'a Line, Option<&'a Excuse>)>,
) {
    let lines: Vec<_> = lines.collect();
    writeln!(out, "## {heading} ({})\n\n```", lines.len()).unwrap();
    let mut pair = None;
    for (line, excuse) in lines {
        if pair != Some(&line.pair) {
            writeln!(out, "== {}", line.pair).unwrap();
            pair = Some(&line.pair);
        }
        writeln!(out, "{}  {}", label(line.outcome()), line.key).unwrap();
        for v in &line.evm {
            writeln!(out, "    EVM:    {v}").unwrap();
        }
        for v in &line.solana {
            writeln!(out, "    Solana: {v}").unwrap();
        }
        if let Some(e) = excuse {
            writeln!(out, "    excused by {}: {}", e.id, e.reason).unwrap();
        }
    }
    out.push_str("```\n\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::test_line as line;
    use crate::excuses::parse;

    const EXCUSES: &str = r#"
        [[excuse]]
        id = "covers-b"
        pair = "E ↔ S"
        key = "b"
        evm = ["1"]
        solana = ["2"]
        reason = "b differs on purpose."

        [[excuse]]
        id = "stale"
        pair = "E ↔ S"
        key = "gone"
        evm = ["1"]
        solana = []
        reason = "No longer applies."
    "#;

    fn report() -> Report {
        Report::build(
            vec!["E ↔ S".into()],
            vec![],
            vec![
                line("a", &["1"], &["1"]),
                line("b", &["1"], &["2"]),
                line("c", &["1"], &[]),
            ],
            parse(EXCUSES).unwrap(),
        )
    }

    #[test]
    fn lines_are_split_into_matches_excused_and_unexcused_and_stale_excuses_are_found() {
        let r = report();
        let keys = |lines: &[Line]| lines.iter().map(|l| l.key.clone()).collect::<Vec<_>>();
        assert_eq!(keys(&r.matches), vec!["a"]);
        assert_eq!(r.excused.len(), 1);
        assert_eq!(
            (r.excused[0].0.key.as_str(), r.excused[0].1.id.as_str()),
            ("b", "covers-b")
        );
        assert_eq!(keys(&r.unexcused), vec!["c"]);
        assert_eq!(
            r.stale.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["stale"]
        );
        assert!(!r.passes());
    }

    #[test]
    fn a_report_passes_only_without_unexcused_lines_stale_excuses_or_failures() {
        let clean = Report::build(vec![], vec![], vec![line("a", &["1"], &["1"])], vec![]);
        assert!(clean.passes());
        let failed = Report::build(
            vec![],
            vec![Failure {
                subject: "evm/cargo:x".into(),
                error: "boom".into(),
            }],
            vec![],
            vec![],
        );
        assert!(!failed.passes());
        assert!(
            failed.failing_text().contains("boom"),
            "{}",
            failed.failing_text()
        );
    }

    #[test]
    fn markdown_keeps_excused_lines_apart_from_matches() {
        let md = report().to_markdown();
        let excused = md.find("## Excused").expect(&md);
        let matches = md.find("## Matches").expect(&md);
        let unexcused = md.find("## Unexcused differences").expect(&md);
        assert!(unexcused < excused && excused < matches, "{md}");
        assert!(
            md[excused..matches].contains("covers-b")
                && md[excused..matches].contains("b differs on purpose."),
            "{md}"
        );
        assert!(!md[matches..].contains("covers-b"), "{md}");
    }
}
