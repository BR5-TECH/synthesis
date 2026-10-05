//! Artifact and draft discussions (CMS-FR-53 ... CMS-FR-59).

use super::*;

// -- Artifact discussions (CMS-FR-55 … CMS-FR-58) -----------------------

/// A real file on disk, because `open_discussion_thread` refuses a target
/// naming a path the project does not hold (CMS-FR-57).
fn make_artifact(root: &Path, rel: &str, body: &str) -> String {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, body).unwrap();
    rel.to_string()
}

/// A discussion log written before the event carried a target still reads.
///
/// The old shape named its draft directly (`"draftId"`), and such lines are
/// on disk in draft folders now. A log is append-only and never
/// rewritten (CMS-FR-01), so a reader that refused the old shape would not
/// migrate those conversations — it would silently erase them, the fold
/// dropping the opening event and CMS-FR-15 then filtering out the
/// comment-less thread.
#[test]
fn a_discussion_log_written_before_targets_existed_still_folds() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft = make_draft(root, "artifact-window");
    let path = discussion_log_path(root, &draft).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        format!(
            "{}\n{}\n",
            serde_json::json!({
                "v": 1,
                "eventId": "e1",
                "threadId": "t-legacy",
                "at": "2026-02-01T00:00:00Z",
                "by": { "kind": "human", "login": "raver119" },
                "type": "discussion_opened",
                "draftId": draft,
            }),
            serde_json::json!({
                "v": 1,
                "eventId": "e2",
                "threadId": "t-legacy",
                "at": "2026-02-01T00:00:01Z",
                "by": { "kind": "human", "login": "raver119" },
                "type": "comment_added",
                "commentId": "c1",
                "body": "rework the graduation part",
                "quotes": [],
            }),
        ),
    )
    .unwrap();

    let folded = fold_discussion(root, &draft);
    assert_eq!(folded.len(), 1, "the conversation is still there");
    assert_eq!(folded[0].id, "t-legacy");
    assert!(!folded[0].is_fragment_targeted());
    assert!(folded[0].draft_id().is_some());
    assert_eq!(folded[0].draft_id(), Some(draft.as_str()));
    assert_eq!(folded[0].comments.len(), 1);
    assert_eq!(folded[0].comments[0].body, "rework the graduation part");

    // And a comment appended now joins it rather than starting a second
    // thread: the two shapes are one conversation.
    add_comment_to(
        root,
        ThreadRef::discussion(&draft),
        "t-legacy",
        "and the archive part?".into(),
        Vec::new(),
        Vec::new(),
        &human("raver119"),
        "2026-02-02T00:00:00Z",
    )
    .unwrap();
    let after = fold_discussion(root, &draft);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].comments.len(), 2);
}

/// CMS-FR-55, CMS-FR-56, CMS-FR-57, CMS-FR-01, CMS-FR-37: the reserved log lands beside the anchored one, the anchored one
/// gains no line, and the thread folds as an artifact-scoped discussion.
#[test]
fn an_artifact_discussion_writes_a_reserved_log_beside_the_anchored_one() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let artifact = make_artifact(root, "specifications/ui/EDT-editor.md", "# Editor\n");
    open(root, &artifact, anchor(0, 8, "# Editor"), "pinned", "2026-02-01T00:00:00Z");

    let thread = open_artifact_discussion(root, &artifact, "@arch is this ready?", "2026-02-02T00:00:00Z");

    let discussion_log = root.join(artifact_discussion_log_rel(&artifact));
    let anchored_log = root.join(log_rel(&artifact));
    assert!(discussion_log.is_file(), "the reserved log is written");
    assert!(anchored_log.is_file());
    // Two lines: the opening event and its first comment, in one append.
    let lines: Vec<String> = std::fs::read_to_string(&discussion_log)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("discussion_opened"));
    assert!(lines[0].contains("\"kind\":\"artifact\""));
    // CMS-FR-57: the anchored log beside it gained nothing.
    assert_eq!(
        std::fs::read_to_string(&anchored_log).unwrap().lines().count(),
        2
    );

    assert!(!thread.is_fragment_targeted());
    assert!(thread.artifact_id().is_some());
    assert_eq!(thread.artifact_id(), Some(artifact.as_str()));
    assert!(thread.fragment_target.is_none());
    assert!(thread.draft_id().is_none());
    assert_eq!(thread.comments.len(), 1);

    // CMS-FR-56: beside the anchored log, in the store's own comments folder.
    assert!(root
        .join(COMMENTS_REL)
        .join(format!("{}{DISCUSSION_LOG_MARKER}.jsonl", log_id(&artifact)))
        .is_file());
}

/// CMS-FR-58, CMS-FR-40, CMS-FR-32: each list command serves exactly its own kind, and the
/// project-wide read serves both because the *scope* is what decides.
#[test]
fn the_list_commands_divide_a_files_anchored_threads_from_its_discussions() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let artifact = make_artifact(root, "specifications/ui/EDT-editor.md", "# Editor\n");
    open(root, &artifact, anchor(0, 8, "# Editor"), "one", "2026-02-01T00:00:00Z");
    open(root, &artifact, anchor(0, 8, "# Editor"), "two", "2026-02-01T00:00:01Z");
    let discussion = open_artifact_discussion(root, &artifact, "whole thing", "2026-02-02T00:00:00Z");

    let anchored = list_fragment_discussions_in(root, &artifact);
    assert_eq!(anchored.len(), 2);
    assert!(anchored.iter().all(|t| t.is_fragment_targeted()));
    assert!(!anchored.iter().any(|t| t.id == discussion.id));

    let discussions = fold_artifact_discussion(root, &artifact);
    assert_eq!(discussions.len(), 1);
    assert_eq!(discussions[0].id, discussion.id);

    // CMS-FR-40: the scope decides, not the kind.
    let all = list_all_discussions_in(root, root);
    assert_eq!(all.len(), 3);
    assert!(all.iter().any(|i| i.discussion.id == discussion.id));
    let listed = all.iter().find(|i| i.discussion.id == discussion.id).unwrap();
    assert_eq!(listed.discussion.artifact_id(), Some(artifact.as_str()));
    assert!(!listed.owner_unavailable);
}

/// CMS-FR-58, CMS-FR-40, CMS-FR-32: a draft's discussions are part of the project-wide read,
/// each under its draft target.
#[test]
fn a_drafts_discussions_reach_the_project_wide_read_under_their_draft_target() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft = make_draft(root, "artifact-window");
    let opened = open_discussion(root, &draft, "private", "2026-02-01T00:00:00Z");
    let all = list_all_discussions_in(root, root);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].discussion.id, opened.id);
    assert_eq!(all[0].discussion.target, DiscussionTarget::Draft { draft_id: draft });
    assert!(!all[0].owner_unavailable);
}

/// CMS-FR-24, CMS-FR-33, CMS-FR-55: a rename moves both logs together, the marker intact.
#[test]
fn a_rename_follows_both_of_a_files_logs() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let old = make_artifact(root, "specs/old.md", "# Old\n");
    open(root, &old, anchor(0, 5, "# Old"), "pinned", "2026-02-01T00:00:00Z");
    let discussion = open_artifact_discussion(root, &old, "whole", "2026-02-02T00:00:00Z");

    std::fs::rename(root.join("specs/old.md"), root.join("specs/new.md")).unwrap();
    let moved = follow_rename(root, root, "specs/old.md", "specs/new.md");
    assert_eq!(moved, 2, "both logs move");

    assert!(root.join(log_rel("specs/new.md")).is_file());
    assert!(root.join(artifact_discussion_log_rel("specs/new.md")).is_file());
    assert!(!root.join(artifact_discussion_log_rel("specs/old.md")).exists());

    let after = fold_artifact_discussion(root, "specs/new.md");
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, discussion.id);
    assert_eq!(after[0].artifact_id(), Some("specs/new.md"));
}

/// A file discussed but never annotated still follows its rename — neither log
/// is a precondition for moving the other.
#[test]
fn a_rename_follows_a_discussion_log_with_no_anchored_log_beside_it() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let old = make_artifact(root, "specs/old.md", "# Old\n");
    open_artifact_discussion(root, &old, "whole", "2026-02-02T00:00:00Z");

    std::fs::rename(root.join("specs/old.md"), root.join("specs/new.md")).unwrap();
    assert_eq!(follow_rename(root, root, "specs/old.md", "specs/new.md"), 1);
    assert_eq!(fold_artifact_discussion(root, "specs/new.md").len(), 1);
}

/// CMS-FR-54, CMS-FR-49, CMS-FR-59: every other command reaches an artifact discussion through the
/// `artifactId` locator, and the one it refuses is the re-anchor.
#[test]
fn every_command_but_the_reanchor_serves_an_artifact_discussion() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let artifact = make_artifact(root, "specs/spec.md", "# Spec\n");
    // An anchored thread on the same file, so the locator is genuinely
    // ambiguous and has to be resolved by looking.
    let anchored = open(root, &artifact, anchor(0, 6, "# Spec"), "pinned", "2026-02-01T00:00:00Z");
    let discussion = open_artifact_discussion(root, &artifact, "whole", "2026-02-02T00:00:00Z");

    let target = target_of(root, Some(&artifact), None, None, &discussion.id).unwrap();
    assert_eq!(target.scope, LogScope::ArtifactDiscussion { artifact_id: &artifact });
    // The anchored thread still resolves to the anchored log.
    let anchored_ref = target_of(root, Some(&artifact), None, None, &anchored.id).unwrap();
    assert_eq!(anchored_ref.scope, LogScope::Artifact);

    let replied = add_comment_to(
        root,
        target,
        &discussion.id,
        "and the archive part?".into(),
        Vec::new(),
        Vec::new(),
        &human("raver119"),
        "2026-02-03T00:00:00Z",
    )
    .unwrap();
    assert_eq!(replied.comments.len(), 2);

    let (locked, changed) = set_lock_to(
        root,
        target,
        &discussion.id,
        true,
        &human("raver119"),
        "2026-02-04T00:00:00Z",
    )
    .unwrap();
    assert!(locked.locked && changed);
    let (resolved, changed) = set_resolution_to(
        root,
        target,
        &discussion.id,
        true,
        &human("raver119"),
        "2026-02-05T00:00:00Z",
    )
    .unwrap();
    assert!(resolved.resolved && changed);

    // CMS-FR-59: the one command a discussion refuses, in either scope.
    assert_eq!(
        reanchor_to(
            root,
            target,
            &discussion.id,
            anchor(0, 5, "intro"),
            &human("raver119"),
            "2026-02-06T00:00:00Z",
        )
        .unwrap_err(),
        ERR_NOT_FRAGMENT_TARGETED
    );

    // The anchored thread beside it is untouched by all of that.
    let still = list_fragment_discussions_in(root, &artifact);
    assert_eq!(still.len(), 1);
    assert!(!still[0].locked);
    assert!(!still[0].resolved);
}

/// CMS-FR-33: a discussion whose file has gone still lists, and still names
/// the file it was about — a conversation outlives the thing it discussed.
#[test]
fn a_discussion_on_a_deleted_file_still_lists_as_unresolved() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let artifact = make_artifact(root, "specs/gone.md", "# Gone\n");
    let thread = open_artifact_discussion(root, &artifact, "whole", "2026-02-01T00:00:00Z");
    std::fs::remove_file(root.join(&artifact)).unwrap();

    let all = list_all_discussions_in(root, root);
    let listed = all
        .iter()
        .find(|i| i.discussion.id == thread.id)
        .expect("the conversation outlives the file");
    assert!(listed.owner_unavailable, "the ARTIFACT is what is unresolved");
    assert_eq!(listed.discussion.artifact_id(), Some(artifact.as_str()));
}

/// CMS-FR-40: a draft's conversation never reaches the project-wide read,
/// even if its log is somehow sitting in the committed folder.
#[test]
fn a_draft_targeted_log_in_the_committed_folder_contributes_nothing() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let path = root.join(COMMENTS_REL).join("deadbeef.discussion.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::json!({
                "v": 1, "eventId": "e1", "threadId": "t1",
                "at": "2026-02-01T00:00:00Z",
                "by": { "kind": "human", "login": "raver119" },
                "type": "discussion_opened",
                "target": { "kind": "draft", "draftId": "d1" },
            }),
        ),
    )
    .unwrap();
    // It names no artifact, so it is skipped rather than leaking a draft's
    // private conversation into the panel that serves the project.
    assert!(list_all_discussions_in(root, root).is_empty());
}

/// A thread id in neither of a file's two logs is `thread_not_found` — the
/// ambiguous locator resolves by looking, and finding nothing is an answer.
#[test]
fn a_thread_id_in_neither_log_is_not_found() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let artifact = make_artifact(root, "specs/spec.md", "# Spec\n");
    open(root, &artifact, anchor(0, 6, "# Spec"), "pinned", "2026-02-01T00:00:00Z");
    open_artifact_discussion(root, &artifact, "whole", "2026-02-02T00:00:00Z");

    assert_eq!(
        target_of(root, Some(&artifact), None, None, "no-such-thread").unwrap_err(),
        ERR_DISCUSSION_NOT_FOUND
    );
    // And a thread belonging to a DIFFERENT file does not resolve here either.
    let other = make_artifact(root, "specs/other.md", "# Other\n");
    let elsewhere = open_artifact_discussion(root, &other, "theirs", "2026-02-03T00:00:00Z");
    assert_eq!(
        target_of(root, Some(&artifact), None, None, &elsewhere.id).unwrap_err(),
        ERR_DISCUSSION_NOT_FOUND
    );
}

/// CMS-FR-57: a target that escapes the project, or names a directory, is
/// refused on the same terms a missing file is.
#[test]
fn a_target_that_escapes_or_names_a_directory_is_refused() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("specs")).unwrap();
    for bad in ["../outside.md", "specs"] {
        assert_eq!(
            open_discussion_in(
                root,
                root,
                &artifact_target(bad),
                None,
                "hello".into(),
                Vec::new(),
                &human("raver119"),
                "2026-02-01T00:00:00Z",
            )
            .unwrap_err(),
            ERR_ARTIFACT_NOT_FOUND,
            "refused: {bad}"
        );
    }
    assert!(list_all_discussions_in(root, root).is_empty());
}

/// CMS-FR-57: a target naming nothing the project holds is a typed refusal
/// that writes nothing.
#[test]
fn a_target_naming_nothing_is_refused_without_a_write() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    assert_eq!(
        open_discussion_in(
            root,
            root,
            &artifact_target("specs/absent.md"),
            None,
            "hello".into(),
            vec![inline_png("shot.png", b"bytes")],
            &human("raver119"),
            "2026-02-01T00:00:00Z",
        )
        .unwrap_err(),
        ERR_ARTIFACT_NOT_FOUND
    );
    assert!(!root.join(COMMENTS_REL).join("attachments").exists());
    assert!(list_all_discussions_in(root, root).is_empty());
}

/// CMS-FR-49: an artifact discussion's attachments are committed with the
/// logs, in the folder its anchored threads' attachments already use.
#[test]
fn an_artifact_discussions_attachments_are_committed_beside_the_logs() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let artifact = make_artifact(root, "specs/spec.md", "# Spec\n");
    let png = b"\x89PNG\r\n\x1a\nshot";
    let thread = open_discussion_in(
        root,
        root,
        &artifact_target(&artifact),
        None,
        "look".into(),
        vec![inline_png("shot.png", png)],
        &human("raver119"),
        "2026-02-01T00:00:00Z",
    )
    .unwrap();
    let Attachment::Blob { digest, .. } = &thread.comments[0].attachments[0] else {
        panic!("expected a stored blob");
    };
    let stored = root.join(COMMENTS_REL).join(ATTACHMENTS_SUBDIR).join(digest);
    assert!(stored.is_file(), "stored under the committed folder");
    assert_eq!(std::fs::read(&stored).unwrap(), png);
    assert!(!root.join(DRAFTS_REL).exists(), "nothing under any draft");
}
