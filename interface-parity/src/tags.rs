use std::collections::BTreeSet;

use crate::compare::Surface;

/// The part of a tag before its semver version: `bindings/v3.0.0` gives
/// `bindings/v`. A tag with no version suffix is its own scheme.
pub fn prefix(tag: &str) -> &str {
    tag.char_indices()
        .find(|(i, _)| semver::Version::parse(&tag[*i..]).is_ok())
        .map_or(tag, |(i, _)| &tag[..i])
}

pub fn surface(tags: &[String]) -> Surface {
    let mut s = Surface::default();
    for p in tags.iter().map(|t| prefix(t)).collect::<BTreeSet<_>>() {
        s.insert(format!("tag-prefix {p}"), "present");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefix_is_what_precedes_the_semver_version() {
        assert_eq!(prefix("bindings/v3.0.0"), "bindings/v");
        assert_eq!(prefix("contracts/v2.0.0-rc.7.1"), "contracts/v");
        assert_eq!(prefix("v1.0.0-beta"), "v");
        assert_eq!(prefix("1.2.3"), "");
        assert_eq!(
            prefix("release-candidate"),
            "release-candidate",
            "a tag without a version is its own scheme"
        );
    }

    #[test]
    fn the_surface_holds_each_prefix_once() {
        let s = surface(&["v1.0.0".into(), "v1.1.0".into(), "bindings/v1.0.0".into()]);
        let mut expected = Surface::default();
        expected.insert("tag-prefix bindings/v", "present");
        expected.insert("tag-prefix v", "present");
        assert_eq!(s, expected);
    }
}
