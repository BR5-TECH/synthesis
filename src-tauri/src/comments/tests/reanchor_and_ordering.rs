//! Re-anchoring and comment order (CMS-FR-21 ... CMS-FR-23).

use super::*;

// -- CMS-FR-21, CMS-FR-22 (reanchor) -----------------------------------------------

#[test]
fn a_locked_and_resolved_thread_still_follows_its_text() {
    // CMS-FR-22 / CMS-FR-21: refusing this would strand every settled thread
    // the moment the artifact above it grew.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(
        root,
        "specs/a.md",
        anchor(40, 61, "the first session"),
        "first",
        "2026-01-01T00:00:00Z",
    );
    let by = human("raver119");
    set_lock_in(root, "specs/a.md", &t.id, true, &by, "2026-01-02T00:00:00Z").unwrap();
    set_resolution_in(root, "specs/a.md", &t.id, true, &by, "2026-01-03T00:00:00Z").unwrap();

    let moved = reanchor_in(
        root,
        "specs/a.md",
        &t.id,
        anchor(72, 93, "the first session"),
        &by,
        "2026-01-04T00:00:00Z",
    )
    .unwrap();
    assert_eq!(
        moved.fragment_target,
        Some(FragmentTarget::in_artifact("specs/a.md", 72, 93, "the first session")),
    );
    assert!(moved.locked && moved.resolved);

    // An anchor that is not a range at all is refused and appends nothing.
    let lines = log_lines(root, "specs/a.md").len();
    assert_eq!(
        reanchor_in(
            root,
            "specs/a.md",
            &t.id,
            anchor(10, 10, "x"),
            &by,
            "2026-01-05T00:00:00Z"
        )
        .unwrap_err(),
        ERR_INVALID_FRAGMENT
    );
    assert_eq!(
        reanchor_in(
            root,
            "specs/a.md",
            &t.id,
            anchor(10, 4, "x"),
            &by,
            "2026-01-05T00:00:00Z"
        )
        .unwrap_err(),
        ERR_INVALID_FRAGMENT
    );
    assert_eq!(log_lines(root, "specs/a.md").len(), lines);

    // Re-anchoring to where it already is appends nothing either.
    reanchor_in(
        root,
        "specs/a.md",
        &t.id,
        anchor(72, 93, "the first session"),
        &by,
        "2026-01-06T00:00:00Z",
    )
    .unwrap();
    assert_eq!(log_lines(root, "specs/a.md").len(), lines);
}

#[test]
fn opening_a_thread_on_a_degenerate_anchor_is_refused() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    assert_eq!(
        open_artifact_fragment_in(
            root,
            "specs/a.md",
            anchor(5, 5, ""),
            "body".into(),
            Vec::new(),
            &human("raver119"),
            "2026-01-01T00:00:00Z"
        )
        .unwrap_err(),
        ERR_INVALID_FRAGMENT
    );
    assert!(!comments_dir(root).exists(), "nothing was written");
}

// -- CMS-FR-23 (ordering) -----------------------------------------------

#[test]
fn threads_come_back_in_anchor_order_with_every_state_among_them() {
    // CMS-FR-23.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let by = human("raver119");
    let late = open(
        root,
        "specs/a.md",
        anchor(900, 910, "ninehundred"),
        "c",
        "2026-01-01T00:00:00Z",
    );
    let early = open(
        root,
        "specs/a.md",
        anchor(120, 130, "onetwenty"),
        "a",
        "2026-01-01T00:00:01Z",
    );
    let mid = open(
        root,
        "specs/a.md",
        anchor(450, 460, "fourfifty"),
        "b",
        "2026-01-01T00:00:02Z",
    );
    set_lock_in(root, "specs/a.md", &mid.id, true, &by, "2026-01-02T00:00:00Z").unwrap();
    set_resolution_in(root, "specs/a.md", &late.id, true, &by, "2026-01-02T00:00:00Z")
        .unwrap();

    let ids: Vec<String> = list_fragment_discussions_in(root, "specs/a.md")
        .into_iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(ids, vec![early.id, mid.id, late.id]);
    assert_eq!(
        list_fragment_discussions_in(root, "specs/a.md").len(),
        3,
        "a locked or resolved thread is never withheld"
    );
}
