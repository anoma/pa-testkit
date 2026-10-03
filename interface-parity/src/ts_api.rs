use std::process::Command;

use anyhow::Context;

use crate::cmd;
use crate::compare::Surface;
use crate::packages::{Kind, Package};

const SCRIPT: &str = include_str!("ts_exports.mjs");

/// The exported items of a built npm package's types entry, read with the
/// package's own TypeScript compiler.
pub fn exports(pkg: &Package) -> anyhow::Result<Surface> {
    let Kind::Npm(manifest) = &pkg.kind else {
        anyhow::bail!("{} is not an npm package", pkg.id);
    };
    let types = manifest["exports"]["."]["types"]
        .as_str()
        .or(manifest["types"].as_str())
        .or(manifest["typings"].as_str())
        .with_context(|| {
            format!(
                "{} names no types entry (exports[\".\"].types, types or typings)",
                pkg.manifest.display()
            )
        })?;
    let out = cmd::stdout_with_input(
        Command::new("node")
            .current_dir(pkg.dir())
            .args(["--input-type=module", "-", "."])
            .arg(types),
        SCRIPT.as_bytes(),
    )?;
    let mut s = Surface::default();
    for line in out.lines() {
        let item: serde_json::Value =
            serde_json::from_str(line).with_context(|| format!("parsing {line}"))?;
        let (Some(key), Some(value)) = (item["key"].as_str(), item["value"].as_str()) else {
            anyhow::bail!("an exported item without a key and a value: {line}");
        };
        s.insert(key, value);
    }
    Ok(s)
}
