use std::path::Path;
use std::process::Command;

use crate::cmd;

/// Runs git, in `dir` if given, and returns its stdout.
pub fn git(dir: Option<&Path>, args: &[&str]) -> anyhow::Result<String> {
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    cmd::stdout(command.args(args))
}

/// Makes `dir` a checkout of exactly `commit` from `url`.
pub fn checkout(url: &str, commit: &str, dir: &Path) -> anyhow::Result<()> {
    if dir.join(".git").exists() && git(Some(dir), &["rev-parse", "HEAD"])?.trim() == commit {
        return Ok(());
    }
    cmd::fresh_dir(dir)?;
    git(Some(dir), &["init", "-q"])?;
    git(Some(dir), &["fetch", "-q", "--depth", "1", url, commit])?;
    git(Some(dir), &["checkout", "-q", "--detach", "FETCH_HEAD"])?;
    Ok(())
}

/// Every tag of the remote, sorted.
pub fn tags(url: &str) -> anyhow::Result<Vec<String>> {
    let out = git(None, &["ls-remote", "--tags", "--refs", url])?;
    let mut tags: Vec<String> = out
        .lines()
        .filter_map(|l| l.split('\t').nth(1))
        .filter_map(|r| r.strip_prefix("refs/tags/"))
        .map(str::to_owned)
        .collect();
    tags.sort();
    Ok(tags)
}
