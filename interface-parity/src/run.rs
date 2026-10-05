use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};

use crate::compare::{Line, Surface, compare};
use crate::inputs::{PairEntry, Pairs, Side, load_pins, read};
use crate::packages::{Kind, Package, cargo_metadata_surface, discover};
use crate::report::{Failure, Report};
use crate::{cmd, excuses, fetch, files, rust_api, tags, ts_api};

/// Each repository or package with its side and, unless extraction failed, its surface.
type Surfaces = BTreeMap<String, (Side, Option<Surface>)>;

/// Everything a package publishes: its metadata or manifest, its API and its
/// non-source shipped files.
pub fn package_surface(pkg: &Package, work: &Path) -> anyhow::Result<Surface> {
    match &pkg.kind {
        Kind::Cargo(meta) => {
            let mut s = cargo_metadata_surface(meta);
            s.extend(rust_api::surface(meta)?);
            s.extend(files::cargo_files(pkg, meta, work)?);
            Ok(s)
        }
        Kind::Npm(manifest) => {
            // Install, then build the package as publishing would.
            cmd::stdout(Command::new("npm").current_dir(pkg.dir()).args([
                "ci",
                "--ignore-scripts",
                "--no-audit",
                "--no-fund",
            ]))?;
            cmd::stdout(
                Command::new("npm")
                    .current_dir(pkg.dir())
                    .args(["publish", "--dry-run"]),
            )?;
            let mut s = ts_api::exports(pkg, manifest)?;
            s.extend(files::npm_files(pkg, work)?);
            Ok(s)
        }
    }
}

/// Compares the pinned repositories named by `inputs/pins.toml`, paired by
/// `inputs/pairs.toml` and excused by `inputs/excuses.toml`, checking them
/// out under `work`.
pub fn run(inputs: &Path, work: &Path) -> anyhow::Result<Report> {
    let pins = load_pins(&inputs.join("pins.toml"))?;
    let pairs: Pairs = read(&inputs.join("pairs.toml"))?;
    let excuses = excuses::load(&inputs.join("excuses.toml"))?;

    let mut failures = vec![];
    let mut repositories = Surfaces::new();
    let mut packages = Surfaces::new();
    for pin in &pins {
        let dir = work.join("repos").join(&pin.name);
        fetch::checkout(&pin.url, &pin.commit, &dir)
            .with_context(|| format!("checking out {} at {}", pin.name, pin.commit))?;
        let tags =
            fetch::tags(&pin.url).with_context(|| format!("listing tags of {}", pin.name))?;
        repositories.insert(pin.name.clone(), (pin.side, Some(tags::surface(&tags))));
        let (found, discovery_failures) = discover(&pin.name, &dir);
        failures.extend(discovery_failures);
        for pkg in found {
            let surface = package_surface(&pkg, work)
                .map_err(|e| {
                    failures.push(Failure {
                        subject: pkg.id.clone(),
                        error: format!("{e:#}"),
                    })
                })
                .ok();
            packages.insert(pkg.id, (pin.side, surface));
        }
    }

    let mut labels = vec![];
    let mut lines = vec![];
    compare_all(&pairs.repository, &repositories, &mut labels, &mut lines)?;
    compare_all(&pairs.package, &packages, &mut labels, &mut lines)?;
    Ok(Report::build(labels, failures, lines, excuses))
}

/// Compares each pair, then each entry in no pair against nothing. A name may
/// sit in several pairs. Pairs whose either side failed to extract add no
/// lines; the failure itself is in the report.
fn compare_all(
    pairs: &[PairEntry],
    surfaces: &Surfaces,
    labels: &mut Vec<String>,
    lines: &mut Vec<Line>,
) -> anyhow::Result<()> {
    let mut paired = BTreeSet::new();
    for pair in pairs {
        for (name, side) in [(&pair.evm, Side::Evm), (&pair.solana, Side::Solana)] {
            match surfaces.get(name) {
                Some((pinned, _)) if *pinned == side => {}
                Some(_) => bail!("{name} is paired on the {side:?} side but pinned on the other"),
                None => bail!(
                    "{name} is paired but is neither a pinned repository nor a published package of one"
                ),
            }
            paired.insert(name);
        }
        let label = format!("{} ↔ {}", pair.evm, pair.solana);
        if let (Some(evm), Some(solana)) = (&surfaces[&pair.evm].1, &surfaces[&pair.solana].1) {
            lines.extend(compare(&label, evm, solana));
        }
        labels.push(label);
    }
    let empty = Surface::default();
    for (name, (side, surface)) in surfaces {
        if paired.contains(name) {
            continue;
        }
        let label = format!("{name} (unpaired)");
        if let Some(s) = surface {
            lines.extend(match side {
                Side::Evm => compare(&label, s, &empty),
                Side::Solana => compare(&label, &empty, s),
            });
        }
        labels.push(label);
    }
    Ok(())
}
