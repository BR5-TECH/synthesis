//! One vocabulary for the artifact types (ASC-FR-02).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

/// ASC-FR-02: one vocabulary, not two.
///
/// `as_str` is what an agent's input names the material's type with
/// (`../core/AGC-agent-conversations.md` AGC-FR-07) and serde is what
/// `.synthesis/library.toml` stores; a `#[serde(rename)]` on any variant
/// would silently make the two disagree, and the agent would be told a type
/// the Library does not use.
#[test]
fn every_types_stated_name_is_the_name_it_is_stored_under() {
    for t in EVERY_TYPE {
        assert_eq!(
            serde_json::to_value(t).expect("serialises"),
            serde_json::json!(t.as_str()),
            "{t:?} states a different name than it stores",
        );
        // …and the round trip, so a name that is written can be read back.
        assert_eq!(ArtifactType::from_marker(t.as_str()), Some(t));
    }
    // Distinct, so no two types can be told apart in one vocabulary and not
    // the other.
    let mut names: Vec<&str> = EVERY_TYPE.iter().map(|t| t.as_str()).collect();
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    assert_eq!(names.len(), count, "two types share a name");
}
