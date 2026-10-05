//! Notes per content root, the authoring revision, and the reminders.
//!
//! One part of `mod.rs`, which holds the helpers these use.

use super::*;

// -- NTC-FR-15 (worktrees) ----------------------------------------------

#[test]
fn each_content_root_holds_its_own_notes() {
    // NTC-FR-15: `.synthesis/notes/` is resolved against the
    // active content root, so switching worktrees switches note sets and
    // copies nothing between them.
    let a = temp_root();
    let b = temp_root();
    create(&crate::fs::RootFs::for_root(a.path()), NoteScope::Project, "one", "2026-01-01T00:00:00Z");
    create(&crate::fs::RootFs::for_root(a.path()), NoteScope::Project, "two", "2026-01-02T00:00:00Z");
    create(&crate::fs::RootFs::for_root(b.path()), NoteScope::Project, "only", "2026-01-03T00:00:00Z");

    assert_eq!(list_all_notes_in(
            &crate::fs::RootFs::for_root(a.path()),
            &crate::fs::RootFs::for_root(a.path()),
        ).len(), 2);
    let from_b = list_all_notes_in(
        &crate::fs::RootFs::for_root(b.path()),
        &crate::fs::RootFs::for_root(b.path()),
    );
    assert_eq!(from_b.len(), 1);
    assert_eq!(from_b[0].note.body, "only");
    assert_eq!(
        std::fs::read_dir(a.path().join(".synthesis/notes"))
            .unwrap()
            .count(),
        2,
        "nothing was copied out of the other worktree"
    );
}

// -- NTC-FR-18 / NTC-FR-17, PST-FR-12 (revision + reminders) -----------------------

#[test]
fn a_notes_authoring_revision_survives_every_list() {
    // NTC-FR-18.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "a.md");
    create_note_in(
        root,
        entity("a.md"),
        "historical".into(),
        None,
        Some("7e3f1a2".into()),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    create(root, entity("a.md"), "current", "2026-01-02T00:00:00Z");

    let items = list_all_notes_in(root, root);
    let historical = items.iter().find(|i| i.note.body == "historical").unwrap();
    let current = items.iter().find(|i| i.note.body == "current").unwrap();
    assert_eq!(historical.note.revision.as_deref(), Some("7e3f1a2"));
    assert!(current.note.revision.is_none());
}

#[test]
fn only_reminder_carrying_notes_are_visible_to_the_reminder_enumeration() {
    // PST-FR-12 / NTC-FR-17.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "a.md");
    create_note_in(
        root,
        entity("a.md"),
        "later".into(),
        Some("2026-06-02T09:00:00Z".into()),
        None,
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    create_note_in(
        root,
        NoteScope::Project,
        "sooner".into(),
        Some("2026-06-01T09:00:00Z".into()),
        None,
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    create(root, NoteScope::Project, "no reminder", "2026-01-01T00:00:00Z");

    let reminders = reminder_notes(root);
    assert_eq!(reminders.len(), 2);
    assert_eq!(reminders[0].body, "sooner", "soonest first");
    assert_eq!(reminders[1].body, "later");
    assert_eq!(
        reminders[0].scope,
        NoteScope::Project,
        "each carries its scope, so the widget can route to it"
    );
    assert_eq!(reminders[1].reminder.as_deref(), Some("2026-06-02T09:00:00Z"));
}
