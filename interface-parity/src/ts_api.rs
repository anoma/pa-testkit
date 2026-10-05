use std::process::Command;

use anyhow::Context;

use crate::cmd;
use crate::compare::Surface;
use crate::packages::Package;

const SCRIPT: &str = include_str!("ts_exports.mjs");

/// The exported items of a built npm package's types entry, read with the
/// package's own TypeScript compiler. A package with an `exports` map is
/// resolved through it by its own name, any other package as its directory.
pub fn exports(pkg: &Package, manifest: &serde_json::Value) -> anyhow::Result<Surface> {
    let spec = if manifest.get("exports").is_some() {
        manifest["name"]
            .as_str()
            .expect("discover keeps only npm packages with a name")
    } else {
        "./"
    };
    let out = cmd::stdout(Command::new("node").current_dir(pkg.dir()).args([
        "--input-type=module",
        "--eval",
        SCRIPT,
        ".",
        spec,
    ]))?;
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
