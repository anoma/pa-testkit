use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use interface_parity::cmd;
use interface_parity::fetch::git;

/// A new empty directory under the tests' temporary directory, unique to this
/// call, so tests running in parallel never share one.
pub fn scratch(name: &str) -> PathBuf {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "{name}-{}-{}",
        std::process::id(),
        CALLS.fetch_add(1, Ordering::Relaxed)
    ));
    cmd::fresh_dir(&dir).unwrap();
    dir
}

/// Commits `tests/fixtures/<name>` as a git repository with the tags listed
/// in its `TAGS` file, and returns its `file://` URL and commit.
pub fn fixture_repo(name: &str) -> (String, String) {
    let dir = scratch(&format!("repo-{name}"));
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    cmd::stdout(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(&dir),
    )
    .unwrap();
    let git = |args: &[&str]| git(Some(&dir), args).unwrap().trim().to_owned();
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&[
        "-c",
        "user.name=fixture",
        "-c",
        "user.email=fixture@example.com",
        "commit",
        "-qm",
        name,
    ]);
    let tags = std::fs::read_to_string(dir.join("TAGS")).unwrap();
    for tag in tags.lines().filter(|l| !l.is_empty()) {
        git(&["tag", tag]);
    }
    (
        format!("file://{}", dir.display()),
        git(&["rev-parse", "HEAD"]),
    )
}
