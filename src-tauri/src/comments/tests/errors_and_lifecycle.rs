//! Errors, containment and the append-only lifecycle (CMS-FR-15 ... CMS-FR-30).

use super::*;

// -- CMS-FR-15 ----------------------------------------------------------

#[test]
fn a_thread_with_no_comment_is_not_served() {
    // CMS-FR-15.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let opened = Event {
        v: 1,
        event_id: "e1".into(),
        thread_id: "t1".into(),
        at: "2026-01-01T00:00:00Z".into(),
        by: human("raver119"),
        body: EventBody::ThreadOpened {
            artifact_path: "specs/a.md".into(),
            anchor: legacy_anchor(0, 3, "abc"),
        },
    };
    let path = log_path(root, "specs/a.md").unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_string(&opened).unwrap() + "\n").unwrap();

    assert!(list_fragment_discussions_in(root, "specs/a.md").is_empty());
}

// -- CMS-FR-28, CMS-FR-XQBM (worktrees) ---------------------------------

/// CMS-FR-28, CMS-FR-XQBM: the store is the **repository's**, so every worktree
/// of one repository reads and writes one set of logs and a change of active
/// worktree changes nothing a reader sees.
///
/// The two worktrees are real directories here, distinct from the store and
/// from each other, which is the whole of what this asserts: a thread opened
/// while one is active is read back while the other is, out of one file.
#[test]
fn every_worktree_of_one_repository_reads_and_writes_one_set_of_logs() {
    let store_dir = temp_root();
    let store = &fsa::RootFs::for_root(store_dir.path());
    let primary = temp_root();
    let linked = temp_root();

    let opened = open(
        store,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "written from the primary worktree",
        "2026-01-01T00:00:00Z",
    );

    // The switch: a different worktree, the same store.
    for worktree in [&primary, &linked] {
        let threads = list_fragment_discussions_in(store, "specs/a.md");
        assert_eq!(threads, vec![opened.clone()], "the same thread, either way");
        assert!(
            !comments_dir(worktree.path()).exists(),
            "and nothing was written into a worktree (CMS-FR-VJRP)",
        );
    }

    // A second store is a second repository, and holds none of it (CMS-FR-XQBM).
    let other_repository = temp_root();
    assert!(
        list_fragment_discussions_in(&fsa::RootFs::for_root(other_repository.path()), "specs/a.md").is_empty(),
    );
}

/// CMS-FR-01, CMS-FR-VJRP, RMS-FR-HDNZ: a write reaches the store and leaves the
/// worktree byte-for-byte as it was.
///
/// The two roots are separate directories, so this fails if any path a write
/// composes is resolved against the worktree rather than the store.
#[test]
fn a_write_leaves_the_worktree_byte_for_byte_as_it_was() {
    let store_dir = temp_root();
    let worktree_dir = temp_root();
    let store = &fsa::RootFs::for_root(store_dir.path());
    let worktree = &fsa::RootFs::for_root(worktree_dir.path());
    std::fs::create_dir_all(worktree_dir.path().join("specs")).unwrap();
    std::fs::write(worktree_dir.path().join("specs/a.md"), "abc\n").unwrap();

    let before = tree_snapshot(worktree_dir.path());

    let thread = open(
        store,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "a remark",
        "2026-01-01T00:00:00Z",
    );
    add_comment_in(
        store,
        "specs/a.md",
        &thread.id,
        "a reply".into(),
        Vec::new(),
        vec![inline_png("shot.png", b"bytes")],
        &human("raver119"),
        "2026-01-01T00:00:01Z",
    )
    .unwrap();
    // The project-wide read takes both roots, and still writes into neither.
    list_all_discussions_in(store, worktree);

    assert_eq!(
        tree_snapshot(worktree_dir.path()),
        before,
        "a conversation modified the worktree",
    );
    assert!(comments_dir(store_dir.path()).is_dir(), "and reached the store");
}

/// Every file under `dir`, as `(relative path, bytes)`, for asserting a tree is
/// untouched.
fn tree_snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let path = entry.path();
            if path.is_dir() {
                out.push((format!("{rel}/"), Vec::new()));
                walk(&path, &rel, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.push((rel, bytes));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, "", &mut out);
    out.sort();
    out
}

// -- CMS-FR-30 / CMS-FR-27, FSA-FR-10 (errors + containment) -----------------------

#[test]
fn an_unknown_thread_is_the_typed_not_found() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let by = human("raver119");
    for result in [
        add_comment_in(root, "specs/a.md", "nope", "x".into(), vec![], Vec::new(), &by, "t"),
        set_lock_in(root, "specs/a.md", "nope", true, &by, "t"),
        set_resolution_in(root, "specs/a.md", "nope", true, &by, "t"),
        reanchor_in(root, "specs/a.md", "nope", anchor(0, 1, "a"), &by, "t"),
    ] {
        assert_eq!(result.unwrap_err(), ERR_DISCUSSION_NOT_FOUND);
    }
}

#[test]
fn nothing_is_written_outside_the_comments_folder() {
    // CMS-FR-27 / FSA-FR-10. The artifact id comes from the frontend, so it
    // is the input a crafted path would ride in — it is hashed into a bare
    // hex filename before it touches a path, which is what makes an escape
    // unrepresentable rather than merely rejected.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("victim.md"), b"precious").unwrap();

    for evil in ["../victim", "../../etc/passwd", "a/../../b", "/etc/passwd"] {
        open(
            root,
            evil,
            anchor(0, 3, "abc"),
            "attempt",
            "2026-01-01T00:00:00Z",
        );
    }
    assert_eq!(
        std::fs::read_to_string(root.join("victim.md")).unwrap(),
        "precious"
    );
    // An attachment's *filename* is log data and must never reach a path
    // either: the bytes are named by their digest, whatever the file was
    // called (CMS-FR-44).
    let thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "with a picture".into(),
        vec![inline_png("../../../evil.png", b"payload")],
        &human("raver119"),
        "2026-01-02T00:00:00Z",
    )
    .unwrap();
    let digest = match only_attachments(&thread)[0] {
        Attachment::Blob { digest, filename, .. } => {
            assert_eq!(filename, "../../../evil.png", "the name is recorded as given");
            digest.clone()
        }
        other => panic!("expected a blob, got {other:?}"),
    };
    assert!(
        attachments_dir(root).join(&digest).exists(),
        "the bytes are at the digest, not at the name",
    );
    assert!(!root.join("evil.png").exists(), "nothing escaped");

    // Every write landed inside the comments folder: a hex-named log, the
    // `.gitattributes`, or the attachments folder holding bare digests.
    for entry in std::fs::read_dir(comments_dir(root)).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ATTACHMENTS_SUBDIR {
            for blob in std::fs::read_dir(entry.path()).unwrap() {
                let blob = blob.unwrap().file_name().to_string_lossy().into_owned();
                assert!(
                    is_storage_digest(&blob),
                    "an attachment is stored under a bare digest, found {blob}",
                );
            }
            continue;
        }
        assert!(
            name == ".gitattributes"
                || (name.ends_with(".jsonl")
                    && name
                        .trim_end_matches(".jsonl")
                        .chars()
                        .all(|c| c.is_ascii_hexdigit())),
            "unexpected file {name}"
        );
    }
}

// -- CMS-FR-04, CMS-FR-05 (append-only over a whole lifecycle) ---------------------

#[test]
fn every_step_adds_exactly_one_line_and_rewrites_none() {
    // CMS-FR-04 / CMS-FR-05.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let by = human("raver119");
    let t = open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "first",
        "2026-01-01T00:00:00Z",
    );
    let id = t.id.clone();

    let mut previous = log_lines(root, "specs/a.md");
    assert_eq!(previous.len(), 2);

    let steps: Vec<Box<dyn Fn() -> Result<Discussion, String>>> = vec![
        Box::new(|| {
            add_comment_in(
                root,
                "specs/a.md",
                &id,
                "reply one".into(),
                vec![],
                Vec::new(),
                &by,
                "2026-01-02T00:00:00Z")
        }),
        Box::new(|| {
            add_comment_in(
                root,
                "specs/a.md",
                &id,
                "reply two".into(),
                vec![],
                Vec::new(),
                &by,
                "2026-01-03T00:00:00Z")
        }),
        Box::new(|| set_lock_in(root, "specs/a.md", &id, true, &by, "2026-01-04T00:00:00Z")),
        Box::new(|| {
            set_resolution_in(root, "specs/a.md", &id, true, &by, "2026-01-05T00:00:00Z")
        }),
        Box::new(|| {
            set_resolution_in(root, "specs/a.md", &id, false, &by, "2026-01-06T00:00:00Z")
        }),
        Box::new(|| set_lock_in(root, "specs/a.md", &id, false, &by, "2026-01-07T00:00:00Z")),
        Box::new(|| {
            reanchor_in(
                root,
                "specs/a.md",
                &id,
                anchor(9, 12, "abc"),
                &by,
                "2026-01-08T00:00:00Z",
            )
        }),
    ];

    let allowed = [
        "discussion_opened",
        "comment_added",
        "thread_locked",
        "thread_unlocked",
        "thread_resolved",
        "thread_reopened",
        "fragment_moved",
    ];

    for step in steps {
        step().unwrap();
        let now = log_lines(root, "specs/a.md");
        assert_eq!(now.len(), previous.len() + 1, "exactly one line per step");
        assert_eq!(
            now[..previous.len()],
            previous[..],
            "every earlier line is byte-for-byte unchanged"
        );
        let last: serde_json::Value = serde_json::from_str(now.last().unwrap()).unwrap();
        let kind = last["type"].as_str().unwrap();
        assert!(allowed.contains(&kind), "unexpected event type {kind}");
        previous = now;
    }

    // And there is no event in the vocabulary that edits or removes anything.
    assert!(!allowed.iter().any(|k| k.contains("edit") || k.contains("delete")));
}
