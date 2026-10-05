//! The agent write path (CMS-FR-41, CMS-FR-17, CMS-FR-26).

use super::*;

// -- CMS-FR-41, CMS-FR-17, CMS-FR-26: the agent write path ------------------------------------

#[test]
fn a_lock_refuses_an_agent_exactly_as_it_refuses_a_person() {
    // CMS-FR-41, CMS-FR-17, CMS-FR-26.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "@arch can you tighten this?",
        "2026-01-01T00:00:00Z",
    );
    set_lock_in(
        root,
        "specs/a.md",
        &t.id,
        true,
        &human("ada"),
        "2026-01-01T00:00:01Z",
    )
    .unwrap();
    let lines_while_locked = std::fs::read_to_string(log_path(root, "specs/a.md").unwrap())
        .unwrap()
        .lines()
        .count();

    assert_eq!(
        append_agent_comment(
            root,
            LogScope::Artifact,
            "specs/a.md",
            &t.id,
            "Shortened to two lines.".into(),
            &agent("a1", "arch"),
            "2026-01-01T00:00:02Z",
        )
        .unwrap_err(),
        ERR_DISCUSSION_LOCKED,
    );
    assert_eq!(
        std::fs::read_to_string(log_path(root, "specs/a.md").unwrap())
            .unwrap()
            .lines()
            .count(),
        lines_while_locked,
        "a refused agent write gains the log no line",
    );

    // Unlocked, the same call appends one ordinary comment.
    set_lock_in(
        root,
        "specs/a.md",
        &t.id,
        false,
        &human("ada"),
        "2026-01-01T00:00:03Z",
    )
    .unwrap();
    let updated = append_agent_comment(
        root,
        LogScope::Artifact,
        "specs/a.md",
        &t.id,
        "Shortened to two lines.".into(),
        &agent("a1", "arch"),
        "2026-01-01T00:00:04Z",
    )
    .unwrap();
    assert_eq!(updated.comments.len(), 2);
    assert_eq!(updated.comments[1].body, "Shortened to two lines.");
    match &updated.comments[1].author {
        Participant::Agent {
            agent_id, handle, ..
        } => {
            assert_eq!(agent_id, "a1");
            assert_eq!(handle, "arch");
        }
        other => panic!("expected an agent author, got {other:?}"),
    }
    // The appended line is the same `comment_added` event with the same
    // fields: a reader of the raw log can tell who appended it only from the
    // participant already stamped in `by`.
    let raw = std::fs::read_to_string(log_path(root, "specs/a.md").unwrap()).unwrap();
    let last: serde_json::Value =
        serde_json::from_str(raw.lines().last().unwrap()).unwrap();
    assert_eq!(last["type"], "comment_added");
    assert_eq!(last["by"]["kind"], "agent");
    assert!(last.get("isAgent").is_none());
}

#[test]
fn a_draft_scoped_agent_comment_lands_in_the_store_under_the_drafts_id() {
    // CMS-FR-37 / CMS-FR-VJRP: a `draft`-scoped log lives in the repository
    // machine store under the draft's stable id, so nothing about where the
    // draft is filed reaches the path and nothing is written into the project.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    crate::drafts::create_drafts_folder_impl(root, "", "UI").unwrap();
    let draft = crate::drafts::create_draft_impl(root, Some("spec"), Some("UI"))
        .unwrap()
        .draft;
    let draft_id = draft.id.as_str();
    let file_rel = "ui/spec.md";
    let log_dir = draft_comments_dir(root, draft_id).unwrap();
    // Pinned to the literal path, not just to whatever the resolver returns:
    // seeding and reading back through one function would pass whatever it
    // said, the bug included.
    assert_eq!(
        log_dir,
        root.join("drafts").join(draft_id).join("comments"),
    );
    assert!(
        !root.join(DRAFTS_REL).join("UI").join(draft_id).join("comments").exists(),
        "and nothing sits inside the draft's own folder",
    );
    std::fs::create_dir_all(&log_dir).unwrap();
    let opened = serde_json::json!({
        "v": 1, "eventId": "e1", "threadId": "t1", "at": "2026-01-01T00:00:00Z",
        "by": { "kind": "human", "login": "ada" },
        "type": "thread_opened",
        "artifactPath": file_rel,
        "anchor": { "start": 0, "end": 3, "quote": "abc" },
    });
    let commented = serde_json::json!({
        "v": 1, "eventId": "e2", "threadId": "t1", "at": "2026-01-01T00:00:01Z",
        "by": { "kind": "human", "login": "ada" },
        "type": "comment_added",
        "commentId": "c1", "body": "@arch thoughts?", "quotes": [],
    });
    std::fs::write(
        log_dir.join(format!("{}.jsonl", log_id(file_rel))),
        format!("{opened}\n{commented}\n"),
    )
    .unwrap();

    let updated = append_agent_comment(
        root,
        LogScope::Draft { draft_id },
        file_rel,
        "t1",
        "Two specs, I think.".into(),
        &agent("a1", "arch"),
        "2026-01-01T00:00:02Z",
    )
    .unwrap();
    assert_eq!(updated.comments.len(), 2);
    assert!(
        !root.join(COMMENTS_REL).exists(),
        "nothing was written into the committed comments folder",
    );
    let draft_target = DiscussionTarget::Draft {
        draft_id: draft_id.to_string(),
    };
    assert_eq!(
        locate_discussion_of(root, &draft_target, "t1").unwrap().fragment_path(),
        file_rel,
    );
    // CMS-FR-VJRP: nothing under the drafts root was written by this module,
    // the log standing outside the worktree altogether.
    assert!(
        !root.join(DRAFTS_REL).join("UI").join(draft_id).join("comments").exists(),
    );
}

#[test]
fn a_thread_is_locatable_by_its_id_alone_in_both_scopes() {
    // The read path a dispatched agent turn takes: an origin names a
    // conversation, not the file it is anchored in.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "hello",
        "2026-01-01T00:00:00Z",
    );
    let found = read_discussion_by_id(root, root, &t.id).unwrap();
    assert_eq!(found.artifact_id(), Some("specs/a.md"));
    assert_eq!(found.id, t.id);
    assert!(read_discussion_by_id(root, root, "no-such-thread").is_none());
    let draft_target = DiscussionTarget::Draft {
        draft_id: "d1".to_string(),
    };
    assert!(locate_discussion_of(root, &draft_target, &t.id).is_none());
}
