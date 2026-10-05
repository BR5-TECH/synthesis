//! Quotes, locks and resolution (CMS-FR-16 ... CMS-FR-20).

use super::*;

// -- CMS-FR-16 (quoting) ------------------------------------------------

#[test]
fn a_quote_must_name_a_comment_in_the_same_thread() {
    // CMS-FR-16.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t1 = open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "first",
        "2026-01-01T00:00:00Z",
    );
    let t2 = open(
        root,
        "specs/a.md",
        anchor(10, 13, "def"),
        "other",
        "2026-01-01T00:00:01Z",
    );
    let c1 = t1.comments[0].id.clone();

    let updated = add_comment_in(
        root,
        "specs/a.md",
        &t1.id,
        "agreed".into(),
        vec![CommentQuote {
            comment_id: c1.clone(),
            excerpt: "first".into(),
        }],
        Vec::new(),
        &human("octocat"),
        "2026-01-02T00:00:00Z")
    .unwrap();
    assert_eq!(updated.comments.len(), 2);
    assert_eq!(updated.comments[1].quotes[0].comment_id, c1);

    let before = log_lines(root, "specs/a.md").len();
    let err = add_comment_in(
        root,
        "specs/a.md",
        &t1.id,
        "nope".into(),
        vec![CommentQuote {
            comment_id: t2.comments[0].id.clone(),
            excerpt: "other".into(),
        }],
        Vec::new(),
        &human("octocat"),
        "2026-01-03T00:00:00Z")
    .unwrap_err();
    assert_eq!(err, ERR_QUOTED_COMMENT_NOT_IN_DISCUSSION);
    assert_eq!(
        log_lines(root, "specs/a.md").len(),
        before,
        "nothing was appended"
    );
}

// -- CMS-FR-17, CMS-FR-18 / CMS-FR-20 / CMS-FR-19 (lock + resolve) -----------------

#[test]
fn a_locked_thread_takes_no_comment_until_it_is_unlocked() {
    // CMS-FR-18 / CMS-FR-17.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "first",
        "2026-01-01T00:00:00Z",
    );

    set_lock_in(
        root,
        "specs/a.md",
        &t.id,
        true,
        &human("raver119"),
        "2026-01-02T00:00:00Z",
    )
    .unwrap();

    let before = log_lines(root, "specs/a.md").len();
    assert_eq!(
        add_comment_in(
            root,
            "specs/a.md",
            &t.id,
            "blocked".into(),
            vec![],
            Vec::new(),
            &human("octocat"),
            "2026-01-03T00:00:00Z"
        )
        .unwrap_err(),
        ERR_DISCUSSION_LOCKED
    );
    assert_eq!(log_lines(root, "specs/a.md").len(), before);

    set_lock_in(
        root,
        "specs/a.md",
        &t.id,
        false,
        &human("raver119"),
        "2026-01-04T00:00:00Z",
    )
    .unwrap();
    let after = add_comment_in(
        root,
        "specs/a.md",
        &t.id,
        "now fine".into(),
        vec![],
        Vec::new(),
        &human("octocat"),
        "2026-01-05T00:00:00Z")
    .unwrap();
    assert_eq!(after.comments.len(), 2);
}

#[test]
fn lock_and_resolution_move_independently_in_every_combination() {
    // CMS-FR-18 / CMS-FR-20.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "first",
        "2026-01-01T00:00:00Z",
    );
    let by = human("raver119");
    let id = t.id.clone();

    let resolved = set_resolution_in(root, "specs/a.md", &id, true, &by, "2026-01-02T00:00:00Z")
        .unwrap();
    assert!(resolved.resolved && !resolved.locked);
    // A resolved thread still takes a comment — only a lock stops that.
    add_comment_in(
        root,
        "specs/a.md",
        &id,
        "still talking".into(),
        vec![],
        Vec::new(),
        &by,
        "2026-01-03T00:00:00Z")
    .unwrap();

    let both =
        set_lock_in(root, "specs/a.md", &id, true, &by, "2026-01-04T00:00:00Z").unwrap();
    assert!(both.locked && both.resolved);

    // Reopening a locked thread leaves it locked.
    let reopened =
        set_resolution_in(root, "specs/a.md", &id, false, &by, "2026-01-05T00:00:00Z").unwrap();
    assert!(reopened.locked && !reopened.resolved);
}

#[test]
fn setting_a_state_a_thread_already_holds_appends_nothing() {
    // CMS-FR-19.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "first",
        "2026-01-01T00:00:00Z",
    );
    let by = human("raver119");
    set_resolution_in(root, "specs/a.md", &t.id, true, &by, "2026-01-02T00:00:00Z").unwrap();

    let lines = log_lines(root, "specs/a.md").len();
    let again =
        set_resolution_in(root, "specs/a.md", &t.id, true, &by, "2026-09-09T00:00:00Z")
            .unwrap();
    assert_eq!(log_lines(root, "specs/a.md").len(), lines);
    assert_eq!(again.updated_at, "2026-01-02T00:00:00Z");

    // Same for an unlock on a thread that was never locked.
    set_lock_in(root, "specs/a.md", &t.id, false, &by, "2026-09-09T00:00:00Z").unwrap();
    assert_eq!(log_lines(root, "specs/a.md").len(), lines);
}
