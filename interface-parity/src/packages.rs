use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::compare::Surface;
use crate::report::Failure;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Cargo,
    Npm,
}

#[derive(Clone, Debug)]
pub struct Package {
    /// `<repo>/cargo:<name>` or `<repo>/npm:<name>`.
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub dir: PathBuf,
    pub manifest: PathBuf,
    /// The crate name of the library target, if the package has one.
    pub lib_name: Option<String>,
    /// The package's entry in `cargo metadata`.
    pub cargo: Option<serde_json::Value>,
}

const LIBRARY_KINDS: [&str; 6] = ["lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"];

fn manifests(dir: &Path, file: &str, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        if entry.file_type()?.is_dir() {
            if !matches!(name.to_str(), Some("target" | "node_modules" | ".git")) {
                manifests(&entry.path(), file, found)?;
            }
        } else if name == file {
            found.push(entry.path());
        }
    }
    Ok(())
}

fn cargo_metadata(manifest: &Path) -> Result<serde_json::Value, String> {
    let out = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .map_err(|e| format!("running cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

fn lib_name(pkg: &serde_json::Value) -> Option<String> {
    pkg["targets"].as_array()?.iter().find_map(|t| {
        let is_library = t["kind"]
            .as_array()?
            .iter()
            .any(|k| k.as_str().is_some_and(|k| LIBRARY_KINDS.contains(&k)));
        is_library.then(|| t["name"].as_str().unwrap_or_default().replace('-', "_"))
    })
}

/// Every published Cargo and npm package under `root`, sorted by id. A
/// manifest that cannot be read becomes a failure.
pub fn discover(repo: &str, root: &Path) -> (Vec<Package>, Vec<Failure>) {
    let mut packages = BTreeMap::new();
    let mut failures = vec![];
    let (mut cargo_manifests, mut npm_manifests) = (vec![], vec![]);
    if let Err(e) = manifests(root, "Cargo.toml", &mut cargo_manifests)
        .and_then(|()| manifests(root, "package.json", &mut npm_manifests))
    {
        failures.push(Failure {
            subject: format!("{repo} (walking {})", root.display()),
            error: e.to_string(),
        });
        return (vec![], failures);
    }
    for manifest in cargo_manifests {
        let metadata = match cargo_metadata(&manifest) {
            Ok(m) => m,
            Err(error) => {
                failures.push(Failure {
                    subject: format!("{repo} (cargo metadata {})", manifest.display()),
                    error,
                });
                continue;
            }
        };
        for pkg in metadata["packages"].as_array().into_iter().flatten() {
            if pkg["publish"]
                .as_array()
                .is_some_and(|registries| registries.is_empty())
            {
                continue;
            }
            let name = pkg["name"].as_str().unwrap_or_default().to_owned();
            let manifest = PathBuf::from(pkg["manifest_path"].as_str().unwrap_or_default());
            let id = format!("{repo}/cargo:{name}");
            packages.entry(id.clone()).or_insert_with(|| Package {
                id,
                kind: Kind::Cargo,
                name,
                dir: manifest.parent().map(Path::to_owned).unwrap_or_default(),
                manifest,
                lib_name: lib_name(pkg),
                cargo: Some(pkg.clone()),
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
            kind: Kind::Npm,
            name: name.to_owned(),
            dir: manifest.parent().map(Path::to_owned).unwrap_or_default(),
            manifest,
            lib_name: None,
            cargo: None,
        });
    }
    (packages.into_values().collect(), failures)
}

/// Name, version, features, targets and dependencies of a Cargo package.
pub fn cargo_metadata_surface(pkg: &Package) -> Surface {
    let mut s = Surface::default();
    let Some(m) = &pkg.cargo else {
        return s;
    };
    s.insert("package name", m["name"].as_str().unwrap_or_default());
    s.insert("package version", m["version"].as_str().unwrap_or_default());
    for (feature, enables) in m["features"].as_object().into_iter().flatten() {
        s.insert(format!("package feature {feature}"), enables.to_string());
    }
    for target in m["targets"].as_array().into_iter().flatten() {
        let kinds: Vec<&str> = target["kind"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|k| k.as_str())
            .collect();
        s.insert(
            format!(
                "package target {}",
                target["name"].as_str().unwrap_or_default()
            ),
            kinds.join(","),
        );
    }
    for dep in m["dependencies"].as_array().into_iter().flatten() {
        s.insert(
            format!(
                "package dependency {} {}",
                dep["name"].as_str().unwrap_or_default(),
                dep["kind"].as_str().unwrap_or("normal")
            ),
            format!(
                "req={} optional={} default-features={} features={} target={}",
                dep["req"],
                dep["optional"],
                dep["uses_default_features"],
                dep["features"],
                dep["target"]
            ),
        );
    }
    s
}
