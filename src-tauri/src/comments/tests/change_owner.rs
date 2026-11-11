//! The `change` owner of a discussion: an OpenSpec change on a work stream
//! (CMS-FR-36, CMS-FR-UEXD, CMS-FR-GJEY, CMS-FR-84, CMS-FR-YQAX, CMS-FR-ZILU,
//! CMS-FR-WEWU, CMS-FR-QLMV, CMS-FR-TQVB).

use super::*;

const STREAM: &str = "s1";
const CHANGE: &str = "add-auth";

fn change() -> DiscussionTarget {
    DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() }
}

fn fragment(path: &str, start: usize, end: usize) -> FragmentTarget {
    FragmentTarget { owner: change(), path: path.into(), start, end, quote: "x".into() }
}

fn open_on(
    root: &crate::fs::RootFs,
    fragment: Option<FragmentTarget>,
    attachments: Vec<AttachmentInput>,
) -> Result<Discussion, String> {
    open_discussion_unchecked(
        root,
        &change(),
        fragment.map(|f| validate_fragment(&change(), &f)).transpose()?,
        "body".into(),
        attachments,
        &human("raver119"),
        "2026-01-01T00:00:00Z",
    )
}

// CMS-FR-36 / CMS-FR-UEXD: the owner names the stream id and the change name,
// and its wire shape is `{ kind: "change", streamId, change }`.
#[test]
fn a_change_owner_names_its_stream_and_change_on_the_wire() {
    let json = serde_json::to_value(change()).expect("json");
    assert_eq!(json, serde_json::json!({ "kind": "change", "streamId": STREAM, "change": CHANGE }));
    let back: DiscussionTarget = serde_json::from_value(json).expect("back");
    assert_eq!(back, change());
    assert_eq!(back.change_owner(), Some((STREAM, CHANGE)));
}

// CMS-FR-GJEY: the whole-change discussion is `discussion.jsonl` and a
// fragment discussion is `<log-id>.jsonl`, both in the change's own folder of
// the store, with the attachments beside them.
#[test]
fn a_change_owners_logs_live_in_its_own_folder() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let whole = open_on(&root, None, Vec::new()).expect("whole");
    let pinned = open_on(
        &root,
        Some(fragment("specs/auth/spec.md", 0, 4)),
        vec![AttachmentInput::Inline {
            media_type: "image/png".into(),
            filename: "a.png".into(),
            data: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"\x89PNG"),
        }],
    )
    .expect("fragment");
    let folder = dir.path().join(format!("changes/{STREAM}/{CHANGE}/comments"));
    assert!(folder.join("discussion.jsonl").is_file());
    assert!(folder.join(format!("{}.jsonl", log_id("specs/auth/spec.md"))).is_file());
    assert!(folder.join("attachments").is_dir());
    assert!(!dir.path().join("comments").exists());
    let listed = list_discussions_in(&root, &change());
    let ids: Vec<&str> = listed.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, [pinned.id.as_str(), whole.id.as_str()]);
    assert_eq!(
        listed[0].fragment_target.as_ref().map(|f| f.path.as_str()),
        Some("specs/auth/spec.md")
    );
    assert_eq!(listed[0].target, change());
}

// CMS-FR-84: a change fragment carries a change-relative path, which must stay
// inside the change folder; an empty range is refused too.
#[test]
fn a_change_fragment_needs_a_change_relative_path() {
    for bad in ["", "../x.md", "/abs.md", "a/../../b.md", "./a.md"] {
        assert_eq!(
            validate_fragment(&change(), &fragment(bad, 0, 2)),
            Err(ERR_INVALID_FRAGMENT.to_string()),
            "{bad}"
        );
    }
    assert_eq!(
        validate_fragment(&change(), &fragment("tasks.md", 3, 3)),
        Err(ERR_INVALID_FRAGMENT.to_string())
    );
    let other = FragmentTarget {
        owner: DiscussionTarget::Draft { draft_id: "d".into() },
        ..fragment("tasks.md", 0, 2)
    };
    assert_eq!(validate_fragment(&change(), &other), Err(ERR_INVALID_FRAGMENT.to_string()));
    assert!(validate_fragment(&change(), &fragment("specs/auth/spec.md", 0, 2)).is_ok());
}

// CMS-FR-GJEY: a stream id or change name that is not a valid path segment
// names no change folder.
#[test]
fn an_invalid_owner_names_no_folder() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let bad = DiscussionTarget::Change { stream_id: "..".into(), change: CHANGE.into() };
    assert!(open_discussion_unchecked(
        &root,
        &bad,
        None,
        "b".into(),
        Vec::new(),
        &human("r"),
        "2026-01-01T00:00:00Z"
    )
    .is_err());
    assert!(!is_change_owner("s1", "Not-Valid"));
    assert!(!is_change_owner("s/1", CHANGE));
    assert!(list_discussions_in(&root, &bad).is_empty());
}

// CMS-FR-YQAX / CMS-FR-TQVB: a `proposal_hunk` that carries `change` names a
// hunk of a change edit proposal; it carries exactly one target, and a change
// that names no valid owner is malformed.
#[test]
fn a_proposal_hunk_reference_may_name_a_change() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let reference =
        |draft_id: Option<&str>, change: Option<ChangeOwnerRef>| AttachmentInput::ProposalHunk {
            proposal_id: "p".into(),
            hunk_id: "h".into(),
            draft_id: draft_id.map(str::to_string),
            artifact_id: None,
            change,
            path: "tasks.md".into(),
        };
    let ok = ChangeOwnerRef { stream_id: STREAM.into(), change: CHANGE.into() };
    let opened = open_on(&root, None, vec![reference(None, Some(ok.clone()))]).expect("opened");
    assert!(matches!(
        &opened.comments[0].attachments[0],
        Attachment::ProposalHunk { change: Some(c), path, .. } if *c == ok && path == "tasks.md"
    ));
    assert_eq!(
        open_on(&root, None, vec![reference(Some("d"), Some(ok))]).map(|_| ()),
        Err(ERR_MALFORMED_ATTACHMENT.to_string())
    );
    let invalid = ChangeOwnerRef { stream_id: STREAM.into(), change: "Bad Name".into() };
    assert_eq!(
        open_on(&root, None, vec![reference(None, Some(invalid))]).map(|_| ()),
        Err(ERR_MALFORMED_ATTACHMENT.to_string())
    );
}

// CMS-FR-QLMV / CMS-FR-42: the `change_proposal` attachment reaches a comment
// through the agent write path, and its wire kind is `changeProposal`.
#[test]
fn a_change_proposal_reference_is_appended_by_the_agent_path() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let thread = open_on(&root, None, Vec::new()).expect("opened");
    let reference = Attachment::ChangeProposal {
        proposal_id: "p".into(),
        stream_id: STREAM.into(),
        change: CHANGE.into(),
        update: Some(ProposalUpdateCounts { changed: 1, added: 0, withdrawn: 2 }),
    };
    let appended = append_agent_comment_with(
        &root,
        thread.log_ref(),
        &thread.id,
        "c1".into(),
        "offer".into(),
        vec![reference.clone()],
        Stamp { by: &agent("a1", "arch"), at: "2026-01-01T00:00:01Z" },
    )
    .expect("appended");
    assert_eq!(appended.comments[1].attachments, vec![reference.clone()]);
    let json = serde_json::to_value(&reference).expect("json");
    assert_eq!(json["kind"], "changeProposal");
    assert_eq!(json["streamId"], STREAM);
    assert_eq!(json["update"]["withdrawn"], 2);
}

// CMS-FR-ZILU: a change owner's logs are listed project-wide and found by id
// whatever has become of the change or its stream.
#[test]
fn a_change_owners_discussions_are_listed_and_found_by_id() {
    let store_dir = temp_root();
    let worktree_dir = temp_root();
    let store = crate::fs::RootFs::for_root(store_dir.path());
    let worktree = crate::fs::RootFs::for_root(worktree_dir.path());
    let thread = open_on(&store, None, Vec::new()).expect("opened");
    let all = list_all_discussions_in(&store, &worktree);
    assert!(all.iter().any(|item| item.discussion.id == thread.id));
    assert_eq!(
        read_discussion_by_id(&store, &worktree, &thread.id).map(|d| d.target),
        Some(change())
    );
    assert_eq!(stored_change_owners(&store), vec![(STREAM.to_string(), CHANGE.to_string())]);
}

// CMS-FR-WEWU: a change fragment discussion moves on the terms of an artifact
// owner's, in its own log.
#[test]
fn a_change_fragment_moves_like_an_artifact_fragment() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let thread = open_on(&root, Some(fragment("tasks.md", 0, 4)), Vec::new()).expect("opened");
    let (moved, appended) = move_fragment_to(
        &root,
        thread.log_ref(),
        &thread.id,
        fragment("tasks.md", 10, 14),
        &human("raver119"),
        "2026-01-01T00:00:02Z",
    )
    .expect("moved");
    assert!(appended);
    assert_eq!(moved.fragment_target.as_ref().map(|f| (f.start, f.end)), Some((10, 14)));
    assert_eq!(
        list_discussions_in(&root, &change())[0].fragment_target.as_ref().map(|f| f.start),
        Some(10)
    );
}

// CMS-FR-MTUL / CMS-FR-42: a `new_change_subject` input is stored verbatim and
// names a valid change owner; a blank or invalid one is malformed. Its wire
// kind is `newChangeSubject`, beside `newChangeProposal` (CMS-FR-QYGP).
#[test]
fn a_new_change_subject_names_a_valid_change() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let input = |proposal: &str, change: &str| AttachmentInput::NewChangeSubject {
        proposal_id: proposal.into(),
        stream_id: STREAM.into(),
        change: change.into(),
    };
    let opened = open_on(&root, None, vec![input("p1", CHANGE)]).expect("opened");
    let stored = opened.comments[0].attachments[0].clone();
    assert_eq!(
        stored,
        Attachment::NewChangeSubject {
            proposal_id: "p1".into(),
            stream_id: STREAM.into(),
            change: CHANGE.into()
        }
    );
    assert_eq!(serde_json::to_value(&stored).expect("json")["kind"], "newChangeSubject");
    for bad in [input(" ", CHANGE), input("p1", "Not Valid")] {
        assert_eq!(
            open_on(&root, None, vec![bad]).map(|_| ()),
            Err(ERR_MALFORMED_ATTACHMENT.to_string())
        );
    }
    let reference = Attachment::NewChangeProposal {
        proposal_id: "p1".into(),
        stream_id: STREAM.into(),
        change: CHANGE.into(),
        update: Some(NewDraftUpdateAction::Withdraw),
    };
    let json = serde_json::to_value(&reference).expect("json");
    assert_eq!(json["kind"], "newChangeProposal");
    assert_eq!(json["update"], "withdraw");
}
