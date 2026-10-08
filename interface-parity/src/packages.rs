use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::compare::Surface;
use crate::files::walk;
use crate::report::Failure;

/// What a package is, with its parsed manifest.
#[derive(Clone, Debug)]
pub enum Kind {
    Cargo(Box<cargo_metadata::Package>),
    Npm(serde_json::Value),
}

#[derive(Clone, Debug)]
pub struct Package {
    /// `<repo>/cargo:<name>` or `<repo>/npm:<name>`.
    pub id: String,
    pub kind: Kind,
    pub manifest: PathBuf,
}

impl Package {
    pub fn dir(&self) -> &Path {
        self.manifest
            .parent()
            .expect("a manifest path names a file inside a directory")
    }
}

/// A Cargo package's library target, of any crate type.
pub fn lib_target(pkg: &cargo_metadata::Package) -> Option<&cargo_metadata::Target> {
    pkg.targets.iter().find(|t| {
        t.is_lib()
            || t.is_rlib()
            || t.is_dylib()
            || t.is_cdylib()
            || t.is_staticlib()
            || t.is_proc_macro()
    })
}

/// Adds `pkg` under its id; two packages with one id are an error.
fn collect(packages: &mut BTreeMap<String, Package>, pkg: Package) -> anyhow::Result<()> {
    match packages.entry(pkg.id.clone()) {
        Entry::Occupied(first) => anyhow::bail!(
            "two packages have the id {}: {} and {}",
            pkg.id,
            first.get().manifest.display(),
            pkg.manifest.display()
        ),
        Entry::Vacant(slot) => {
            slot.insert(pkg);
            Ok(())
        }
    }
}

/// Every published Cargo and npm package under `root`, outside build outputs,
/// sorted by id. A manifest that cannot be read, or a second package with an
/// id already taken, becomes a failure.
pub fn discover(repo: &str, root: &Path) -> (Vec<Package>, Vec<Failure>) {
    let mut packages = BTreeMap::new();
    let mut failures = vec![];
    let mut files = vec![];
    let build_output = |dir: &Path| {
        dir.file_name()
            .is_some_and(|n| n == "target" || n == "node_modules" || n == ".git")
    };
    // cargo reports manifest paths with `..` resolved, so the walked paths
    // must be too for `seen` below to recognise a manifest cargo returned.
    let walked = std::fs::canonicalize(root)
        .with_context(|| format!("resolving {}", root.display()))
        .and_then(|root| walk(&root, &build_output, &mut files));
    if let Err(e) = walked {
        failures.push(Failure {
            subject: format!("{repo} (walking {})", root.display()),
            error: format!("{e:#}"),
        });
        return (vec![], failures);
    }
    let named = |name: &str| -> Vec<PathBuf> {
        files
            .iter()
            .filter(|f| f.file_name().is_some_and(|n| n == name))
            .cloned()
            .collect()
    };
    let (cargo_manifests, npm_manifests) = (named("Cargo.toml"), named("package.json"));
    let mut found = vec![];
    // One `cargo metadata` call returns every member of a workspace, so a
    // manifest an earlier call already returned needs no call of its own.
    let mut seen = BTreeSet::new();
    for manifest in cargo_manifests {
        if seen.contains(&manifest) {
            continue;
        }
        let metadata = match cargo_metadata::MetadataCommand::new()
            .manifest_path(&manifest)
            .no_deps()
            .exec()
        {
            Ok(m) => m,
            Err(e) => {
                failures.push(Failure {
                    subject: format!("{repo} (cargo metadata {})", manifest.display()),
                    error: e.to_string(),
                });
                continue;
            }
        };
        for pkg in metadata.packages {
            let manifest = pkg.manifest_path.clone().into_std_path_buf();
            seen.insert(manifest.clone());
            if pkg.publish.as_ref().is_some_and(Vec::is_empty) {
                continue;
            }
            found.push(Package {
                id: format!("{repo}/cargo:{}", pkg.name),
                kind: Kind::Cargo(Box::new(pkg)),
                manifest,
            });
        }
    }
    for manifest in npm_manifests {
        let json: serde_json::Value = match std::fs::read_to_string(&manifest)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        {
            Ok(j) => j,
            Err(error) => {
                failures.push(Failure {
                    subject: format!("{repo} ({})", manifest.display()),
                    error,
                });
                continue;
            }
        };
        // npm refuses to publish a package without a name or marked private.
        let Some(name) = json["name"].as_str() else {
            continue;
        };
        if json["private"].as_bool() == Some(true) {
            continue;
        }
        found.push(Package {
            id: format!("{repo}/npm:{name}"),
            kind: Kind::Npm(json),
            manifest,
        });
    }
    for pkg in found {
        if let Err(e) = collect(&mut packages, pkg) {
            failures.push(Failure {
                subject: repo.to_owned(),
                error: format!("{e:#}"),
            });
        }
    }
    (packages.into_values().collect(), failures)
}

/// Name, version, features, targets and dependencies of a Cargo package.
pub fn cargo_metadata_surface(pkg: &cargo_metadata::Package) -> Surface {
    let mut s = Surface::default();
    s.insert("package name", pkg.name.as_str());
    s.insert("package version", pkg.version.to_string());
    for (feature, enables) in &pkg.features {
        s.insert(format!("package feature {feature}"), format!("{enables:?}"));
    }
    for target in &pkg.targets {
        let kinds: Vec<String> = target.kind.iter().map(ToString::to_string).collect();
        s.insert(format!("package target {}", target.name), kinds.join(","));
    }
    for dep in &pkg.dependencies {
        s.insert(
            format!("package dependency {} {}", dep.name, dep.kind),
            format!(
                "req={} optional={} default-features={} features={:?} target={}",
                dep.req,
                dep.optional,
                dep.uses_default_features,
                dep.features,
                dep.target
                    .as_ref()
                    .map_or_else(|| "any".to_owned(), ToString::to_string)
            ),
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn npm(manifest: &str) -> Package {
        Package {
            id: "r/npm:x".into(),
            kind: Kind::Npm(serde_json::json!({"name": "x"})),
            manifest: manifest.into(),
        }
    }

    #[test]
    fn a_second_package_with_a_taken_id_is_an_error_naming_both_manifests() {
        let mut packages = BTreeMap::new();
        collect(&mut packages, npm("a/package.json")).unwrap();
        let err = collect(&mut packages, npm("b/package.json"))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("r/npm:x")
                && err.contains("a/package.json")
                && err.contains("b/package.json"),
            "{err}"
        );
        assert_eq!(
            packages["r/npm:x"].manifest,
            PathBuf::from("a/package.json"),
            "the first package keeps the id"
        );
    }
}
