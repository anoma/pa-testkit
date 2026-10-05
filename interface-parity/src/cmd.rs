use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};

/// Runs `cmd` and returns its stdout. A failure to start, a non-zero exit or
/// non-UTF-8 output is an error carrying the command line and its stderr.
pub fn stdout(cmd: &mut Command) -> anyhow::Result<String> {
    let out = cmd.output().with_context(|| format!("running {cmd:?}"))?;
    if !out.status.success() {
        bail!(
            "{cmd:?} failed ({}):\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8(out.stdout).with_context(|| format!("{cmd:?} printed non-UTF-8 output"))
}

/// Makes `dir` an empty directory, removing whatever was there.
pub fn fresh_dir(dir: &Path) -> anyhow::Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))
}
