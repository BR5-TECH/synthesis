//! The discussion a note carries, and the deletion that removes both.
//!
//! One part of `mod.rs`, which holds the helpers these use.

use super::*;

// -- Discussions (NTC-FR-19 … NTC-FR-22) --------------------------------

/// NTC-FR-19: every list command reports the association, and opening one
/// rewrites no note file.
#[test]
fn list_items_carry_the_note_s_discussion() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(dir.path(), "specs/a.md");
    let a = create(root, entity("specs/a.md"), "note a", "2025-03-04T10:00:00Z");
    let b = create(root, NoteScope::Project, "note b", "2025-03-04T10:01:00Z");

    // NTS-FR-31 / NTC-FR-19: snapshotted **before** the discussion is opened,
    // and of the note being discussed — so the comparison below pins that
    // opening one writes nothing into the note, `updated_at` included.
    let before_a = std::fs::read_to_string(note_path(dir.path(), &a.id).unwrap()).unwrap();
    let before = std::fs::read_to_string(note_path(dir.path(), &b.id).unwrap()).unwrap();
    let thread = discuss(root, &a.id, "about a");

    let find = |items: Vec<NoteListItem>, id: &str| {
        items.into_iter().find(|i| i.note.id == id).map(|i| i.discussion_id)
    };
    assert_eq!(find(list_all_notes_in(root, root), &a.id), Some(Some(thread.clone())));
    assert_eq!(find(list_all_notes_in(root, root), &b.id), Some(None));
    assert_eq!(
        find(list_notes_for_entity_in(root, root, "specs/a.md"), &a.id),
        Some(Some(thread.clone())),
    );
    assert_eq!(find(list_project_notes_in(root, root), &b.id), Some(None));

    // NTC-FR-19: nothing about the association is written into a note file.
    assert_eq!(
        std::fs::read_to_string(note_path(dir.path(), &b.id).unwrap()).unwrap(),
        before,
    );
    let a_file = std::fs::read_to_string(note_path(dir.path(), &a.id).unwrap()).unwrap();
    assert_eq!(
        a_file, before_a,
        "opening a discussion rewrites no byte of the note — updated_at included",
    );
    assert!(
        !a_file.contains(&thread),
        "the thread id is nowhere in the note's own file",
    );

    // And it appears for b the moment b is discussed.
    let b_thread = discuss(root, &b.id, "about b");
    assert_eq!(find(list_all_notes_in(root, root), &b.id), Some(Some(b_thread)));
}

/// NTC-FR-20, NTC-FR-07, NTC-FR-11, NTC-FR-12: the association survives everything that can happen to a note.
#[test]
fn a_note_s_discussion_survives_edits_moves_and_an_unresolved_entity() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(dir.path(), "specs/a.md");
    touch(dir.path(), "specs/b.md");
    let note = create(root, entity("specs/a.md"), "original", "2025-03-04T10:00:00Z");
    let thread = discuss(root, &note.id, "opening");

    let assoc = |id: &str| {
        list_all_notes_in(root, root)
            .into_iter()
            .find(|i| i.note.id == id)
            .and_then(|i| i.discussion_id)
    };
    let comments = |root: &crate::fs::RootFs, id: &str| {
        crate::comments::note_discussion_of(root, id)
            .map(|t| t.comments.len())
            .unwrap_or(0)
    };

    for fields in [
        NoteFields { body: Some("edited".into()), ..Default::default() },
        NoteFields { reminder: Some(Some("2025-03-05T09:00:00Z".into())), ..Default::default() },
        NoteFields { reminder: Some(None), ..Default::default() },
        NoteFields { scope: Some(entity("specs/b.md")), ..Default::default() },
        NoteFields { scope: Some(NoteScope::Project), ..Default::default() },
    ] {
        update_note_in(root, &note.id, fields, "2025-03-04T12:00:00Z").unwrap();
        assert_eq!(assoc(&note.id), Some(thread.clone()));
        assert_eq!(comments(root, &note.id), 1);
    }

    // A correlated rename, then a deletion that leaves the note unresolved.
    update_note_in(
        root,
        &note.id,
        NoteFields { scope: Some(entity("specs/a.md")), ..Default::default() },
        "2025-03-04T12:00:00Z",
    )
    .unwrap();
    std::fs::rename(dir.path().join("specs/a.md"), dir.path().join("specs/c.md")).unwrap();
    follow_rename(root, "specs/a.md", "specs/c.md");
    assert_eq!(assoc(&note.id), Some(thread.clone()));

    std::fs::remove_file(dir.path().join("specs/c.md")).unwrap();
    let item = list_all_notes_in(root, root)
        .into_iter()
        .find(|i| i.note.id == note.id)
        .unwrap();
    assert!(item.unresolved, "the entity is gone");
    assert_eq!(item.discussion_id, Some(thread), "the conversation is not");
    assert_eq!(comments(root, &note.id), 1);
}

/// NTC-FR-21, CMS-FR-64: deletion removes the note, its conversation, and its blobs,
/// and touches nothing else.
#[test]
fn deleting_a_note_removes_its_discussion_with_it() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let doomed = create(root, NoteScope::Project, "doomed", "2025-03-04T10:00:00Z");
    let kept = create(root, NoteScope::Project, "kept", "2025-03-04T10:01:00Z");
    let doomed_thread = discuss(root, &doomed.id, "about the doomed one");
    let kept_thread = discuss(root, &kept.id, "about the kept one");

    delete_note_in(root, root, &doomed.id).unwrap();

    assert!(!note_path(dir.path(), &doomed.id).unwrap().is_file());
    assert!(!dir
        .path()
        .join(format!("comments/notes/{}", doomed.id))
        .exists());
    assert!(crate::comments::read_discussion_by_id(root, root, &doomed_thread).is_none());
    assert!(!crate::comments::note_discussion_index(root).contains_key(&doomed.id));

    // The other note and its conversation are untouched.
    assert!(note_path(dir.path(), &kept.id).unwrap().is_file());
    assert!(crate::comments::read_discussion_by_id(root, root, &kept_thread).is_some());
}

/// NTC-FR-21 / NTC-FR-19, CMS-FR-62: the transaction's ordering is what makes it
/// recoverable — a cleanup that fails leaves both whole, and a retry after a
/// partial application completes rather than failing on work already done.
#[test]
fn the_deletion_transaction_is_ordered_and_recoverable() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = create(root, NoteScope::Project, "a note", "2025-03-04T10:00:00Z");
    let thread = discuss(root, &note.id, "opening");

    // The state a crash between the two steps leaves: the discussion gone,
    // the note still there. A retry completes it.
    crate::comments::delete_note_discussion_in(root, &note.id).unwrap();
    assert!(note_path(dir.path(), &note.id).unwrap().is_file());
    let item = list_all_notes_in(root, root)
        .into_iter()
        .find(|i| i.note.id == note.id)
        .unwrap();
    assert_eq!(
        item.discussion_id, None,
        "the row reports no conversation, so Discuss begins a fresh one",
    );
    assert!(crate::comments::read_discussion_by_id(root, root, &thread).is_none());

    delete_note_in(root, root, &note.id).unwrap();
    assert!(!note_path(dir.path(), &note.id).unwrap().is_file());

    // NTC-FR-21: a note carrying no discussion deletes exactly as it did
    // before, the cleanup step finding nothing to remove.
    let plain = create(root, NoteScope::Project, "no discussion", "2025-03-04T10:00:00Z");
    delete_note_in(root, root, &plain.id).unwrap();
    assert!(!note_path(dir.path(), &plain.id).unwrap().is_file());
}

/// NTC-FR-22, CMS-FR-62: a deleted note is a note no operation resolves, so nothing
/// can be created for it afterwards.
#[test]
fn nothing_can_be_created_for_a_deleted_note() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = create(root, NoteScope::Project, "a note", "2025-03-04T10:00:00Z");
    discuss(root, &note.id, "opening");
    delete_note_in(root, root, &note.id).unwrap();

    assert_eq!(
        update_note_in(
            root,
            &note.id,
            NoteFields { body: Some("x".into()), ..Default::default() },
            "2025-03-04T13:00:00Z",
        )
        .unwrap_err(),
        ERR_NOTE_NOT_FOUND,
    );
    assert_eq!(delete_note_in(root, root, &note.id).unwrap_err(), ERR_NOTE_NOT_FOUND);
    assert_eq!(
        crate::comments::get_or_create_note_discussion_in(
            root,
            root,
            &note.id,
            "try again".into(),
            Vec::new(),
            &crate::comments::Participant::Human {
                login: "raver119".into(),
                display_name: None,
                email: None,
            },
            "2025-03-04T13:00:00Z",
        )
        .unwrap_err(),
        crate::comments::ERR_NOTE_NOT_FOUND,
    );
    assert!(!dir
        .path()
        .join(format!("comments/notes/{}", note.id))
        .exists());
    assert!(list_all_notes_in(root, root).iter().all(|i| i.note.id != note.id));
}
/// NTC-FR-21: a cleanup that fails leaves **both** whole and reports the
/// typed error, and the retry completes.
///
/// Fault-injected by making the conversation folder undeletable, which is
/// what makes this test able to tell the two step orders apart: reverse them
/// in `delete_note_in` and the note file is gone here, which is exactly the
/// state NTC-FR-21 says a failure must not leave.
#[cfg(unix)]
#[test]
fn a_failed_cleanup_deletes_nothing_and_reports_the_typed_error() {
    use std::os::unix::fs::PermissionsExt;

    // The undeletable folder is made so by a chmod, which does not bind
    // root: there the removal succeeds and the two step orders this test
    // tells apart become indistinguishable again.
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_failed_cleanup_deletes_nothing_and_reports_the_typed_error",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = create(root, NoteScope::Project, "a note", "2025-03-04T10:00:00Z");
    let thread = discuss(root, &note.id, "opening");

    // The note's conversation folder sits inside `notes/`; making the parent
    // read-only is what stops the recursive removal.
    let parent = dir.path().join("comments/notes");
    let original = std::fs::metadata(&parent).unwrap().permissions();
    let mut locked = original.clone();
    locked.set_mode(0o500);
    std::fs::set_permissions(&parent, locked).unwrap();

    let err = delete_note_in(root, root, &note.id).unwrap_err();

    // Restore before asserting, so a failure here does not leave the temp
    // directory undeletable.
    std::fs::set_permissions(&parent, original).unwrap();

    assert_eq!(err, ERR_DISCUSSION_CLEANUP_FAILED);
    // NTC-FR-21: nothing was reported as deleted, and nothing was.
    assert!(
        note_path(dir.path(), &note.id).unwrap().is_file(),
        "the note file is still there — the cleanup runs first, so a failure \
         of it removes nothing",
    );
    let still = crate::comments::note_discussion_of(root, &note.id).expect("the discussion");
    assert_eq!(still.id, thread);
    assert_eq!(still.comments.len(), 1, "holding every comment it held");

    // And the retry, with the fault cleared, completes both steps.
    delete_note_in(root, root, &note.id).unwrap();
    assert!(!note_path(dir.path(), &note.id).unwrap().is_file());
    assert!(crate::comments::note_discussion_of(root, &note.id).is_none());
}
