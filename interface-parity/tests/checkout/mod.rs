use std::path::PathBuf;

use crate::common;

/// A checkout of the fixture repository `name`.
pub fn checkout(name: &str) -> PathBuf {
    let (url, commit) = common::fixture_repo(name);
    let dir = common::scratch(&format!("checkout-{name}"));
    interface_parity::fetch::checkout(&url, &commit, &dir).unwrap();
    dir
}
