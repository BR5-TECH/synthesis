//! The backend-side attachment read (CMS-FR-67).

use super::*;

// -- CMS-FR-48, CMS-FR-49, CMS-FR-43: the backend-side attachment read (CMS-FR-67) -------------

/// CMS-FR-67, CMS-FR-48, CMS-FR-49, CMS-FR-43: `read_attachment_bytes` serves a stored blob on exactly the
/// terms `read_comment_attachment` serves one to a surface, refuses
/// everything that scope does not carry, and fetches nothing.
#[test]
fn cms_ts68_read_attachment_bytes_matches_the_surface_read_and_fetches_nothing() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft = make_draft(root, "artifact-window");
    let png = b"\x89PNGbytes";
    let thread = open_discussion_in(
        root,
        root,
        &DiscussionTarget::Draft {
            draft_id: draft.clone(),
        },
        None,
        "look at this".into(),
        vec![
            inline_png("diff.png", png),
            url_png("https://example.test/a.png", Some("spec v2")),
        ],
        &human("raver119"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let digest = match only_attachments(&thread)[0] {
        Attachment::Blob { digest, .. } => digest.clone(),
        other => panic!("expected a blob, got {other:?}"),
    };

    let internal =
        read_attachment_bytes(root, root, &thread.id, &digest, Some(&draft)).unwrap();
    let surface =
        read_attachment_in(root, attachment_scope(&thread), &thread, &digest).unwrap();
    assert_eq!(internal, surface, "one path with one set of rules");
    assert_eq!(internal.media_type, "image/png");
    assert_eq!(internal.filename, "diff.png");
    use base64::Engine as _;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(internal.data.as_bytes())
            .unwrap(),
        png,
    );

    // A `url` attachment and a digest this scope does not carry are both
    // `attachment_not_found`, and no address was requested for either.
    for absent in ["https://example.test/a.png", "deadbeef"] {
        assert_eq!(
            read_attachment_bytes(root, root, &thread.id, absent, Some(&draft)).unwrap_err(),
            ERR_ATTACHMENT_NOT_FOUND,
            "{absent}",
        );
    }
    // A thread the caller named wrongly finds nothing rather than being
    // served another scope's file (CMS-FR-49).
    assert_eq!(
        read_attachment_bytes(root, root, "no-such-thread", &digest, Some(&draft)).unwrap_err(),
        ERR_DISCUSSION_NOT_FOUND,
    );
}

/// CMS-FR-67, CMS-FR-48, CMS-FR-49, CMS-FR-43: it is registered as no Tauri command, so it widens nothing a
/// frontend call can reach.
#[test]
fn cms_ts68_read_attachment_bytes_is_not_a_registered_command() {
    const LIB: &str = include_str!("../../lib.rs");
    let handler = LIB
        .split_once("generate_handler![")
        .expect("generate_handler! invocation")
        .1
        .split_once("])")
        .expect("end of generate_handler!")
        .0;
    assert!(
        !handler.contains("read_attachment_bytes"),
        "CMS-FR-67 is an internal call and reaches no frontend",
    );
    // Named here so a rename fails to compile rather than silently making
    // the scan above vacuous.
    let _ = read_attachment_bytes;
}

/// CMS-FR-53, CMS-FR-55, CMS-FR-57, CMS-FR-58: a discussion is opened with its first comment, in the draft's
/// one reserved log, carrying no anchor and no file.
#[test]
fn opening_a_discussion_writes_the_reserved_log_and_carries_no_anchor() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft = make_draft(root, "artifact-window");

    let thread = open_discussion(
        root,
        &draft,
        "rework the graduation part",
        "2026-02-01T00:00:00Z",
    );

    let lines = discussion_log_lines(root, &draft);
    assert_eq!(lines.len(), 2, "one opening and one comment, in one append");
    assert!(lines[0].contains("\"type\":\"discussion_opened\""));
    assert!(lines[1].contains("\"type\":\"comment_added\""));

    assert!(!thread.is_fragment_targeted());
    assert!(thread.draft_id().is_some());
    assert_eq!(thread.draft_id(), Some(draft.as_str()));
    assert_eq!(thread.artifact_id(), None);
    assert_eq!(thread.fragment_target, None);
    assert_eq!(thread.comments.len(), 1);
    assert!(matches!(thread.comments[0].author, Participant::Human { .. }));

    // CMS-FR-55: the filename is reserved rather than derived, and lives
    // inside the draft's own committed folder.
    assert!(draft_comments_dir(root, &draft)
        .unwrap()
        .join(DISCUSSION_LOG_NAME)
        .is_file());
    assert!(
        !comments_dir(root).exists(),
        "nothing was written into the committed comments folder",
    );

    // CMS-FR-58: a second discussion joins the same log, and the two read
    // back in the order they were begun.
    let second = open_discussion(root, &draft, "and the archive part?", "2026-02-02T00:00:00Z");
    assert_eq!(discussion_log_lines(root, &draft).len(), 4);
    let listed = fold_discussion(root, &draft);
    assert_eq!(
        listed.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        vec![thread.id.as_str(), second.id.as_str()],
    );
}

/// CMS-FR-53, on the wire rather than in memory: a whole-target discussion
/// serializes an explicit `"fragmentTarget": null`.
///
/// Every other test in here compares folded structs, so a field that is
/// merely *skipped* on serialization reads as `None` on both sides and looks
/// right. The caller across the boundary gets the JSON, and the contract it is
/// typed against declares `fragmentTarget` a required field holding `null`. A
/// skipped field arrives as `undefined` there, which is not `null`.
#[test]
fn a_whole_target_discussion_serializes_a_fragment_target_that_is_present_and_null() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft = make_draft(root, "artifact-window");
    let thread = open_discussion(root, &draft, "where should this go?", "2026-02-01T00:00:00Z");

    let json = serde_json::to_value(&thread).expect("a thread serializes");
    let object = json.as_object().expect("an object");
    assert!(
        object.contains_key("fragmentTarget"),
        "the field is present, not skipped: {json}",
    );
    assert!(object["fragmentTarget"].is_null());
    assert_eq!(object["target"]["kind"], serde_json::json!("draft"));
    assert_eq!(object["target"]["draftId"], serde_json::json!(draft));
    for removed in ["scope", "kind", "artifactId", "draftId", "noteId", "anchor"] {
        assert!(!object.contains_key(removed), "{removed} is not a field of the record");
    }

    // A fragment discussion still carries the fragment itself.
    let anchored = open(
        root,
        "DRS.md",
        anchor(0, 3, "abc"),
        "a passage",
        "2026-02-01T00:00:00Z",
    );
    let anchored = serde_json::to_value(&anchored).expect("a thread serializes");
    assert_eq!(anchored["fragmentTarget"]["quote"], serde_json::json!("abc"));

    // A payload that omits the field reads back as whole-target.
    let without = serde_json::json!({
        "id": thread.id,
        "target": { "kind": "draft", "draftId": draft },
        "comments": [],
        "locked": false,
        "resolved": false,
        "createdAt": "2026-02-01T00:00:00Z",
        "updatedAt": "2026-02-01T00:00:00Z",
    });
    let parsed: Discussion = serde_json::from_value(without).expect("still parses");
    assert_eq!(parsed.fragment_target, None);
}

/// CMS-FR-54, CMS-FR-49, CMS-FR-41, CMS-FR-17: everything an anchored thread affords, a discussion affords —
/// but for anchoring.
#[test]
fn a_discussion_takes_every_operation_an_anchored_thread_takes() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft = make_draft(root, "artifact-window");
    let target = ThreadRef::discussion(&draft);
    let thread = open_discussion(root, &draft, "first", "2026-02-01T00:00:00Z");
    let first_comment = thread.comments[0].id.clone();

    // A reply, quoting the opening comment.
    let replied = add_comment_to(
        root,
        target,
        &thread.id,
        "agreed".into(),
        vec![CommentQuote {
            comment_id: first_comment.clone(),
            excerpt: "first".into(),
        }],
        vec![inline_png("shot.png", b"bytes")],
        &human("raver119"),
        "2026-02-01T00:01:00Z",
    )
    .unwrap();
    assert_eq!(replied.comments.len(), 2);
    assert_eq!(replied.comments[1].quotes[0].comment_id, first_comment);

    // CMS-FR-49: the blob lands in the draft's own attachment folder and
    // reads back through the same path an anchored thread's does.
    let digest = match only_attachments(&replied)[0] {
        Attachment::Blob { digest, .. } => digest.clone(),
        other => panic!("expected a blob, got {other:?}"),
    };
    assert!(draft_comments_dir(root, &draft)
        .unwrap()
        .join(ATTACHMENTS_SUBDIR)
        .join(&digest)
        .is_file());
    let content = read_attachment_in(root, attachment_scope(&replied), &replied, &digest)
        .expect("a discussion's attachment is served back");
    assert_eq!(content.filename, "shot.png");

    // Resolve, reopen, and the fold still reads as an unanchored thread.
    let (resolved, appended) = set_resolution_to(
        root,
        target,
        &thread.id,
        true,
        &human("raver119"),
        "2026-02-01T00:02:00Z",
    )
    .unwrap();
    assert!(appended && resolved.resolved && resolved.fragment_target.is_none());
    let (reopened, _) = set_resolution_to(
        root,
        target,
        &thread.id,
        false,
        &human("raver119"),
        "2026-02-01T00:03:00Z",
    )
    .unwrap();
    assert!(!reopened.resolved);

    // CMS-FR-41: an agent writes through `append_as`, on exactly these terms.
    let answered = append_agent_comment_to(
        root,
        target,
        &thread.id,
        "here is what I'd do".into(),
        &agent("a1", "arch"),
        "2026-02-01T00:04:00Z",
    )
    .unwrap();
    assert!(matches!(
        answered.comments.last().unwrap().author,
        Participant::Agent { .. }
    ));

    // CMS-FR-17: and a lock stops the agent exactly as it stops a person.
    set_lock_to(
        root,
        target,
        &thread.id,
        true,
        &human("raver119"),
        "2026-02-01T00:05:00Z",
    )
    .unwrap();
    let before = discussion_log_lines(root, &draft).len();
    assert_eq!(
        append_agent_comment_to(
            root,
            target,
            &thread.id,
            "again".into(),
            &agent("a1", "arch"),
            "2026-02-01T00:06:00Z",
        )
        .unwrap_err(),
        ERR_DISCUSSION_LOCKED,
    );
    assert_eq!(discussion_log_lines(root, &draft).len(), before);
}

/// CMS-FR-54, CMS-FR-59, CMS-FR-52: re-anchoring is the one command of the contract surface a
/// discussion refuses.
#[test]
fn a_discussion_refuses_a_reanchor_and_appends_nothing() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft = make_draft(root, "artifact-window");
    let thread = open_discussion(root, &draft, "first", "2026-02-01T00:00:00Z");
    let before = discussion_log_lines(root, &draft);

    assert_eq!(
        reanchor_to(
            root,
            ThreadRef::discussion(&draft),
            &thread.id,
            anchor(0, 5, "intro"),
            &human("raver119"),
            "2026-02-01T00:01:00Z",
        )
        .unwrap_err(),
        ERR_NOT_FRAGMENT_TARGETED,
    );
    assert_eq!(discussion_log_lines(root, &draft), before);
}

/// CMS-FR-56, CMS-FR-58, CMS-FR-40: a draft's whole-target discussion is served by the owner
/// listing and the project-wide listing, and by no per-file fragment listing.
#[test]
fn a_whole_target_discussion_reaches_no_per_file_listing() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let draft = make_draft(root, "artifact-window");
    let discussion = open_discussion(root, &draft, "the whole thing", "2026-02-01T00:00:00Z");

    // Two anchored threads on the draft's own files, in the same folder.
    for file in ["ui/spec.md", "notes.md"] {
        append_as_scoped(
            root,
            LogScope::Draft { draft_id: &draft },
            file,
            &human("raver119"),
            "2026-02-01T00:00:00Z",
            vec![
                (
                    format!("t-{file}"),
                    EventBody::ThreadOpened {
                        artifact_path: file.into(),
                        anchor: legacy_anchor(0, 3, "abc"),
                    },
                ),
                (
                    format!("t-{file}"),
                    EventBody::CommentAdded {
                        comment_id: format!("c-{file}"),
                        body: "x".into(),
                        quotes: Vec::new(),
                        attachments: Vec::new(),
                    },
                ),
            ],
        )
        .unwrap();
    }

    for file in ["ui/spec.md", "notes.md"] {
        let listed = list_draft_fragment_discussions_in(root, &draft, file);
        assert_eq!(listed.len(), 1, "only that file's anchored thread");
        assert!(listed.iter().all(|t| t.is_fragment_targeted()));
        assert!(listed.iter().all(|t| t.id != discussion.id));
    }
    assert!(
        list_all_draft_fragment_discussions_in(root, &draft)
            .iter()
            .all(|t| t.id != discussion.id),
        "the reserved log is not one of the per-file logs",
    );
    let all = list_all_discussions_in(root, root);
    let listed = all
        .iter()
        .find(|i| i.discussion.id == discussion.id)
        .expect("CMS-FR-40: the project-wide listing carries a draft's discussion");
    assert_eq!(listed.discussion.target, DiscussionTarget::Draft { draft_id: draft.clone() });
}

/// CMS-FR-51, CMS-FR-57, CMS-FR-55, CMS-FR-39: a `draft_id` naming no draft appends nothing, and no draft
/// file can ever be given the reserved log's filename.
#[test]
fn a_discussion_needs_a_draft_and_the_reserved_name_collides_with_nothing() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());

    assert_eq!(
        open_discussion_in(
            root,
            root,
            &DiscussionTarget::Draft {
                draft_id: "no-such-draft".to_string(),
            },
            None,
            "hello".into(),
            vec![inline_png("shot.png", b"bytes")],
            &human("raver119"),
            "2026-02-01T00:00:00Z",
        )
        .unwrap_err(),
        ERR_DRAFT_NOT_FOUND,
    );
    assert!(
        !root.join(DRAFTS_REL).join("no-such-draft").exists(),
        "nothing was written, the attachment included",
    );

    // CMS-FR-55: `log_id` is 32 hex characters, so no derivation of it can
    // produce the reserved name however the author names a file.
    let draft = make_draft(root, "artifact-window");
    assert_ne!(format!("{}.jsonl", log_id("discussion")), DISCUSSION_LOG_NAME);
    assert_ne!(
        format!("{}.jsonl", log_id("discussion.jsonl")),
        DISCUSSION_LOG_NAME,
    );
    let discussion = open_discussion(root, &draft, "first", "2026-02-01T00:00:00Z");
    let derived = ThreadRef::draft_file(&draft, "discussion")
        .scope
        .log_path(root, "discussion")
        .unwrap();
    assert_ne!(
        derived,
        discussion_log_path(root, &draft).unwrap(),
        "a file named `discussion` gets a log of its own",
    );
    assert_eq!(fold_discussion(root, &draft).len(), 1);
    assert_eq!(fold_discussion(root, &draft)[0].id, discussion.id);
}

/// CMS-FR-51, CMS-FR-57, CMS-FR-55 / CMS-FR-39: deleting the draft takes its discussions with it,
/// and touches no artifact-scoped log.
#[test]
fn deleting_a_draft_takes_its_discussion_log_and_leaves_the_project_alone() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    open(root, "specs/a.md", anchor(0, 3, "one"), "project", "2026-01-01T00:00:00Z");
    let project_log = log_path(root, "specs/a.md").unwrap();
    let before = std::fs::read(&project_log).unwrap();

    let draft = make_draft(root, "artifact-window");
    open_discussion(root, &draft, "private thinking", "2026-02-01T00:00:00Z");
    let discussion_log = discussion_log_path(root, &draft).unwrap();
    assert!(discussion_log.is_file());

    crate::drafts::delete_draft_impl(root, root, &draft).expect("delete");

    assert!(!discussion_log.exists(), "the log went with the draft's folder");
    assert!(fold_discussion(root, &draft).is_empty());
    assert_eq!(
        std::fs::read(&project_log).unwrap(),
        before,
        "every artifact-scoped log is byte-for-byte what it was",
    );
}

/// CMS-FR-48 / CMS-FR-49: a draft's attachment is served against that draft
/// and against nothing else.
#[test]
fn an_attachment_read_is_confined_to_the_scope_its_caller_names() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let png = b"shared-bytes";

    // The same bytes attached in the project and in a draft's discussion.
    let artifact_thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "x".into(),
        vec![inline_png("shot.png", png)],
        &human("raver119"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let draft = make_draft(root, "artifact-window");
    let discussion = open_discussion_in(
        root,
        root,
        &DiscussionTarget::Draft {
            draft_id: draft.clone(),
        },
        None,
        "look".into(),
        vec![inline_png("shot.png", png)],
        &human("raver119"),
        "2026-02-01T00:00:00Z",
    )
    .unwrap();

    // Named with its draft, the discussion's own copy is what is served.
    let found = locate_discussion_for_attachment(root, root, &discussion.id, Some(&draft))
        .expect("the discussion is in that draft");
    assert_eq!(found.id, discussion.id);
    assert_eq!(
        attachment_scope(&found),
        LogScope::Draft { draft_id: &draft },
    );

    // One discussion has one identity: named with no draft, or with a draft that
    // does not hold it, the id still resolves to the same record, and the scope
    // the blob is read from is the discussion's own owner.
    let by_id = locate_discussion_for_attachment(root, root, &discussion.id, None)
        .expect("the discussion is found by its id alone");
    assert_eq!(by_id, found);
    assert_eq!(
        locate_discussion_for_attachment(root, root, &artifact_thread.id, None)
            .expect("the artifact thread")
            .id,
        artifact_thread.id,
    );
    let other = make_draft(root, "second-draft");
    let via_other = locate_discussion_for_attachment(root, root, &discussion.id, Some(&other))
        .expect("a draft that does not hold it falls back to the id");
    assert_eq!(via_other, found);
    let artifact_via_draft =
        locate_discussion_for_attachment(root, root, &artifact_thread.id, Some(&draft))
            .expect("the artifact thread is found by id");
    assert_eq!(
        attachment_scope(&artifact_via_draft),
        LogScope::Artifact,
        "its blobs are read from its own owner's scope, not the draft named",
    );
}

/// CMS-FR-53: the kind is fixed by the opening event, and an opening event of
/// the wrong kind for the log it is in surfaces no thread at all.
#[test]
fn an_opening_event_of_the_wrong_kind_for_its_log_folds_to_nothing() {
    let anchored = |thread_id: &str| {
        vec![
            Event {
                v: SCHEMA_VERSION,
                event_id: format!("{thread_id}-e1"),
                thread_id: thread_id.into(),
                at: "2026-02-01T00:00:00Z".into(),
                by: human("raver119"),
                body: EventBody::ThreadOpened {
                    artifact_path: "a.md".into(),
                    anchor: legacy_anchor(0, 3, "abc"),
                },
            },
            Event {
                v: SCHEMA_VERSION,
                event_id: format!("{thread_id}-e2"),
                thread_id: thread_id.into(),
                at: "2026-02-01T00:00:01Z".into(),
                by: human("raver119"),
                body: EventBody::CommentAdded {
                    comment_id: "c1".into(),
                    body: "x".into(),
                    quotes: Vec::new(),
                    attachments: Vec::new(),
                },
            },
        ]
    };
    assert!(
        fold_events_for(FoldTarget::DraftDiscussion { draft_id: "d1" }, anchored("t1"))
            .is_empty(),
        "an anchored thread in the reserved discussion log is not served",
    );

    let discussion = vec![
        Event {
            v: SCHEMA_VERSION,
            event_id: "e1".into(),
            thread_id: "t1".into(),
            at: "2026-02-01T00:00:00Z".into(),
            by: human("raver119"),
            body: EventBody::DiscussionOpened {
                target: Some(DiscussionTarget::Draft {
                    draft_id: "d1".into(),
                }),
                draft_id: None,
                fragment_target: None,
            },
        },
        Event {
            v: SCHEMA_VERSION,
            event_id: "e2".into(),
            thread_id: "t1".into(),
            at: "2026-02-01T00:00:01Z".into(),
            by: human("raver119"),
            body: EventBody::CommentAdded {
                comment_id: "c1".into(),
                body: "x".into(),
                quotes: Vec::new(),
                attachments: Vec::new(),
            },
        },
    ];
    assert!(
        fold_events("a.md", discussion.clone()).is_empty(),
        "a discussion in a per-file log is not served either",
    );
    let folded = fold_events_for(FoldTarget::DraftDiscussion { draft_id: "d1" }, discussion);
    assert_eq!(folded.len(), 1);
    assert!(!folded[0].is_fragment_targeted());
}
