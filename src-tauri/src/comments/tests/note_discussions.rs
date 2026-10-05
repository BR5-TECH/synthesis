//! Note discussions (CMS-FR-62 ... CMS-FR-64).

use super::*;

// -- Note discussions (CMS-FR-62 … CMS-FR-64) ---------------------------

/// A note on disk, so `get_or_create_note_discussion_in`'s existence check
/// passes. Written through the notes module so the two agree on the shape.
fn seed_note(root: &crate::fs::RootFs, body: &str) -> String {
    crate::notes::create_note_in(
        root,
        crate::notes::NoteScope::Project,
        body.into(),
        None,
        None,
        "2025-03-04T10:00:00Z",
    )
    .unwrap()
    .id
}

fn note_log_lines(root: &crate::fs::RootFs, note_id: &str) -> Vec<String> {
    let path = note_discussion_log_path(root, note_id).unwrap();
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect()
}

/// CMS-FR-62, CMS-FR-37, CMS-FR-56, CMS-FR-51: the opening writes the note's
/// own log in the store, and folds to a note-scoped discussion.
#[test]
fn opening_a_note_discussion_writes_its_own_committed_log() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = seed_note(root, "the rail's floor is wrong on a narrow tab");

    let (thread, created) = get_or_create_note_discussion_in(
        root,
        root,
        &note,
        "why is this still open?".into(),
        Vec::new(),
        &human("raver119"),
        "2025-03-04T11:00:00Z",
    )
    .unwrap();

    assert!(created, "the first call created the discussion");
    assert!(thread.note_id().is_some());
    assert!(!thread.is_fragment_targeted());
    assert_eq!(thread.note_id(), Some(note.as_str()));
    assert_eq!(thread.artifact_id(), None, "a note's discussion is about the note");
    assert_eq!(thread.draft_id(), None);
    assert!(thread.fragment_target.is_none(), "a discussion is pinned to nothing");
    assert_eq!(thread.comments.len(), 1, "exactly one opening comment");
    assert!(matches!(thread.comments[0].author, Participant::Human { .. }));

    // CMS-FR-37: `comments/notes/<note-id>/discussion.jsonl` inside the store.
    let lines = note_log_lines(root, &note);
    assert_eq!(lines.len(), 2, "one discussion_opened and one comment_added");
    assert!(dir
        .path()
        .join(format!("comments/notes/{note}/discussion.jsonl"))
        .is_file());
}

/// CMS-FR-62, CMS-FR-52: idempotent, and the exclusion holds under repetition.
#[test]
fn a_note_carries_at_most_one_discussion() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = seed_note(root, "check the FR numbering");
    let by = human("raver119");

    let (first, created) = get_or_create_note_discussion_in(
        root,
        root,
        &note,
        "the first opening".into(),
        Vec::new(),
        &by,
        "2025-03-04T11:00:00Z",
    )
    .unwrap();
    assert!(created);

    let (second, created_again) = get_or_create_note_discussion_in(
        root,
        root,
        &note,
        "a different opening message".into(),
        Vec::new(),
        &by,
        "2025-03-04T12:00:00Z",
    )
    .unwrap();

    assert!(!created_again, "the second call created nothing");
    assert_eq!(second.id, first.id, "one conversation, not two");
    assert_eq!(second.comments.len(), 1, "no second opening comment");
    assert_eq!(second.comments[0].body, "the first opening");
    assert_eq!(
        note_log_lines(root, &note).len(),
        2,
        "the later body was written nowhere",
    );
}

/// CMS-FR-62, CMS-FR-52 (concurrency half): two callers racing resolve to one thread
/// with one opening comment.
#[test]
fn concurrent_openings_resolve_to_one_discussion() {
    let dir = temp_root();
    let root_path = dir.path().to_path_buf();
    let root = &crate::fs::RootFs::for_root(&root_path);
    let note = seed_note(root, "a race");

    let ids: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let root_path = root_path.clone();
                let note = note.clone();
                scope.spawn(move || {
                    let root = crate::fs::RootFs::for_root(&root_path);
                    get_or_create_note_discussion_in(
                        &root,
                        &root,
                        &note,
                        format!("opening {i}"),
                        Vec::new(),
                        &Participant::Human {
                            login: "raver119".into(),
                            display_name: None,
                            email: None,
                        },
                        "2025-03-04T11:00:00Z",
                    )
                    .unwrap()
                    .0
                    .id
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let first = &ids[0];
    assert!(
        ids.iter().all(|id| id == first),
        "every caller got the same thread: {ids:?}",
    );
    assert_eq!(
        note_log_lines(root, &note).len(),
        2,
        "one discussion_opened and one comment_added, however many raced",
    );
    assert_eq!(fold_note_discussion(root, &note).len(), 1);
}

/// CMS-FR-62, CMS-FR-57: the refusals, none of which writes anything.
#[test]
fn a_note_discussion_refuses_an_unknown_note_an_empty_body_and_open_discussion() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let by = human("raver119");

    // An id naming no note.
    let err = get_or_create_note_discussion_in(
        root,
        root,
        "note-nobody",
        "hello".into(),
        Vec::new(),
        &by,
        "2025-03-04T11:00:00Z",
    )
    .unwrap_err();
    assert_eq!(err, ERR_NOTE_NOT_FOUND);
    assert!(
        !dir.path().join("comments/notes").exists(),
        "no folder was created for a note that does not exist",
    );

    // A body of whitespace alone.
    let note = seed_note(root, "a real note");
    let err = get_or_create_note_discussion_in(
        root,
        root,
        &note,
        "   \n  ".into(),
        Vec::new(),
        &by,
        "2025-03-04T11:00:00Z",
    )
    .unwrap_err();
    assert_eq!(err, ERR_EMPTY_BODY);
    assert!(
        note_discussion_of(root, &note).is_none(),
        "the note still carries no discussion",
    );

    // CMS-FR-57: `open_discussion_thread` cannot open a note's, before or
    // after it has one — it is the operation that could open a second.
    for _ in 0..2 {
        let err = open_discussion_in(
            root,
            root,
            &DiscussionTarget::Note {
                note_id: note.clone(),
            },
            None,
            "by the wrong door".into(),
            Vec::new(),
            &by,
            "2025-03-04T11:00:00Z",
        )
        .unwrap_err();
        assert_eq!(err, ERR_NOT_SUPPORTED);
        // Give it a discussion for the second pass.
        get_or_create_note_discussion_in(
            root,
            root,
            &note,
            "the real opening".into(),
            Vec::new(),
            &by,
            "2025-03-04T11:00:00Z",
        )
        .unwrap();
    }
    assert_eq!(fold_note_discussion(root, &note).len(), 1);
}

/// CMS-FR-54, CMS-FR-49, CMS-FR-41, CMS-FR-59, CMS-FR-61, CMS-FR-40: every other operation behaves exactly as it does on an
/// artifact discussion, and the thread is served by `read_comment_thread`
/// although no project-wide list returns it.
#[test]
fn a_note_discussion_behaves_like_any_other_discussion() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = seed_note(root, "a note worth discussing");
    let by = human("raver119");
    let (thread, _) = get_or_create_note_discussion_in(
        root,
        root,
        &note,
        "opening".into(),
        Vec::new(),
        &by,
        "2025-03-04T11:00:00Z",
    )
    .unwrap();
    let target = ThreadRef::note_discussion(&note);

    // A reply, an agent's answer, a lock, and a resolution.
    add_comment_to(
        root,
        target,
        &thread.id,
        "a reply".into(),
        Vec::new(),
        Vec::new(),
        &by,
        "2025-03-04T12:00:00Z",
    )
    .unwrap();
    append_agent_comment_to(
        root,
        target,
        &thread.id,
        "an answer".into(),
        &agent("a1", "arch"),
        "2025-03-04T12:30:00Z",
    )
    .unwrap();
    let (resolved, changed) =
        set_resolution_to(root, target, &thread.id, true, &by, "2025-03-04T13:00:00Z")
            .unwrap();
    assert!(resolved.resolved && changed);
    let (locked, changed) =
        set_lock_to(root, target, &thread.id, true, &by, "2025-03-04T13:30:00Z").unwrap();
    assert!(locked.locked && changed);
    assert_eq!(locked.comments.len(), 3, "opening, reply, agent answer");

    // CMS-FR-59: the one command a discussion refuses, in every scope.
    let err = reanchor_to(
        root,
        target,
        &thread.id,
        anchor(0, 5, "intro"),
        &by,
        "2025-03-04T14:00:00Z",
    )
    .unwrap_err();
    assert_eq!(err, ERR_NOT_FRAGMENT_TARGETED);

    // CMS-FR-49: a blob attached in a note's conversation lands in that
    // note's own folder and is served back from it. Without this the Note
    // arm of `attachment_scope` could fall through to the artifact folder
    // and every other assertion here would still hold.
    let with_blob = add_comment_to(
        root,
        target,
        &thread.id,
        "with a picture".into(),
        Vec::new(),
        vec![AttachmentInput::Inline {
            filename: "diff.png".into(),
            media_type: "image/png".into(),
            data: b64(b"PNGBYTES"),
        }],
        &by,
        "2025-03-04T14:30:00Z",
    );
    // The thread was locked above, so unlock it for this append.
    let _ = with_blob;
    set_lock_to(root, target, &thread.id, false, &by, "2025-03-04T14:20:00Z").unwrap();
    let folded = add_comment_to(
        root,
        target,
        &thread.id,
        "with a picture".into(),
        Vec::new(),
        vec![AttachmentInput::Inline {
            filename: "diff.png".into(),
            media_type: "image/png".into(),
            data: b64(b"PNGBYTES"),
        }],
        &by,
        "2025-03-04T14:30:00Z",
    )
    .unwrap();
    let digest = match folded.comments.last().unwrap().attachments.first().unwrap() {
        Attachment::Blob { digest, .. } => digest.clone(),
        other => panic!("expected a blob, got {other:?}"),
    };
    assert!(
        dir.path()
            .join(format!("comments/notes/{note}/attachments/{digest}"))
            .is_file(),
        "the blob is in the note's own folder",
    );
    // Through the same two steps the command performs, so the Note arm of
    // `attachment_scope` and the note fallback in `locate_discussion_for_attachment`
    // are both exercised rather than bypassed.
    let located = locate_discussion_for_attachment(root, root, &thread.id, None)
        .expect("a note's thread is found without a draft id");
    assert!(located.note_id().is_some());
    let served = read_attachment_in(root, attachment_scope(&located), &located, &digest)
        .expect("served back");
    assert_eq!(served.data, b64(b"PNGBYTES"));
    assert_eq!(served.filename, "diff.png");

    // CMS-FR-61 / CMS-FR-40: served by id, and listed by the project-wide read
    // beside an artifact discussion under its note target.
    let read = read_discussion_by_id(root, root, &thread.id).expect("served by thread id");
    assert_eq!(read.id, thread.id);
    assert!(read.note_id().is_some());
    std::fs::write(dir.path().join("spec.md"), "a spec").unwrap();
    let artifact_thread = open(root, "spec.md", anchor(0, 1, "a"), "pinned", "2025-03-04T15:00:00Z");
    let listed = list_all_discussions_in(root, root);
    let mut listed_ids: Vec<String> = listed.iter().map(|i| i.discussion.id.clone()).collect();
    listed_ids.sort();
    let mut expected_ids = vec![artifact_thread.id.clone(), thread.id.clone()];
    expected_ids.sort();
    assert_eq!(
        listed_ids, expected_ids,
        "the artifact's thread and the note's discussion are both in the project-wide list",
    );
    let note_item = listed.iter().find(|i| i.discussion.id == thread.id).unwrap();
    assert!(matches!(note_item.discussion.target, DiscussionTarget::Note { .. }));
}

/// CMS-FR-62: a refused attachment leaves the note with **no** discussion —
/// not a log holding an opening event and no comment, and not a folder
/// presenting as a conversation.
#[test]
fn a_refused_attachment_leaves_the_note_without_a_discussion() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = seed_note(root, "a note");

    let err = get_or_create_note_discussion_in(
        root,
        root,
        &note,
        "with a bad attachment".into(),
        vec![AttachmentInput::Inline {
            filename: "thing.bin".into(),
            media_type: "application/octet-stream".into(),
            data: b64(b"BYTES"),
        }],
        &human("raver119"),
        "2025-03-04T11:00:00Z",
    )
    .unwrap_err();

    assert_eq!(err, ERR_UNSUPPORTED_MEDIA_TYPE);
    assert!(note_discussion_of(root, &note).is_none());
    assert!(
        note_log_lines(root, &note).is_empty(),
        "no log holding an opening event and no comment",
    );
    assert!(!note_discussion_index(root).contains_key(&note));
}

/// CMS-FR-58: `list_discussion_threads` for a note target.
#[test]
fn listing_a_note_target_returns_its_one_discussion_or_none() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = seed_note(root, "a note");

    assert!(fold_note_discussion(root, &note).is_empty());
    let (thread, _) = get_or_create_note_discussion_in(
        root,
        root, &note, "opening".into(), Vec::new(), &human("raver119"), "2025-03-04T11:00:00Z",
    )
    .unwrap();
    let listed = fold_note_discussion(root, &note);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, thread.id);
}

/// CMS-FR-37: a `note_id` that would escape the comments root is refused by
/// the FSA gate rather than naming a directory outside it.
#[test]
fn a_note_id_cannot_escape_the_comments_root() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    for hostile in ["../../escaped", "..", "a/../../b"] {
        assert!(
            note_comments_dir(root, hostile).is_err(),
            "{hostile:?} resolves outside the comments root",
        );
        assert!(note_discussion_of(root, hostile).is_none());
        // And the removal refuses rather than deleting something else.
        assert!(delete_note_discussion_in(root, hostile).is_err());
    }
    // The gate alone would admit these: they normalise to somewhere still
    // inside the project root. `is_note_id` is what refuses them.
    for hostile in ["a b", "a.b", "a/b", ""] {
        assert!(note_comments_dir(root, hostile).is_err(), "{hostile:?}");
    }
}

/// CMS-FR-63, CMS-FR-64, CMS-FR-49: the index, and the removal that takes one note's conversation
/// and nothing else.
#[test]
fn the_note_index_and_the_removal_are_scoped_to_one_note() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let by = human("raver119");
    let a = seed_note(root, "note a");
    let b = seed_note(root, "note b");
    let c = seed_note(root, "note c, undiscussed");

    let (thread_a, _) = get_or_create_note_discussion_in(
        root,
        root, &a, "about a".into(), Vec::new(), &by, "2025-03-04T11:00:00Z",
    )
    .unwrap();
    let (thread_b, _) = get_or_create_note_discussion_in(
        root,
        root, &b, "about b".into(), Vec::new(), &by, "2025-03-04T11:00:00Z",
    )
    .unwrap();
    // A blob in each, so the removal is shown not to reach a neighbour's.
    for (note, thread) in [(&a, &thread_a), (&b, &thread_b)] {
        add_comment_to(
            root,
            ThreadRef::note_discussion(note),
            &thread.id,
            "with a picture".into(),
            Vec::new(),
            vec![AttachmentInput::Inline {
                filename: "diff.png".into(),
                media_type: "image/png".into(),
                data: b64(b"PNGBYTES"),
            }],
            &by,
            "2025-03-04T12:00:00Z",
        )
        .unwrap();
    }

    // CMS-FR-63.
    let index = note_discussion_index(root);
    assert_eq!(index.get(&a), Some(&thread_a.id));
    assert_eq!(index.get(&b), Some(&thread_b.id));
    assert!(!index.contains_key(&c), "an undiscussed note has no entry");
    assert_eq!(index.len(), 2);

    let b_dir = dir.path().join(format!("comments/notes/{b}"));
    let b_before = std::fs::read_to_string(b_dir.join("discussion.jsonl")).unwrap();

    // CMS-FR-64.
    delete_note_discussion_in(root, &a).unwrap();
    assert!(!dir
        .path()
        .join(format!("comments/notes/{a}"))
        .exists());
    assert!(read_discussion_by_id(root, root, &thread_a.id).is_none());
    assert!(!note_discussion_index(root).contains_key(&a));

    // The neighbour is byte-for-byte what it was, blob included.
    assert_eq!(
        std::fs::read_to_string(b_dir.join("discussion.jsonl")).unwrap(),
        b_before,
    );
    assert_eq!(
        std::fs::read_dir(b_dir.join("attachments")).unwrap().count(),
        1,
        "b's blob is untouched",
    );
    assert!(read_discussion_by_id(root, root, &thread_b.id).is_some());

    // Idempotent: a second removal, and one for a note that never had a
    // discussion, both report success having removed nothing.
    delete_note_discussion_in(root, &a).unwrap();
    delete_note_discussion_in(root, &c).unwrap();
}
/// CMS-FR-62 / NTC-FR-22: an opening racing a deletion never leaves a
/// conversation for a note that is gone.
///
/// The window is between the opening's existence check and its append: a
/// deletion landing in it would remove the note while the opening was still
/// committed to writing. Both take one lock, so the two orders are the only
/// outcomes — the opening wins and the deletion removes what it wrote, or
/// the deletion wins and the opening refuses.
#[test]
fn an_opening_racing_a_deletion_never_orphans_a_discussion() {
    for _ in 0..24 {
        let dir = temp_root();
        let root_path = dir.path().to_path_buf();
        let root = &crate::fs::RootFs::for_root(&root_path);
        let note = seed_note(root, "about to be deleted");

        let (opened, deleted) = std::thread::scope(|scope| {
            let a = {
                let root_path = root_path.clone();
                let note = note.clone();
                scope.spawn(move || {
                    let root = crate::fs::RootFs::for_root(&root_path);
                    get_or_create_note_discussion_in(
                        &root,
                        &root,
                        &note,
                        "racing".into(),
                        Vec::new(),
                        &Participant::Human {
                            login: "raver119".into(),
                            display_name: None,
                            email: None,
                        },
                        "2025-03-04T11:00:00Z",
                    )
                    .is_ok()
                })
            };
            let b = {
                let root_path = root_path.clone();
                let note = note.clone();
                scope.spawn(move || {
                    let root = crate::fs::RootFs::for_root(&root_path);
                    crate::notes::delete_note_in(&root, &root, &note).is_ok()
                })
            };
            (a.join().unwrap(), b.join().unwrap())
        });

        let note_alive = crate::notes::note_exists(root, &note);
        let discussion = note_discussion_of(root, &note);
        let folder = dir
            .path()
            .join(format!("comments/notes/{note}"));

        if note_alive {
            // The deletion lost, or refused: the note is whole, and it
            // carries whatever the opening produced.
            assert_eq!(discussion.is_some(), opened, "a live note's discussion matches the opening");
        } else {
            // The note is gone, so nothing may remain of a conversation
            // about it — whichever order the two ran in (NTC-FR-22).
            assert!(deleted, "the note is gone, so the deletion succeeded");
            assert!(
                discussion.is_none(),
                "a deleted note has no discussion left",
            );
            assert!(
                !folder.exists(),
                "a deleted note leaves no conversation folder behind",
            );
            assert!(
                !note_discussion_index(root).contains_key(&note),
                "and no index entry",
            );
        }
    }
}
