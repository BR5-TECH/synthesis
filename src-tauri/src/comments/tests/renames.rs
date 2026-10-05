//! The log follows a renamed artifact (CMS-FR-24, CMS-FR-25).

use super::*;

// -- CMS-FR-24 / CMS-FR-25 (renames) ------------------------------------

#[test]
fn a_followed_rename_moves_the_log_without_rewriting_a_line() {
    // CMS-FR-24.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    let t = open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "keep me",
        "2026-01-01T00:00:00Z",
    );
    let before = log_lines(root, "specs/a.md");

    assert_eq!(rename_and_follow(root, "specs/a.md", "specs/b.md"), 1);

    assert_eq!(
        log_lines(root, "specs/b.md"),
        before,
        "no line inside the log changed"
    );
    let moved = list_fragment_discussions_in(root, "specs/b.md");
    assert_eq!(moved.len(), 1);
    assert_eq!(moved[0].id, t.id);
    assert_eq!(moved[0].updated_at, t.updated_at, "no updated_at moved");
    assert!(list_fragment_discussions_in(root, "specs/a.md").is_empty());
}

#[test]
fn a_second_rename_of_the_same_artifact_is_followed_too() {
    // The correlation must run off the log's *filename*, which this function
    // keeps in sync. Correlating off the `artifact_path` inside the log would
    // work exactly once — the field is written at creation and, because the
    // log is append-only, never updated — and then silently stop matching,
    // stranding the conversation under a name nothing looks for.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    open(root, "specs/a.md", anchor(0, 3, "abc"), "keep me", "2026-01-01T00:00:00Z");

    assert_eq!(rename_and_follow(root, "specs/a.md", "specs/b.md"), 1);
    assert_eq!(rename_and_follow(root, "specs/b.md", "specs/c.md"), 1);
    assert_eq!(rename_and_follow(root, "specs/c.md", "specs/d.md"), 1);

    let final_threads = list_fragment_discussions_in(root, "specs/d.md");
    assert_eq!(final_threads.len(), 1, "the thread survived three renames");
    assert_eq!(final_threads[0].comments[0].body, "keep me");
    for stale in ["specs/a.md", "specs/b.md", "specs/c.md"] {
        assert!(list_fragment_discussions_in(root, stale).is_empty(), "{stale} still resolves");
    }
    assert_eq!(
        std::fs::read_dir(comments_dir(root))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("jsonl"))
            .count(),
        1,
        "and no stranded copies were left behind"
    );
}

#[test]
fn a_delete_leaves_the_log_in_place() {
    // CMS-FR-25: nothing an author wrote is lost to a move this
    // module could not follow.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "orphan",
        "2026-01-01T00:00:00Z",
    );
    let path = log_path(root, "specs/a.md").unwrap();
    assert!(path.exists());

    // A rename this module cannot correlate never reaches follow_rename.
    assert_eq!(follow_rename(root, root, "specs/zzz.md", "specs/yyy.md"), 0);
    assert_eq!(follow_rename(root, root, "specs/a.md", "specs/a.md"), 0);
    assert_eq!(follow_rename(root, root, "", "x.md"), 0);
    assert!(path.exists(), "the log is retained rather than removed");
    assert_eq!(list_fragment_discussions_in(root, "specs/a.md").len(), 1);
}

#[test]
fn following_a_folder_rename_moves_the_logs_inside_it() {
    // CMS-FR-24: a log is named from its artifact's path, so a folder move
    // changes the name of every log underneath it.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "old/a.md");
    touch(root, "old/deep/b.md");
    touch(root, "other.md");
    open(root, "old/a.md", anchor(0, 3, "abc"), "one", "2026-01-01T00:00:00Z");
    open(
        root,
        "old/deep/b.md",
        anchor(0, 3, "abc"),
        "two",
        "2026-01-01T00:00:00Z",
    );
    open(root, "other.md", anchor(0, 3, "abc"), "untouched", "2026-01-01T00:00:00Z");

    assert_eq!(rename_and_follow(root, "old", "new"), 2);

    assert_eq!(list_fragment_discussions_in(root, "new/a.md")[0].comments[0].body, "one");
    assert_eq!(
        list_fragment_discussions_in(root, "new/deep/b.md")[0].comments[0].body,
        "two"
    );
    assert!(list_fragment_discussions_in(root, "old/a.md").is_empty());
    assert_eq!(
        list_fragment_discussions_in(root, "other.md")[0].comments[0].body,
        "untouched",
        "a log on an unrelated artifact must not be re-filed"
    );

    // And a second folder rename is followed just as the first was.
    assert_eq!(rename_and_follow(root, "new", "newer"), 2);
    assert_eq!(list_fragment_discussions_in(root, "newer/a.md")[0].comments[0].body, "one");
    assert_eq!(
        list_fragment_discussions_in(root, "newer/deep/b.md")[0].comments[0].body,
        "two"
    );
}

#[test]
fn a_prefix_that_is_not_a_folder_boundary_is_not_followed() {
    // "old" must not match "older.md": the walk enumerates what actually
    // moved, so an unrelated sibling is never swept up.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "older.md");
    touch(root, "old/x.md");
    open(root, "older.md", anchor(0, 3, "abc"), "one", "2026-01-01T00:00:00Z");

    assert_eq!(rename_and_follow(root, "old", "new"), 0);
    assert_eq!(list_fragment_discussions_in(root, "older.md").len(), 1);
}

#[test]
fn a_rename_onto_an_artifact_that_already_has_a_log_merges_nothing() {
    // Overwriting would silently destroy the destination's conversation.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    touch(root, "specs/b.md");
    open(root, "specs/a.md", anchor(0, 3, "abc"), "from a", "2026-01-01T00:00:00Z");
    open(root, "specs/b.md", anchor(0, 3, "abc"), "from b", "2026-01-01T00:00:00Z");

    // The destination already holds a log, so nothing is re-filed.
    assert_eq!(follow_rename(root, root, "specs/a.md", "specs/b.md"), 0);
    assert_eq!(list_fragment_discussions_in(root, "specs/a.md")[0].comments[0].body, "from a");
    assert_eq!(list_fragment_discussions_in(root, "specs/b.md")[0].comments[0].body, "from b");
}
