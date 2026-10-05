// Version resolution for the Synthesis backend microservice.
//
// Specification: specifications/server/BMS-backend-microservice.md
// Requirements: BMS-FR-14, BMS-FR-16, BMS-FR-26.
//
// This file holds no inner doc comment and no `use` statement, because
// `build.rs` pulls it in with `include!`. The crate compiles the same file as a
// module, so the build script's decision is tested by the crate's own tests.

/// The value the resolver returns when no source supplies a version.
pub const UNDEFINED_VERSION: &str = "undefined";

/// Selects the build version from the sources of BMS-FR-14, in order.
///
/// The order is: the `SYNTHESIS_BUILD_VERSION` build argument, then the exact
/// Git tag at the commit that is built, then the short Git commit hash, then
/// the literal string `undefined`.
///
/// The function is pure: it reads no file, runs no command, and holds no state.
/// A source that is absent, empty, or only white space counts as absent
/// (BMS-FR-26).
pub fn resolve_version(
    build_argument: Option<&str>,
    exact_tag: Option<&str>,
    short_hash: Option<&str>,
) -> String {
    for source in [build_argument, exact_tag, short_hash]
        .into_iter()
        .flatten()
    {
        let value = source.trim();
        if !value.is_empty() {
            return value.to_string();
        }
    }
    UNDEFINED_VERSION.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // BMS-FR-14, BMS-FR-26: the build argument wins over every other source.
    #[test]
    fn build_argument_wins_over_tag_and_hash() {
        let resolved = resolve_version(Some("v1.4.0"), Some("v1.3.9"), Some("a1b2c3d"));
        assert_eq!(resolved, "v1.4.0");
    }

    // BMS-FR-14, BMS-FR-26: without a build argument, the exact tag wins over the hash.
    #[test]
    fn exact_tag_wins_when_no_build_argument() {
        let resolved = resolve_version(None, Some("v1.3.9"), Some("a1b2c3d"));
        assert_eq!(resolved, "v1.3.9");
    }

    // BMS-FR-14, BMS-FR-26: the short hash is used when neither of the two above is there.
    #[test]
    fn short_hash_is_used_when_no_tag() {
        let resolved = resolve_version(None, None, Some("a1b2c3d"));
        assert_eq!(resolved, "a1b2c3d");
    }

    // BMS-FR-14, BMS-FR-26: with no source at all, the resolver returns `undefined`.
    #[test]
    fn undefined_when_no_source_supplies_a_value() {
        assert_eq!(resolve_version(None, None, None), "undefined");
    }

    // BMS-FR-16: a Git command that fails writes an empty string rather than a
    // value. An empty or blank source must fall through to the next one, or the
    // image would report a blank version.
    #[test]
    fn blank_sources_fall_through_to_the_next_source() {
        assert_eq!(resolve_version(Some(""), Some("v1.3.9"), None), "v1.3.9");
        assert_eq!(
            resolve_version(Some("  "), None, Some("a1b2c3d")),
            "a1b2c3d"
        );
        assert_eq!(
            resolve_version(Some(""), Some(" "), Some("\n")),
            "undefined"
        );
    }

    // Git command output carries a trailing newline. The resolver trims it, so
    // the version constant holds no white space.
    #[test]
    fn surrounding_white_space_is_trimmed() {
        assert_eq!(resolve_version(None, Some(" v1.3.9\n"), None), "v1.3.9");
    }
}
