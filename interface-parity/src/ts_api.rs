use std::path::Path;
use std::process::Command;

use crate::compare::Surface;
use crate::files::npm_files;
use crate::packages::Package;

const SCRIPT: &str = include_str!("ts_exports.mjs");

fn run(dir: &Path, program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("running {program}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{program} {} failed:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Builds the package as publishing would, then reads its exported items and
/// its non-source shipped files.
pub fn surface(pkg: &Package, work: &Path) -> Result<Surface, String> {
    run(
        &pkg.dir,
        "npm",
        &["ci", "--ignore-scripts", "--no-audit", "--no-fund"],
    )?;
    let published = run(&pkg.dir, "npm", &["publish", "--dry-run", "--json"])?;
    let published: serde_json::Value = serde_json::from_str(&published)
        .map_err(|e| format!("parsing npm publish --dry-run --json: {e}\n{published}"))?;
    let listed: Vec<String> = published["files"]
        .as_array()
        .ok_or_else(|| format!("npm publish --dry-run --json listed no files: {published}"))?
        .iter()
        .filter_map(|f| f["path"].as_str().map(str::to_owned))
        .collect();
    let mut s = npm_files(pkg, &listed)?;

    let manifest = std::fs::read_to_string(&pkg.manifest)
        .map_err(|e| format!("reading {}: {e}", pkg.manifest.display()))?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest)
        .map_err(|e| format!("parsing {}: {e}", pkg.manifest.display()))?;
    let types = manifest["exports"]["."]["types"]
        .as_str()
        .or(manifest["types"].as_str())
        .or(manifest["typings"].as_str())
        .ok_or("package.json names no types entry (exports[\".\"].types, types or typings)")?;
    std::fs::create_dir_all(work).map_err(|e| format!("creating {}: {e}", work.display()))?;
    let script = work.join("ts_exports.mjs");
    std::fs::write(&script, SCRIPT).map_err(|e| format!("writing {}: {e}", script.display()))?;
    let exports = run(
        &pkg.dir,
        "node",
        &[&script.to_string_lossy(), &pkg.dir.to_string_lossy(), types],
    )?;
    for line in exports.lines() {
        let item: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("{e}: {line}"))?;
        s.insert(
            item["key"].as_str().unwrap_or_default(),
            item["value"].as_str().unwrap_or_default(),
        );
    }
    Ok(s)
}
