//! Module surface, index membership, drafts, chunking, language, and skipped files.

use super::*;

// ---------------------------------------------------------------------------
// BMI-FR-01 — the module exposes no command and no event
// ---------------------------------------------------------------------------

#[test]
fn no_part_of_this_module_is_reachable_from_the_frontend() {
    // BMI-FR-01. The claim rests entirely on this module's ABSENCE from the
    // handler, exactly as the credential-reading guards in `lib.rs` do: every
    // other registration test checks the forward direction — that a name IS
    // registered — so adding `#[tauri::command]` to `search` tomorrow would
    // turn nothing else red.
    const LIB: &str = include_str!("../../lib.rs");
    let handler = LIB
        .split_once("generate_handler![")
        .expect("generate_handler! invocation")
        .1
        .split_once("])")
        .expect("end of generate_handler!")
        .0;
    assert!(
        !handler.contains("bm25_index"),
        "no operation of the BM25 index module may be a registered command: \
         its whole surface is the internal Rust API (BMI-FR-01)",
    );

    // And the module's own source declares no command or event of its own.
    // Comments are stripped first: this module's own documentation discusses
    // the attribute it must not carry, and a naive scan would match that prose
    // and fail for the opposite of the reason this test exists.
    let source = [
        include_str!("../../bm25_index.rs"),
        include_str!("../chunking.rs"),
        include_str!("../reconcile.rs"),
        include_str!("../retrieval.rs"),
        include_str!("../scheduling.rs"),
        include_str!("../sources.rs"),
    ]
    .join("\n");
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !code.contains("#[tauri::command]"),
        "BMI-FR-01: this module registers no Tauri command",
    );
    assert!(
        !code.contains(".emit("),
        "BMI-FR-01: this module emits no Tauri event of its own",
    );
}

// ---------------------------------------------------------------------------
// BMI-FR-02, BMI-FR-10 / BMI-FR-03 — index selection and membership
// ---------------------------------------------------------------------------

#[test]
fn an_index_holds_only_the_files_of_its_own_type_and_a_query_can_select_it() {
    // BMI-FR-02 / BMI-FR-03 / BMI-FR-10.
    let set = pass(
        &IndexSet::default(),
        vec![
            artifact(IndexId::Skill, ".claude/skills/review.md", "worktree teardown"),
            artifact(IndexId::Spec, "specifications/wtc.md", "worktree teardown"),
        ],
        PassScope::ALL,
    );

    let only_skill = search_snapshot(&set, &[IndexId::Skill], "teardown", 10);
    assert_eq!(
        hit_paths(&only_skill),
        vec![(IndexId::Skill, ".claude/skills/review.md".to_string())],
        "a query naming one index must not reach into another"
    );

    let both = search_snapshot(&set, &[], "teardown", 10);
    assert_eq!(
        hit_paths(&both),
        vec![
            (IndexId::Skill, ".claude/skills/review.md".to_string()),
            (IndexId::Spec, "specifications/wtc.md".to_string()),
        ],
        "an empty selection is every index (BMI-FR-10)"
    );
    for hit in &both {
        assert_eq!(
            hit.node_id.as_deref(),
            Some(hit.path.as_str()),
            "an artifact chunk carries the ASC node id, which is its path"
        );
        assert!(hit.draft_id.is_none());
    }
}

#[test]
fn a_file_with_no_resolved_type_is_indexed_nowhere() {
    // BMI-FR-03: unclassified source files, playbooks, workstreams, and roles
    // carrying no artifact type contribute no chunks. `collect_sources` is what
    // enforces this in production; here the claim is that nothing else lets
    // them in — an index only ever receives what a source names.
    let set = pass(
        &IndexSet::default(),
        vec![artifact(IndexId::Spec, "specifications/a.md", "teardown")],
        PassScope::ALL,
    );
    assert_eq!(set.file_count(IndexId::Spec), 1);
    for index in IndexId::ALL {
        if index != IndexId::Spec {
            assert_eq!(set.file_count(index), 0, "{index:?} must be empty");
        }
    }
    assert!(
        search_snapshot(&set, &[], "src/main.rs", 10).is_empty(),
        "a file that was never a source cannot be found"
    );
}

// ---------------------------------------------------------------------------
// DRS-FR-11, BMI-FR-04 — drafts, both statuses, and never in an artifact index
// ---------------------------------------------------------------------------

#[test]
fn drafts_of_either_status_are_indexed_and_only_in_the_drafts_index() {
    // BMI-FR-04's status clause, at the layer below prompt selection. Status is
    // invisible here by construction: a draft's prompt is a source on identical
    // terms whatever its record says, which is the whole of the claim that an
    // archived draft indexes like an active one.
    //
    // Which file of a draft is its prompt is decided a layer up, in
    // `collect_sources` via `drafts::require_prompt` — covered by
    // `an_inconsistent_draft_contributes_nothing_to_the_drafts_index`. These
    // fixtures are synthetic sources and are not prompt paths.
    let set = pass(
        &IndexSet::default(),
        vec![
            draft("d-active", "spec.md", "worktree teardown notes"),
            draft("d-archived", "old.md", "worktree teardown notes"),
        ],
        PassScope::ALL,
    );
    let hits = search_snapshot(&set, &[IndexId::Drafts], "teardown", 10);
    assert_eq!(hits.len(), 2);
    let mut ids: Vec<Option<String>> = hits.iter().map(|h| h.draft_id.clone()).collect();
    ids.sort();
    assert_eq!(
        ids,
        vec![
            Some("d-active".to_string()),
            Some("d-archived".to_string())
        ],
    );
    for hit in &hits {
        assert!(
            hit.node_id.is_none(),
            "a draft chunk has no scan node, so it carries no node id"
        );
    }
    for index in IndexId::ALL {
        if index != IndexId::Drafts {
            assert_eq!(set.file_count(index), 0, "no draft reaches {index:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// BMI-FR-05 / BMI-FR-06 / BMI-FR-07 — chunking
// ---------------------------------------------------------------------------

#[test]
fn a_markdown_file_splits_at_every_heading_and_no_chunk_nests_in_another() {
    // BMI-FR-05: a chunk runs from one heading to the next heading of ANY
    // level, so a `##` section is not swallowed by the `#` above it.
    let text = "\
A preamble paragraph.

# Title

Body of the title.

## Intent

The intent.

### Detail

The detail.
";
    let chunks = chunk_text(text);
    assert_eq!(chunks.len(), 4, "preamble + three headings: {chunks:#?}");
    assert!(chunks[0].contains("A preamble paragraph"));
    assert!(chunks[0].starts_with("A preamble"), "the preamble is its own chunk");
    assert!(chunks[1].starts_with("# Title"));
    assert!(
        !chunks[1].contains("## Intent"),
        "a heading must end the chunk above it, not nest inside it"
    );
    assert!(chunks[2].starts_with("## Intent"));
    assert!(chunks[3].starts_with("### Detail"));
}

#[test]
fn a_file_whose_first_line_is_a_heading_has_no_preamble_chunk() {
    let chunks = chunk_text("# Only\n\nBody.\n");
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].starts_with("# Only"));
}

#[test]
fn a_hash_inside_a_fenced_code_block_is_not_a_heading() {
    // A shell comment is not a section boundary, and splitting on one would cut
    // a code sample in half.
    let text = "\
# Title

```sh
# rebuild the index
cargo run
```

More body.
";
    let chunks = chunk_text(text);
    assert_eq!(chunks.len(), 1, "the fence holds it together: {chunks:#?}");
    assert!(chunks[0].contains("cargo run"));
}

#[test]
fn a_heading_less_file_splits_into_bounded_blocks_at_blank_lines() {
    // BMI-FR-06: no file is ever one unbounded document, and a boundary falls
    // at a blank line.
    let paragraph = "lorem ipsum dolor sit amet ".repeat(40); // ~1080 chars
    let text = std::iter::repeat(paragraph.trim())
        .take(12)
        .collect::<Vec<_>>()
        .join("\n\n");
    assert!(text.chars().count() > MAX_CHUNK_CHARS * 2);

    let chunks = chunk_text(&text);
    assert!(chunks.len() > 2, "a large file must not be one document");
    for chunk in &chunks {
        assert!(
            chunk.chars().count() <= MAX_CHUNK_CHARS,
            "every chunk is bounded: {} chars",
            chunk.chars().count()
        );
        assert!(
            !chunk.starts_with(' ') && !chunk.starts_with('\n'),
            "a boundary falls at a blank line, not mid-whitespace"
        );
    }
}

#[test]
fn one_heading_section_over_the_ceiling_is_split_further() {
    // BMI-FR-06: the same rule applies inside a heading section.
    // Paragraphs, so the split goes through the blank-line rule rather than
    // through the hard-split fallback — those are two different code paths and
    // BMI-FR-06 names this one.
    let paragraph = "lorem ipsum dolor sit amet consectetur ".repeat(20);
    let body = std::iter::repeat(paragraph.trim())
        .take(15)
        .collect::<Vec<_>>()
        .join("\n\n");
    let text = format!("# Title\n\n{body}");
    assert!(text.chars().count() > MAX_CHUNK_CHARS);
    let chunks = chunk_text(&text);
    assert!(chunks.len() > 1, "an over-long section splits: {}", chunks.len());
    for chunk in &chunks {
        assert!(chunk.chars().count() <= MAX_CHUNK_CHARS);
        assert!(
            !chunk.trim_end().ends_with("consectetu"),
            "a boundary falls between paragraphs, never mid-word: {chunk:?}"
        );
    }
}

#[test]
fn a_single_paragraph_over_the_ceiling_is_hard_split_on_character_boundaries() {
    // The one case with no blank line to cut at. Multi-byte characters must
    // survive the cut.
    let text = "ü".repeat(MAX_CHUNK_CHARS * 2 + 7);
    let chunks = chunk_text(&text);
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].chars().count(), MAX_CHUNK_CHARS);
    assert_eq!(chunks[2].chars().count(), 7);
    assert_eq!(
        chunks.concat().chars().count(),
        MAX_CHUNK_CHARS * 2 + 7,
        "nothing was lost or duplicated at a boundary"
    );
}

#[test]
fn a_whitespace_only_chunk_is_dropped_and_ordinals_stay_contiguous() {
    // BMI-FR-07: ordinals are the chunk's position in file order. A chunk that
    // holds nothing would match no query and would only shift every later one.
    // A whitespace-only PREAMBLE: the section before the first heading holds
    // nothing but blank lines, so it is the chunk that must be dropped. (A
    // whitespace-only section *under* a heading is not whitespace-only — it
    // still carries the heading line — so it would prove nothing here.)
    let chunks = chunk_text("\n   \n\n# One\n\nalpha\n\n# Two\n\nbravo\n");
    assert_eq!(
        chunks.len(),
        2,
        "the empty preamble contributes no chunk: {chunks:#?}"
    );
    assert!(chunks[0].starts_with("# One"));
    assert!(chunks[1].starts_with("# Two"));

    // And a blank paragraph between two real ones in a heading-less file.
    let heading_less = chunk_text("alpha\n\n   \n\nbravo\n");
    assert!(
        heading_less.iter().all(|c| !c.trim().is_empty()),
        "no chunk is whitespace alone: {heading_less:#?}"
    );

    let set = pass(
        &IndexSet::default(),
        vec![artifact(
            IndexId::Spec,
            "a.md",
            "\n   \n\n# One\n\nalpha\n\n# Two\n\nbravo\n\n# Three\n\ncharlie\n",
        )],
        PassScope::ALL,
    );
    let mut ordinals: Vec<u32> = search_snapshot(&set, &[IndexId::Spec], "alpha bravo charlie", 10)
        .iter()
        .map(|h| h.chunk_ordinal)
        .collect();
    ordinals.sort();
    assert_eq!(
        ordinals,
        vec![0, 1, 2],
        "the surviving chunks are numbered contiguously from zero, so a dropped \
         chunk does not leave a hole in the ordinals"
    );
}

// ---------------------------------------------------------------------------
// BMI-FR-08 — language detection and stemming
// ---------------------------------------------------------------------------

#[test]
fn a_files_language_is_detected_once_and_governs_every_chunk_of_it() {
    // BMI-FR-08: detection is per file, so a two-word heading chunk is stemmed
    // as the document it came from rather than guessed at on its own.
    assert_eq!(detect_file_language(GERMAN), Language::German);

    let set = pass(
        &IndexSet::default(),
        vec![artifact(IndexId::Spec, "de.md", GERMAN)],
        PassScope::ALL,
    );
    assert_eq!(
        set.language_of(IndexId::Spec, "de.md"),
        Some(Language::German),
        "the whole file is indexed under one language"
    );

    // The short heading-only section is reachable, which it could only be if it
    // went into the German shard along with the rest of the file rather than
    // being detected — and mis-detected — on its own.
    // The discriminating query: the file says `Verzeichnisse`, and only a German
    // stemmer folds that onto `Verzeichnis`. Forced into an English shard the
    // same corpus answers this with nothing, so this assertion fails if the
    // per-file language ever stops reaching the shard that is built.
    assert!(
        !search_snapshot(&set, &[IndexId::Spec], "Verzeichnis", 10).is_empty(),
        "a German-stemmed shard answers a German-inflected query"
    );
    let english = pass(
        &IndexSet::default(),
        vec![artifact(IndexId::Spec, "en.md", "Directories and teardown.")],
        PassScope::ALL,
    );
    assert!(
        search_snapshot(&english, &[IndexId::Spec], "Verzeichnis", 10).is_empty(),
        "precondition: the query is answerable only by the German shard"
    );

    // And the short heading-only section is reachable, which it could only be
    // if it went into the German shard with the rest of the file rather than
    // being detected — and mis-detected — on its own few words.
    let hits = search_snapshot(&set, &[IndexId::Spec], "Überschrift Abschnitt", 10);
    assert!(
        hits.iter().any(|h| h.text.contains("## Überschrift")),
        "the heading chunk must be findable: {:#?}",
        hits.iter().map(|h| &h.text).collect::<Vec<_>>()
    );
}

#[test]
fn an_undetectable_language_falls_back_to_english_rather_than_going_unindexed() {
    // BMI-FR-08: never unindexed, never unstemmed.
    assert_eq!(detect_file_language(""), Language::English);
    assert_eq!(detect_file_language("   \n\n  "), Language::English);
    // Punctuation and symbols carry no language signal.
    assert_eq!(detect_file_language("{ } [ ] ; , . 42"), Language::English);

    let set = pass(
        &IndexSet::default(),
        vec![artifact(IndexId::Flow, "x.flow", "{\"nodes\": [\"alpha\"]}")],
        PassScope::ALL,
    );
    assert_eq!(set.language_of(IndexId::Flow, "x.flow"), Some(Language::English));
    assert!(!search_snapshot(&set, &[IndexId::Flow], "alpha", 10).is_empty());
}

#[test]
fn stemming_is_applied_so_an_inflected_query_matches() {
    // BMI-FR-08: every chunk is stemmed; none is indexed raw. If stemming were
    // off, "running" would not answer a query for "run".
    let set = pass(
        &IndexSet::default(),
        vec![artifact(
            IndexId::Spec,
            "a.md",
            "The indexer reconciles by running passes over the candidate files.",
        )],
        PassScope::ALL,
    );
    assert!(
        !search_snapshot(&set, &[IndexId::Spec], "run", 10).is_empty(),
        "an English stemmer maps `running` and `run` onto the same term"
    );
}

// ---------------------------------------------------------------------------
// BMI-FR-09, BMI-FR-23 — skipped files
// ---------------------------------------------------------------------------

#[test]
fn an_unreadable_file_is_skipped_with_a_reason_and_the_pass_completes() {
    // BMI-FR-09: skipped rather than fatal.
    let mut skipped: Vec<(String, String)> = Vec::new();
    let (set, stats) = reconcile(
        &IndexSet::default(),
        vec![
            unreadable(IndexId::Spec, "binary.md", "not valid UTF-8 text"),
            artifact(IndexId::Spec, "good.md", "teardown"),
        ],
        PassScope::ALL,
        |_, file, reason| skipped.push((file.path.clone(), reason.to_string())),
        |_| {},
    );
    assert_eq!(
        skipped,
        vec![("binary.md".to_string(), "not valid UTF-8 text".to_string())]
    );
    assert_eq!(stats.files_skipped, 1);
    assert_eq!(stats.files_indexed, 1);
    assert_eq!(set.paths(IndexId::Spec), vec!["good.md".to_string()]);
    assert!(!search_snapshot(&set, &[], "teardown", 10).is_empty());
}

#[test]
fn a_file_that_becomes_unreadable_leaves_the_index() {
    // An index must never keep chunks it can no longer justify.
    let first = pass(
        &IndexSet::default(),
        vec![artifact(IndexId::Spec, "a.md", "teardown")],
        PassScope::ALL,
    );
    assert_eq!(first.file_count(IndexId::Spec), 1);

    let second = pass(
        &first,
        vec![unreadable(IndexId::Spec, "a.md", "exceeds the ceiling")],
        PassScope::ALL,
    );
    assert_eq!(second.file_count(IndexId::Spec), 0);
    assert!(search_snapshot(&second, &[], "teardown", 10).is_empty());
}

