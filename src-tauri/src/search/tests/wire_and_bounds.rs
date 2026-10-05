//! The wire vocabulary, the registration guards, and the bounds the sweep
//! runs under.
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// Wire vocabulary + registration guards
// -----------------------------------------------------------------------

#[test]
fn modes_and_scopes_decode_from_the_wire_strings_the_frontend_sends() {
    assert_eq!(
        serde_json::from_str::<SearchMode>("\"literal_insensitive\"").unwrap(),
        SearchMode::LiteralInsensitive
    );
    assert_eq!(
        serde_json::from_str::<SearchMode>("\"smart_case\"").unwrap(),
        SearchMode::SmartCase
    );
    assert_eq!(
        serde_json::from_str::<SearchMode>("\"regex\"").unwrap(),
        SearchMode::Regex
    );
    assert_eq!(
        serde_json::from_str::<SearchScope>("\"capped\"").unwrap(),
        SearchScope::Capped
    );
    assert_eq!(
        serde_json::from_str::<SearchScope>("\"full\"").unwrap(),
        SearchScope::Full
    );
    assert!(serde_json::from_str::<SearchMode>("\"fuzzy\"").is_err());
    assert!(serde_json::from_str::<SearchScope>("\"all\"").is_err());
}

#[test]
fn every_end_reason_serialises_to_the_documented_vocabulary() {
    let reasons = [
        (EndReason::Completed, "completed"),
        (EndReason::Capped, "capped"),
        (EndReason::Cancelled, "cancelled"),
        (EndReason::Superseded, "superseded"),
        (EndReason::Failed, "failed"),
    ];
    for (reason, wire) in reasons {
        assert_eq!(serde_json::to_value(reason).unwrap(), wire);
    }
    // And every one of them terminates the search's PRG operation
    // (SCC-FR-18: whatever the reason).
    for (reason, _) in reasons {
        assert!(!reason.operation_state().is_in_flight(), "{reason:?}");
    }
}

#[test]
fn search_command_functions_are_in_scope() {
    // Compile-time guard: renaming or removing a command without updating
    // `generate_handler!` / `COMMAND_NAMES` fails to compile here.
    let _ = start_search::<tauri::test::MockRuntime>;
    let _ = cancel_search;
}

#[test]
fn the_consumer_pool_is_bounded_and_does_not_grow_with_the_tree() {
    // SCC NFR: a fixed size chosen from available parallelism, bounded so a
    // search never starves the rest of the application.
    let size = consumer_pool_size();
    assert!((2..=8).contains(&size), "pool size out of bounds: {size}");
}

#[test]
fn the_work_queue_is_bounded_so_the_producer_cannot_buffer_the_whole_tree() {
    let (tx, rx) = sync_channel::<Candidate>(QUEUE_CAPACITY);
    for n in 0..QUEUE_CAPACITY {
        tx.try_send(Candidate {
            id: format!("{n}"),
            name: format!("{n}"),
            path: format!("{n}"),
            ordinal: n as u64,
            artifact_type: None,
        })
        .expect("within capacity");
    }
    assert!(
        tx.try_send(Candidate {
            id: "overflow".into(),
            name: "overflow".into(),
            path: "overflow".into(),
            ordinal: 999,
            artifact_type: None,
        })
        .is_err(),
        "the queue must apply back-pressure rather than growing without bound"
    );
    drop(rx);
}

#[test]
fn a_search_stopped_while_the_queue_is_full_still_terminates() {
    // The producer's back-pressure loop must observe the stop rather than
    // park inside a blocking send: once the collector stops draining, every
    // consumer exits, and a producer parked on a full queue would never see
    // the stop — hanging the join, and with it the thread that owes the one
    // terminal event of SCC-FR-13.
    //
    // The cap is what makes this deterministic. Over an enumeration far
    // larger than both the cap and the queue, the collector breaks at the
    // cap while the producer is still enumerating with a full queue behind
    // it. If the shutdown path regressed, this test would not fail — it
    // would HANG, which is itself the signal.
    //
    // The trailing `read < total` is a weaker claim than the hang, and it
    // races for the reason spelled out on `ts14` above: nothing throttles
    // the consumers against the collector, so a merely large tree can be
    // drained before the cap is observed. The enumeration is decoupled from
    // the tree there and here for the same reason.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let on_disk = CAPPED_HIT_LIMIT * 2;
    for n in 0..on_disk {
        write(root, &format!("f{n:04}.md"), "needle\n");
    }
    let files = scanning::candidate_files(&scanning::scan(root));
    let total = 100_000;
    let candidates = Arc::new(
        (0..total)
            .map(|n| Candidate {
                ordinal: n as u64,
                ..files[n % on_disk].clone()
            })
            .collect::<Vec<_>>(),
    );
    assert!(
        candidates.len() > QUEUE_CAPACITY + CAPPED_HIT_LIMIT,
        "the enumeration must outrun both the queue and the cap for the \
         producer to still be enumerating when the collector breaks"
    );

    let read = Arc::new(AtomicU64::new(0));
    let sink = Recorder::default();
    let reason = run_search(
        &sink,
        root,
        candidates,
        Arc::new(Matcher::compile("needle", SearchMode::LiteralInsensitive).unwrap()),
        SearchScope::Capped,
        &Stop::new(),
        "s",
        &{
            let read = Arc::clone(&read);
            move |consumed| read.store(consumed, Ordering::SeqCst)
        },
    );

    assert_eq!(reason, EndReason::Capped);
    assert_eq!(sink.end_reason(), EndReason::Capped);
    assert!(
        (read.load(Ordering::SeqCst) as usize) < total,
        "the producer ran the whole tree rather than stopping at the cap"
    );
}

#[test]
fn a_superseded_search_ends_with_that_reason_and_streams_nothing_after() {
    // SCC-FR-12, SCC-FR-13's other half: the registry decides *that* a search is
    // superseded (covered above), and the pipeline must carry that reason
    // through to its one terminal event rather than reporting `cancelled`.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    for n in 0..50 {
        write(root, &format!("f{n:02}.md"), "needle\n");
    }
    let candidates = Arc::new(scanning::candidate_files(&scanning::scan(root)));

    let stop = Stop::new();
    stop.supersede();
    let sink = Recorder::default();
    let reason = run_search(
        &sink,
        root,
        candidates,
        Arc::new(Matcher::compile("needle", SearchMode::LiteralInsensitive).unwrap()),
        SearchScope::Full,
        &stop,
        "superseded-search",
        &|_| {},
    );

    assert_eq!(reason, EndReason::Superseded);
    // `end_reason` also asserts nothing carried this id after the terminal
    // event, which is what lets a consumer discard its accumulator there.
    assert_eq!(sink.end_reason(), EndReason::Superseded);
    assert!(sink.hits().is_empty());

    // A cancel arriving afterwards must not relabel the outcome.
    stop.cancel();
    assert_eq!(stop.reason(), Some(EndReason::Superseded));
}

#[test]
fn every_end_reason_maps_to_the_right_terminal_operation_state() {
    // SCC-FR-18: the search's PRG operation terminates whatever the reason,
    // and *how* it terminates is what the status bar renders. A cancelled
    // search reported as `finished` would claim work completed that did not.
    assert_eq!(
        EndReason::Completed.operation_state(),
        OperationState::Finished
    );
    assert_eq!(EndReason::Capped.operation_state(), OperationState::Finished);
    assert_eq!(
        EndReason::Cancelled.operation_state(),
        OperationState::Cancelled
    );
    assert_eq!(
        EndReason::Superseded.operation_state(),
        OperationState::Cancelled
    );
    assert_eq!(EndReason::Failed.operation_state(), OperationState::Failed);
}

#[test]
fn a_symlink_is_not_a_candidate_at_all() {
    // The SCC non-functional requirement that no search reads outside the
    // active content root. `resolve_under` is syntactic, so a symlink INSIDE
    // the root can still point anywhere — and `metadata`/`read` follow it,
    // which would put another checkout's (or /etc's) bytes into a snippet
    // attributed to a path under this project.
    #[cfg(unix)]
    {
        let outside = tempfile::TempDir::new().unwrap();
        let secret = outside.path().join("secret.txt");
        std::fs::write(&secret, "needle: this lives outside the project\n").unwrap();

        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        write(root, "innocent.md", "nothing to see\n");
        std::os::unix::fs::symlink(&secret, root.join("link-to-secret.md")).unwrap();

        let (sink, _) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
        for hit in sink.hits() {
            assert_ne!(
                hit.match_kind,
                MatchKind::Content,
                "a symlink's target was read for content: {hit:?}"
            );
            assert!(
                hit.snippet.is_none(),
                "content from outside the content root reached a snippet: {hit:?}"
            );
        }

        // ASC-FR-23: a symlink yields no node, so it is not in the candidate
        // list this search draws from (ASC-FR-17) and is not findable even by
        // its own name. This is stronger than "never read through": an
        // oversized or non-UTF-8 candidate stays findable by path (SCC-FR-15)
        // because it is a real file that merely cannot be read, whereas a
        // symlink is not surfaced as a file at all.
        let (by_name, _) = search(
            root,
            "link-to-secret",
            SearchMode::LiteralInsensitive,
            SearchScope::Full,
        );
        assert!(
            by_name.hits().is_empty(),
            "a symlink must not be findable by name either: {:?}",
            by_name.hits()
        );
        // The real file beside it is unaffected.
        let (real, _) = search(root, "innocent", SearchMode::LiteralInsensitive, SearchScope::Full);
        assert_eq!(real.hits().len(), 1);
    }
}

#[test]
fn an_empty_file_whose_path_matches_still_produces_a_name_hit() {
    // `lines()` over an empty file yields nothing, so the content pass finds
    // no match and the path pass must still answer (SCC-FR-07).
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, "needle-empty.md", "");

    let (sink, _) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
    let hits = sink.hits();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].match_kind, MatchKind::Name);
}

#[test]
fn a_file_at_exactly_the_content_ceiling_is_still_read() {
    // SCC-FR-15 excludes a file whose size *exceeds* the ceiling; one
    // sitting exactly on it is ordinary content. An off-by-one here silently
    // stops scanning a whole size class.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let mut body = String::from("needle\n");
    body.push_str(&"x".repeat(MAX_CONTENT_BYTES as usize - body.len()));
    assert_eq!(body.len() as u64, MAX_CONTENT_BYTES);
    write(root, "exactly-at-the-ceiling.log", &body);

    let (sink, _) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
    let hits = sink.hits();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].match_kind, MatchKind::Content, "{hits:?}");
    assert_eq!(hits[0].line, Some(1));
}

#[test]
fn crlf_content_reports_line_numbers_and_snippets_without_the_carriage_return() {
    // A Windows-authored file is ordinary project content (PST-FR-23 keeps
    // the convention per project). A snippet carrying a stray `\r` renders
    // as a control character in the overlay.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, "crlf.md", "first\r\nsecond needle here\r\nthird\r\n");

    let (sink, _) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
    let hits = sink.hits();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, Some(2));
    assert_eq!(hits[0].snippet.as_deref(), Some("second needle here"));
}
