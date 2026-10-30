//! The `proposal_hunk` attachment (CMS-FR-42, CMS-FR-43, CMS-FR-TQVB).

use super::*;

fn hunk_input(hunk_id: &str) -> AttachmentInput {
    AttachmentInput::ProposalHunk {
        proposal_id: "p1".into(),
        hunk_id: hunk_id.into(),
        draft_id: Some("d1".into()),
        artifact_id: None,
        path: "prompt.md".into(),
    }
}

fn hunk_stored(hunk_id: &str) -> Attachment {
    Attachment::ProposalHunk {
        proposal_id: "p1".into(),
        hunk_id: hunk_id.into(),
        draft_id: Some("d1".into()),
        artifact_id: None,
        path: "prompt.md".into(),
    }
}

fn draft_target(draft_id: &str) -> DiscussionTarget {
    DiscussionTarget::Draft { draft_id: draft_id.to_string() }
}

// CMS-FR-TQVB / CMS-FR-43 / DCP-FR-FHWN: an opening comment carries a
// `proposal_hunk` verbatim, and nothing is stored for it.
#[test]
fn an_opening_comment_carries_a_proposal_hunk_verbatim() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(root, "Hunk talk");
    let opened = open_discussion_in(
        root,
        root,
        &draft_target(&draft_id),
        None,
        "Why this change?".into(),
        vec![hunk_input("h1")],
        Stamp { by: &human("ada"), at: "2026-01-01T00:00:00Z" },
    )
    .expect("opened");
    assert_eq!(opened.comments[0].attachments, vec![hunk_stored("h1")]);

    let folded = locate_discussion_of(root, &draft_target(&draft_id), &opened.id).expect("found");
    assert_eq!(folded.comments[0].attachments, vec![hunk_stored("h1")], "it folds as written");
    // CMS-FR-48: it holds no content, so nothing serves it.
    assert_eq!(
        read_attachment_in(root, LogScope::Draft { draft_id: &draft_id }, &folded, "h1")
            .unwrap_err(),
        ERR_ATTACHMENT_NOT_FOUND,
    );
}

// CMS-FR-TQVB / CMS-FR-43: a reply carries a `proposal_hunk` through
// `add_comment`, beside the other attachment kinds an input may name.
#[test]
fn a_reply_carries_a_proposal_hunk_through_add_comment() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(root, "Hunk talk");
    let opened = open_discussion(root, &draft_id, "first", "2026-01-01T00:00:00Z");
    let replied = add_comment_to(
        root,
        ThreadRef::discussion(&draft_id),
        &opened.id,
        "About this one.".into(),
        Vec::new(),
        vec![hunk_input("h2"), url_png("https://example.test/a.png", None)],
        Stamp { by: &human("ada"), at: "2026-01-01T00:01:00Z" },
    )
    .expect("replied");
    let attachments = &replied.comments[1].attachments;
    assert_eq!(attachments[0], hunk_stored("h2"));
    assert!(matches!(attachments[1], Attachment::Url { .. }));
}

// CMS-FR-TQVB / CMS-FR-41: an agent's comment carries a `proposal_hunk`
// through the agent write path, on the terms an author's does.
#[test]
fn an_agent_comment_carries_a_proposal_hunk_through_append_as() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(root, "Hunk talk");
    let opened = open_discussion(root, &draft_id, "@arch why?", "2026-01-01T00:00:00Z");
    let answered = append_agent_comment_with(
        root,
        ThreadRef::discussion(&draft_id),
        &opened.id,
        "c-agent".into(),
        "Because it leads with the point.".into(),
        vec![hunk_stored("h3")],
        Stamp { by: &agent("a1", "arch"), at: "2026-01-01T00:01:00Z" },
    )
    .expect("appended");
    let posted = answered.comments.last().expect("the agent's comment");
    assert_eq!(posted.id, "c-agent");
    assert_eq!(posted.attachments, vec![hunk_stored("h3")]);
}

// CMS-FR-43 / CMS-FR-47: a `proposal_hunk` with an empty part names nothing,
// so it is a `malformed_attachment` and nothing is appended.
#[test]
fn a_proposal_hunk_with_an_empty_part_is_malformed() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(root, "Hunk talk");
    let opened = open_discussion(root, &draft_id, "first", "2026-01-01T00:00:00Z");
    let before = discussion_log_lines(root, &draft_id).len();
    let refused = add_comment_to(
        root,
        ThreadRef::discussion(&draft_id),
        &opened.id,
        "About nothing.".into(),
        Vec::new(),
        vec![hunk_input("  ")],
        Stamp { by: &human("ada"), at: "2026-01-01T00:01:00Z" },
    );
    assert_eq!(refused.unwrap_err(), ERR_MALFORMED_ATTACHMENT);
    assert_eq!(discussion_log_lines(root, &draft_id).len(), before);
}

// CMS-FR-17 / CMS-FR-TQVB: a resolved discussion refuses a comment carrying a
// `proposal_hunk` exactly as it refuses any other, for an author and an agent.
#[test]
fn a_resolved_discussion_refuses_a_proposal_hunk_comment_from_anyone() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(root, "Hunk talk");
    let opened = open_discussion(root, &draft_id, "first", "2026-01-01T00:00:00Z");
    let target = ThreadRef::discussion(&draft_id);
    set_resolution_to(root, target, &opened.id, true, &human("ada"), "2026-01-01T00:01:00Z")
        .expect("resolved");
    let before = discussion_log_lines(root, &draft_id).len();

    let human_write = add_comment_to(
        root,
        target,
        &opened.id,
        "About this one.".into(),
        Vec::new(),
        vec![hunk_input("h4")],
        Stamp { by: &human("ada"), at: "2026-01-01T00:02:00Z" },
    );
    assert_eq!(human_write.unwrap_err(), ERR_DISCUSSION_RESOLVED);
    let agent_write = append_agent_comment_with(
        root,
        target,
        &opened.id,
        "c-agent".into(),
        "Because.".into(),
        vec![hunk_stored("h4")],
        Stamp { by: &agent("a1", "arch"), at: "2026-01-01T00:03:00Z" },
    );
    assert_eq!(agent_write.unwrap_err(), ERR_DISCUSSION_RESOLVED);
    assert_eq!(discussion_log_lines(root, &draft_id).len(), before, "no line was appended");
}

fn artifact_hunk(
    hunk_id: &str,
    draft_id: Option<&str>,
    artifact_id: Option<&str>,
) -> AttachmentInput {
    AttachmentInput::ProposalHunk {
        proposal_id: "pp1".into(),
        hunk_id: hunk_id.into(),
        draft_id: draft_id.map(Into::into),
        artifact_id: artifact_id.map(Into::into),
        path: "prompts/review.md".into(),
    }
}

// CMS-FR-TQVB / CMS-FR-AHDQ: a hunk discussion of an artifact is a discussion
// record of that artifact whose opening comment carries one `proposal_hunk`
// naming the artifact, stored verbatim.
#[test]
fn an_artifact_hunk_discussion_carries_an_artifact_proposal_hunk() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(dir.path().join("prompts")).expect("dirs");
    std::fs::write(dir.path().join("prompts/review.md"), "# Review\n").expect("artifact");
    let target = DiscussionTarget::Artifact { artifact_id: "prompts/review.md".into() };
    let opened = open_discussion_in(
        root,
        root,
        &target,
        None,
        "@arch why this change?".into(),
        vec![artifact_hunk("h1", None, Some("prompts/review.md"))],
        Stamp { by: &human("ada"), at: "2026-01-01T00:00:00Z" },
    )
    .expect("opened");
    let stored = Attachment::ProposalHunk {
        proposal_id: "pp1".into(),
        hunk_id: "h1".into(),
        draft_id: None,
        artifact_id: Some("prompts/review.md".into()),
        path: "prompts/review.md".into(),
    };
    assert_eq!(opened.comments[0].attachments, vec![stored.clone()]);
    assert!(!opened.is_fragment_targeted(), "a whole-target discussion");
    let folded = locate_discussion_of(root, &target, &opened.id).expect("found");
    assert_eq!(folded.comments[0].attachments, vec![stored]);
}

// CMS-FR-TQVB: a `proposal_hunk` naming neither a draft nor an artifact, or
// both, names no one target, so it is a `malformed_attachment` and nothing is
// appended.
#[test]
fn a_proposal_hunk_naming_neither_or_both_targets_is_malformed() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(root, "Hunk talk");
    let opened = open_discussion(root, &draft_id, "first", "2026-01-01T00:00:00Z");
    let before = discussion_log_lines(root, &draft_id).len();
    for input in [
        artifact_hunk("h1", None, None),
        artifact_hunk("h1", Some(&draft_id), Some("prompts/review.md")),
        artifact_hunk("h1", None, Some("  ")),
    ] {
        let refused = add_comment_to(
            root,
            ThreadRef::discussion(&draft_id),
            &opened.id,
            "About this one.".into(),
            Vec::new(),
            vec![input.clone()],
            Stamp { by: &human("ada"), at: "2026-01-01T00:01:00Z" },
        );
        assert_eq!(refused.unwrap_err(), ERR_MALFORMED_ATTACHMENT, "{input:?}");
    }
    assert_eq!(discussion_log_lines(root, &draft_id).len(), before);
}
