use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::compare::Surface;
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

    pub fn name(&self) -> &str {
        match &self.kind {
            Kind::Cargo(pkg) => &pkg.name,
            Kind::Npm(json) => json["name"]
                .as_str()
                .expect("discover keeps only npm packages with a name"),
        }
    }

    /// The crate name of a Cargo package's library target, if it has one.
    pub fn lib_name(&self) -> Option<String> {
        let Kind::Cargo(pkg) = &self.kind else {
            return None;
        };
        pkg.targets
            .iter()
            .find(|t| {
                t.is_lib()
                    || t.is_rlib()
                    || t.is_dylib()
                    || t.is_cdylib()
                    || t.is_staticlib()
                    || t.is_proc_macro()
            })
            .map(|t| t.name.replace('-', "_"))
    }
}

/// Every `Cargo.toml` and `package.json` under `dir`, outside build outputs.
fn manifests(dir: &Path, cargo: &mut Vec<PathBuf>, npm: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        if entry.file_type()?.is_dir() {
            if !matches!(name.to_str(), Some("target" | "node_modules" | ".git")) {
                manifests(&entry.path(), cargo, npm)?;
            }
        } else if name == "Cargo.toml" {
            cargo.push(entry.path());
        } else if name == "package.json" {
            npm.push(entry.path());
        }
    }
    Ok(())
}

/// Every published Cargo and npm package under `root`, sorted by id. A
/// manifest that cannot be read becomes a failure.
pub fn discover(repo: &str, root: &Path) -> (Vec<Package>, Vec<Failure>) {
    let mut packages = BTreeMap::new();
    let mut failures = vec![];
    let (mut cargo_manifests, mut npm_manifests) = (vec![], vec![]);
    if let Err(e) = manifests(root, &mut cargo_manifests, &mut npm_manifests) {
        failures.push(Failure {
            subject: format!("{repo} (walking {})", root.display()),
            error: e.to_string(),
        });
        return (vec![], failures);
    }
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
            let id = format!("{repo}/cargo:{}", pkg.name);
            packages.entry(id.clone()).or_insert_with(|| Package {
                id,
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
        let id = format!("{repo}/npm:{name}");
        packages.entry(id.clone()).or_insert_with(|| Package {
            id,
            kind: Kind::Npm(json.clone()),
            manifest,
        });
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
