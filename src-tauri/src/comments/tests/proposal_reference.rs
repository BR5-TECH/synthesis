//! Proposal references on a comment (CMS-FR-60, CMS-FR-66).

use super::*;

// -----------------------------------------------------------------------
// CMS-FR-42, CMS-FR-48, CMS-FR-49 / CMS-FR-50 — the proposal reference (CMS-FR-60)
// -----------------------------------------------------------------------

/// The invariant that protects an accept-this-rewrite control: a reference
/// the frontend could compose would let a surface offer the author a control
/// that applies text no agent ever proposed.
#[test]
fn no_frontend_route_can_put_a_proposal_reference_on_a_comment() {
    const THIS: &str = include_str!("../commands.rs");
    const MODEL: &str = include_str!("../model.rs");

    // Every `#[tauri::command]` in this module that takes attachments takes
    // `AttachmentInput`, never the stored `Attachment` — so no frontend call
    // can hand in a value of the shape a `proposal` reference has. Checked
    // over the source rather than the handler list, because it is the
    // *parameter type* that is the guarantee.
    for command in THIS.split("#[tauri::command]").skip(1) {
        let signature = command.split_once('{').map(|(head, _)| head).unwrap_or(command);
        assert!(
            !signature.contains("Vec<Attachment>") && !signature.contains(": Attachment"),
            "a command takes a stored Attachment rather than an AttachmentInput \
             (CMS-FR-60): {signature}",
        );
    }

    // And `AttachmentInput` — the only shape a caller supplies — has no
    // variant that could produce one.
    let input = MODEL
        .split_once("pub enum AttachmentInput")
        .expect("the input enum")
        .1
        .split_once("\n}")
        .expect("its body")
        .0;
    assert!(
        !input.to_lowercase().contains("proposal"),
        "AttachmentInput has no proposal variant (CMS-FR-60): {input}",
    );
}

#[test]
fn a_proposal_reference_folds_holds_no_content_and_is_served_by_nothing() {
    let dir = temp_root();
    let root = fsa::RootFs::for_root(dir.path());
    std::fs::write(dir.path().join("spec.md"), "hello there").unwrap();
    let thread = open_artifact_fragment_in(
        &root,
        "spec.md",
        anchor(0, 5, "hello"),
        "@arch what would you change?".into(),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();

    append_agent_comment_with(
        &root,
        ThreadRef::artifact("spec.md"),
        &thread.id,
        "c-proposal".into(),
        "Leads with the point.".into(),
        vec![Attachment::Proposal {
            proposal_id: "p1".into(),
            draft_id: "d1".into(),
            path: "ui/spec.md".into(),
        }],
        &agent("a1", "arch"),
        "2026-01-01T00:01:00Z",
    )
    .expect("appended");

    let folded = list_fragment_discussions_in(&root, "spec.md")
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    let posted = folded.comments.last().expect("the agent's comment");
    assert_eq!(posted.id, "c-proposal", "the caller's id is what was written");
    match &posted.attachments[0] {
        Attachment::Proposal { proposal_id, draft_id, path } => {
            assert_eq!(proposal_id, "p1");
            assert_eq!(draft_id, "d1");
            assert_eq!(path, "ui/spec.md");
        }
        other => panic!("expected a proposal reference, got {other:?}"),
    }

    // It holds no content, so nothing serves it and no folder gained a file.
    assert_eq!(
        read_attachment_in(&root, LogScope::Artifact, &folded, "p1").unwrap_err(),
        ERR_ATTACHMENT_NOT_FOUND,
    );
    assert!(
        !attachments_dir(dir.path()).exists()
            || std::fs::read_dir(attachments_dir(dir.path()))
                .unwrap()
                .next()
                .is_none(),
        "a reference stores nothing (CMS-FR-49)",
    );

    // CMS-FR-60, CMS-FR-50: the reference is immutable, whatever becomes of what it
    // names — this module never reads the proposal and never learns of a
    // decision.
    let again = list_fragment_discussions_in(&root, "spec.md")
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    assert_eq!(
        again.comments.last().unwrap().attachments,
        posted.attachments,
        "byte-for-byte what it was (CMS-FR-50)",
    );
}

/// CMS-FR-66, CMS-FR-42, CMS-FR-43 / CMS-FR-50 / CMS-FR-60: a `prompt_proposal` reference reaches a
/// comment through `append_as` alone, folds as an ordinary comment, holds no
/// content, and is never read, resolved, or validated (CMS-FR-66).
#[test]
fn a_prompt_proposal_reference_folds_holds_no_content_and_is_never_validated() {
    let dir = temp_root();
    let root = fsa::RootFs::for_root(dir.path());
    std::fs::write(dir.path().join("spec.md"), "hello there").unwrap();
    let thread = open_artifact_fragment_in(
        &root,
        "spec.md",
        anchor(0, 5, "hello"),
        "@arch what would you change?".into(),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();

    append_agent_comment_with(
        &root,
        ThreadRef::artifact("spec.md"),
        &thread.id,
        "c-prompt-proposal".into(),
        "Leads with the point.".into(),
        vec![Attachment::PromptProposal {
            proposal_id: "pp1".into(),
            artifact_id: "prompts/review.md".into(),
            path: "prompts/review.md".into(),
        }],
        &agent("a1", "arch"),
        "2026-01-01T00:01:00Z",
    )
    .expect("appended");

    let folded = list_fragment_discussions_in(&root, "spec.md")
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    let posted = folded.comments.last().expect("the agent's comment");
    assert_eq!(posted.id, "c-prompt-proposal");
    assert_eq!(posted.attachments.len(), 1, "and it is the only attachment");
    match &posted.attachments[0] {
        Attachment::PromptProposal { proposal_id, artifact_id, path } => {
            assert_eq!(proposal_id, "pp1");
            assert_eq!(artifact_id, "prompts/review.md");
            assert_eq!(path, "prompts/review.md");
        }
        other => panic!("expected a prompt proposal reference, got {other:?}"),
    }
    // CMS-FR-42: no media type, no digest, and no bytes.
    let encoded = serde_json::to_value(&posted.attachments[0]).expect("json");
    assert_eq!(encoded["kind"], "promptProposal");
    for absent in ["mediaType", "digest", "bytes", "url", "filename"] {
        assert!(encoded.get(absent).is_none(), "it carries no {absent}");
    }

    // CMS-FR-66, CMS-FR-42, CMS-FR-43: nothing serves it, and no folder gained a file.
    assert_eq!(
        read_attachment_in(&root, LogScope::Artifact, &folded, "pp1").unwrap_err(),
        ERR_ATTACHMENT_NOT_FOUND,
    );
    assert!(
        !attachments_dir(dir.path()).exists()
            || std::fs::read_dir(attachments_dir(dir.path()))
                .unwrap()
                .next()
                .is_none(),
        "a reference stores nothing (CMS-FR-49)",
    );

    // CMS-FR-66, CMS-FR-50: the artifact it names is deleted, and the fold still serves
    // the attachment rather than dropping or erroring on it.
    std::fs::remove_file(dir.path().join("spec.md")).unwrap();
    let again = list_fragment_discussions_in(&root, "spec.md")
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    assert_eq!(
        again.comments.last().unwrap().attachments,
        posted.attachments,
        "byte-for-byte what it was (CMS-FR-50)",
    );

    // CMS-FR-66, CMS-FR-42, CMS-FR-43 / CMS-FR-60: no `AttachmentInput` variant produces one, so no
    // caller of a Tauri command can put either reference kind on a comment.
    const SOURCE: &str = include_str!("../model.rs");
    let input = SOURCE
        .split("pub enum AttachmentInput")
        .nth(1)
        .expect("the input enum")
        .split("\n}")
        .next()
        .expect("its body");
    assert!(!input.contains("Proposal"), "no proposal variant: {input}");
}

/// CMS-FR-37, CMS-FR-49, CMS-FR-55, CMS-FR-39: a draft's whole review lives in
/// the repository machine store under the draft's stable id, so filing the
/// draft and moving it between folders leave it exactly where it is — and
/// deleting the draft takes it.
#[test]
fn a_drafts_review_is_addressed_by_its_stable_id_and_a_move_leaves_it_there() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    crate::drafts::create_drafts_folder_impl(root, "", "UI").unwrap();
    crate::drafts::create_drafts_folder_impl(root, "UI", "Components").unwrap();
    crate::drafts::create_drafts_folder_impl(root, "", "backend").unwrap();
    let draft = crate::drafts::create_draft_impl(root, Some("spec"), Some("UI/Components"))
        .unwrap()
        .draft
        .id;
    let file_rel = "ui/spec.md";

    // An anchored thread on one of the draft's files, and a discussion about
    // the draft entire, each carrying an attachment.
    append_as_scoped(
        root,
        LogScope::Draft { draft_id: &draft },
        file_rel,
        &human("raver119"),
        "2026-02-01T00:00:00Z",
        vec![
            (
                "t1".to_string(),
                EventBody::ThreadOpened {
                    artifact_path: file_rel.into(),
                    anchor: legacy_anchor(0, 3, "abc"),
                },
            ),
            (
                "t1".to_string(),
                EventBody::CommentAdded {
                    comment_id: "c1".into(),
                    body: "tighten this".into(),
                    quotes: Vec::new(),
                    attachments: store_attachments(
                        root,
                        LogScope::Draft { draft_id: &draft },
                        vec![inline_png("anchored.png", b"anchored-bytes")],
                    )
                    .unwrap(),
                },
            ),
        ],
    )
    .unwrap();
    let discussion = open_discussion_in(
        root,
        root,
        &DiscussionTarget::Draft {
            draft_id: draft.clone(),
        },
        None,
        "rework the graduation part".into(),
        vec![inline_png("shot.png", b"discussion-bytes")],
        &human("raver119"),
        "2026-02-01T00:00:01Z",
    )
    .unwrap();

    // CMS-FR-37 / CMS-FR-55: both logs are in the store, under the draft's
    // stable id, and nothing was written inside the draft's own folder.
    let filed = root.join(".synthesis/drafts/UI/Components").join(&draft);
    let review = root.join("drafts").join(&draft).join("comments");
    assert!(review.join(DISCUSSION_LOG_NAME).is_file());
    assert!(review.join(format!("{}.jsonl", log_id(file_rel))).is_file());
    assert!(
        !filed.join("comments").exists(),
        "nothing was written inside the draft's own folder",
    );
    assert!(
        !root.join(COMMENTS_REL).join("attachments").exists(),
        "and nothing in the artifact scope's own folder",
    );

    let anchored_before = list_draft_fragment_discussions_in(root, &draft, file_rel);
    let discussions_before = fold_discussion(root, &draft);
    assert_eq!(anchored_before.len(), 1);
    assert_eq!(discussions_before.len(), 1);

    // The move changes nothing about the review: the store is addressed by the
    // draft's stable id, so the same threads and the same attachments are read
    // back at the same path.
    crate::drafts::move_draft_to_folder_impl(root, &draft, "backend").unwrap();

    assert_eq!(list_draft_fragment_discussions_in(root, &draft, file_rel), anchored_before);
    assert_eq!(fold_discussion(root, &draft), discussions_before);
    assert!(!filed.exists(), "the draft's own folder moved");

    let moved_comments = draft_comments_dir(root, &draft).unwrap();
    assert_eq!(moved_comments, review, "the review did not move with it");
    for digest in [
        fsa::sha256_bytes(b"anchored-bytes"),
        fsa::sha256_bytes(b"discussion-bytes"),
    ] {
        assert!(
            moved_comments.join(ATTACHMENTS_SUBDIR).join(&digest).is_file(),
            "the blob stayed where it was",
        );
    }
    // CMS-FR-49: each is served back on the same terms — the anchored thread's
    // blob as readily as the discussion's.
    let served = read_attachment_in(
        root,
        attachment_scope(&discussion),
        &fold_discussion(root, &draft)[0],
        &fsa::sha256_bytes(b"discussion-bytes"),
    )
    .expect("the discussion's blob");
    assert_eq!(served.filename, "shot.png");
    let anchored = read_attachment_in(
        root,
        LogScope::Draft { draft_id: &draft },
        &list_draft_fragment_discussions_in(root, &draft, file_rel)[0],
        &fsa::sha256_bytes(b"anchored-bytes"),
    )
    .expect("the anchored thread's blob");
    assert_eq!(anchored.filename, "anchored.png");

    // An attachment stored *after* the move lands in the same folder, the
    // draft's id being what addresses it.
    store_attachments(
        root,
        LogScope::Draft { draft_id: &draft },
        vec![inline_png("later.png", b"after-the-move")],
    )
    .unwrap();
    assert!(moved_comments
        .join(ATTACHMENTS_SUBDIR)
        .join(fsa::sha256_bytes(b"after-the-move"))
        .is_file());
    assert!(
        !root.join(".synthesis/drafts/backend").join(&draft).join("comments").exists(),
        "and still nothing inside the draft's own folder",
    );

    // CMS-FR-39: deleting the draft takes the whole review with it.
    crate::drafts::delete_draft_impl(root, root, &draft).unwrap();
    assert!(list_draft_fragment_discussions_in(root, &draft, file_rel).is_empty());
    assert!(fold_discussion(root, &draft).is_empty());
    assert!(
        !root.join(".synthesis/drafts/backend").join(&draft).exists(),
        "nothing anywhere under the drafts root names that draft",
    );
    assert!(
        !review.exists(),
        "and the store holds nothing for it either (CMS-FR-39)",
    );
}

/// CMS-FR-61, CMS-FR-30, CMS-FR-40, CMS-FR-25, CMS-FR-52: one thread by its id alone, whatever kind and whichever scope
/// — and nothing written or announced by asking.
#[test]
fn a_thread_is_read_by_its_id_alone_in_either_scope() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    root.write_text_atomic(&root.join("onboarding.md"), "the first session\n")
        .unwrap();

    let anchored = open(
        root,
        "onboarding.md",
        anchor(0, 17, "the first session"),
        "which session?",
        "2026-02-01T00:00:00Z",
    );
    let artifact_discussion = open_artifact_discussion(
        root,
        "onboarding.md",
        "about the whole file",
        "2026-02-01T00:01:00Z",
    );
    let draft = make_draft(root, "artifact-window");
    let draft_discussion =
        open_discussion(root, &draft, "about the whole draft", "2026-02-01T00:02:00Z");

    // A draft-scoped **anchored** thread, which is the fourth combination and
    // reaches `read_discussion_by_id` through its own branch.
    append_as_scoped(
        root,
        LogScope::Draft { draft_id: &draft },
        "ui/spec.md",
        &human("raver119"),
        "2026-02-01T00:03:00Z",
        vec![
            (
                "t-draft-anchored".to_string(),
                EventBody::ThreadOpened {
                    artifact_path: "ui/spec.md".into(),
                    anchor: legacy_anchor(0, 3, "abc"),
                },
            ),
            (
                "t-draft-anchored".to_string(),
                EventBody::CommentAdded {
                    comment_id: "c-draft-anchored".into(),
                    body: "about this passage of the draft".into(),
                    quotes: Vec::new(),
                    attachments: Vec::new(),
                },
            ),
        ],
    )
    .unwrap();
    let draft_anchored = list_draft_fragment_discussions_in(root, &draft, "ui/spec.md")
        .into_iter()
        .next()
        .expect("the draft's anchored thread");

    for expected in [
        &anchored,
        &artifact_discussion,
        &draft_discussion,
        &draft_anchored,
    ] {
        let found = read_discussion_by_id(root, root, &expected.id)
            .unwrap_or_else(|| panic!("thread {} is readable by id", expected.id));
        assert_eq!(found.id, expected.id);
        assert_eq!(found.target, expected.target);
        assert_eq!(found.fragment_target, expected.fragment_target);
        assert_eq!(
            found.comments.iter().map(|c| c.body.clone()).collect::<Vec<_>>(),
            expected.comments.iter().map(|c| c.body.clone()).collect::<Vec<_>>(),
        );
    }
    // CMS-FR-40: the project-wide list carries both draft-owned discussions,
    // each under its draft target.
    let all = list_all_discussions_in(root, root);
    for expected in [&draft_discussion, &draft_anchored] {
        let listed = all
            .iter()
            .find(|i| i.discussion.id == expected.id)
            .unwrap_or_else(|| panic!("{} is in the project-wide list", expected.id));
        assert_eq!(listed.discussion.target, expected.target);
    }

    // CMS-FR-30: an id no log holds.
    assert!(read_discussion_by_id(root, root, "no-such-thread").is_none());

    // CMS-FR-25: the log outlives the artifact it was written about.
    let before = std::fs::read_to_string(log_path(root, "onboarding.md").unwrap()).unwrap();
    std::fs::remove_file(root.join("onboarding.md")).unwrap();
    assert_eq!(
        read_discussion_by_id(root, root, &anchored.id).map(|t| t.id),
        Some(anchored.id.clone()),
    );

    // CMS-FR-52: reading appends nothing anywhere — the draft's own logs
    // included, which one artifact log being byte-identical would not show.
    assert_eq!(
        std::fs::read_to_string(log_path(root, "onboarding.md").unwrap()).unwrap(),
        before,
    );
    assert_eq!(
        discussion_log_lines(root, &draft).len(),
        2,
        "the draft's discussion log is exactly the two lines that opened it",
    );
    assert_eq!(
        list_draft_fragment_discussions_in(root, &draft, "ui/spec.md").len(),
        1,
        "and its per-file log still holds exactly the one thread",
    );
}
