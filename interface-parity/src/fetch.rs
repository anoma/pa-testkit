use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, ensure};
use sha2::{Digest, Sha256};

use crate::cmd;
use crate::inputs::Foundry;

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

/// The `forge` of `foundry`'s release for this platform, downloaded from
/// GitHub into `tools` once and extracted from the tarball after its sha256
/// matches the pin.
pub fn foundry(foundry: &Foundry, tools: &Path) -> anyhow::Result<PathBuf> {
    let platform = foundry_platform()?;
    let pinned = foundry.sha256.get(platform).with_context(|| {
        format!(
            "the Foundry v{} pin has no sha256 for {platform}",
            foundry.version
        )
    })?;
    let name = format!("foundry_v{}_{platform}.tar.gz", foundry.version);
    let dir = tools.join(format!("foundry-v{}-{platform}", foundry.version));
    let tarball = dir.join(&name);
    if !tarball.exists() {
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let url = format!(
            "https://github.com/foundry-rs/foundry/releases/download/v{}/{name}",
            foundry.version
        );
        // A download of its own, so concurrent fetches never share a file;
        // the rename into place is atomic.
        let partial = dir.join(format!(
            "{name}.part-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        cmd::stdout(
            Command::new("curl")
                .args(["-fsSL", "-o"])
                .arg(&partial)
                .arg(&url),
        )?;
        std::fs::rename(&partial, &tarball)
            .with_context(|| format!("moving {} into place", partial.display()))?;
    }
    let bytes =
        std::fs::read(&tarball).with_context(|| format!("reading {}", tarball.display()))?;
    let digest = hex::encode(Sha256::digest(&bytes));
    ensure!(
        &digest == pinned,
        "{} has sha256 {digest}, not the pinned {pinned}",
        tarball.display()
    );
    cmd::stdout(
        Command::new("tar")
            .arg("xzf")
            .arg(&tarball)
            .arg("-C")
            .arg(&dir),
    )?;
    Ok(dir.join("forge"))
}

/// The platform name of this machine's Foundry release tarball.
fn foundry_platform() -> anyhow::Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux_amd64",
        ("linux", "aarch64") => "linux_arm64",
        ("macos", "x86_64") => "darwin_amd64",
        ("macos", "aarch64") => "darwin_arm64",
        (os, arch) => anyhow::bail!("Foundry publishes no release for {os} on {arch}"),
    })
}
