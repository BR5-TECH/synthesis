//! Attachments: staging, storage and refusal (CMS-FR-42 ... CMS-FR-50).

use super::*;

// -- CMS-FR-43, CMS-FR-44 … CMS-FR-05, CMS-FR-46: attachments (CMS-FR-42 … CMS-FR-50) --

/// CMS-FR-42, CMS-FR-43, CMS-FR-44: both kinds are stored as the contract declares them, and
/// byte-identical content is one file however many comments attach it.
#[test]
fn an_attachment_is_stored_by_kind_and_deduplicated_by_content() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let png = b"\x89PNG\r\n\x1a\nfake-bytes";
    let thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "look at this".into(),
        vec![
            inline_png("diff.png", png),
            url_png("https://example.test/spec.png", Some("spec v2")),
        ],
        &human("raver119"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();

    let stored = only_attachments(&thread);
    assert_eq!(stored.len(), 2, "both kinds are carried by the comment");
    let digest = match stored[0] {
        Attachment::Blob {
            digest,
            media_type,
            filename,
            bytes,
        } => {
            assert_eq!(media_type, "image/png");
            assert_eq!(filename, "diff.png");
            assert_eq!(*bytes, png.len() as u64, "the decoded length, not base64's");
            assert_eq!(digest, &fsa::sha256_bytes(png), "named by its content");
            digest.clone()
        }
        other => panic!("expected a blob, got {other:?}"),
    };
    match stored[1] {
        Attachment::Url { url, label, .. } => {
            assert_eq!(url, "https://example.test/spec.png");
            assert_eq!(label.as_deref(), Some("spec v2"));
        }
        other => panic!("expected a url, got {other:?}"),
    }
    assert_eq!(
        std::fs::read(attachments_dir(root).join(&digest)).unwrap(),
        png,
        "the file holds exactly the decoded bytes",
    );

    // CMS-FR-44: the same content under another name writes nothing new.
    add_comment_in(
        root,
        "specs/a.md",
        &thread.id,
        "same picture".into(),
        vec![],
        vec![inline_png("renamed.png", png)],
        &human("octocat"),
        "2026-01-02T00:00:00Z",
    )
    .unwrap();
    assert_eq!(
        stored_files(root),
        vec![digest.clone()],
        "byte-identical content is one file",
    );
    let refreshed = list_fragment_discussions_in(root, "specs/a.md");
    let both = only_attachments(&refreshed[0]);
    let digests: Vec<&String> = both
        .iter()
        .filter_map(|a| match a {
            Attachment::Blob { digest, .. } => Some(digest),
            _ => None,
        })
        .collect();
    assert_eq!(digests, vec![&digest, &digest], "both name the same file");
    // A count alone would also pass if the second attach rewrote the file.
    // A sentinel is what proves the `already` short-circuit actually fired.
    let path = attachments_dir(root).join(&digest);
    std::fs::write(&path, b"sentinel").unwrap();
    add_comment_in(
        root,
        "specs/a.md",
        &thread.id,
        "third time".into(),
        vec![],
        vec![inline_png("again.png", png)],
        &human("octocat"),
        "2026-01-03T00:00:00Z",
    )
    .unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"sentinel",
        "an existing digest is left untouched rather than rewritten",
    );
    std::fs::write(&path, png).unwrap();
}

/// CMS-FR-43: a `url` is recorded verbatim and never resolved. Nothing in
/// this module reaches the network, so an address that does not exist is
/// stored exactly like one that does.
#[test]
fn a_url_attachment_is_recorded_without_being_fetched() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "see".into(),
        vec![url_png("https://nowhere.invalid/missing.png", None)],
        &human("raver119"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    match only_attachments(&thread)[0] {
        Attachment::Url { url, label, .. } => {
            assert_eq!(url, "https://nowhere.invalid/missing.png");
            assert!(label.is_none(), "an empty label is absent, not empty");
        }
        other => panic!("expected a url, got {other:?}"),
    }
    assert!(
        stored_files(root).is_empty(),
        "a url stores no bytes anywhere",
    );
}

/// CMS-FR-45: the bytes reach disk before the line naming them.
///
/// Asserted on the *intermediate* state, because that is the only place the
/// ordering is observable: after a successful append both writes have
/// happened whichever order they happened in. Making the log append fail
/// after staging is what separates the two, and it pins the second half of
/// the requirement too — at worst an unreferenced file, never a comment
/// pointing at nothing.
#[test]
fn a_failed_append_leaves_the_bytes_rather_than_a_dangling_reference() {
    // The append is made to fail by a chmod, which binds no process running
    // as root — there the write would succeed and the ordering this test
    // exists to observe would never arise.
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_failed_append_leaves_the_bytes_rather_than_a_dangling_reference",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let thread = open(root, "specs/a.md", anchor(0, 5, "hello"), "x", "2026-01-01T00:00:00Z");
    let before = std::fs::read_to_string(log_path(root, "specs/a.md").unwrap()).unwrap();

    // Readable but not writable: `find_thread` still folds the thread, the
    // lock and quote checks still pass, the attachments are still staged and
    // written — and then `append_lines` cannot open the file. That is the
    // only window in which the ordering is observable.
    let log = log_path(root, "specs/a.md").unwrap();
    let mut perms = std::fs::metadata(&log).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&log, perms).unwrap();

    let png = b"orphaned payload";
    let err = add_comment_in(
        root,
        "specs/a.md",
        &thread.id,
        "with a picture".into(),
        vec![],
        vec![inline_png("a.png", png)],
        &human("raver119"),
        "2026-01-02T00:00:00Z",
    );
    assert!(err.is_err(), "the append could not be written");

    // The bytes are there, unreferenced — CMS-FR-45's stated worst case, and
    // CMS-FR-46's "nothing collects" applied to it.
    assert!(
        attachments_dir(root).join(fsa::sha256_bytes(png)).exists(),
        "the bytes were written before the line that would have named them",
    );

    // And the conversation is untouched.
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        before,
        "the log gained no line",
    );
    let folded = list_fragment_discussions_in(root, "specs/a.md");
    assert_eq!(folded[0].comments.len(), 1, "no comment was added");
}

/// The happy-path half: every blob a fold serves resolves to a file.
#[test]
fn the_bytes_are_written_before_the_line_that_names_them() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let png = b"payload";
    let thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "x".into(),
        vec![inline_png("a.png", png)],
        &human("raver119"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    for attachment in only_attachments(&thread) {
        let Attachment::Blob { digest, .. } = attachment else {
            continue;
        };
        assert!(
            attachments_dir(root).join(digest).exists(),
            "every blob the fold serves resolves to a file",
        );
    }
}

/// CMS-FR-47 / CMS-FR-44, the arms the matrix above leaves out.
#[test]
fn the_refusal_matrix_covers_urls_filenames_and_the_size_boundary() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let thread = open(root, "specs/a.md", anchor(0, 5, "hello"), "x", "2026-01-01T00:00:00Z");
    let by = human("raver119");

    let post = |attachments: Vec<AttachmentInput>| {
        add_comment_in(
            root,
            "specs/a.md",
            &thread.id,
            "try".into(),
            vec![],
            attachments,
            &by,
            "2026-01-02T00:00:00Z",
        )
    };

    // A `url` input's media type is checked on the same terms an inline
    // one's is.
    assert_eq!(
        post(vec![AttachmentInput::Url {
            url: "https://example.test/a.zip".into(),
            media_type: "application/zip".into(),
            label: None,
        }])
        .unwrap_err(),
        ERR_UNSUPPORTED_MEDIA_TYPE,
    );

    // An inline input with no usable filename does not parse.
    for blank in ["", "   "] {
        assert_eq!(
            post(vec![AttachmentInput::Inline {
                media_type: "image/png".into(),
                filename: blank.into(),
                data: b64(b"x"),
            }])
            .unwrap_err(),
            ERR_MALFORMED_ATTACHMENT,
        );
    }

    // The bound is inclusive: exactly the limit is accepted, one past it is
    // not — so a `>=` slipping in for a `>` is caught.
    assert!(
        post(vec![inline_png("exact.png", &vec![7u8; MAX_ATTACHMENT_BYTES])]).is_ok(),
        "exactly the bound is within it",
    );
    assert_eq!(
        post(vec![inline_png("over.png", &vec![7u8; MAX_ATTACHMENT_BYTES + 1])]).unwrap_err(),
        ERR_ATTACHMENT_TOO_LARGE,
    );
}

/// CMS-FR-47 / CMS-FR-17: a refusal for a reason that has nothing to do with
/// attachments still stores none of them.
///
/// `add_comment_in` stores attachments *last*, after every other check has
/// had its say — this is the test that makes that ordering a property rather
/// than an accident of how the function reads.
#[test]
fn a_refusal_for_any_reason_leaves_the_attachments_folder_clean() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let thread = open(root, "specs/a.md", anchor(0, 5, "hello"), "x", "2026-01-01T00:00:00Z");
    let other = open(root, "specs/b.md", anchor(0, 5, "hello"), "y", "2026-01-01T00:00:00Z");
    let by = human("raver119");

    // A quote naming a comment in a different thread.
    assert_eq!(
        add_comment_in(
            root,
            "specs/a.md",
            &thread.id,
            "quoting elsewhere".into(),
            vec![CommentQuote {
                comment_id: other.comments[0].id.clone(),
                excerpt: "y".into(),
            }],
            vec![inline_png("never-stored.png", b"never")],
            &by,
            "2026-01-02T00:00:00Z",
        )
        .unwrap_err(),
        ERR_QUOTED_COMMENT_NOT_IN_DISCUSSION,
    );
    assert!(stored_files(root).is_empty(), "the quote was refused first");

    // A locked thread.
    set_lock_in(root, "specs/a.md", &thread.id, true, &by, "2026-01-03T00:00:00Z").unwrap();
    assert_eq!(
        add_comment_in(
            root,
            "specs/a.md",
            &thread.id,
            "into a locked thread".into(),
            vec![],
            vec![inline_png("never-stored.png", b"never")],
            &by,
            "2026-01-04T00:00:00Z",
        )
        .unwrap_err(),
        ERR_DISCUSSION_LOCKED,
    );
    assert!(stored_files(root).is_empty(), "the lock was refused first");
}

/// CMS-FR-47, CMS-FR-52: every refusal, and the all-or-nothing rule that a list holding
/// one acceptable attachment and one refused stores neither.
#[test]
fn an_unacceptable_attachment_refuses_the_whole_append() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let thread = open(root, "specs/a.md", anchor(0, 5, "hello"), "x", "2026-01-01T00:00:00Z");
    let before = std::fs::read_to_string(log_path(root, "specs/a.md").unwrap()).unwrap();

    let cases: Vec<(&str, AttachmentInput)> = vec![
        (
            ERR_UNSUPPORTED_MEDIA_TYPE,
            AttachmentInput::Inline {
                media_type: "application/zip".into(),
                filename: "bundle.zip".into(),
                data: b64(b"PK"),
            },
        ),
        (
            ERR_ATTACHMENT_TOO_LARGE,
            AttachmentInput::Inline {
                media_type: "image/png".into(),
                filename: "huge.png".into(),
                data: b64(&vec![0u8; MAX_ATTACHMENT_BYTES + 1]),
            },
        ),
        (
            ERR_MALFORMED_ATTACHMENT,
            AttachmentInput::Inline {
                media_type: "image/png".into(),
                filename: "bad.png".into(),
                data: "!!!not base64!!!".into(),
            },
        ),
        (
            ERR_MALFORMED_ATTACHMENT,
            AttachmentInput::Url {
                url: "javascript:alert(1)".into(),
                media_type: "image/png".into(),
                label: None,
            },
        ),
    ];
    for (expected, bad) in cases {
        let err = add_comment_in(
            root,
            "specs/a.md",
            &thread.id,
            "nope".into(),
            vec![],
            // The acceptable one is first, so nothing about ordering saves it.
            vec![inline_png("ok.png", b"fine"), bad],
            &human("raver119"),
            "2026-01-02T00:00:00Z",
        )
        .unwrap_err();
        assert_eq!(err, expected);
        assert!(
            stored_files(root).is_empty(),
            "{expected}: the acceptable attachment was not stored either",
        );
        assert_eq!(
            std::fs::read_to_string(log_path(root, "specs/a.md").unwrap()).unwrap(),
            before,
            "{expected}: the log gained no line",
        );
    }
}

/// CMS-FR-48: reading a stored attachment back, and the two ways a digest
/// resolves to nothing.
#[test]
fn reading_an_attachment_serves_its_bytes_or_says_it_is_not_there() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let png = b"\x89PNGbytes";
    let thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "x".into(),
        vec![
            inline_png("diff.png", png),
            url_png("https://example.test/a.png", None),
        ],
        &human("raver119"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let digest = match only_attachments(&thread)[0] {
        Attachment::Blob { digest, .. } => digest.clone(),
        other => panic!("expected a blob, got {other:?}"),
    };

    let content = read_attachment_in(root, LogScope::Artifact, &thread, &digest).unwrap();
    assert_eq!(content.media_type, "image/png");
    assert_eq!(content.filename, "diff.png");
    use base64::Engine as _;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(content.data.as_bytes())
            .unwrap(),
        png,
    );

    assert_eq!(
        read_attachment_in(root, LogScope::Artifact, &thread, "deadbeef").unwrap_err(),
        ERR_ATTACHMENT_NOT_FOUND,
    );
    // A url's address is not a digest: this module holds no content for one.
    assert_eq!(
        read_attachment_in(
            root,
            LogScope::Artifact,
            &thread,
            "https://example.test/a.png"
        )
        .unwrap_err(),
        ERR_ATTACHMENT_NOT_FOUND,
    );
}

/// CMS-FR-49, CMS-FR-39: the two scopes never share a blob, so deleting the draft takes
/// only its copy and the project's is untouched.
#[test]
fn the_scopes_store_identical_content_separately() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let png = b"shared-bytes";
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
    let digest = match only_attachments(&artifact_thread)[0] {
        Attachment::Blob { digest, .. } => digest.clone(),
        other => panic!("expected a blob, got {other:?}"),
    };

    let draft_id = make_draft(root, "spec");
    let draft_scope = LogScope::Draft { draft_id: &draft_id };
    let stored = store_attachments(root, draft_scope, vec![inline_png("shot.png", png)]).unwrap();
    let draft_digest = match &stored[0] {
        Attachment::Blob { digest, .. } => digest.clone(),
        other => panic!("expected a blob, got {other:?}"),
    };
    assert_eq!(draft_digest, digest, "content addressing is scope-blind");

    let project_copy = attachments_dir(root).join(&digest);
    let draft_copy = draft_comments_dir(root, &draft_id)
        .unwrap()
        .join(ATTACHMENTS_SUBDIR)
        .join(&digest);
    assert_eq!(
        draft_copy,
        root.join("drafts")
            .join(&draft_id)
            .join("comments")
            .join(ATTACHMENTS_SUBDIR)
            .join(&digest),
        "in the store, under the draft's stable id (CMS-FR-49)",
    );
    assert!(project_copy.exists() && draft_copy.exists(), "one copy each");

    // Deleting the draft removes its whole folder, the attachments with it.
    crate::drafts::delete_draft_impl(root, root, &draft_id).unwrap();
    assert!(!draft_copy.exists(), "the draft's copy went with the draft");
    assert_eq!(
        std::fs::read(&project_copy).unwrap(),
        png,
        "the project's copy is byte-for-byte what it was",
    );
}

/// CMS-FR-50, CMS-FR-05, CMS-FR-46: nothing edits an attachment after the fact, and no blob is
/// ever collected — not even one no surviving comment references.
#[test]
fn attachments_are_immutable_and_never_collected() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "x".into(),
        vec![inline_png("a.png", b"one"), inline_png("b.png", b"two")],
        &human("raver119"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let before = stored_files(root);
    assert_eq!(before.len(), 2);

    // CMS-FR-46: an orphan — content in the folder no comment names — is
    // left exactly where it is, because nothing collects.
    let orphan = fsa::sha256_bytes(b"unreferenced");
    root.write_bytes_atomic(attachments_dir(root).join(&orphan), b"unreferenced").unwrap();

    let by = human("raver119");
    set_lock_in(root, "specs/a.md", &thread.id, true, &by, "2026-01-02T00:00:00Z").unwrap();
    set_resolution_in(root, "specs/a.md", &thread.id, true, &by, "2026-01-03T00:00:00Z")
        .unwrap();
    reanchor_in(
        root,
        "specs/a.md",
        &thread.id,
        anchor(9, 14, "world"),
        &by,
        "2026-01-04T00:00:00Z",
    )
    .unwrap();

    let mut after = stored_files(root);
    after.sort();
    let mut expected = before;
    expected.push(orphan);
    expected.sort();
    assert_eq!(after, expected, "every file is still there, orphan included");

    // The folded comment's attachments are exactly what its line carried.
    let refreshed = list_fragment_discussions_in(root, "specs/a.md");
    let names: Vec<&str> = only_attachments(&refreshed[0])
        .iter()
        .filter_map(|a| match a {
            Attachment::Blob { filename, .. } => Some(filename.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(names, vec!["a.png", "b.png"]);
}

/// CMS-FR-48 / FSA-FR-10: a digest is caller-supplied text on a path, so the
/// escape gate is what has to stop it, and a digest belonging to another
/// conversation is not this thread's to serve.
#[test]
fn a_crafted_digest_reads_nothing_outside_its_own_thread() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("secret.txt"), b"not yours").unwrap();

    let thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "x".into(),
        vec![inline_png("mine.png", b"mine")],
        &human("raver119"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();

    // A traversal out of the attachments folder is refused rather than read.
    for crafted in [
        "../../../secret.txt",
        "../../secret.txt",
        "..%2F..%2Fsecret.txt",
        "/etc/passwd",
    ] {
        let err = read_attachment_in(root, LogScope::Artifact, &thread, crafted).unwrap_err();
        assert_eq!(
            err, ERR_ATTACHMENT_NOT_FOUND,
            "{crafted} must not resolve to anything",
        );
    }

    // A digest that really is on disk, but belongs to a different thread, is
    // not this thread's to serve: the name and media type a read serves back
    // come from the log line, and this thread's log carries no such line.
    let other = open_artifact_fragment_in(
        root,
        "specs/b.md",
        anchor(0, 5, "hello"),
        "y".into(),
        vec![inline_png("theirs.png", b"theirs")],
        &human("raver119"),
        "2026-01-02T00:00:00Z",
    )
    .unwrap();
    let other_digest = match only_attachments(&other)[0] {
        Attachment::Blob { digest, .. } => digest.clone(),
        other => panic!("expected a blob, got {other:?}"),
    };
    assert!(attachments_dir(root).join(&other_digest).exists());
    assert_eq!(
        read_attachment_in(root, LogScope::Artifact, &thread, &other_digest).unwrap_err(),
        ERR_ATTACHMENT_NOT_FOUND,
    );
}

/// CMS-FR-47: what the accepted set actually admits. Pinned separately
/// because it is the rule a reviewer is most likely to widen by accident.
#[test]
fn the_accepted_media_types_are_images_pdf_and_plain_text() {
    for ok in [
        "image/png",
        "image/jpeg",
        "IMAGE/PNG",
        "application/pdf",
        "text/plain",
        "text/plain; charset=utf-8",
    ] {
        assert!(media_type_accepted(ok), "{ok} should be accepted");
    }
    for bad in [
        "application/zip",
        "application/octet-stream",
        "text/html",
        "video/mp4",
        "",
    ] {
        assert!(!media_type_accepted(bad), "{bad} should be refused");
    }
}

/// CMS-FR-19 / CMS-FR-52: the no-op that must stay unobservable. The bool is
/// what the command reads to decide whether to announce anything.
#[test]
fn setting_the_state_a_thread_already_holds_reports_no_append() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let thread = open(root, "specs/a.md", anchor(0, 5, "hello"), "x", "2026-01-01T00:00:00Z");
    let by = human("raver119");

    let (_, appended) =
        set_lock_reporting(root, "specs/a.md", &thread.id, true, &by, "2026-01-02T00:00:00Z")
            .unwrap();
    assert!(appended, "a real change appends");
    let (_, again) =
        set_lock_reporting(root, "specs/a.md", &thread.id, true, &by, "2026-01-03T00:00:00Z")
            .unwrap();
    assert!(!again, "setting the state it already holds appends nothing");

    let (_, resolved) = set_resolution_reporting(
        root,
        "specs/a.md",
        &thread.id,
        false,
        &by,
        "2026-01-04T00:00:00Z",
    )
    .unwrap();
    assert!(!resolved, "already unresolved: nothing to append");
}

/// CMS-FR-48 / FSA-FR-10: a digest read back out of a log becomes a path, so
/// the *stored* value is attacker-controlled too.
///
/// The log is committed, union-merged, and editable by hand, so a crafted
/// `comment_added` line is a realistic way in — and the escape gate alone is
/// not enough, because the attachments folder sits three components below the
/// root and three `..` segments climb out of it while staying inside it.
#[test]
fn a_crafted_stored_digest_cannot_climb_out_of_the_attachments_folder() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("SECRET.txt"), b"top secret").unwrap();

    let thread = open(root, "specs/a.md", anchor(0, 5, "hello"), "x", "2026-01-01T00:00:00Z");

    // A line as a malicious contributor's merged commit would carry it: the
    // attachment looks ordinary, and its digest is a path out of the folder.
    let escapes = "../../../SECRET.txt";
    append_as(
        root,
        "specs/a.md",
        &human("attacker"),
        "2026-01-02T00:00:00Z",
        vec![(
            thread.id.clone(),
            EventBody::CommentAdded {
                comment_id: "c-evil".into(),
                body: "look at this".into(),
                quotes: Vec::new(),
                attachments: vec![Attachment::Blob {
                    digest: escapes.into(),
                    media_type: "image/png".into(),
                    filename: "innocent.png".into(),
                    bytes: 10,
                }],
            },
        )],
    )
    .unwrap();

    // The fold serves the line — a damaged *value* is not a damaged line, and
    // CMS-FR-08 keeps the rest of the conversation readable either way.
    let folded = list_fragment_discussions_in(root, "specs/a.md")
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(folded.comments.len(), 2, "the crafted comment is still read");

    // But it serves no bytes: the digest is not a digest.
    assert_eq!(
        read_attachment_in(root, LogScope::Artifact, &folded, escapes).unwrap_err(),
        ERR_ATTACHMENT_NOT_FOUND,
    );

    // The same holds for every shape that is not 64 lowercase hex characters,
    // including one that reaches a *different scope*'s private folder.
    for crafted in [
        "../../drafts/d1/comments/attachments/deadbeef",
        "../../../.git/config",
        "..",
        "a/b",
        "A".repeat(64).as_str(),
        &format!("{}x", "a".repeat(63)),
        "",
    ] {
        assert!(
            !is_storage_digest(crafted),
            "{crafted:?} must not pass for a digest",
        );
        assert_eq!(
            LogScope::Artifact
                .attachment_path(root, crafted)
                .unwrap_err(),
            ERR_ATTACHMENT_NOT_FOUND,
            "{crafted:?} must not resolve to a path at all",
        );
    }

    // A real digest still works, so the guard has not simply broken reading.
    let good = open_artifact_fragment_in(
        root,
        "specs/b.md",
        anchor(0, 5, "hello"),
        "y".into(),
        vec![inline_png("real.png", b"real")],
        &human("raver119"),
        "2026-01-03T00:00:00Z",
    )
    .unwrap();
    let digest = match only_attachments(&good)[0] {
        Attachment::Blob { digest, .. } => digest.clone(),
        other => panic!("expected a blob, got {other:?}"),
    };
    assert!(is_storage_digest(&digest), "what we store must pass our own check");
    assert!(read_attachment_in(root, LogScope::Artifact, &good, &digest).is_ok());
}

/// CMS-FR-52: re-anchoring to the anchor a thread already holds appends
/// nothing, so it must announce nothing either.
#[test]
fn reanchoring_to_the_same_anchor_reports_no_append() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let thread = open(root, "specs/a.md", anchor(0, 5, "hello"), "x", "2026-01-01T00:00:00Z");
    let by = human("raver119");

    let (_, appended) = reanchor_reporting(
        root,
        "specs/a.md",
        &thread.id,
        anchor(9, 14, "world"),
        &by,
        "2026-01-02T00:00:00Z",
    )
    .unwrap();
    assert!(appended, "a real move appends");

    let (_, again) = reanchor_reporting(
        root,
        "specs/a.md",
        &thread.id,
        anchor(9, 14, "world"),
        &by,
        "2026-01-03T00:00:00Z",
    )
    .unwrap();
    assert!(!again, "the anchor it already holds appends nothing");
}

/// CMS-FR-51 / CMS-FR-52: every append announces itself, and nothing else
/// does.
///
/// A source guard rather than a runtime one, on the same reasoning as
/// `lib.rs`'s registration guards: the commands take an `AppHandle` and
/// resolve a GitHub identity through the keychain, so standing one up in a
/// unit test would test the harness rather than the rule. What can go wrong
/// here is narrow and textual — someone adds a command that appends and
/// forgets to emit, or gates an emit on nothing when the operation has a
/// no-op branch — and both are visible in the source.
///
/// The failure this catches is silent either way: a missing emit leaves a
/// surface stale until it is reopened, and a spurious one makes every open
/// rail redraw for a write that never happened.
#[test]
fn every_command_that_appends_announces_it_and_no_command_announces_a_no_op() {
    const SOURCE: &str = include_str!("../commands.rs");

    /// The body of `pub fn NAME<...>(...) { ... }`, to its closing brace at
    /// column 0 — every command in this module is a top-level fn.
    fn command_body<'a>(source: &'a str, name: &str) -> &'a str {
        // The name must end where the signature does, or `add_comment` finds
        // `add_comment_in` — a different function with different rules.
        let needle = format!("pub fn {name}");
        let at = source
            .match_indices(&needle)
            .find(|(i, _)| {
                matches!(
                    source[i + needle.len()..].chars().next(),
                    Some('<') | Some('(')
                )
            })
            .map(|(i, _)| i)
            .unwrap_or_else(|| panic!("{name} is not a function in this module"));
        let rest = &source[at..];
        let end = rest.find("\n}\n").expect("unterminated fn");
        &rest[..end]
    }

    // These three have a legitimate no-op branch (CMS-FR-19, CMS-FR-21), so
    // each must read the `appended` flag its `*_reporting` variant returns
    // rather than emitting unconditionally.
    for (command, reporting) in [
        ("set_discussion_lock", "set_lock_to"),
        ("set_discussion_resolution", "set_resolution_to"),
        ("reanchor_discussion_fragment", "move_fragment_to"),
    ] {
        let body = command_body(SOURCE, command);
        assert!(
            body.contains(reporting),
            "{command} must call {reporting}, which is what reports whether \
             a line was actually appended (CMS-FR-52)",
        );
        assert!(
            body.contains("if appended"),
            "{command} can no-op, so its emit must be gated on `appended` \
             (CMS-FR-52) — an unconditional emit redraws every open surface \
             for a write that did not happen",
        );
    }

    // These always append when they succeed, so they always announce.
    for command in ["open_discussion", "add_comment"] {
        let body = command_body(SOURCE, command);
        assert!(
            body.contains("emit_discussion_changed"),
            "{command} always appends on success, so it must always announce \
             the thread it wrote into (CMS-FR-51)",
        );
        assert!(
            !body.contains("if appended"),
            "{command} has no no-op branch; gating its emit would be dead \
             code hiding a missing announcement",
        );
    }

    // A read announces nothing.
    for command in [
        "list_discussions",
        "list_all_discussions",
        "read_discussion",
        "read_comment_attachment",
        "resolve_comment_author_identity",
    ] {
        assert!(
            !command_body(SOURCE, command).contains("emit_discussion_changed"),
            "{command} writes nothing, so it must stay silent (CMS-FR-52)",
        );
    }
}

#[test]
fn commands_are_in_scope() {
    // A rename of any command fn fails to compile here, mirroring the
    // `*_are_in_scope` guard the other domain modules keep.
    let _ = list_discussions;
    let _ = crate::comments::open_discussion::<tauri::Wry>;
    let _ = add_comment::<tauri::Wry>;
    let _ = set_discussion_lock::<tauri::Wry>;
    let _ = set_discussion_resolution::<tauri::Wry>;
    let _ = reanchor_discussion_fragment::<tauri::Wry>;
    let _ = resolve_comment_author_identity;
    let _ = list_all_discussions;
    let _ = read_discussion;
    let _ = get_or_create_note_discussion::<tauri::Wry>;
    let _ = read_comment_attachment;
}
