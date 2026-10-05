//! The log on disk and the fold over it (CMS-FR-01 ... CMS-FR-11).

use super::*;

// -- CMS-FR-01, CMS-FR-14, CMS-FR-11 ----------------------------------------------------------

#[test]
fn opening_a_thread_writes_the_pair_and_returns_it_folded() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());

    let thread = open(
        root,
        "specs/a.md",
        anchor(40, 61, "the first session"),
        "needs tightening",
        "2026-01-01T00:00:00Z",
    );

    let lines = log_lines(root, "specs/a.md");
    assert_eq!(lines.len(), 2, "one append, two events (CMS-FR-14)");
    assert!(lines[0].contains("discussion_opened"));
    assert!(lines[1].contains("comment_added"));

    assert_eq!(thread.comments.len(), 1);
    assert_eq!(thread.comments[0].body, "needs tightening");
    assert!(matches!(
        thread.comments[0].author,
        Participant::Human { .. }
    ));
    assert_eq!(thread.created_at, thread.updated_at);
    assert!(!thread.locked);
    assert!(!thread.resolved);
    assert_eq!(thread.artifact_id(), Some("specs/a.md"));
    assert!(thread.artifact_id().is_some());
    assert!(thread.is_fragment_targeted());
    assert_eq!(
        thread.fragment_target,
        Some(FragmentTarget::in_artifact("specs/a.md", 40, 61, "the first session")),
    );
}

// -- CMS-FR-02 ----------------------------------------------------------

#[test]
fn the_same_artifact_path_resolves_to_the_same_log_filename_everywhere() {
    // CMS-FR-02: two clones must write into one file, or a union merge has
    // nothing to merge.
    let a = temp_root();
    let b = temp_root();
    open(
        &fsa::RootFs::for_root(a.path()),
        "specs/a.md",
        anchor(0, 5, "hello"),
        "one",
        "2026-01-01T00:00:00Z",
    );
    open(
        &fsa::RootFs::for_root(b.path()),
        "specs/a.md",
        anchor(0, 5, "hello"),
        "two",
        "2026-01-01T00:00:00Z",
    );

    let name = |root: &Path| {
        std::fs::read_dir(comments_dir(root))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".jsonl"))
            .collect::<Vec<_>>()
    };
    assert_eq!(name(a.path()), name(b.path()));

    // And a different artifact gets a different log.
    assert_ne!(log_id("specs/a.md"), log_id("specs/b.md"));
    // Deterministic across calls, not seeded by a clock.
    assert_eq!(log_id("specs/a.md"), log_id("specs/a.md"));
}

// -- CMS-FR-XQBM, CMS-FR-VJRP ------------------------------------------------

/// CMS-FR-XQBM, CMS-FR-VJRP, CMS-FR-01: a log is written into the store and
/// nowhere else, and nothing this module writes is a Git artifact.
#[test]
fn an_append_writes_into_the_store_and_leaves_no_git_artifact() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "hi",
        "2026-01-01T00:00:00Z",
    );

    // CMS-FR-01: `comments/<log-id>.jsonl` inside the store, with no
    // `.synthesis/` anywhere in the path.
    let log = dir
        .path()
        .join(COMMENTS_REL)
        .join(format!("{}.jsonl", log_id("specs/a.md")));
    assert!(log.is_file(), "the log is written into the store");
    assert!(!dir.path().join(".synthesis").exists(), "no project folder");

    // CMS-FR-VJRP: no `.gitattributes` and no other Git artifact is written,
    // the store standing outside every worktree.
    for entry in std::fs::read_dir(dir.path().join(COMMENTS_REL)).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        assert!(!name.starts_with(".git"), "{name} is a Git artifact");
    }
}

// -- CMS-FR-06, CMS-FR-09 ----------------------------------------------------------

#[test]
fn a_log_whose_every_line_was_duplicated_folds_to_the_same_state() {
    // CMS-FR-06: this is what an import pass over an already-imported log and a
    // recovered concurrent append both produce, so the fold has to be
    // idempotent over it or every conversation doubles (RMS-FR-SUAK).
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let thread = open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "first",
        "2026-01-01T00:00:00Z",
    );
    add_comment_in(
        root,
        "specs/a.md",
        &thread.id,
        "second".into(),
        vec![],
        Vec::new(),
        &human("octocat"),
        "2026-01-02T00:00:00Z")
    .unwrap();

    let before = list_fragment_discussions_in(root, "specs/a.md");

    let path = log_path(root, "specs/a.md").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("{text}{text}")).unwrap();

    let after = list_fragment_discussions_in(root, "specs/a.md");
    assert_eq!(after, before, "duplicated lines fold identically");
    assert_eq!(after[0].comments.len(), 2);
}

// -- CMS-FR-07, CMS-FR-08 ----------------------------------------------------------

#[test]
fn damaged_and_future_lines_are_skipped_and_left_on_disk() {
    // CMS-FR-07 / CMS-FR-08.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let thread = open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "first",
        "2026-01-01T00:00:00Z",
    );
    add_comment_in(
        root,
        "specs/a.md",
        &thread.id,
        "second".into(),
        vec![],
        Vec::new(),
        &human("raver119"),
        "2026-01-02T00:00:00Z")
    .unwrap();

    let path = log_path(root, "specs/a.md").unwrap();
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("{\"v\":2,\"eventId\":\"x\",\"threadId\":\"t\",\"at\":\"z\",\"by\":{\"kind\":\"human\",\"login\":\"a\"},\"type\":\"thread_locked\"}\n");
    text.push_str("this is not json at all\n");
    text.push_str("{\"v\":1,\"eventId\":\"y\",\"threadId\":\"t\",\"at\":\"z\",\"by\":{\"kind\":\"human\",\"login\":\"a\"},\"type\":\"galaxy_brained\"}\n");
    // Missing the `anchor` a thread_opened requires.
    text.push_str("{\"v\":1,\"eventId\":\"z\",\"threadId\":\"t2\",\"at\":\"z\",\"by\":{\"kind\":\"human\",\"login\":\"a\"},\"type\":\"thread_opened\"}\n");
    std::fs::write(&path, &text).unwrap();

    let threads = list_fragment_discussions_in(root, "specs/a.md");
    assert_eq!(threads.len(), 1, "the well-formed thread is still served");
    assert_eq!(threads[0].comments.len(), 2);
    assert!(!threads[0].locked, "the v=2 lock line was skipped");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        text,
        "every line is left on disk untouched"
    );
}

// -- CMS-FR-09, CMS-FR-10 ----------------------------------------------------------

#[test]
fn the_fold_replays_by_instant_rather_than_by_physical_line_order() {
    // CMS-FR-09: a union merge interleaves two branches' lines by position,
    // so physical order carries no meaning across a merge.
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
    let commented = Event {
        v: 1,
        event_id: "e2".into(),
        thread_id: "t1".into(),
        at: "2026-01-02T00:00:00Z".into(),
        by: human("raver119"),
        body: EventBody::CommentAdded {
            comment_id: "c1".into(),
            body: "hi".into(),
            quotes: vec![],
            attachments: Vec::new(),
        },
    };
    let resolved = Event {
        v: 1,
        event_id: "e3".into(),
        thread_id: "t1".into(),
        at: "2026-01-03T00:00:00Z".into(),
        by: human("raver119"),
        body: EventBody::ThreadResolved,
    };

    // Written out of order on purpose.
    let path = log_path(root, "specs/a.md").unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        [&commented, &opened, &resolved]
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .unwrap();

    let threads = list_fragment_discussions_in(root, "specs/a.md");
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].comments.len(), 1, "the comment was still folded");
    assert!(threads[0].resolved);
    assert_eq!(threads[0].created_at, "2026-01-01T00:00:00Z");
    assert_eq!(threads[0].updated_at, "2026-01-03T00:00:00Z");
}

#[test]
fn a_comment_that_ties_or_precedes_its_own_opening_is_still_folded() {
    // CMS-FR-09. `open_artifact_fragment_in` stamps the opening pair with one `at` and
    // `now_rfc3339` is second-granularity, so the pair is ALWAYS a tie — and a
    // union merge can bring a comment from another clone whose `event_id`
    // sorts below the local `thread_opened`'s, or whose clock ran behind.
    // A one-pass fold would drop the comment, and CMS-FR-15 would then drop
    // the whole thread: the conversation would vanish with its data intact on
    // disk.
    let opened = Event {
        v: 1,
        event_id: "zzzz-late-id".into(),
        thread_id: "t1".into(),
        at: "2026-01-01T00:00:00Z".into(),
        by: human("raver119"),
        body: EventBody::ThreadOpened {
            artifact_path: "specs/a.md".into(),
            anchor: legacy_anchor(0, 3, "abc"),
        },
    };
    // Same instant, lexicographically smaller id — sorts *before* the opening.
    let tied = Event {
        v: 1,
        event_id: "aaaa-early-id".into(),
        thread_id: "t1".into(),
        at: "2026-01-01T00:00:00Z".into(),
        by: human("octocat"),
        body: EventBody::CommentAdded {
            comment_id: "c1".into(),
            body: "tied".into(),
            quotes: vec![],
            attachments: Vec::new(),
        },
    };
    // And one whose clock ran a whole second behind the opening.
    let earlier = Event {
        v: 1,
        event_id: "bbbb".into(),
        thread_id: "t1".into(),
        at: "2025-12-31T23:59:59Z".into(),
        by: human("octocat"),
        body: EventBody::CommentAdded {
            comment_id: "c0".into(),
            body: "from a skewed clock".into(),
            quotes: vec![],
            attachments: Vec::new(),
        },
    };

    let folded = fold_events("specs/a.md", vec![opened, tied, earlier]);
    assert_eq!(folded.len(), 1, "the thread survives: {folded:?}");
    let bodies: Vec<&str> = folded[0].comments.iter().map(|c| c.body.as_str()).collect();
    assert_eq!(
        bodies,
        vec!["from a skewed clock", "tied"],
        "both comments are folded, in instant order"
    );
}

#[test]
fn the_fold_is_independent_of_the_order_its_events_arrive_in() {
    // CMS-FR-09 promises the fold is total and deterministic "whatever order
    // its lines physically appear in". A union merge interleaves by position,
    // so this is the property, not a nicety.
    let events = vec![
        Event {
            v: 1,
            event_id: "e1".into(),
            thread_id: "t1".into(),
            at: "2026-01-01T00:00:00Z".into(),
            by: human("raver119"),
            body: EventBody::ThreadOpened {
                artifact_path: "specs/a.md".into(),
                anchor: legacy_anchor(0, 3, "abc"),
            },
        },
        Event {
            v: 1,
            event_id: "e2".into(),
            thread_id: "t1".into(),
            at: "2026-01-02T00:00:00Z".into(),
            by: human("raver119"),
            body: EventBody::CommentAdded {
                comment_id: "c1".into(),
                body: "one".into(),
                quotes: vec![],
                attachments: Vec::new(),
            },
        },
        Event {
            v: 1,
            event_id: "e3".into(),
            thread_id: "t1".into(),
            at: "2026-01-03T00:00:00Z".into(),
            by: human("raver119"),
            body: EventBody::ThreadLocked,
        },
    ];
    let expected = fold_events("specs/a.md", events.clone());
    // Every permutation of three events.
    for order in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
        let shuffled: Vec<Event> = order.iter().map(|i| events[*i].clone()).collect();
        assert_eq!(
            fold_events("specs/a.md", shuffled),
            expected,
            "order {order:?} folded differently"
        );
    }
}

#[test]
fn an_event_for_a_thread_that_was_never_opened_is_dropped() {
    let orphan = Event {
        v: 1,
        event_id: "e1".into(),
        thread_id: "no-such-thread".into(),
        at: "2026-01-01T00:00:00Z".into(),
        by: human("raver119"),
        body: EventBody::CommentAdded {
            comment_id: "c1".into(),
            body: "into the void".into(),
            quotes: vec![],
            attachments: Vec::new(),
        },
    };
    assert!(fold_events("specs/a.md", vec![orphan]).is_empty());
}

#[test]
fn a_second_opening_for_one_thread_id_cannot_rewrite_its_anchor() {
    let first = Event {
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
    let mut second = first.clone();
    second.event_id = "e2".into();
    second.at = "2026-01-05T00:00:00Z".into();
    second.body = EventBody::ThreadOpened {
        artifact_path: "specs/a.md".into(),
        anchor: legacy_anchor(900, 903, "xyz"),
    };
    let comment = Event {
        v: 1,
        event_id: "e3".into(),
        thread_id: "t1".into(),
        at: "2026-01-02T00:00:00Z".into(),
        by: human("raver119"),
        body: EventBody::CommentAdded {
            comment_id: "c1".into(),
            body: "hi".into(),
            quotes: vec![],
            attachments: Vec::new(),
        },
    };
    let folded = fold_events("specs/a.md", vec![first, second, comment]);
    assert_eq!(folded.len(), 1);
    assert_eq!(folded[0].fragment_target, Some(FragmentTarget::in_artifact("specs/a.md", 0, 3, "abc")), "the first opening wins");
}

#[test]
fn two_different_events_sharing_an_event_id_fold_as_one() {
    // CMS-FR-06 dedupes by `event_id`. That runs before the sort, so it must
    // not depend on which of the two arrived first.
    let base = Event {
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
    let c1 = Event {
        v: 1,
        event_id: "dup".into(),
        thread_id: "t1".into(),
        at: "2026-01-02T00:00:00Z".into(),
        by: human("raver119"),
        body: EventBody::CommentAdded {
            comment_id: "c1".into(),
            body: "first".into(),
            quotes: vec![],
            attachments: Vec::new(),
        },
    };
    let mut c2 = c1.clone();
    c2.body = EventBody::CommentAdded {
        comment_id: "c2".into(),
        body: "second".into(),
        quotes: vec![],
        attachments: Vec::new(),
    };
    let folded = fold_events("specs/a.md", vec![base, c1, c2]);
    assert_eq!(folded[0].comments.len(), 1, "one id, one event");
}

// -- CMS-FR-27, RMS-FR-DGWY, RMS-FR-PFOB ------------------------------------

/// CMS-FR-27, RMS-FR-DGWY: an id crafted to name a folder outside the store's
/// own is refused before a path is composed, and nothing is written.
///
/// The FSA escape gate alone is not enough for these two shapes: a `..` segment
/// composes a path that normalises to somewhere still *inside* the store, so the
/// gate admits it — and one of the operations behind them is a recursive delete.
#[test]
fn an_id_that_would_escape_its_own_folder_is_refused_having_written_nothing() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let before = std::fs::read_dir(dir.path()).map(|e| e.count()).unwrap_or(0);

    for crafted in ["..", "../..", "a/b", ""] {
        assert!(
            draft_comments_dir(root, crafted).is_err(),
            "{crafted} named a draft's folder",
        );
        assert!(
            note_comments_dir(root, crafted).is_err(),
            "{crafted} named a note's folder",
        );
    }
    assert_eq!(
        std::fs::read_dir(dir.path()).map(|e| e.count()).unwrap_or(0),
        before,
        "nothing was created by a refusal",
    );
}

/// RMS-FR-PFOB, CMS-FR-06: two processes may append to one log at once. Each
/// writes whole lines, so a reader never reads a part-written record, and the
/// fold applies the first event it sees for an identity and ignores every later
/// one — so a line both wrote counts once.
#[test]
fn concurrent_appends_all_land_and_a_duplicated_line_folds_once() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let thread = open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "first",
        "2026-01-01T00:00:00Z",
    );

    // Ten appends from ten threads into one log, as two application processes
    // holding one store would make them.
    let thread_id = thread.id.as_str();
    std::thread::scope(|scope| {
        for n in 0..10 {
            scope.spawn(move || {
                add_comment_in(
                    root,
                    "specs/a.md",
                    thread_id,
                    format!("reply {n}"),
                    Vec::new(),
                    Vec::new(),
                    &human("raver119"),
                    "2026-01-01T00:00:01Z",
                )
                .expect("append");
            });
        }
    });

    let folded = list_fragment_discussions_in(root, "specs/a.md");
    assert_eq!(folded.len(), 1);
    assert_eq!(folded[0].comments.len(), 11, "every append landed whole");

    // The line an import or a repeated append writes twice folds once
    // (CMS-FR-06), which is what makes a shared store safe to write into.
    let path = log_path(root.path(), "specs/a.md").expect("path");
    let text = std::fs::read_to_string(&path).expect("log");
    let last = text.lines().last().expect("a line").to_string();
    root.append_lines(&path, &[last]).expect("the duplicate");
    let after = list_fragment_discussions_in(root, "specs/a.md");
    assert_eq!(after, folded, "the duplicate changed nothing");
}

/// CMS-FR-VJRP, PST-FR-DQZT: no write of this module arms the debounced commit.
///
/// Asserted against the source rather than against a repository, because what
/// has to stay true is that the seam is not reached at all: the store stands
/// outside every worktree, so a commit of a conversation would be a commit of a
/// path no repository holds. A test over a repository would pass by finding
/// nothing to commit, which is the same answer a re-armed committer gives on a
/// project whose store happens to sit elsewhere.
#[test]
fn no_write_of_this_module_arms_the_debounced_commit() {
    for source in [
        include_str!("../../comments.rs"),
        include_str!("../opening.rs"),
        include_str!("../mutating.rs"),
        include_str!("../attachment_store.rs"),
        include_str!("../rename.rs"),
        include_str!("../listing.rs"),
        include_str!("../locate.rs"),
        include_str!("../paths.rs"),
        include_str!("../scope.rs"),
        include_str!("../fold.rs"),
        include_str!("../constants.rs"),
    ] {
        assert!(
            !source.contains("storage_floor::commit"),
            "a conversation reaches the committer",
        );
    }
}
