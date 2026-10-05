//! The bound on the body, which counts bytes of UTF-8.
//!
//! One part of `mod.rs`, which holds the helpers these use.

use super::*;

// -- The body bound, the channel, and record resolution ------------------
//    (NTC-FR-23 … NTC-FR-25)

/// NTC-FR-24, NTC-FR-23, NTC-FR-05: the bound is on **bytes of UTF-8**, and a note is refused
/// whole rather than trimmed to fit.
#[test]
fn a_body_above_one_kib_is_refused_and_nothing_is_written() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());

    // Exactly at the bound is accepted.
    let at_bound = "a".repeat(MAX_NOTE_BODY_BYTES);
    let note = create_note_in(
        root,
        NoteScope::Project,
        at_bound.clone(),
        None,
        None,
        "2026-01-01T00:00:00Z",
    )
    .expect("1024 bytes is at the bound, not over it");
    assert_eq!(note.body, at_bound);

    // One byte over is not, and leaves nothing behind.
    let before = list_all_notes_in(root, root).len();
    let err = create_note_in(
        root,
        NoteScope::Project,
        "a".repeat(MAX_NOTE_BODY_BYTES + 1),
        None,
        None,
        "2026-01-02T00:00:00Z",
    )
    .expect_err("1025 bytes is over the bound");
    assert!(
        err.starts_with(ERR_NOTE_BODY_TOO_LARGE),
        "the typed error names itself: {err:?}",
    );
    assert!(
        err.contains(&MAX_NOTE_BODY_BYTES.to_string()),
        "and names the bound: {err:?}",
    );
    assert_eq!(
        list_all_notes_in(root, root).len(),
        before,
        "no file appeared and no note is listed",
    );
}

/// NTC-FR-24, NTC-FR-23, NTC-FR-05: an update carrying an over-long `body` writes nothing, and one
/// carrying no `body` at all is unaffected.
#[test]
fn an_update_above_the_bound_leaves_the_note_exactly_as_it_was() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = create_note_in(
        root,
        NoteScope::Project,
        "short".to_string(),
        Some("2027-01-01T00:00:00Z".to_string()),
        None,
        "2026-01-01T00:00:00Z",
    )
    .unwrap();

    let err = update_note_in(
        root,
        &note.id,
        NoteFields {
            body: Some("a".repeat(MAX_NOTE_BODY_BYTES + 1)),
            ..Default::default()
        },
        "2026-06-06T00:00:00Z",
    )
    .expect_err("over the bound");
    assert!(err.starts_with(ERR_NOTE_BODY_TOO_LARGE));

    let after = load_note(root, &note.id).expect("the note stands");
    assert_eq!(after.body, "short", "the stored body is what it was");
    assert_eq!(after.updated_at, note.updated_at, "and so is `updated_at`");

    // NTC-FR-05: an update carrying no `body` is unaffected by the bound.
    let cleared = update_note_in(
        root,
        &note.id,
        NoteFields {
            reminder: Some(None),
            ..Default::default()
        },
        "2026-06-06T00:00:00Z",
    )
    .expect("the reminder clears");
    assert_eq!(cleared.reminder, None);
    assert_eq!(cleared.body, "short");
}

/// NTC-FR-24, NTC-FR-23, NTC-FR-05: bytes, not characters — 400 emoji are under the character
/// count and over the byte bound.
#[test]
fn the_bound_counts_utf8_bytes_rather_than_characters() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let body = "\u{1F600}".repeat(400);
    assert!(body.chars().count() < MAX_NOTE_BODY_BYTES);
    assert!(body.len() > MAX_NOTE_BODY_BYTES);

    let err = create_note_in(
        root,
        NoteScope::Project,
        body,
        None,
        None,
        "2026-01-01T00:00:00Z",
    )
    .expect_err("over the bound in bytes");
    assert!(err.starts_with(ERR_NOTE_BODY_TOO_LARGE));
    assert!(
        list_all_notes_in(root, root).is_empty(),
        "nothing was truncated to fit",
    );
}
