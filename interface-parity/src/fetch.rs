use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};

/// Runs git and returns its stdout; a non-zero exit is an error carrying stderr.
pub fn git(dir: Option<&Path>, args: &[&str]) -> anyhow::Result<String> {
    let mut cmd = Command::new("git");
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    let out = cmd
        .args(args)
        .output()
        .with_context(|| format!("running git {args:?}"))?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8(out.stdout)?)
}

/// Makes `dir` a checkout of exactly `commit` from `url`.
pub fn checkout(url: &str, commit: &str, dir: &Path) -> anyhow::Result<()> {
    if dir.join(".git").exists() && git(Some(dir), &["rev-parse", "HEAD"])?.trim() == commit {
        return Ok(());
    }
    if dir.exists() {
        std::fs::remove_dir_all(dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    git(Some(dir), &["init", "-q"])?;
    git(Some(dir), &["fetch", "-q", "--depth", "1", url, commit])?;
    git(Some(dir), &["checkout", "-q", "--detach", "FETCH_HEAD"])?;
    Ok(())
}

/// Every tag of the remote, without the peeled `^{}` entries.
pub fn tags(url: &str) -> anyhow::Result<Vec<String>> {
    let out = git(None, &["ls-remote", "--tags", url])?;
    let mut tags: Vec<String> = out
        .lines()
        .filter_map(|l| l.split('\t').nth(1))
        .filter_map(|r| r.strip_prefix("refs/tags/"))
        .filter(|t| !t.ends_with("^{}"))
        .map(str::to_owned)
        .collect();
    tags.sort();
    Ok(tags)
}
