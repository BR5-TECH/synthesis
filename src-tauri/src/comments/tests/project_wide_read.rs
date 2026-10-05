//! The project-wide thread read (CMS-FR-32 ... CMS-FR-35).

use super::*;

// -- CMS-FR-33 … CMS-FR-34: the project-wide read (CMS-FR-32 … CMS-FR-35) --

/// Write a raw line into an artifact's log, bypassing the event builders, so
/// a test can produce a log shape the writers deliberately cannot.
fn write_raw_log(root: &Path, artifact: &str, lines: &[&str]) {
    let dir = comments_dir(root);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{}.jsonl", log_id(artifact)));
    let mut text = lines.join("\n");
    text.push('\n');
    std::fs::write(path, text).unwrap();
}

/// The (stem, events) pairs `list_all_discussions_in` works from.
fn read_logs(root: &Path) -> Vec<(String, Vec<Event>)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(comments_dir(root)).unwrap().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let stem = path.file_stem().unwrap().to_str().unwrap().to_string();
        out.push((stem, parse_events(&std::fs::read_to_string(&path).unwrap())));
    }
    out
}

fn listed_ids(items: &[DiscussionListItem]) -> Vec<String> {
    items.iter().map(|i| i.discussion.id.clone()).collect()
}

#[test]
fn list_all_returns_every_thread_with_its_artifact_most_recent_first() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    touch(root, "specs/b.md");

    let older = open(root, "specs/a.md", anchor(0, 3, "one"), "first", "2024-01-01T00:00:00Z");
    let locked = open(root, "specs/b.md", anchor(0, 3, "two"), "second", "2024-01-02T00:00:00Z");
    let resolved = open(root, "specs/b.md", anchor(9, 12, "six"), "third", "2024-01-03T00:00:00Z");
    set_lock_in(root, "specs/b.md", &locked.id, true, &human("raver119"), "2024-01-04T00:00:00Z").unwrap();
    set_resolution_in(root, "specs/b.md", &resolved.id, true, &human("raver119"), "2024-01-05T00:00:00Z").unwrap();

    let items = list_all_discussions_in(root, root);
    assert_eq!(items.len(), 3, "no thread is withheld for its lock or resolution");

    // CMS-FR-32: most-recently-active first — the resolve at ..05 is the
    // latest event, then the lock at ..04, then the untouched thread.
    assert_eq!(listed_ids(&items), vec![resolved.id.clone(), locked.id.clone(), older.id.clone()]);

    // CMS-FR-33: each thread names the artifact it was opened against.
    let by_id = |id: &str| items.iter().find(|i| i.discussion.id == id).unwrap();
    assert_eq!(by_id(&older.id).discussion.file_rel(), "specs/a.md");
    assert_eq!(by_id(&locked.id).discussion.file_rel(), "specs/b.md");
    assert!(by_id(&locked.id).discussion.locked);
    assert!(by_id(&resolved.id).discussion.resolved);
    assert!(items.iter().all(|i| !i.owner_unavailable), "every artifact exists on disk");
}

#[test]
fn an_unreadable_log_is_skipped_and_every_other_is_still_served() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    touch(root, "specs/b.md");
    open(root, "specs/a.md", anchor(0, 3, "one"), "first", "2024-01-01T00:00:00Z");
    open(root, "specs/b.md", anchor(0, 3, "two"), "second", "2024-01-02T00:00:00Z");

    // A directory wearing a log's name: `read_text` fails on it exactly as it
    // would on a log whose permissions deny a read.
    let broken = comments_dir(root).join("00112233445566778899aabbccddeeff.jsonl");
    std::fs::create_dir_all(&broken).unwrap();

    // CMS-FR-35: the two readable logs are still served, and no error is
    // raised for the third.
    assert_eq!(list_all_discussions_in(root, root).len(), 2);
    assert!(broken.is_dir(), "the unreadable entry is left untouched");
}

#[test]
fn a_log_with_no_thread_opened_names_no_artifact_and_is_dropped() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    let kept = open(root, "specs/a.md", anchor(0, 3, "one"), "first", "2024-01-01T00:00:00Z");

    // CMS-FR-33: a `comment_added` with no `thread_opened` anywhere in its
    // log names no artifact, so nothing can attribute, open, or anchor it.
    write_raw_log(
        root,
        "specs/orphan.md",
        &[r#"{"v":1,"eventId":"e1","threadId":"t1","at":"2024-06-01T00:00:00Z","by":{"kind":"human","login":"raver119"},"type":"comment_added","commentId":"c1","body":"stranded","quotes":[]}"#],
    );

    let items = list_all_discussions_in(root, root);
    assert_eq!(listed_ids(&items), vec![kept.id], "the stranded thread is absent");
}

#[test]
fn a_renamed_artifacts_threads_list_under_its_current_path() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    let thread = open(root, "specs/a.md", anchor(0, 3, "one"), "first", "2024-01-01T00:00:00Z");

    assert_eq!(rename_and_follow(root, "specs/a.md", "specs/renamed.md"), 1);

    // The log's *lines* still record `specs/a.md` — it is append-only, and
    // `follow_rename` moves the file rather than rewriting them. The listing
    // must still name where the artifact now is, or the panel's click-through
    // would open nothing and the row would read as a dead file.
    let items = list_all_discussions_in(root, root);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].discussion.id, thread.id);
    assert_eq!(items[0].discussion.file_rel(), "specs/renamed.md");
    assert!(!items[0].owner_unavailable, "the artifact exists under its new name");
}

#[test]
fn a_deleted_artifacts_threads_list_unresolved_under_their_last_known_path() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    open(root, "specs/a.md", anchor(0, 3, "one"), "first", "2024-01-01T00:00:00Z");
    std::fs::remove_file(root.join("specs/a.md")).unwrap();

    // CMS-FR-25: the log is retained, so the conversation outlives the file.
    let items = list_all_discussions_in(root, root);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].discussion.file_rel(), "specs/a.md");
    assert!(items[0].owner_unavailable, "there is nothing left to open");
}

#[test]
fn a_renamed_then_deleted_artifacts_threads_survive_under_their_new_path() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    open(root, "specs/a.md", anchor(0, 3, "one"), "first", "2024-01-01T00:00:00Z");
    assert_eq!(rename_and_follow(root, "specs/a.md", "specs/renamed.md"), 1);
    std::fs::remove_file(root.join("specs/renamed.md")).unwrap();

    // The recorded path no longer hashes to the filename (the rename), AND
    // the index cannot supply a current path (the delete). The fallback is
    // what keeps the conversation readable rather than dropping it —
    // CMS-FR-25's "nothing an author wrote is lost".
    let items = list_all_discussions_in(root, root);
    assert_eq!(items.len(), 1);
    assert!(items[0].owner_unavailable);
    assert!(!items[0].discussion.file_rel().is_empty());
}

#[test]
fn threads_sharing_an_instant_are_ordered_by_id_rather_than_by_directory() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    touch(root, "specs/b.md");
    // The same instant in two different logs, so the only thing that can
    // order them is the tie-break — without it the order follows `read_dir`
    // and differs between machines.
    let at = "2024-01-01T00:00:00Z";
    let one = open(root, "specs/a.md", anchor(0, 3, "one"), "first", at);
    let two = open(root, "specs/b.md", anchor(0, 3, "two"), "second", at);

    let mut expected = vec![one.id, two.id];
    expected.sort();
    assert_eq!(listed_ids(&list_all_discussions_in(root, root)), expected);
}

#[test]
fn the_project_is_enumerated_only_when_a_rename_has_desynced_a_log() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    touch(root, "specs/b.md");
    open(root, "specs/a.md", anchor(0, 3, "one"), "first", "2024-01-01T00:00:00Z");
    open(root, "specs/b.md", anchor(0, 3, "two"), "second", "2024-01-02T00:00:00Z");

    // CMS-FR-34: nothing renamed, so every log's recorded path still hashes
    // to its own filename and the walk is not needed.
    assert!(!needs_path_index(&read_logs(root)));

    assert_eq!(rename_and_follow(root, "specs/a.md", "specs/renamed.md"), 1);
    // One log now disagrees with its filename, which is the whole trigger.
    assert!(needs_path_index(&read_logs(root)));
}

#[test]
fn a_comments_folder_holding_only_gitattributes_lists_nothing() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(comments_dir(root)).unwrap();
    std::fs::write(comments_dir(root).join(".gitattributes"), "*.jsonl merge=union\n").unwrap();
    assert!(list_all_discussions_in(root, root).is_empty());
}

#[test]
fn list_all_on_a_project_with_no_comments_folder_is_empty() {
    let dir = temp_root();
    assert!(list_all_discussions_in(
        &crate::fs::RootFs::for_root(dir.path()),
        &crate::fs::RootFs::for_root(dir.path()),
    ).is_empty());
}
