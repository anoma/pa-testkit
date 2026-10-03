use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A new empty directory under the tests' temporary directory, unique to this
/// call, so tests running in parallel never share one.
pub fn scratch(name: &str) -> PathBuf {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "{name}-{}-{}",
        std::process::id(),
        CALLS.fetch_add(1, Ordering::Relaxed)
    ));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Commits `tests/fixtures/<name>` as a git repository with the tags listed
/// in its `TAGS` file, and returns its `file://` URL and commit.
pub fn fixture_repo(name: &str) -> (String, String) {
    let dir = scratch(&format!("repo-{name}"));
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
        &dir,
    );
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "-A"]);
    git(
        &dir,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.com",
            "commit",
            "-qm",
            name,
        ],
    );
    let tags = std::fs::read_to_string(dir.join("TAGS")).unwrap();
    for tag in tags.lines().filter(|l| !l.is_empty()) {
        git(&dir, &["tag", tag]);
    }
    (
        format!("file://{}", dir.display()),
        git(&dir, &["rev-parse", "HEAD"]),
    )
}
