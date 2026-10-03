use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;
use sha2::{Digest, Sha256};

use crate::cmd;
use crate::compare::Surface;
use crate::packages::{Kind, Package};

/// Extensions of the files whose content the Rust and TypeScript API
/// extractions already cover.
const CARGO_SOURCES: &[&str] = &[".rs"];
const NPM_SOURCES: &[&str] = &[".ts", ".js", ".mjs", ".cjs", ".map"];

/// One item per JSON scalar and per empty array or object, keyed by its JSON
/// pointer (RFC 6901), so a difference names the exact place in the file.
fn flatten_json(prefix: &str, value: &serde_json::Value, s: &mut Surface) {
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
            other => s.insert(format!("{prefix}#{pointer}"), other.to_string()),
        }
    }
    walk(prefix, String::new(), value, s);
}

/// Every file under `dir`, as paths relative to `root` joined with `/`.
fn files_under(root: &Path, dir: &Path, found: &mut Vec<String>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            files_under(root, &path, found)?;
        } else {
            let relative = path.strip_prefix(root)?.components();
            found.push(
                relative
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
    Ok(())
}

/// The non-source files of an unpacked package: JSON files key by key, any
/// other file by its SHA-256.
fn unpacked_surface(root: &Path, sources: &[&str]) -> anyhow::Result<Surface> {
    let mut paths = vec![];
    files_under(root, root, &mut paths)?;
    let mut s = Surface::default();
    for path in paths
        .iter()
        .filter(|p| !sources.iter().any(|ext| p.ends_with(ext)))
    {
        let full = root.join(path);
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
    if dir.exists() {
        std::fs::remove_dir_all(dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
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

/// The non-source files of the `.crate` archive `cargo package` builds.
pub fn cargo_files(pkg: &Package, work: &Path) -> anyhow::Result<Surface> {
    let Kind::Cargo(meta) = &pkg.kind else {
        anyhow::bail!("{} is not a Cargo package", pkg.id);
    };
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
    unpacked_surface(&unpacked.join(stem), CARGO_SOURCES)
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
    unpacked_surface(&unpacked.join("package"), NPM_SOURCES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::compare;

    #[test]
    fn json_flattens_to_one_key_per_scalar_and_empty_container() {
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
                ("file d.json#/a~1b/~0", "true"),
                ("file d.json#/production", "[]"),
                ("file d.json#/staging/0/chainId", "1"),
            ]
        );
    }
}
