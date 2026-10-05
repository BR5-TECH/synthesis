//! Record resolution: the documents the search reads and the record a caller
//! asks for by identifier.
//!
//! One part of `mod.rs`, which holds the helpers these use.

use super::*;

/// NTC-FR-11, NTC-FR-24, NTC-FR-14: `note_documents` returns one entry per well-formed note and
/// names the ones it would not serve.
#[test]
fn note_documents_returns_every_well_formed_note_and_names_the_rest() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let first = create(root, NoteScope::Project, "first body", "2026-01-01T00:00:00Z");
    let second = create(root, NoteScope::Project, "second body", "2026-01-02T00:00:00Z");
    std::fs::write(
        notes_dir(dir.path()).join("broken.toml"),
        "this is not = = toml",
    )
    .unwrap();

    let (documents, skipped) = note_documents(root);
    assert_eq!(documents.len(), 2, "exactly the two well-formed notes");
    // NTC-FR-09: most-recently-edited first.
    assert_eq!(
        documents,
        vec![
            NoteDocument {
                note_id: second.id.clone(),
                body: "second body".to_string(),
            },
            NoteDocument {
                note_id: first.id.clone(),
                body: "first body".to_string(),
            },
        ],
    );
    assert_eq!(
        skipped
            .iter()
            .map(|s| (s.note_id.as_str(), s.reason))
            .collect::<Vec<_>>(),
        vec![("broken", SKIP_UNREADABLE)],
        "the malformed file is named rather than swallowed",
    );
    // Reading is not activity on a note.
    assert_eq!(load_note(root, &first.id).unwrap().updated_at, first.updated_at);
    assert_eq!(second.updated_at, "2026-01-02T00:00:00Z");
}

/// NTC-FR-14: the three shapes a note file is skipped for, each named with
/// its own reason.
///
/// A note refused for one reason and reported as another sends an author
/// looking for a fault they do not have — a corrupt file when what they
/// actually did was rename one, say — so the reason is worth pinning
/// separately from the skip.
#[test]
fn note_documents_tells_the_three_skip_reasons_apart() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let good = create(root, NoteScope::Project, "well formed", "2026-01-01T00:00:00Z");
    let dir_path = notes_dir(dir.path());
    std::fs::create_dir_all(&dir_path).unwrap();

    std::fs::write(dir_path.join("unreadable.toml"), "this is not = = toml").unwrap();
    // A file whose stem disagrees with the id inside it — a merge
    // resolution, a hand edit, a `git mv`.
    std::fs::write(
        dir_path.join("mislabelled.toml"),
        "id = \"somebody-else\"\nscope = \"project\"\nbody = \"x\"\n\
         created_at = \"2026-01-01T00:00:00Z\"\nupdated_at = \"2026-01-01T00:00:00Z\"\n",
    )
    .unwrap();
    // A file that parses and describes no note this module can serve.
    std::fs::write(
        dir_path.join("unknown-scope.toml"),
        "id = \"unknown-scope\"\nscope = \"elsewhere\"\nbody = \"x\"\n\
         created_at = \"2026-01-01T00:00:00Z\"\nupdated_at = \"2026-01-01T00:00:00Z\"\n",
    )
    .unwrap();
    // A file that is not TOML at all is not even considered.
    std::fs::write(dir_path.join("notes.md"), "# not a note\n").unwrap();

    let (documents, skipped) = note_documents(root);
    assert_eq!(
        documents.iter().map(|d| d.note_id.as_str()).collect::<Vec<_>>(),
        vec![good.id.as_str()],
    );
    let reasons: std::collections::BTreeMap<&str, &str> = skipped
        .iter()
        .map(|s| (s.note_id.as_str(), s.reason))
        .collect();
    assert_eq!(
        reasons,
        [
            ("mislabelled", SKIP_ID_MISMATCH),
            ("unknown-scope", SKIP_MALFORMED),
            ("unreadable", SKIP_UNREADABLE),
        ]
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>(),
        "each skip says which failure it was, and a non-TOML file is not one",
    );
    // And the reasons are distinct phrases, so telling them apart is possible.
    assert_eq!(
        [SKIP_UNREADABLE, SKIP_ID_MISMATCH, SKIP_MALFORMED]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3,
    );
}

/// NTC-FR-24, NTC-FR-25, NTC-FR-14: `note_record` answers from disk, so it reports a note as it
/// now stands.
#[test]
fn note_record_reports_the_note_as_it_now_stands() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(dir.path().join("specifications")).unwrap();
    std::fs::write(dir.path().join("specifications/a.md"), "# A\n").unwrap();
    let note = create_note_in(
        root,
        entity("specifications/a.md"),
        "the original body".to_string(),
        Some("2027-05-05T00:00:00Z".to_string()),
        Some("cafe123".to_string()),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let thread = discuss(root, &note.id, "a comment about it");
    assert!(!thread.is_empty());

    let read = note_record(root, &note.id).expect("the note resolves");
    assert_eq!(read, note, "every field of the record, and nothing beside it");

    // Moved and edited — the record follows, without an index in between.
    update_note_in(
        root,
        &note.id,
        NoteFields {
            body: Some("the new body".to_string()),
            scope: Some(NoteScope::Project),
            ..Default::default()
        },
        "2026-02-02T00:00:00Z",
    )
    .unwrap();
    let moved = note_record(root, &note.id).expect("still resolves");
    assert_eq!(moved.scope, NoteScope::Project);
    assert_eq!(moved.body, "the new body");

    // Repeated reads change nothing.
    let stamp = moved.updated_at.clone();
    for _ in 0..3 {
        assert_eq!(note_record(root, &note.id).unwrap().updated_at, stamp);
    }

    // NTC-FR-14: an unknown id and a malformed file are both "not found".
    assert_eq!(
        note_record(root, "no-such-note").unwrap_err(),
        ERR_NOTE_NOT_FOUND,
    );
    std::fs::write(
        notes_dir(dir.path()).join("damaged.toml"),
        "not = = toml",
    )
    .unwrap();
    assert_eq!(note_record(root, "damaged").unwrap_err(), ERR_NOTE_NOT_FOUND);

    // NTC-FR-14: and one whose file name disagrees with the id inside it,
    // which the listing path skips for the same reason — answering for it
    // would hand a caller an id no later call could resolve.
    let mismatched = format!(
        "id = \"somebody-else\"\nscope = \"project\"\nbody = \"x\"\n\
         created_at = \"2026-01-01T00:00:00Z\"\nupdated_at = \"2026-01-01T00:00:00Z\"\n"
    );
    std::fs::write(notes_dir(dir.path()).join("mislabelled.toml"), mismatched).unwrap();
    assert_eq!(
        note_record(root, "mislabelled").unwrap_err(),
        ERR_NOTE_NOT_FOUND,
    );
    assert!(
        !list_all_notes_in(root, root)
            .iter()
            .any(|i| i.note.id == "somebody-else"),
        "and the listing path skips it too",
    );
}

/// NTC-FR-24, NTC-FR-25, NTC-FR-14: `note_record` is an internal Rust call and no Tauri command.
#[test]
fn note_record_is_registered_as_no_command() {
    const LIB: &str = include_str!("../../lib.rs");
    let handler = LIB
        .split_once("generate_handler![")
        .expect("generate_handler! invocation")
        .1
        .split_once("])")
        .expect("end of generate_handler!")
        .0;
    assert!(
        !handler.contains("note_record"),
        "NTC-FR-25: no frontend surface reaches a note through it",
    );
    assert!(!handler.contains("note_documents"));
}
