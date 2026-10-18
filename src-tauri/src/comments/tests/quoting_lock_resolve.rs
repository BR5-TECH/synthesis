//! Quotes and the one resolved state (CMS-FR-16 ... CMS-FR-20, CMS-FR-WKZR).

use super::*;

// -- CMS-FR-16 (quoting) ------------------------------------------------

// CMS-FR-16: a quote names a comment of the same discussion.
#[test]
fn a_quote_must_name_a_comment_in_the_same_thread() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t1 = open(root, "specs/a.md", anchor(0, 3, "abc"), "first", "2026-01-01T00:00:00Z");
    let t2 = open(root, "specs/a.md", anchor(10, 13, "def"), "other", "2026-01-01T00:00:01Z");
    let c1 = t1.comments[0].id.clone();

    let updated = add_comment_in(
        root,
        "specs/a.md",
        &t1.id,
        "agreed".into(),
        vec![CommentQuote { comment_id: c1.clone(), excerpt: "first".into() }],
        Vec::new(),
        Stamp { by: &human("octocat"), at: "2026-01-02T00:00:00Z" },
    )
    .unwrap();
    assert_eq!(updated.comments.len(), 2);
    assert_eq!(updated.comments[1].quotes[0].comment_id, c1);

    let before = log_lines(root, "specs/a.md").len();
    let err = add_comment_in(
        root,
        "specs/a.md",
        &t1.id,
        "nope".into(),
        vec![CommentQuote { comment_id: t2.comments[0].id.clone(), excerpt: "other".into() }],
        Vec::new(),
        Stamp { by: &human("octocat"), at: "2026-01-03T00:00:00Z" },
    )
    .unwrap_err();
    assert_eq!(err, ERR_QUOTED_COMMENT_NOT_IN_DISCUSSION);
    assert_eq!(log_lines(root, "specs/a.md").len(), before, "nothing was appended");
}

// -- CMS-FR-17 / CMS-FR-18 / CMS-FR-19 / CMS-FR-20 (resolution) ---------

fn comment(root: &crate::fs::RootFs, id: &str, body: &str, at: &str) -> Result<Discussion, String> {
    add_comment_in(
        root,
        "specs/a.md",
        id,
        body.into(),
        vec![],
        Vec::new(),
        Stamp { by: &human("octocat"), at },
    )
}

// CMS-FR-17 / CMS-FR-18: a resolved discussion refuses a comment with
// `discussion_resolved` and appends nothing; a reopen lets it take one again.
#[test]
fn a_resolved_thread_takes_no_comment_until_it_is_reopened() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(root, "specs/a.md", anchor(0, 3, "abc"), "first", "2026-01-01T00:00:00Z");
    assert!(!t.resolved, "a newly opened discussion is active");

    let resolved = set_resolution_in(
        root,
        "specs/a.md",
        &t.id,
        true,
        &human("raver119"),
        "2026-01-02T00:00:00Z",
    )
    .unwrap();
    assert!(resolved.resolved);

    let before = log_lines(root, "specs/a.md").len();
    assert_eq!(
        comment(root, &t.id, "blocked", "2026-01-03T00:00:00Z").unwrap_err(),
        ERR_DISCUSSION_RESOLVED
    );
    assert_eq!(ERR_DISCUSSION_RESOLVED, "discussion_resolved");
    assert_eq!(log_lines(root, "specs/a.md").len(), before);

    let reopened = set_resolution_in(
        root,
        "specs/a.md",
        &t.id,
        false,
        &human("raver119"),
        "2026-01-04T00:00:00Z",
    )
    .unwrap();
    assert!(!reopened.resolved);
    let after = comment(root, &t.id, "now fine", "2026-01-05T00:00:00Z").unwrap();
    assert_eq!(after.comments.len(), 2);
}

// CMS-FR-19: setting the state a discussion already holds appends nothing.
#[test]
fn setting_a_state_a_thread_already_holds_appends_nothing() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(root, "specs/a.md", anchor(0, 3, "abc"), "first", "2026-01-01T00:00:00Z");
    let by = human("raver119");
    set_resolution_in(root, "specs/a.md", &t.id, true, &by, "2026-01-02T00:00:00Z").unwrap();

    let lines = log_lines(root, "specs/a.md").len();
    let again =
        set_resolution_in(root, "specs/a.md", &t.id, true, &by, "2026-09-09T00:00:00Z").unwrap();
    assert_eq!(log_lines(root, "specs/a.md").len(), lines);
    assert_eq!(again.updated_at, "2026-01-02T00:00:00Z");

    // The same for a reopen of a discussion that is already active.
    set_resolution_in(root, "specs/a.md", &t.id, false, &by, "2026-01-03T00:00:00Z").unwrap();
    let lines = log_lines(root, "specs/a.md").len();
    let (_, appended) =
        set_resolution_reporting(root, "specs/a.md", &t.id, false, &by, "2026-09-09T00:00:00Z")
            .unwrap();
    assert!(!appended);
    assert_eq!(log_lines(root, "specs/a.md").len(), lines);
}

// -- CMS-FR-WKZR (a legacy lock reads as a resolution) -------------------

// CMS-FR-WKZR / CMS-FR-17: a legacy `thread_locked` line folds as resolved,
// and the discussion refuses a comment exactly as a resolved one does.
#[test]
fn a_legacy_lock_reads_as_resolved() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(root, "specs/a.md", anchor(0, 3, "abc"), "first", "2026-01-01T00:00:00Z");
    write_legacy(root, t.log_ref(), &t.id, EventBody::ThreadLocked, "2026-01-02T00:00:00Z");

    let folded = list_fragment_discussions_in(root, "specs/a.md");
    assert!(folded[0].resolved, "a legacy lock reads as a resolution");
    assert_eq!(
        comment(root, &t.id, "blocked", "2026-01-03T00:00:00Z").unwrap_err(),
        ERR_DISCUSSION_RESOLVED
    );
}

// CMS-FR-WKZR: a legacy lock released by a later `thread_unlocked` reads as
// active, and the log is never rewritten.
#[test]
fn a_legacy_lock_released_by_an_unlock_reads_as_active() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(root, "specs/a.md", anchor(0, 3, "abc"), "first", "2026-01-01T00:00:00Z");
    write_legacy(root, t.log_ref(), &t.id, EventBody::ThreadLocked, "2026-01-02T00:00:00Z");
    write_legacy(root, t.log_ref(), &t.id, EventBody::ThreadUnlocked, "2026-01-03T00:00:00Z");
    let before = log_lines(root, "specs/a.md");

    assert!(!list_fragment_discussions_in(root, "specs/a.md")[0].resolved);
    assert_eq!(log_lines(root, "specs/a.md"), before, "the fold rewrites nothing");
}

// CMS-FR-WKZR: a legacy lock that stands keeps the discussion resolved after
// the resolution of its own was released by an earlier reopen; only a reopen
// after the lock clears it.
#[test]
fn a_legacy_lock_after_a_reopen_still_reads_as_resolved() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(root, "specs/a.md", anchor(0, 3, "abc"), "first", "2026-01-01T00:00:00Z");
    let by = human("raver119");
    set_resolution_in(root, "specs/a.md", &t.id, true, &by, "2026-01-02T00:00:00Z").unwrap();
    // An older build: reopened, then locked. The reopen was written before the
    // lock, so it does not release it.
    write_legacy(root, t.log_ref(), &t.id, EventBody::ThreadReopened, "2026-01-03T00:00:00Z");
    write_legacy(root, t.log_ref(), &t.id, EventBody::ThreadLocked, "2026-01-04T00:00:00Z");

    assert!(list_fragment_discussions_in(root, "specs/a.md")[0].resolved, "the lock stands");

    // An older build: resolved, then reopened after the lock. The reopen
    // clears the lock too, so the discussion reads as active.
    write_legacy(root, t.log_ref(), &t.id, EventBody::ThreadResolved, "2026-01-05T00:00:00Z");
    write_legacy(root, t.log_ref(), &t.id, EventBody::ThreadReopened, "2026-01-06T00:00:00Z");
    assert!(!list_fragment_discussions_in(root, "specs/a.md")[0].resolved);
}

// CMS-FR-WKZR: an unlock releases the lock only, so a resolution that stands
// keeps the discussion resolved.
#[test]
fn an_unlock_leaves_a_standing_resolution_resolved() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(root, "specs/a.md", anchor(0, 3, "abc"), "first", "2026-01-01T00:00:00Z");
    let by = human("raver119");
    set_resolution_in(root, "specs/a.md", &t.id, true, &by, "2026-01-02T00:00:00Z").unwrap();
    write_legacy(root, t.log_ref(), &t.id, EventBody::ThreadLocked, "2026-01-03T00:00:00Z");
    write_legacy(root, t.log_ref(), &t.id, EventBody::ThreadUnlocked, "2026-01-04T00:00:00Z");

    assert!(list_fragment_discussions_in(root, "specs/a.md")[0].resolved, "the resolution stands");
}

// CMS-FR-18 / CMS-FR-20 / CMS-FR-WKZR: a reopen of a discussion that is both
// resolved and legacy-locked appends `thread_unlocked` and `thread_reopened`
// in one append, and leaves it active.
#[test]
fn a_reopen_of_a_resolved_and_locked_thread_clears_both() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(root, "specs/a.md", anchor(0, 3, "abc"), "first", "2026-01-01T00:00:00Z");
    let by = human("raver119");
    set_resolution_in(root, "specs/a.md", &t.id, true, &by, "2026-01-02T00:00:00Z").unwrap();
    write_legacy(root, t.log_ref(), &t.id, EventBody::ThreadLocked, "2026-01-03T00:00:00Z");
    let before = log_lines(root, "specs/a.md").len();

    let (reopened, appended) =
        set_resolution_reporting(root, "specs/a.md", &t.id, false, &by, "2026-01-04T00:00:00Z")
            .unwrap();
    assert!(appended);
    assert!(!reopened.resolved);
    let lines = log_lines(root, "specs/a.md");
    assert_eq!(lines.len(), before + 2);
    let types = line_types(&lines[before..]);
    assert!(types.contains(&"thread_unlocked".to_string()), "got {types:?}");
    assert!(types.contains(&"thread_reopened".to_string()), "got {types:?}");
    comment(root, &t.id, "talking again", "2026-01-05T00:00:00Z").unwrap();
}

// CMS-FR-18 / CMS-FR-20: a reopen of a discussion stored as locked only appends
// `thread_unlocked` with `thread_reopened`, and leaves it active.
#[test]
fn a_reopen_of_a_legacy_locked_thread_appends_unlock_and_reopen() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(root, "specs/a.md", anchor(0, 3, "abc"), "first", "2026-01-01T00:00:00Z");
    write_legacy(root, t.log_ref(), &t.id, EventBody::ThreadLocked, "2026-01-02T00:00:00Z");
    let before = log_lines(root, "specs/a.md").len();

    let reopened = set_resolution_in(
        root,
        "specs/a.md",
        &t.id,
        false,
        &human("raver119"),
        "2026-01-03T00:00:00Z",
    )
    .unwrap();
    assert!(!reopened.resolved);
    let lines = log_lines(root, "specs/a.md");
    let mut types = line_types(&lines[before..]);
    types.sort();
    assert_eq!(types, vec!["thread_reopened".to_string(), "thread_unlocked".to_string()]);
    assert!(!list_fragment_discussions_in(root, "specs/a.md")[0].resolved);
}

// CMS-FR-05 / CMS-FR-18: a reopen of a discussion with no legacy lock appends
// `thread_reopened` alone, and no write produces a `thread_locked`.
#[test]
fn a_plain_reopen_appends_only_thread_reopened() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(root, "specs/a.md", anchor(0, 3, "abc"), "first", "2026-01-01T00:00:00Z");
    let by = human("raver119");
    set_resolution_in(root, "specs/a.md", &t.id, true, &by, "2026-01-02T00:00:00Z").unwrap();
    let before = log_lines(root, "specs/a.md").len();
    set_resolution_in(root, "specs/a.md", &t.id, false, &by, "2026-01-03T00:00:00Z").unwrap();

    let lines = log_lines(root, "specs/a.md");
    assert_eq!(line_types(&lines[before..]), vec!["thread_reopened".to_string()]);
    let all = line_types(&lines);
    assert!(!all.iter().any(|t| t == "thread_locked" || t == "thread_unlocked"), "got {all:?}");
}
