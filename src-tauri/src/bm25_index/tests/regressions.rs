//! Regressions — one per defect the review found.

use super::*;

// ---------------------------------------------------------------------------
// Regressions — one per defect the review found
// ---------------------------------------------------------------------------

#[test]
fn a_crlf_heading_less_file_still_splits_at_blank_lines() {
    // BMI-FR-06. `bound_to_blocks` finds a blank line by looking for "\n\n",
    // and a CRLF file's blank line is "\r\n\r\n" — so without normalisation the
    // whole file is one paragraph and falls into the hard split, cutting
    // mid-word. The bug was asymmetric: `split_at_headings` rebuilds from
    // `str::lines`, which drops the `\r`, so files WITH headings were fine and
    // exactly the class BMI-FR-06 names — a heading-less note, a `.flow`'s
    // JSON, any non-Markdown text — was not.
    let paragraph = "lorem ipsum dolor sit amet ".repeat(40);
    let paragraph = paragraph.trim();
    let lf = std::iter::repeat(paragraph)
        .take(12)
        .collect::<Vec<_>>()
        .join("\n\n");
    let crlf = lf.replace('\n', "\r\n");
    assert!(lf.chars().count() > MAX_CHUNK_CHARS * 2);

    let chunks = chunk_text(&crlf);
    assert_eq!(
        chunks,
        chunk_text(&lf),
        "a file's chunks must not depend on its line-ending convention"
    );
    for chunk in &chunks {
        assert!(chunk.chars().count() <= MAX_CHUNK_CHARS);
        assert!(
            !chunk.ends_with("dolor sit ") && !chunk.starts_with("amet "),
            "a boundary falls at a blank line, not mid-word: {:?}",
            &chunk[chunk.len().saturating_sub(40)..]
        );
    }
    // The strongest form of the claim: nothing was lost or duplicated, and
    // every boundary was a blank line.
    assert_eq!(
        chunks.join("\n\n"),
        lf,
        "the chunks of a blank-line split reassemble into the file"
    );
}

#[test]
#[cfg(unix)]
fn a_symlinked_artifact_is_refused_rather_than_indexed_through() {
    // BMI-FR-26. `resolve_under` is syntactic — it rejects a path climbing out
    // with `..` but cannot see a link — so a symlink sitting inside the root
    // points anywhere on the machine while reading as a path under the project.
    // `search` returns a chunk's text verbatim, so indexing through one is a
    // direct disclosure of whatever it targets. This is the refusal
    // `SCC-search.md`'s content read already makes on the same candidate list.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let outside = dir.path().join("outside-secret.txt");
    std::fs::write(&outside, "SUPERSECRET material\n").unwrap();
    std::fs::create_dir_all(root.join("specifications")).unwrap();
    let link = root.join("specifications/leak.md");
    std::os::unix::fs::symlink(&outside, &link).unwrap();

    let reason = read_bounded(&crate::fs::RootFs::for_root(&root), &link).unwrap_err();
    assert!(
        reason.contains("symlink"),
        "a symlinked candidate is refused: {reason}"
    );
    assert!(
        !reason.contains("SUPERSECRET"),
        "and the refusal names the shape, not the target's content: {reason}"
    );

    // A directory in a candidate's place is likewise not read as text.
    std::fs::create_dir_all(root.join("specifications/adir")).unwrap();
    assert!(read_bounded(&crate::fs::RootFs::for_root(&root), &root.join("specifications/adir")).is_err());

    // An ordinary file beside them still reads.
    std::fs::write(root.join("specifications/real.md"), "teardown\n").unwrap();
    assert_eq!(
        read_bounded(&crate::fs::RootFs::for_root(&root), &root.join("specifications/real.md")).unwrap(),
        "teardown\n"
    );
}

#[test]
fn a_file_that_yields_no_chunk_is_recorded_so_it_is_not_re_read_every_pass() {
    // Without the record its checksum can never match, so an empty or
    // whitespace-only file is re-read, re-hashed and re-chunked on every pass
    // for the life of the session.
    let sources = || {
        vec![
            artifact(IndexId::Spec, "empty.md", ""),
            artifact(IndexId::Spec, "blank.md", "   \n\n  \n"),
            artifact(IndexId::Spec, "real.md", "teardown\n"),
        ]
    };
    let (first, first_stats) = pass_with_stats(&IndexSet::default(), sources(), PassScope::ALL);
    assert_eq!(first_stats.files_indexed, 1, "only `real.md` yielded a chunk");
    assert_eq!(first.chunk_count(IndexId::Spec), 1);

    let (_, second_stats) = pass_with_stats(&first, sources(), PassScope::ALL);
    assert_eq!(
        second_stats.files_unchanged, 3,
        "every file short-circuits on its checksum, the empty ones included"
    );
    assert_eq!(second_stats.files_indexed, 0);

    // And a file emptied by an edit loses the chunks it had.
    let emptied = pass(
        &first,
        vec![
            artifact(IndexId::Spec, "empty.md", ""),
            artifact(IndexId::Spec, "blank.md", "   \n\n  \n"),
            artifact(IndexId::Spec, "real.md", "   \n"),
        ],
        PassScope::ALL,
    );
    assert_eq!(emptied.chunk_count(IndexId::Spec), 0);
    assert!(search_snapshot(&emptied, &[], "teardown", 10).is_empty());
}

#[test]
fn a_teardown_landing_while_a_pass_finishes_is_not_overwritten_by_it() {
    // BMI-FR-25. The generation check and the swap have to be one critical
    // section: if they are two, a `clear()` landing between them is overwritten
    // by the very snapshot it was meant to discard, and — because a close
    // schedules no follow-up pass — the closed project's chunks answer queries
    // for the rest of the session.
    let indexer = Bm25Indexer::default();
    indexer.mount(&scratch_root("a"));
    let generation = indexer.generation();
    let stale = pass(
        &IndexSet::default(),
        vec![artifact(IndexId::Spec, "a.md", "teardown")],
        PassScope::ALL,
    );

    // The project closes while the pass is between its reconcile and its swap.
    indexer.clear();

    assert!(
        !indexer.publish_if_current(stale, generation),
        "a pass belonging to a torn-down generation must not publish"
    );
    assert!(
        search(&indexer, &[], "teardown", 10).is_empty(),
        "the close stands: a query after it returns an empty list"
    );

    // A pass of the current generation still publishes normally.
    indexer.mount(&scratch_root("b"));
    let current = indexer.generation();
    let fresh = pass(
        &IndexSet::default(),
        vec![artifact(IndexId::Spec, "b.md", "teardown")],
        PassScope::ALL,
    );
    assert!(indexer.publish_if_current(fresh, current));
    assert!(!search(&indexer, &[], "teardown", 10).is_empty());
}

#[test]
fn the_indexer_holds_no_mechanism_for_deferring_a_pass_across_two_indexes() {
    // BMI-FR-20. A graduation used to be the one operation that touched both
    // halves at once, and it needed a hold so no pass observed a file in the
    // project and in the draft it came from together. It no longer touches both:
    // the draft stays where it is and the published files arrive on the
    // watcher's ordinary report. The hold is therefore gone, and its absence is
    // what says a graduation costs this module nothing of its own.
    let source = [
        include_str!("../../bm25_index.rs"),
        include_str!("../chunking.rs"),
        include_str!("../reconcile.rs"),
        include_str!("../retrieval.rs"),
        include_str!("../scheduling.rs"),
        include_str!("../sources.rs"),
    ]
    .join("\n");
    for spelling in ["suspend", "SuspendGuard", "wait_out_suspension"] {
        assert!(
            !source.contains(spelling),
            "{spelling:?} is still here: nothing in this module has two halves to \
             hold together any more",
        );
    }
}

#[test]
fn one_index_holding_two_languages_answers_each_from_its_own_shard() {
    // BMI-FR-08 forces this: one BM25 engine holds one tokenizer, so a German
    // document in an English-stemmed engine is unfindable by a German query.
    // Each index therefore holds one shard per language present in it.
    let set = pass(
        &IndexSet::default(),
        vec![
            artifact(IndexId::Spec, "de.md", GERMAN),
            artifact(
                IndexId::Spec,
                "en.md",
                "# Directories\n\nThis document describes how the program \
                 searches the directories of the project and classifies the \
                 files it finds, running the teardown of the scan afterwards.",
            ),
        ],
        PassScope::ALL,
    );
    assert_eq!(set.language_of(IndexId::Spec, "de.md"), Some(Language::German));
    assert_eq!(
        set.language_of(IndexId::Spec, "en.md"),
        Some(Language::English)
    );

    // Each query reaches only the shard whose stemming can answer it, and both
    // come back from the one index.
    assert_eq!(
        hit_paths(&search_snapshot(&set, &[IndexId::Spec], "Verzeichnis", 10)),
        vec![(IndexId::Spec, "de.md".to_string())]
    );
    assert_eq!(
        hit_paths(&search_snapshot(&set, &[IndexId::Spec], "teardown", 10)),
        vec![(IndexId::Spec, "en.md".to_string())]
    );
}

#[test]
fn a_file_whose_language_changes_leaves_no_chunk_in_its_old_shard() {
    // The removal is located by the language stored on the file's entry, not by
    // the language of the text now on disk — get that wrong and the old shard
    // keeps every chunk forever.
    let german = pass(
        &IndexSet::default(),
        vec![artifact(IndexId::Spec, "a.md", GERMAN)],
        PassScope::ALL,
    );
    assert!(!search_snapshot(&german, &[], "Verzeichnis", 10).is_empty());

    let rewritten = pass(
        &german,
        vec![artifact(
            IndexId::Spec,
            "a.md",
            "# Directories\n\nThis document now describes the teardown of the \
             scan in English, and nothing about it is German any more.",
        )],
        PassScope::ARTIFACTS,
    );
    assert_eq!(
        rewritten.language_of(IndexId::Spec, "a.md"),
        Some(Language::English)
    );
    assert!(
        search_snapshot(&rewritten, &[], "Verzeichnis", 10).is_empty(),
        "no chunk may survive in the shard the file has left"
    );
    assert!(!search_snapshot(&rewritten, &[], "teardown", 10).is_empty());
    assert_eq!(rewritten.file_count(IndexId::Spec), 1);
}

