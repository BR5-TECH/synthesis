//! The unified discussion: legacy lines read as one record, the fragment rules,
//! and one identity for one discussion.

use super::*;

fn write_log(path: &Path, lines: &[serde_json::Value]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
    std::fs::write(path, body).unwrap();
}

fn legacy_human() -> serde_json::Value {
    serde_json::json!({ "kind": "human", "login": "ada" })
}

/// The lines an older build wrote for one anchored thread: opened, two comments
/// (one quoting the other, one carrying an attachment), moved, locked, resolved.
fn legacy_thread_lines(thread_id: &str, opened_path: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "v": 1, "eventId": format!("{thread_id}-e1"), "threadId": thread_id,
            "at": "2026-01-01T00:00:00Z", "by": legacy_human(),
            "type": "thread_opened", "artifactPath": opened_path,
            "anchor": { "start": 0, "end": 3, "quote": "abc" },
        }),
        serde_json::json!({
            "v": 1, "eventId": format!("{thread_id}-e2"), "threadId": thread_id,
            "at": "2026-01-01T00:00:01Z", "by": legacy_human(),
            "type": "comment_added", "commentId": format!("{thread_id}-c1"),
            "body": "first", "quotes": [],
            "attachments": [{
                "kind": "url", "url": "https://example.test/a.png",
                "mediaType": "image/png", "label": "shot",
            }],
        }),
        serde_json::json!({
            "v": 1, "eventId": format!("{thread_id}-e3"), "threadId": thread_id,
            "at": "2026-01-01T00:00:02Z", "by": legacy_human(),
            "type": "comment_added", "commentId": format!("{thread_id}-c2"),
            "body": "reply",
            "quotes": [{ "commentId": format!("{thread_id}-c1"), "excerpt": "fir" }],
        }),
        serde_json::json!({
            "v": 1, "eventId": format!("{thread_id}-e4"), "threadId": thread_id,
            "at": "2026-01-01T00:00:03Z", "by": legacy_human(),
            "type": "thread_reanchored",
            "anchor": { "start": 10, "end": 20, "quote": "moved" },
        }),
        serde_json::json!({
            "v": 1, "eventId": format!("{thread_id}-e5"), "threadId": thread_id,
            "at": "2026-01-01T00:00:04Z", "by": legacy_human(), "type": "thread_locked",
        }),
        serde_json::json!({
            "v": 1, "eventId": format!("{thread_id}-e6"), "threadId": thread_id,
            "at": "2026-01-01T00:00:05Z", "by": legacy_human(), "type": "thread_resolved",
        }),
    ]
}

// -- CMS-FR-83: legacy lines are read as the unified record -------------------

/// CMS-FR-83, CMS-FR-04, CMS-FR-09, CMS-FR-16, CMS-FR-42: a log of `thread_opened`
/// and `thread_reanchored` lines folds to a unified discussion that keeps the id,
/// the comments, the quotes, the attachments, the lock and the resolution, and
/// whose fragment carries the owner and the path the log's location names.
#[test]
fn a_legacy_anchored_log_folds_to_a_unified_discussion_with_everything_kept() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    // The line records a path the file has since left; the log's own location
    // is what names the artifact (CMS-FR-33).
    touch(root, "specs/a.md");
    write_log(
        &log_path(root, "specs/a.md").unwrap(),
        &legacy_thread_lines("t1", "specs/long-gone.md"),
    );

    let folded = list_fragment_discussions_in(root, "specs/a.md");
    assert_eq!(folded.len(), 1);
    let discussion = &folded[0];
    assert_eq!(discussion.id, "t1", "the thread id is the discussion id");
    assert_eq!(discussion.target, artifact_target("specs/a.md"));
    assert_eq!(
        discussion.fragment_target,
        Some(FragmentTarget::in_artifact("specs/a.md", 10, 20, "moved")),
        "the legacy re-anchor is a fragment move",
    );
    assert!(discussion.locked && discussion.resolved);
    assert_eq!(
        discussion.comments.iter().map(|c| c.body.as_str()).collect::<Vec<_>>(),
        vec!["first", "reply"],
    );
    assert_eq!(discussion.comments[1].quotes[0].comment_id, "t1-c1");
    assert_eq!(discussion.comments[1].quotes[0].excerpt, "fir");
    assert!(matches!(
        discussion.comments[0].attachments[0],
        Attachment::Url { ref url, .. } if url == "https://example.test/a.png"
    ));
    assert_eq!(discussion.created_at, "2026-01-01T00:00:00Z");
    assert_eq!(discussion.updated_at, "2026-01-01T00:00:05Z");

    // Served by the same id from the id-only read and the project-wide read.
    assert_eq!(read_discussion_by_id(root, root, "t1").as_ref(), Some(discussion));
    let all = list_all_discussions_in(root, root);
    assert_eq!(all.len(), 1);
    assert_eq!(&all[0].discussion, discussion);
}

/// CMS-FR-83, CMS-FR-04, CMS-FR-06: folding is a pure function of the lines. A
/// second read, a re-fold, a duplicated `event_id` line, and a fold after a new
/// append all agree, and no read rewrites a legacy line.
#[test]
fn reading_a_legacy_log_again_changes_nothing_and_duplicate_lines_fold_once() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let path = log_path(root, "specs/a.md").unwrap();
    let mut lines = legacy_thread_lines("t1", "specs/a.md");
    write_log(&path, &lines);
    let bytes_before = std::fs::read(&path).unwrap();

    let first = list_fragment_discussions_in(root, "specs/a.md");
    let second = list_fragment_discussions_in(root, "specs/a.md");
    assert_eq!(first, second, "reading twice gives identical results");
    let events = parse_events(&String::from_utf8(bytes_before.clone()).unwrap());
    assert_eq!(
        fold_events("specs/a.md", events.clone()),
        fold_events("specs/a.md", events.clone()),
        "folding twice gives identical results",
    );
    assert_eq!(first, fold_events("specs/a.md", events));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        bytes_before,
        "no read rewrites a legacy line (CMS-FR-04)",
    );

    // The same lines with two of them duplicated, one of them out of place.
    let mut duplicated = lines.clone();
    duplicated.push(lines[1].clone());
    duplicated.insert(0, lines[2].clone());
    write_log(&path, &duplicated);
    assert_eq!(
        list_fragment_discussions_in(root, "specs/a.md"),
        first,
        "a duplicated event_id line folds exactly as the un-duplicated log does",
    );

    // A new unified line appended after the legacy ones: the legacy part reads
    // as it did, and a fold again agrees with itself.
    write_log(&path, &lines);
    // The legacy log holds a lock, which refuses a comment as it always did.
    let log = ThreadRef::artifact("specs/a.md");
    let refused = add_comment_to(
        root, log, "t1", "too early".into(), Vec::new(), Vec::new(),
        &human("raver119"), "2026-01-02T00:00:00Z",
    );
    assert_eq!(refused.unwrap_err(), ERR_DISCUSSION_LOCKED);
    let (unlocked, _) = set_lock_to(root, log, "t1", false, &human("raver119"), "2026-01-02T00:00:00Z").unwrap();
    assert!(!unlocked.locked);
    let appended = add_comment_to(
        root,
        ThreadRef::artifact("specs/a.md"),
        "t1",
        "third".into(),
        Vec::new(),
        Vec::new(),
        &human("raver119"),
        "2026-01-02T00:00:01Z",
    )
    .unwrap();
    assert_eq!(appended.comments.len(), 3);
    assert_eq!(appended.comments[..2], first[0].comments[..]);
    assert_eq!(appended.fragment_target, first[0].fragment_target);
    assert_eq!(appended.id, "t1");
    assert_eq!(list_fragment_discussions_in(root, "specs/a.md"), vec![appended]);
    let on_disk = std::fs::read_to_string(&path).unwrap();
    let kept: String = lines.drain(..).map(|line| format!("{line}\n")).collect();
    assert!(on_disk.starts_with(&kept), "the legacy lines stay in the file untouched");
}

/// CMS-FR-83, CMS-FR-QLDW, CMS-FR-JWVH: a legacy `discussion_opened` line naming
/// only a `draftId` folds as a whole-target discussion of that draft, and the
/// pending question set an older build stored under `threadId` is still the set
/// of that discussion.
#[test]
fn a_legacy_draft_discussion_and_its_pending_question_set_survive() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft = make_draft(root, "legacy-review");
    write_log(
        &discussion_log_path(root, &draft).unwrap(),
        &[
            serde_json::json!({
                "v": 1, "eventId": "e1", "threadId": "d1", "at": "2026-01-01T00:00:00Z",
                "by": legacy_human(), "type": "discussion_opened", "draftId": draft,
            }),
            serde_json::json!({
                "v": 1, "eventId": "e2", "threadId": "d1", "at": "2026-01-01T00:00:01Z",
                "by": legacy_human(), "type": "comment_added", "commentId": "c1",
                "body": "@arch which way?", "quotes": [],
            }),
        ],
    );
    let set_path = ThreadRef::discussion(&draft)
        .scope
        .question_set_path(root, "d1")
        .unwrap();
    std::fs::create_dir_all(set_path.parent().unwrap()).unwrap();
    std::fs::write(
        &set_path,
        serde_json::json!({
            "setId": "s1", "threadId": "d1",
            "askedBy": { "kind": "agent", "agentId": "a1", "handle": "arch" },
            "askedAt": "2026-01-01T00:00:02Z",
            "questions": [{
                "position": 1, "text": "One spec or two?",
                "options": [{ "position": 1, "value": "one" }, { "position": 2, "value": "two" }],
            }],
        })
        .to_string(),
    )
    .unwrap();

    let found = read_discussion_by_id(root, root, "d1").expect("the discussion is read by id");
    assert_eq!(found.target, DiscussionTarget::Draft { draft_id: draft.clone() });
    assert_eq!(found.fragment_target, None, "no fragment: a whole-target discussion");
    assert_eq!(found.comments.len(), 1);
    assert_eq!(list_discussions_in(root, &found.target), vec![found.clone()]);

    let set = read_question_set(root, root, "d1").expect("the pending set survives");
    assert_eq!(set.set_id, "s1");
    assert_eq!(set.discussion_id, "d1", "the old `threadId` is the discussion id");
    assert_eq!(set.questions.len(), 1);
    assert_eq!(read_question_set(root, root, "d1"), Some(set), "and reads the same again");
}

// -- CMS-FR-84: the fragment rules --------------------------------------------

/// CMS-FR-84, CMS-FR-57, CMS-FR-53, CMS-FR-55: a fragment is accepted for an
/// artifact owner and for a draft owner, each into the fragment log of its owner,
/// and an absent fragment is a whole-target discussion in the owner's reserved log.
#[test]
fn a_fragment_is_accepted_for_artifact_and_draft_owners_and_its_absence_means_the_whole_target() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let by = human("raver119");
    touch(root, "specs/a.md");
    let draft = make_draft(root, "prompt");
    let artifact = artifact_target("specs/a.md");
    let draft_target = DiscussionTarget::Draft { draft_id: draft.clone() };
    let open_with = |target: &DiscussionTarget, fragment: Option<FragmentTarget>| {
        open_discussion_in(root, root, target, fragment, "hello".into(), Vec::new(), &by, "2026-02-01T00:00:00Z")
    };

    let on_artifact = open_with(&artifact, Some(FragmentTarget::in_artifact("specs/a.md", 0, 3, "abc"))).unwrap();
    assert_eq!(on_artifact.fragment_target, Some(FragmentTarget::in_artifact("specs/a.md", 0, 3, "abc")));
    assert_eq!(on_artifact.log_ref(), ThreadRef::artifact("specs/a.md"));

    let on_draft = open_with(&draft_target, Some(FragmentTarget::in_draft(&draft, "ui/spec.md", 2, 5, "xyz"))).unwrap();
    assert_eq!(on_draft.fragment_target, Some(FragmentTarget::in_draft(&draft, "ui/spec.md", 2, 5, "xyz")));
    assert_eq!(on_draft.log_ref(), ThreadRef::draft_file(&draft, "ui/spec.md"));

    // The path of an artifact fragment is the artifact's own, whatever is sent.
    let mut sent = FragmentTarget::in_artifact("specs/a.md", 0, 3, "abc");
    sent.path = "elsewhere.md".into();
    let forced = open_with(&artifact, Some(sent)).unwrap();
    assert_eq!(forced.fragment_target.unwrap().path, "specs/a.md");

    for (target, reserved) in [
        (&artifact, ThreadRef::artifact_discussion("specs/a.md")),
        (&draft_target, ThreadRef::discussion(&draft)),
    ] {
        let whole = open_with(target, None).unwrap();
        assert_eq!(whole.fragment_target, None, "absent fragment: the whole target");
        assert!(!whole.is_fragment_targeted());
        assert_eq!(whole.log_ref(), reserved);
    }

    // The owner serves its fragment discussions first, whole-target ones after.
    let listed = list_discussions_in(root, &artifact);
    assert_eq!(
        listed.iter().map(|d| d.is_fragment_targeted()).collect::<Vec<_>>(),
        vec![true, true, false],
    );
}

/// CMS-FR-84, CMS-FR-57, CMS-FR-62, CMS-FR-21: a fragment is refused with a typed
/// error and nothing is appended when it names a note owner, differs from the
/// discussion's own target, is empty, or names no file of a draft; a note target
/// takes no fragment at all.
#[test]
fn a_fragment_is_refused_with_a_typed_error_for_notes_and_malformed_fragments() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let by = human("raver119");
    touch(root, "specs/a.md");
    let draft = make_draft(root, "prompt");
    let before = list_all_discussions_in(root, root).len();
    let attempt = |target: &DiscussionTarget, fragment: FragmentTarget| {
        open_discussion_in(root, root, target, Some(fragment), "hello".into(), Vec::new(), &by, "2026-02-01T00:00:00Z")
            .unwrap_err()
    };
    let artifact = artifact_target("specs/a.md");
    let note = DiscussionTarget::Note { note_id: "n1".into() };
    let note_fragment = FragmentTarget {
        owner: note.clone(),
        path: "n1".into(),
        start: 0,
        end: 3,
        quote: "abc".into(),
    };

    // A note owner has no source to point into.
    assert_eq!(attempt(&artifact, note_fragment.clone()), ERR_INVALID_FRAGMENT);
    // A note target takes no fragment.
    assert_eq!(attempt(&note, note_fragment), ERR_NOT_SUPPORTED);
    assert_eq!(
        attempt(&note, FragmentTarget::in_artifact("specs/a.md", 0, 3, "abc")),
        ERR_NOT_SUPPORTED,
    );
    // An owner that is not the discussion's own target.
    assert_eq!(
        attempt(&artifact, FragmentTarget::in_draft(&draft, "ui/spec.md", 0, 3, "abc")),
        ERR_INVALID_FRAGMENT,
    );
    // An end that is not past its start.
    assert_eq!(attempt(&artifact, FragmentTarget::in_artifact("specs/a.md", 3, 3, "")), ERR_INVALID_FRAGMENT);
    assert_eq!(attempt(&artifact, FragmentTarget::in_artifact("specs/a.md", 5, 2, "x")), ERR_INVALID_FRAGMENT);
    // A draft fragment naming no file.
    let draft_target = DiscussionTarget::Draft { draft_id: draft.clone() };
    assert_eq!(attempt(&draft_target, FragmentTarget::in_draft(&draft, "  ", 0, 3, "abc")), ERR_INVALID_FRAGMENT);

    assert_eq!(list_all_discussions_in(root, root).len(), before, "nothing was appended");
    assert!(list_discussions_in(root, &artifact).is_empty());
}

/// CMS-FR-84, CMS-FR-53, CMS-FR-59, CMS-FR-21: a discussion opened for a Flow, a
/// Diff or a History view sends no fragment target, so it is whole-target: stored
/// in the reserved log, never held to a fragment, and refused a fragment move.
#[test]
fn a_flow_diff_or_history_discussion_is_whole_target_and_cannot_be_given_a_fragment() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let by = human("raver119");
    touch(root, "flows/review.flow");
    let whole = open_artifact_discussion(root, "flows/review.flow", "which node?", "2026-02-01T00:00:00Z");
    assert_eq!(whole.fragment_target, None);
    assert_eq!(whole.log_ref(), ThreadRef::artifact_discussion("flows/review.flow"));

    // No later write gives it one: a move is the one command it refuses.
    let moved = move_fragment_to(
        root,
        whole.log_ref(),
        &whole.id,
        FragmentTarget::in_artifact("flows/review.flow", 0, 4, "node"),
        &by,
        "2026-02-01T00:00:01Z",
    );
    assert_eq!(moved.unwrap_err(), ERR_NOT_FRAGMENT_TARGETED);
    assert_eq!(
        read_discussion_by_id(root, root, &whole.id).unwrap().fragment_target,
        None,
    );
    assert_eq!(log_lines_of(root, whole.log_ref()), 2, "nothing was appended by the refusal");
}

fn log_lines_of(root: &crate::fs::RootFs, log: ThreadRef<'_>) -> usize {
    let path = log.scope.log_path(root, log.file_rel).unwrap();
    std::fs::read_to_string(path).unwrap_or_default().lines().count()
}

// -- CMS-FR-61, CMS-FR-28: one identity -----------------------------------------

/// A discussion of each owner and shape, opened in one store.
fn one_of_each(root: &crate::fs::RootFs) -> Vec<(&'static str, Discussion)> {
    let by = human("raver119");
    touch(root, "specs/a.md");
    let draft = make_draft(root, "prompt");
    let note = crate::notes::create_note_in(
        root,
        crate::notes::NoteScope::Project,
        "a note".into(),
        None,
        None,
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let artifact = artifact_target("specs/a.md");
    let draft_target = DiscussionTarget::Draft { draft_id: draft.clone() };
    let open_with = |target: &DiscussionTarget, fragment: Option<FragmentTarget>, at: &str| {
        open_discussion_in(root, root, target, fragment, "hello".into(), Vec::new(), &by, at).unwrap()
    };
    let (note_discussion, _) = get_or_create_note_discussion_in(
        root,
        root,
        &note.id,
        "about the note".into(),
        Vec::new(),
        &by,
        "2026-02-01T00:00:05Z",
    )
    .unwrap();
    vec![
        ("artifact fragment", open_with(&artifact, Some(FragmentTarget::in_artifact("specs/a.md", 0, 3, "abc")), "2026-02-01T00:00:00Z")),
        ("artifact whole", open_with(&artifact, None, "2026-02-01T00:00:01Z")),
        ("draft fragment", open_with(&draft_target, Some(FragmentTarget::in_draft(&draft, "ui/spec.md", 0, 3, "abc")), "2026-02-01T00:00:02Z")),
        ("draft whole", open_with(&draft_target, None, "2026-02-01T00:00:03Z")),
        ("note", note_discussion),
    ]
}

/// CMS-FR-61, CMS-FR-54, CMS-FR-23, CMS-FR-32, CMS-FR-49: the same id opened,
/// listed under its owner, listed project-wide, read by id and located for an
/// attachment read is one record, whichever route reaches it.
#[test]
fn one_discussion_id_reads_as_the_same_record_from_every_route() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let all = list_all_discussions_in(root, root);
    assert!(all.is_empty());
    let opened = one_of_each(root);
    let project_wide = list_all_discussions_in(root, root);
    assert_eq!(project_wide.len(), opened.len());

    for (label, discussion) in &opened {
        let by_id = read_discussion_by_id(root, root, &discussion.id)
            .unwrap_or_else(|| panic!("{label}: not found by id"));
        assert_eq!(&by_id, discussion, "{label}: by id");
        let by_owner = locate_discussion_of(root, &discussion.target, &discussion.id)
            .unwrap_or_else(|| panic!("{label}: not found under its owner"));
        assert_eq!(&by_owner, discussion, "{label}: under its owner");
        let listed = list_discussions_in(root, &discussion.target)
            .into_iter()
            .find(|d| d.id == discussion.id)
            .unwrap_or_else(|| panic!("{label}: not listed under its owner"));
        assert_eq!(&listed, discussion, "{label}: owner list");
        let item = project_wide
            .iter()
            .find(|i| i.discussion.id == discussion.id)
            .unwrap_or_else(|| panic!("{label}: not in the project-wide list"));
        assert_eq!(&item.discussion, discussion, "{label}: project-wide list");
        assert!(!item.owner_unavailable, "{label}: its owner resolves");
        let for_attachment =
            locate_discussion_for_attachment(root, root, &discussion.id, discussion.draft_id())
                .unwrap_or_else(|| panic!("{label}: not located for an attachment"));
        assert_eq!(&for_attachment, discussion, "{label}: attachment route");
        // A write through the id lands in the same record.
        let replied = add_comment_to(
            root,
            discussion.log_ref(),
            &discussion.id,
            "reply".into(),
            Vec::new(),
            Vec::new(),
            &human("ada"),
            "2026-03-01T00:00:00Z",
        )
        .unwrap();
        assert_eq!(replied.id, discussion.id, "{label}");
        assert_eq!(
            read_discussion_by_id(root, root, &discussion.id).unwrap().comments.len(),
            2,
            "{label}: one record, now with the reply",
        );
    }
    let ids: std::collections::HashSet<_> = opened.iter().map(|(_, d)| d.id.clone()).collect();
    assert_eq!(ids.len(), opened.len(), "five discussions, five identities");
}

/// CMS-FR-32, CMS-FR-33, CMS-FR-25, CMS-FR-39, CMS-FR-57, CMS-FR-61: a discussion
/// whose artifact file or note is gone stays readable and is listed as owner
/// unavailable; a deleted draft takes its discussions with it, and a discussion
/// of a missing owner can no longer be opened.
#[test]
fn a_discussion_of_a_missing_owner_stays_readable_and_a_deleted_draft_takes_its_own() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let opened = one_of_each(root);
    let id_of = |label: &str| opened.iter().find(|(l, _)| *l == label).unwrap().1.clone();
    let artifact_fragment = id_of("artifact fragment");
    let artifact_whole = id_of("artifact whole");
    let draft_whole = id_of("draft whole");
    let draft_fragment = id_of("draft fragment");
    let note = id_of("note");

    // The artifact file is deleted.
    std::fs::remove_file(root.join("specs/a.md")).unwrap();
    let items = list_all_discussions_in(root, root);
    let unavailable = |id: &str| items.iter().find(|i| i.discussion.id == id).unwrap().owner_unavailable;
    assert!(unavailable(&artifact_fragment.id), "the artifact is gone");
    assert!(unavailable(&artifact_whole.id), "the artifact is gone");
    assert!(!unavailable(&draft_whole.id), "the draft is still there");
    assert!(!unavailable(&note.id), "the note is still there");
    // Seen from a project that holds neither the note nor the draft.
    let bare_dir = temp_root();
    let bare = &crate::fs::RootFs::for_root(bare_dir.path());
    let bare_items = list_all_discussions_in(root, bare);
    let bare_unavailable =
        |id: &str| bare_items.iter().find(|i| i.discussion.id == id).unwrap().owner_unavailable;
    assert!(bare_unavailable(&note.id), "the note is not in that project");
    assert!(bare_unavailable(&draft_whole.id), "the draft is not in that project");
    for discussion in [&artifact_fragment, &artifact_whole, &note] {
        assert_eq!(
            read_discussion_by_id(root, root, &discussion.id).as_ref(),
            Some(discussion),
            "an unavailable owner does not hide the conversation",
        );
    }
    assert_eq!(
        open_discussion_in(
            root,
            root,
            &artifact_target("specs/a.md"),
            None,
            "again".into(),
            Vec::new(),
            &human("raver119"),
            "2026-03-01T00:00:00Z",
        )
        .unwrap_err(),
        ERR_ARTIFACT_NOT_FOUND,
        "a whole-target discussion of a file the project does not hold is refused",
    );

    // The draft is deleted: its discussions go, and nothing else does.
    let DiscussionTarget::Draft { draft_id } = &draft_whole.target else {
        panic!("a draft target")
    };
    crate::drafts::delete_draft_impl(root, root, draft_id).unwrap();
    for gone in [&draft_whole, &draft_fragment] {
        assert!(read_discussion_by_id(root, root, &gone.id).is_none());
    }
    assert!(list_discussions_in(root, &draft_whole.target).is_empty());
    assert_eq!(
        open_discussion_in(
            root,
            root,
            &draft_whole.target,
            None,
            "again".into(),
            Vec::new(),
            &human("raver119"),
            "2026-03-01T00:00:00Z",
        )
        .unwrap_err(),
        ERR_DRAFT_NOT_FOUND,
    );
    for kept in [&artifact_fragment, &artifact_whole, &note] {
        assert!(read_discussion_by_id(root, root, &kept.id).is_some(), "untouched by the deletion");
    }
}

fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_files(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// CMS-FR-28, CMS-FR-37, CMS-FR-VJRP: switching the project's worktree selects the
/// same store, so every discussion reads the same, an append lands in the same log,
/// and no log is copied into or created in either worktree.
#[test]
fn a_worktree_switch_keeps_the_same_logs() {
    let store_dir = temp_root();
    let first_dir = temp_root();
    let second_dir = temp_root();
    let store = &crate::fs::RootFs::for_root(store_dir.path());
    let first = &crate::fs::RootFs::for_root(first_dir.path());
    let second = &crate::fs::RootFs::for_root(second_dir.path());
    for worktree in [first, second] {
        touch(worktree, "specs/a.md");
    }
    let by = human("raver119");
    let artifact = artifact_target("specs/a.md");
    let fragment = open_discussion_in(
        store, first, &artifact,
        Some(FragmentTarget::in_artifact("specs/a.md", 0, 3, "abc")),
        "pinned".into(), Vec::new(), &by, "2026-02-01T00:00:00Z",
    )
    .unwrap();
    let whole = open_discussion_in(
        store, first, &artifact, None,
        "whole".into(), Vec::new(), &by, "2026-02-01T00:00:01Z",
    )
    .unwrap();
    let logs_before = walk_files(store_dir.path());

    // Read through the other worktree: the same discussions, byte for byte.
    for expected in [&fragment, &whole] {
        assert_eq!(
            read_discussion_by_id(store, second, &expected.id).as_ref(),
            Some(expected),
            "the same record after the switch",
        );
    }
    let items = list_all_discussions_in(store, second);
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| !i.owner_unavailable));
    assert_eq!(walk_files(store_dir.path()), logs_before, "reading moves and copies no log");

    // Append through the other worktree: it lands in the log the first one wrote.
    let replied = add_comment_to(
        store,
        fragment.log_ref(),
        &fragment.id,
        "from the second checkout".into(),
        Vec::new(),
        Vec::new(),
        &by,
        "2026-02-02T00:00:00Z",
    )
    .unwrap();
    assert_eq!(replied.comments.len(), 2);
    assert_eq!(
        read_discussion_by_id(store, first, &fragment.id).unwrap().comments.len(),
        2,
        "the first worktree sees the append at once",
    );
    assert_eq!(walk_files(store_dir.path()), logs_before, "the append created no new log");
    for worktree_dir in [&first_dir, &second_dir] {
        assert!(
            !walk_files(worktree_dir.path())
                .iter()
                .any(|p| p.to_string_lossy().contains("jsonl")),
            "no log stands inside a worktree",
        );
    }

    // A worktree that does not hold the file lists the owner as unavailable and
    // still serves the discussion.
    let bare_dir = temp_root();
    let bare = &crate::fs::RootFs::for_root(bare_dir.path());
    let bare_items = list_all_discussions_in(store, bare);
    assert_eq!(bare_items.len(), 2);
    assert!(bare_items.iter().all(|i| i.owner_unavailable));
    assert!(read_discussion_by_id(store, bare, &whole.id).is_some());
}
