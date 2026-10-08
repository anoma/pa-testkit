use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};

use crate::compare::{Line, Surface, compare, fold};
use crate::inputs::{InterfaceEntry, PairEntry, Pairs, RepoFile, Side, load_env, load_pins, read};
use crate::packages::{Kind, Package, cargo_metadata_surface, discover};
use crate::report::{Failure, Report};
use crate::{cmd, excuses, fetch, files, generated, interface, rust_api, tags, ts_api};

/// Each repository or package with its side and, unless extraction failed, its surface.
type Surfaces = BTreeMap<String, (Side, Option<Surface>)>;

/// Everything a package publishes: its metadata or manifest, its API and its
/// non-source shipped files. `env` is its repository's build environment and
/// `forge` the Foundry release it pins, which collapses the `forge bind`
/// modules it reproduces into their ABIs. Returns, with the surface, a note
/// for each module left item by item: the report must show why its items are
/// all listed.
pub fn package_surface(
    pkg: &Package,
    work: &Path,
    env: &[(String, String)],
    forge: Option<&Path>,
) -> anyhow::Result<(Surface, Vec<String>)> {
    match &pkg.kind {
        Kind::Cargo(meta) => {
            let mut api = rust_api::api(meta, env)?;
            let notes = match meta.targets.iter().find(|t| t.is_lib()) {
                Some(lib) => {
                    let src = lib.src_path.parent().with_context(|| {
                        format!("{} is a library root with no directory", lib.src_path)
                    })?;
                    generated::collapse_crate(&mut api, src.as_std_path(), forge)?
                }
                None => vec![],
            };
            let mut s = cargo_metadata_surface(meta);
            s.extend(api.surface);
            s.extend(files::cargo_files(pkg, meta, work)?);
            Ok((s, notes))
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
            Ok((s, vec![]))
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
        let env = match &pin.build_env {
            Some(file) => load_env(&dir.join(file))
                .with_context(|| format!("reading the build environment of {}", pin.name))?,
            None => vec![],
        };
        let forge = match &pin.forge {
            Some(foundry) => Some(
                fetch::foundry(foundry, &work.join("tools"))
                    .with_context(|| format!("fetching the Foundry release {} pins", pin.name))?,
            ),
            None => None,
        };
        let (found, discovery_failures) = discover(&pin.name, &dir);
        failures.extend(discovery_failures);
        for pkg in found {
            let surface = match package_surface(&pkg, work, &env, forge.as_deref()) {
                Ok((surface, notes)) => {
                    failures.extend(notes.into_iter().map(|note| Failure {
                        subject: pkg.id.clone(),
                        error: note,
                    }));
                    Some(surface)
                }
                Err(e) => {
                    failures.push(Failure {
                        subject: pkg.id.clone(),
                        error: format!("{e:#}"),
                    });
                    None
                }
            };
            packages.insert(pkg.id, (pin.side, surface));
        }
    }

    // Each interface pair's ABI and IDL, read once. The pair compares its
    // ABI's entries, so the packages of the ABI's repository do not list
    // them again.
    let mut interfaces = vec![];
    for entry in &pairs.interface {
        let evm = interface::abi_file(&interface_file(&entry.abi, Side::Evm, &pins, work)?);
        let solana = interface::idl_file(&interface_file(&entry.idl, Side::Solana, &pins, work)?);
        if let Ok(abi) = &evm {
            let repository = format!("{}/", entry.abi.repository);
            for (_, (_, surface)) in packages
                .iter_mut()
                .filter(|(id, _)| id.starts_with(&repository))
            {
                if let Some(surface) = surface {
                    generated::remove_compared(surface, abi);
                }
            }
        }
        interfaces.push((entry, evm, solana));
    }

    let mut labels = vec![];
    let mut lines = vec![];
    compare_all(
        &pairs.repository,
        &repositories,
        "repository",
        &mut labels,
        &mut lines,
    )?;
    compare_all(
        &pairs.package,
        &packages,
        "package",
        &mut labels,
        &mut lines,
    )?;
    for (entry, evm, solana) in interfaces {
        compare_interface(entry, evm, solana, &mut labels, &mut lines, &mut failures);
    }
    Ok(Report::build(labels, failures, fold(lines), excuses))
}

/// The checkout path under `work` of an interface's file `f`, whose
/// repository must be pinned on `side`.
fn interface_file(
    f: &RepoFile,
    side: Side,
    pins: &[crate::inputs::Pin],
    work: &Path,
) -> anyhow::Result<std::path::PathBuf> {
    match pins.iter().find(|pin| pin.name == f.repository) {
        Some(pin) if pin.side == side => Ok(work.join("repos").join(&f.repository).join(&f.path)),
        Some(_) => bail!(
            "{} is an interface's {side:?} side but pinned on the other",
            f.repository
        ),
        None => bail!(
            "{} is an interface's side but no pinned repository",
            f.repository
        ),
    }
}

/// Compares an EVM contract's ABI with a Solana program's IDL, as read from
/// their pinned repositories' checkouts. A file that could not be read is a
/// failure, and the pair adds no lines.
fn compare_interface(
    entry: &InterfaceEntry,
    evm: anyhow::Result<Surface>,
    solana: anyhow::Result<Surface>,
    labels: &mut Vec<String>,
    lines: &mut Vec<Line>,
    failures: &mut Vec<Failure>,
) {
    let label = format!(
        "{}/{} ↔ {}/{}",
        entry.abi.repository, entry.abi.path, entry.idl.repository, entry.idl.path
    );
    match (evm, solana) {
        (Ok(evm), Ok(solana)) => lines.extend(compare(&label, &evm, &solana)),
        (evm, solana) => {
            for error in [evm.err(), solana.err()].into_iter().flatten() {
                failures.push(Failure {
                    subject: label.clone(),
                    error: format!("{error:#}"),
                });
            }
        }
    }
    labels.push(label);
}

/// Compares each pair, then lists each entry in no pair as one line keyed
/// `subject`: everything it publishes is on its side only, and the line
/// counts it. A name may sit in several pairs. Pairs whose either side failed
/// to extract add no lines; the failure itself is in the report.
fn compare_all(
    pairs: &[PairEntry],
    surfaces: &Surfaces,
    subject: &str,
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
    for (name, (side, surface)) in surfaces {
        if paired.contains(name) {
            continue;
        }
        let label = format!("{name}{}", crate::report::UNPAIRED);
        if let Some(s) = surface {
            let published = vec!["published".to_owned()];
            let (evm, solana) = match side {
                Side::Evm => (published, vec![]),
                Side::Solana => (vec![], published),
            };
            lines.push(Line {
                pair: label.clone(),
                key: subject.to_owned(),
                evm,
                solana,
                inside: s.entries().count(),
            });
        }
        labels.push(label);
    }
    Ok(())
}
