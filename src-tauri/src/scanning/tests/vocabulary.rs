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
        // `doc` is the one type no content marker gives (ASC-FR-JWPH).
        let expected = (t != ArtifactType::Doc).then_some(t);
        assert_eq!(ArtifactType::from_marker(t.as_str()), expected);
    }
    // Distinct, so no two types can be told apart in one vocabulary and not
    // the other.
    let mut names: Vec<&str> = EVERY_TYPE.iter().map(|t| t.as_str()).collect();
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    assert_eq!(names.len(), count, "two types share a name");
}

// ASC-FR-JWPH: `doc` comes from an assignment alone. No path convention and no
// frontmatter marker produce it, and a folder assignment types the files below
// it only where nothing of higher precedence does (ASC-FR-06).
#[test]
fn doc_comes_only_from_an_assignment() {
    let none = |_: &str| ContentFacts::default();
    let marked = |_: &str| read_content_facts("---\ntype: doc\n---\nbody\n");
    let empty = LibraryAssignments::default();
    assert_eq!(infer_from_path("docs/guide.doc.md"), None);
    assert_eq!(classify_file("docs/guide.md", &empty, &marked).0, None);

    let mut stored = LibraryAssignments::default();
    stored.assignments.insert(
        "docs".to_string(),
        Assignment { artifact_type: ArtifactType::Doc, scope: Scope::Folder },
    );
    stored.assignments.insert(
        "docs/kept.md".to_string(),
        Assignment { artifact_type: ArtifactType::Spec, scope: Scope::File },
    );
    assert_eq!(
        classify_file("docs/guide.md", &stored, &none),
        (Some(ArtifactType::Doc), Some(TypeSource::Inherited)),
    );
    // A per-file assignment and an inferred type both win over the folder.
    assert_eq!(classify_file("docs/kept.md", &stored, &none).0, Some(ArtifactType::Spec));
    assert_eq!(classify_file("docs/CLAUDE.md", &stored, &none).0, Some(ArtifactType::Instructions));
}

// AGC-FR-07: `doc` is the one type an agent is not told.
#[test]
fn only_doc_is_hidden_from_agents() {
    for t in EVERY_TYPE {
        assert_eq!(t.is_agent_facing(), t != ArtifactType::Doc, "{t:?}");
    }
}
