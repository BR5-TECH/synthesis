//! The query contract, reconciliation triggers, graduation, determinism, and progress.

use super::*;

// ---------------------------------------------------------------------------
// BMI-FR-10, BMI-FR-12 / BMI-FR-11 — the query contract
// ---------------------------------------------------------------------------

#[test]
fn search_answers_with_a_list_in_every_circumstance() {
    // BMI-FR-10: never an error, whatever it is asked.
    let empty = IndexSet::default();
    assert!(search_snapshot(&empty, &[], "anything", 10).is_empty());

    let set = pass(
        &IndexSet::default(),
        vec![artifact(IndexId::Spec, "a.md", "teardown")],
        PassScope::ALL,
    );
    assert!(search_snapshot(&set, &[], "", 10).is_empty(), "empty query");
    assert!(search_snapshot(&set, &[], "   ", 10).is_empty(), "blank query");
    assert!(search_snapshot(&set, &[], "teardown", 0).is_empty(), "zero limit");
    assert!(
        search_snapshot(&set, &[IndexId::Agent], "teardown", 10).is_empty(),
        "an index that holds nothing"
    );
    assert!(
        search_snapshot(&set, &[], "nothingmatchesthisword", 10).is_empty(),
        "a query nothing matches"
    );
}

#[test]
fn results_are_ranked_descending_and_bounded_by_the_limit() {
    // BMI-FR-10 / BMI-FR-11.
    let sources: Vec<SourceFile> = (0..8)
        .map(|i| {
            // Repetition raises term frequency, so the documents genuinely
            // differ in score rather than tying.
            let body = format!("{} filler words here", "teardown ".repeat(i + 1));
            artifact(IndexId::Spec, &format!("s{i}.md"), &body)
        })
        .collect();
    let set = pass(&IndexSet::default(), sources, PassScope::ALL);

    let hits = search_snapshot(&set, &[IndexId::Spec], "teardown", 5);
    assert_eq!(hits.len(), 5, "the limit bounds the result set");
    assert_eq!(
        hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(),
        vec!["s7.md", "s6.md", "s5.md", "s4.md", "s3.md"],
        "the limit takes the HIGHEST-scoring five — a result set of the five \
         lowest would also be in descending order"
    );
    for pair in hits.windows(2) {
        assert!(
            pair[0].score >= pair[1].score,
            "descending score: {} then {}",
            pair[0].score,
            pair[1].score
        );
    }
    assert!(hits.iter().all(|h| h.score > 0.0));
}

#[test]
fn a_merged_result_set_carries_the_index_each_score_came_from() {
    // BMI-FR-11: a score is computed against its own index's statistics, so a
    // consumer can always tell which corpus produced it.
    let set = pass(
        &IndexSet::default(),
        vec![
            artifact(IndexId::Skill, "s.md", "teardown of the worktree"),
            artifact(IndexId::Spec, "p.md", "teardown of the worktree"),
            draft("d1", "n.md", "teardown of the worktree"),
        ],
        PassScope::ALL,
    );
    let hits = search_snapshot(&set, &[], "teardown", 10);
    let mut indexes: Vec<IndexId> = hits.iter().map(|h| h.index).collect();
    indexes.sort();
    indexes.dedup();
    assert_eq!(
        indexes,
        vec![IndexId::Skill, IndexId::Spec, IndexId::Drafts]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    );
}

#[test]
fn equal_scores_order_identically_across_repeated_queries() {
    // The engine's own ordering passes through a `HashSet`, so without the
    // total tie-break in `search_snapshot` two runs of one query could return
    // equally-scored hits in different orders.
    let sources: Vec<SourceFile> = (0..12)
        .map(|i| artifact(IndexId::Spec, &format!("s{i:02}.md"), "teardown worktree"))
        .collect();
    let set = pass(&IndexSet::default(), sources, PassScope::ALL);

    let first = search_snapshot(&set, &[IndexId::Spec], "teardown", 12);
    for _ in 0..8 {
        let again = search_snapshot(&set, &[IndexId::Spec], "teardown", 12);
        assert_eq!(
            first.iter().map(|h| &h.path).collect::<Vec<_>>(),
            again.iter().map(|h| &h.path).collect::<Vec<_>>(),
            "the same query must order equal scores identically every time"
        );
    }
}

// ---------------------------------------------------------------------------
// BMI-FR-15 / BMI-FR-16 / BMI-FR-17 — the reconciliation triggers
// ---------------------------------------------------------------------------

#[test]
fn a_created_file_enters_its_index_and_a_deleted_one_leaves_it() {
    // BMI-FR-15.
    let start = pass(
        &IndexSet::default(),
        vec![artifact(IndexId::Spec, "a.md", "alpha")],
        PassScope::ALL,
    );
    let created = pass(
        &start,
        vec![
            artifact(IndexId::Spec, "a.md", "alpha"),
            artifact(IndexId::Spec, "new.md", "bravo"),
        ],
        PassScope::ARTIFACTS,
    );
    assert_eq!(
        created.paths(IndexId::Spec),
        vec!["a.md".to_string(), "new.md".to_string()]
    );
    assert!(!search_snapshot(&created, &[], "bravo", 10).is_empty());

    let deleted = pass(
        &created,
        vec![artifact(IndexId::Spec, "a.md", "alpha")],
        PassScope::ARTIFACTS,
    );
    assert_eq!(deleted.paths(IndexId::Spec), vec!["a.md".to_string()]);
    assert!(
        search_snapshot(&deleted, &[], "bravo", 10).is_empty(),
        "a deleted file's chunks go with it"
    );
}

#[test]
fn a_content_edit_replaces_a_files_chunks_whole() {
    // BMI-FR-16: a chunk the edit removed leaves the index with it, rather than
    // surviving as a stale document beside the new ones.
    let before = pass(
        &IndexSet::default(),
        vec![artifact(
            IndexId::Spec,
            "a.md",
            "# One\n\nteardown\n\n# Two\n\nremovable\n",
        )],
        PassScope::ALL,
    );
    assert!(!search_snapshot(&before, &[], "removable", 10).is_empty());

    let after = pass(
        &before,
        vec![artifact(
            IndexId::Spec,
            "a.md",
            "# One\n\nteardown\n\n# Two\n\nreplacement\n",
        )],
        PassScope::ARTIFACTS,
    );
    assert!(
        search_snapshot(&after, &[], "removable", 10).is_empty(),
        "the removed section must not survive the edit"
    );
    assert!(!search_snapshot(&after, &[], "replacement", 10).is_empty());
    assert_eq!(after.chunk_count(IndexId::Spec), 2);
}

#[test]
fn an_unchanged_file_is_not_reindexed() {
    // The checksum comparison is what keeps a pass proportional to what
    // actually moved rather than to the size of the project.
    let first = pass(
        &IndexSet::default(),
        vec![artifact(IndexId::Spec, "a.md", "teardown")],
        PassScope::ALL,
    );
    let (_, stats) = pass_with_stats(
        &first,
        vec![artifact(IndexId::Spec, "a.md", "teardown")],
        PassScope::ARTIFACTS,
    );
    assert_eq!(stats.files_unchanged, 1);
    assert_eq!(stats.files_indexed, 0);
    assert_eq!(stats.files_removed, 0);
}

#[test]
fn a_reclassified_file_moves_between_indexes_and_an_untyped_one_leaves_every_index() {
    // BMI-FR-17.
    let scenario = pass(
        &IndexSet::default(),
        vec![artifact(IndexId::Scenario, "notes/x.md", "teardown")],
        PassScope::ALL,
    );
    assert_eq!(scenario.file_count(IndexId::Scenario), 1);

    let prompt = pass(
        &scenario,
        vec![artifact(IndexId::Prompt, "notes/x.md", "teardown")],
        PassScope::ARTIFACTS,
    );
    assert_eq!(prompt.file_count(IndexId::Prompt), 1);
    assert_eq!(
        prompt.file_count(IndexId::Scenario),
        0,
        "the file must leave the index its old type put it in"
    );
    assert_eq!(
        hit_paths(&search_snapshot(&prompt, &[], "teardown", 10)),
        vec![(IndexId::Prompt, "notes/x.md".to_string())]
    );

    // Cleared: it is a source no longer, so it leaves every index.
    let cleared = pass(&prompt, vec![], PassScope::ARTIFACTS);
    for index in IndexId::ALL {
        assert_eq!(cleared.file_count(index), 0);
    }
}

// ---------------------------------------------------------------------------
// BMI-FR-19, BMI-FR-20 — graduation lands in one pass
// ---------------------------------------------------------------------------

#[test]
fn graduation_removes_the_draft_and_indexes_the_published_files_in_one_pass() {
    // BMI-FR-20. The scope carried by the graduation notification covers both
    // halves, so the removal and the additions are the same reconciliation —
    // there is no intermediate snapshot in which the material is in both, and
    // none in which it is in neither.
    let before = pass(
        &IndexSet::default(),
        vec![
            draft("d1", "ui/a.md", "teardown alpha"),
            draft("d1", "ui/b.md", "teardown bravo"),
        ],
        PassScope::ALL,
    );
    assert_eq!(before.file_count(IndexId::Drafts), 2);
    assert_eq!(before.file_count(IndexId::Spec), 0);

    let after = pass(
        &before,
        vec![
            artifact(IndexId::Spec, "specifications/a.md", "teardown alpha"),
            artifact(IndexId::Spec, "specifications/b.md", "teardown bravo"),
        ],
        PassScope::ALL,
    );
    assert_eq!(
        after.file_count(IndexId::Drafts),
        0,
        "the graduated draft leaves the drafts index"
    );
    assert_eq!(after.file_count(IndexId::Spec), 2);
    assert_eq!(
        hit_paths(&search_snapshot(&after, &[], "teardown", 10)),
        vec![
            (IndexId::Spec, "specifications/a.md".to_string()),
            (IndexId::Spec, "specifications/b.md".to_string()),
        ],
        "no query sees the same material in both indexes"
    );
}

#[test]
fn a_pass_scoped_to_one_half_leaves_the_other_untouched() {
    // The scope is what keeps a draft save from re-reading every artifact, and
    // a tree change from disturbing the drafts index.
    let both = pass(
        &IndexSet::default(),
        vec![
            artifact(IndexId::Spec, "a.md", "alpha"),
            draft("d1", "n.md", "bravo"),
        ],
        PassScope::ALL,
    );

    // A drafts-only pass sees no artifact sources, and must not read that as
    // "every artifact was deleted".
    let drafts_only = pass(&both, vec![draft("d1", "n.md", "bravo")], PassScope::DRAFTS);
    assert_eq!(drafts_only.file_count(IndexId::Spec), 1);
    assert_eq!(drafts_only.file_count(IndexId::Drafts), 1);

    // And the reverse.
    let artifacts_only = pass(
        &drafts_only,
        vec![artifact(IndexId::Spec, "a.md", "alpha")],
        PassScope::ARTIFACTS,
    );
    assert_eq!(artifacts_only.file_count(IndexId::Spec), 1);
    assert_eq!(artifacts_only.file_count(IndexId::Drafts), 1);
}

// ---------------------------------------------------------------------------
// BMI-FR-27 — determinism
// ---------------------------------------------------------------------------

#[test]
fn an_incrementally_maintained_index_equals_one_built_from_scratch() {
    // BMI-FR-27: identical chunk text, identical ordinals, identical
    // membership — and, because a touched shard is rebuilt rather than
    // amended, identical scores too.
    let final_sources = || {
        vec![
            artifact(IndexId::Spec, "a.md", "# A\n\nteardown of the worktree\n"),
            artifact(IndexId::Spec, "b.md", "# B\n\nteardown of the scan\n"),
            artifact(IndexId::Skill, "s.md", "# S\n\nteardown review\n"),
        ]
    };

    // Built in one go.
    let fresh = pass(&IndexSet::default(), final_sources(), PassScope::ALL);

    // Reached by a series of edits: a file created, one created then rewritten,
    // one created then deleted.
    let mut incremental = pass(
        &IndexSet::default(),
        vec![
            artifact(IndexId::Spec, "a.md", "# A\n\nsomething else entirely\n"),
            artifact(IndexId::Spec, "gone.md", "# Gone\n\ntransient\n"),
        ],
        PassScope::ALL,
    );
    incremental = pass(
        &incremental,
        vec![
            artifact(IndexId::Spec, "a.md", "# A\n\nteardown of the worktree\n"),
            artifact(IndexId::Spec, "gone.md", "# Gone\n\ntransient\n"),
            artifact(IndexId::Skill, "s.md", "# S\n\nteardown review\n"),
        ],
        PassScope::ALL,
    );
    incremental = pass(&incremental, final_sources(), PassScope::ALL);

    for index in IndexId::ALL {
        assert_eq!(
            fresh.paths(index),
            incremental.paths(index),
            "{index:?} membership must match"
        );
        assert_eq!(fresh.chunk_count(index), incremental.chunk_count(index));
    }
    let a = search_snapshot(&fresh, &[], "teardown", 10);
    let b = search_snapshot(&incremental, &[], "teardown", 10);
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        assert_eq!(x.path, y.path);
        assert_eq!(x.chunk_ordinal, y.chunk_ordinal);
        assert_eq!(x.text, y.text);
        assert_eq!(
            x.score, y.score,
            "a rebuilt shard fits `avgdl` to what it holds, so an incremental \
             history scores identically to a fresh build"
        );
    }
    assert!(!a.is_empty(), "precondition: the query matches something");
}

#[test]
fn indexing_the_same_tree_twice_yields_the_same_chunks_and_ordinals() {
    // BMI-FR-27, the simpler half.
    let sources = || {
        vec![artifact(
            IndexId::Spec,
            "a.md",
            "# One\n\nalpha\n\n# Two\n\nbravo\n\n# Three\n\ncharlie\n",
        )]
    };
    let first = pass(&IndexSet::default(), sources(), PassScope::ALL);
    let second = pass(&IndexSet::default(), sources(), PassScope::ALL);
    let a = search_snapshot(&first, &[], "alpha bravo charlie", 10);
    let b = search_snapshot(&second, &[], "alpha bravo charlie", 10);
    assert_eq!(a, b);
    assert_eq!(a.len(), 3);
}

// ---------------------------------------------------------------------------
// Progress reporting through a pass (BMI-FR-22, PRG-FR-05, STB-FR-09)
// ---------------------------------------------------------------------------

#[test]
fn a_pass_reports_progress_that_becomes_determinate_and_never_exceeds_its_total() {
    // BMI-FR-22 / PRG-FR-05: the operation becomes determinate once the pass
    // knows how many files it will index.
    let mut seen: Vec<u64> = Vec::new();
    let total = 4u64;
    let sources: Vec<SourceFile> = (0..total)
        .map(|i| artifact(IndexId::Spec, &format!("s{i}.md"), "teardown"))
        .collect();
    reconcile(
        &IndexSet::default(),
        sources,
        PassScope::ALL,
        |_, _, _| {},
        |processed| seen.push(processed),
    );
    assert_eq!(seen, vec![0, 1, 2, 3]);
    assert!(seen.iter().all(|c| *c < total));
}

