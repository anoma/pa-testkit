use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;
use sha2::{Digest, Sha256};

use crate::cmd;
use crate::compare::Surface;
use crate::packages::Package;

/// Extensions of the files whose content the Rust and TypeScript API
/// extractions already cover.
const CARGO_SOURCES: &[&str] = &[".rs"];
const NPM_SOURCES: &[&str] = &[".ts", ".js", ".mjs", ".cjs", ".map"];

/// The files cargo writes into a package archive to describe it: its record
/// of the manifest, the commit and the lock.
const CARGO_DESCRIPTIONS: &[&str] = &[
    ".cargo_vcs_info.json",
    "Cargo.lock",
    "Cargo.toml",
    "Cargo.toml.orig",
];

/// The key under which a file that describes the package rather than ships
/// with it compares, by presence: cargo's `descriptions`, and a readme (any
/// root file whose name starts with `README`, in any case, as npm counts
/// one). Their content cannot match across two repositories, and the
/// manifest compares field by field elsewhere. None for any other file.
fn description_key(path: &str, descriptions: &[&str]) -> Option<String> {
    if descriptions.contains(&path) {
        Some(format!("file {path}"))
    } else if !path.contains('/') && path.to_ascii_uppercase().starts_with("README") {
        Some("file README".to_owned())
    } else {
        None
    }
}

/// The file `prefix` as an item present, and, belonging to it, one item per
/// JSON scalar and per empty array or object, keyed by its JSON pointer
/// (RFC 6901), so a difference names the exact place in the file.
fn flatten_json(prefix: &str, value: &serde_json::Value, s: &mut Surface) {
    s.insert(prefix, "present");
    fn walk(prefix: &str, pointer: String, value: &serde_json::Value, s: &mut Surface) {
        match value {
            serde_json::Value::Object(map) if !map.is_empty() => {
                for (k, v) in map {
                    let token = k.replace('~', "~0").replace('/', "~1");
                    walk(prefix, format!("{pointer}/{token}"), v, s);
                }
            }
            serde_json::Value::Array(items) if !items.is_empty() => {
                for (i, v) in items.iter().enumerate() {
                    walk(prefix, format!("{pointer}/{i}"), v, s);
                }
            }
            other => {
                let key = format!("{prefix}#{pointer}");
                s.insert(&key, other.to_string());
                s.set_parent(key, prefix);
            }
        }
    }
    walk(prefix, String::new(), value, s);
}

/// Every file under `dir` into `found`, in name order, not descending into a
/// directory for which `skip_dir` holds. A symlink is never followed, so a
/// symlinked directory is listed as a file.
pub(crate) fn walk(
    dir: &Path,
    skip_dir: &impl Fn(&Path) -> bool,
    found: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    let mut entries = std::fs::read_dir(dir)
        .and_then(|entries| entries.collect::<Result<Vec<_>, _>>())
        .with_context(|| format!("reading {}", dir.display()))?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("reading the type of {}", path.display()))?;
        if !file_type.is_dir() {
            found.push(path);
        } else if !skip_dir(&path) {
            walk(&path, skip_dir, found)?;
        }
    }
    Ok(())
}

/// The non-source files of an unpacked package: those describing it
/// ([`description_key`], with cargo's `descriptions`) by presence, other JSON
/// files key by key, any other file by its SHA-256.
fn unpacked_surface(
    root: &Path,
    sources: &[&str],
    descriptions: &[&str],
) -> anyhow::Result<Surface> {
    let mut paths = vec![];
    walk(root, &|_| false, &mut paths)?;
    let mut s = Surface::default();
    for full in paths {
        let path = full
            .strip_prefix(root)?
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if sources.iter().any(|ext| path.ends_with(ext)) {
            continue;
        }
        if let Some(key) = description_key(&path, descriptions) {
            s.insert(key, "present");
            continue;
        }
        let bytes = std::fs::read(&full).with_context(|| format!("reading {}", full.display()))?;
        if path.ends_with(".json") {
            let value: serde_json::Value = serde_json::from_slice(&bytes)
                .with_context(|| format!("parsing {}", full.display()))?;
            flatten_json(&format!("file {path}"), &value, &mut s);
        } else {
            s.insert(
                format!("file {path}"),
                format!("sha256 {}", hex::encode(Sha256::digest(&bytes))),
            );
        }
    }
    Ok(s)
}

/// Unpacks a `.crate` or `.tgz` archive into a fresh `dir`.
fn unpack(archive: &Path, dir: &Path) -> anyhow::Result<()> {
    cmd::fresh_dir(dir)?;
    cmd::stdout(
        Command::new("tar")
            .arg("-xzf")
            .arg(archive)
            .arg("-C")
            .arg(dir),
    )?;
    Ok(())
}

/// A directory under `work` for one package.
fn package_dir(work: &Path, pkg: &Package, what: &str) -> PathBuf {
    work.join(what).join(pkg.id.replace(['/', ':', '@'], "_"))
}

/// Whether a registry can take the crate: cargo packages a crate only when
/// every dependency it keeps names a version, and a git or path dependency
/// without one leaves the crate to consumers who take it from git, with no
/// archive. Development dependencies do not count; packaging drops those
/// without a version.
fn registry_can_take(meta: &cargo_metadata::Package) -> bool {
    meta.dependencies.iter().all(|dep| {
        dep.kind == cargo_metadata::DependencyKind::Development
            || dep.req != semver::VersionReq::STAR
            || dep.source.as_ref().is_some_and(|source| {
                source.repr.starts_with("registry+") || source.repr.starts_with("sparse+")
            })
    })
}

/// The non-source files of the `.crate` archive `cargo package` builds; none
/// for a crate a registry cannot take ([`registry_can_take`]).
pub fn cargo_files(
    pkg: &Package,
    meta: &cargo_metadata::Package,
    work: &Path,
) -> anyhow::Result<Surface> {
    if !registry_can_take(meta) {
        return Ok(Surface::default());
    }
    let target = package_dir(work, pkg, "cargo-package");
    cmd::stdout(
        Command::new("cargo")
            .args(["package", "--no-verify", "--allow-dirty", "--manifest-path"])
            .arg(&pkg.manifest)
            .arg("--target-dir")
            .arg(&target),
    )?;
    let stem = format!("{}-{}", meta.name, meta.version);
    let unpacked = package_dir(work, pkg, "unpacked");
    unpack(
        &target.join("package").join(format!("{stem}.crate")),
        &unpacked,
    )?;
    unpacked_surface(&unpacked.join(stem), CARGO_SOURCES, CARGO_DESCRIPTIONS)
}

/// The non-source files of the tarball `npm pack` builds from the package's
/// current state; the package must already be built.
pub fn npm_files(pkg: &Package, work: &Path) -> anyhow::Result<Surface> {
    let destination = package_dir(work, pkg, "npm-pack");
    std::fs::create_dir_all(&destination)
        .with_context(|| format!("creating {}", destination.display()))?;
    let packed = cmd::stdout(
        Command::new("npm")
            .current_dir(pkg.dir())
            .args(["pack", "--json", "--ignore-scripts", "--pack-destination"])
            .arg(&destination),
    )?;
    let packed: serde_json::Value = serde_json::from_str(&packed)
        .with_context(|| format!("parsing npm pack --json: {packed}"))?;
    let filename = packed[0]["filename"]
        .as_str()
        .with_context(|| format!("npm pack --json named no tarball: {packed}"))?;
    let unpacked = package_dir(work, pkg, "unpacked");
    unpack(&destination.join(filename), &unpacked)?;
    unpacked_surface(&unpacked.join("package"), NPM_SOURCES, &[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::compare;

    #[test]
    fn json_flattens_to_the_file_and_one_key_per_scalar_and_empty_container_in_it() {
        let mut s = Surface::default();
        flatten_json(
            "file d.json",
            &serde_json::json!({"staging": [{"chainId": 1}], "production": [], "a/b": {"~": true}}),
            &mut s,
        );
        let lines = compare("p", &s, &Surface::default());
        let got: Vec<(&str, &str)> = lines
            .iter()
            .map(|l| (l.key.as_str(), l.evm[0].as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("file d.json", "present"),
                ("file d.json#/a~1b/~0", "true"),
                ("file d.json#/production", "[]"),
                ("file d.json#/staging/0/chainId", "1"),
            ]
        );
        assert!(
            lines[1..]
                .iter()
                .all(|l| l.parent.as_deref() == Some("file d.json")),
            "{lines:#?}"
        );
    }
}
