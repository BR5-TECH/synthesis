//! Reading a run's log streams back
//! (`GRS-graduation-run-log-storage.md` GRS-FR-RZXA and the paging, scoping,
//! status, and search rules around it).

use super::*;

use crate::graduation::logs::read::{
    GraduationLogCursor, GraduationLogPassScope, DIRECTION_AFTER, DIRECTION_BEFORE,
    SEARCH_MATCHED, SEARCH_NOT_REQUESTED, SEARCH_NO_MATCH, STATUS_AVAILABLE, STATUS_EMPTY,
    STATUS_PERSISTENCE_FAILED, STATUS_UNAVAILABLE,
};
use crate::graduation::logs::{self, GraduationLogPage, GraduationLogStream};

/// A run that has been driven to completion, so both streams hold records of
/// several phases and passes.
pub(super) fn driven(fx: &Fixture) -> GraduationRun {
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");
    fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::revise(ReviewSeverity::Major, "Not yet."),
            Turn::work().writing("src/panel.ts", "2\n"),
            Turn::ready(),
        ]),
    )
}

fn read(
    fx: &Fixture,
    run: &GraduationRun,
    phase_id: &str,
    scope: GraduationLogPassScope,
    stream: GraduationLogStream,
    cursor: Option<GraduationLogCursor>,
    limit: Option<u32>,
    query: Option<&str>,
) -> GraduationLogPage {
    let fs = crate::graduation::store_fs(&fx.app).expect("the store");
    let paths = logs::paths_for(&fx.app, &run.id).expect("the paths");
    logs::read::read_page(
        &fs,
        &paths,
        &run.logs,
        &run.id,
        phase_id,
        &scope,
        stream,
        cursor,
        limit,
        query.map(str::to_string),
    )
    .expect("a page")
}

fn sequences(page: &GraduationLogPage) -> Vec<u64> {
    page.entries
        .iter()
        .map(|entry| {
            entry
                .record
                .get("sequence")
                .and_then(|v| v.as_u64())
                .unwrap_or_default()
        })
        .collect()
}

/// GRS-FR-RZXA, GRS-FR-NKZP, GRS-FR-TQAO: a read answers for one run, one
/// phase, one scope, and one stream, and a pass scope holds that pass alone.
#[test]
fn a_pass_scope_returns_that_pass_and_no_run_level_record() {
    let fx = Fixture::new();
    let run = driven(&fx);

    let page = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Pass { pass: 1 },
        GraduationLogStream::Structured,
        None,
        None,
        None,
    );
    assert_eq!(page.status, STATUS_AVAILABLE);
    assert!(!page.entries.is_empty(), "pass 1 of working wrote something");
    for entry in &page.entries {
        assert_eq!(entry.record["phase_id"].as_str(), Some("working"));
        assert_eq!(entry.record["pass"].as_u64(), Some(1));
        assert!(!entry.presentation.run_level);
    }
}

/// GRS-FR-JXRV, GRS-FR-NKZP: a run-level scope holds the records whose
/// persisted `pass` is null and no pass's own, and nothing assigns one to a
/// pass.
#[test]
fn a_run_level_scope_returns_the_records_whose_pass_is_null_alone() {
    let fx = Fixture::new();
    let run = driven(&fx);

    let page = read(
        &fx,
        &run,
        "queued",
        GraduationLogPassScope::RunLevel,
        GraduationLogStream::Structured,
        None,
        None,
        None,
    );
    assert_eq!(page.status, STATUS_AVAILABLE);
    assert!(!page.entries.is_empty(), "the queue wait wrote something");
    for entry in &page.entries {
        assert_eq!(entry.record["phase_id"].as_str(), Some("queued"));
        assert!(entry.record["pass"].is_null());
        assert!(entry.presentation.run_level);
    }

    // The same phase read for a pass holds none of them.
    let as_pass = read(
        &fx,
        &run,
        "queued",
        GraduationLogPassScope::Pass { pass: 1 },
        GraduationLogStream::Structured,
        None,
        None,
        None,
    );
    assert!(as_pass.entries.is_empty());
    assert_eq!(as_pass.status, STATUS_EMPTY);
}

/// GRS-FR-EMTV, GRS-FR-BWQK: a phase scope returns the pass-scoped and the
/// run-level records of that phase alike, in one ascending order, and it is
/// exactly the union of the scopes that stand inside it.
#[test]
fn a_phase_scope_returns_everything_the_phase_holds_in_one_order() {
    let fx = Fixture::new();
    let run = driven(&fx);

    for phase_id in ["queued", "working", "review", "done"] {
        let whole = read(
            &fx,
            &run,
            phase_id,
            GraduationLogPassScope::Phase,
            GraduationLogStream::Structured,
            None,
            Some(1000),
            None,
        );
        let read_sequences = sequences(&whole);
        let mut sorted = read_sequences.clone();
        sorted.sort_unstable();
        assert_eq!(read_sequences, sorted, "{phase_id} is ascending by sequence");

        let mut parts: Vec<u64> = sequences(&read(
            &fx,
            &run,
            phase_id,
            GraduationLogPassScope::RunLevel,
            GraduationLogStream::Structured,
            None,
            Some(1000),
            None,
        ));
        for pass in 1..=2 {
            parts.extend(sequences(&read(
                &fx,
                &run,
                phase_id,
                GraduationLogPassScope::Pass { pass },
                GraduationLogStream::Structured,
                None,
                Some(1000),
                None,
            )));
        }
        parts.sort_unstable();
        assert_eq!(
            parts, read_sequences,
            "{phase_id} holds nothing outside its own scopes"
        );
    }

    // The run-level records stand in the phase that wrote them and are marked
    // as run-level rather than as a pass's own.
    let queued = read(
        &fx,
        &run,
        "queued",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        Some(1000),
        None,
    );
    assert!(queued.entries.iter().all(|e| e.presentation.run_level));
    let working = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        Some(1000),
        None,
    );
    assert!(working.entries.iter().any(|e| !e.presentation.run_level));
}

/// GRS-FR-HVUJ, GRS-FR-JUFY: the record is returned as the file holds it, and a
/// pass-scope read and a phase-scope read return one identical record.
#[test]
fn a_record_reads_back_identically_through_two_scopes() {
    let fx = Fixture::new();
    let run = driven(&fx);

    let by_pass = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Pass { pass: 1 },
        GraduationLogStream::Structured,
        None,
        Some(1000),
        None,
    );
    let by_phase = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        Some(1000),
        None,
    );
    let first = &by_pass.entries[0].record;
    let same = by_phase
        .entries
        .iter()
        .find(|entry| entry.record["record_id"] == first["record_id"])
        .expect("the phase scope holds it too");
    assert_eq!(&same.record, first);
}

/// GRS-FR-GSUY, GRS-FR-OWDT: an absent cursor returns the newest page, a
/// `before` cursor the page preceding it, and the two never overlap.
#[test]
fn paging_walks_backwards_without_returning_a_record_twice() {
    let fx = Fixture::new();
    let run = driven(&fx);

    let newest = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        Some(2),
        None,
    );
    assert_eq!(newest.entries.len(), 2);
    assert!(!newest.oldest_reached, "more history stands behind it");
    let older_cursor = newest.older_cursor.clone().expect("an older cursor");
    assert_eq!(older_cursor.direction, DIRECTION_BEFORE);

    let older = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        Some(older_cursor),
        Some(2),
        None,
    );
    let head: Vec<u64> = sequences(&older);
    let tail: Vec<u64> = sequences(&newest);
    assert!(
        head.iter().all(|s| !tail.contains(s)),
        "the two pages are disjoint"
    );
    assert!(head.iter().max() < tail.iter().min(), "older is older");
    assert_eq!(newest.matched_total, older.matched_total);
}

/// GRS-FR-GSUY: an `after` cursor returns the records later than it, ascending,
/// which is how a following surface reads what arrived.
#[test]
fn an_after_cursor_returns_only_what_came_later() {
    let fx = Fixture::new();
    let run = driven(&fx);

    let all = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        Some(1000),
        None,
    );
    let first = sequences(&all)[0];
    let after = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        Some(GraduationLogCursor {
            run_id: run.id.clone(),
            stream: GraduationLogStream::Structured,
            direction: DIRECTION_AFTER.to_string(),
            sequence: first,
        }),
        Some(1000),
        None,
    );
    assert!(sequences(&after).iter().all(|s| *s > first));
}

/// GRS-FR-UEIL: a cursor of another run or another stream is refused rather
/// than answered from the wrong record set.
#[test]
fn a_cursor_of_another_scope_is_refused() {
    let fx = Fixture::new();
    let run = driven(&fx);
    let fs = crate::graduation::store_fs(&fx.app).expect("the store");
    let paths = logs::paths_for(&fx.app, &run.id).expect("the paths");

    let refused = logs::read::read_page(
        &fs,
        &paths,
        &run.logs,
        &run.id,
        "working",
        &GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        Some(GraduationLogCursor {
            run_id: "another-run".to_string(),
            stream: GraduationLogStream::Structured,
            direction: DIRECTION_AFTER.to_string(),
            sequence: 1,
        }),
        None,
        None,
    );
    assert_eq!(
        refused.unwrap_err(),
        logs::read::ERR_CURSOR_NOT_FOR_THIS_SCOPE
    );
}

/// GRS-FR-OYIC, GRS-FR-NSGX: search is case-insensitive substring over what the
/// surface displays, and a query matching nothing is never an empty scope.
#[test]
fn search_matches_what_is_displayed_and_never_empties_the_scope() {
    let fx = Fixture::new();
    let run = driven(&fx);

    let plain = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        Some(1000),
        None,
    );
    assert_eq!(plain.search, SEARCH_NOT_REQUESTED);

    let event = plain.entries[0].record["event"]
        .as_str()
        .expect("an event")
        .to_string();
    let matched = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        Some(1000),
        Some(&event.to_uppercase()),
    );
    assert_eq!(matched.search, SEARCH_MATCHED);
    assert!(matched.matched_total >= 1);

    let missing = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        Some(1000),
        Some("no-record-carries-this-text"),
    );
    assert_eq!(missing.search, SEARCH_NO_MATCH);
    assert_eq!(missing.matched_total, 0);
    assert_eq!(
        missing.status, STATUS_AVAILABLE,
        "the scope's own records settle the status"
    );
}

/// GRS-FR-OYIC, GRS-FR-QVTA: a source search matches the decoded output, so a
/// reader searches what is on screen.
#[test]
fn a_source_search_matches_the_decoded_output() {
    let fx = Fixture::new();
    let run = driven(&fx);

    let page = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Source,
        None,
        Some(1000),
        Some("THE SCRIPTED TURN"),
    );
    assert_eq!(page.search, SEARCH_MATCHED);
    assert!(page.matched_total >= 1);
}

/// GRS-FR-OYIC: a run-level record is searchable by the marker it is shown
/// with, so nothing a reader can see is unmatchable.
#[test]
fn a_run_level_record_is_matched_by_the_marker_it_is_shown_with() {
    let fx = Fixture::new();
    let run = driven(&fx);

    let page = read(
        &fx,
        &run,
        "queued",
        GraduationLogPassScope::RunLevel,
        GraduationLogStream::Structured,
        None,
        Some(1000),
        Some("Run-Level"),
    );
    assert_eq!(page.search, SEARCH_MATCHED);
    assert!(page.matched_total >= 1);
}

/// GRS-FR-IQUA: a scope holding no record is `empty`, and that is never a read
/// that failed.
#[test]
fn a_scope_that_holds_nothing_is_empty_rather_than_failed() {
    let fx = Fixture::new();
    let run = driven(&fx);

    let page = read(
        &fx,
        &run,
        "queued",
        GraduationLogPassScope::Pass { pass: 7 },
        GraduationLogStream::Source,
        None,
        None,
        None,
    );
    assert_eq!(page.status, STATUS_EMPTY);
    assert!(page.entries.is_empty());
    assert!(page.failure.is_none());
    assert!(page.oldest_reached);
    assert!(page.older_cursor.is_none());
}

/// GRS-FR-KYWE, GRS-FR-EYNU: a line inside the file that does not decode is a
/// typed read failure naming how far the read got, and it is not a write
/// failure.
#[test]
fn a_damaged_line_reads_as_a_typed_read_failure() {
    let fx = Fixture::new();
    let run = driven(&fx);
    let path = logs::paths_for(&fx.app, &run.id)
        .expect("the paths")
        .stream(GraduationLogStream::Structured);
    let text = std::fs::read_to_string(&path).expect("the stream");
    let mut lines: Vec<&str> = text.lines().collect();
    lines.insert(2, "{ this is not json");
    std::fs::write(&path, lines.join("\n")).expect("the damaged stream");

    let page = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        None,
        None,
    );
    assert_eq!(page.status, STATUS_UNAVAILABLE);
    let failure = page.failure.expect("a typed read failure");
    assert_eq!(failure.kind, "read");
    assert_eq!(failure.code, logs::index::CODE_STREAM_CORRUPT);
    assert_eq!(failure.stream, GraduationLogStream::Structured);
    assert!(failure.stopped_sequence.is_some());
    assert!(failure.byte_offset.is_some());
    assert!(failure.pending_record_ids.is_empty());
}

/// GRS-FR-THZA: a failed write of the read stream is reported as a persistence
/// failure, so a surface says output stopped rather than that a run wrote none.
#[test]
fn a_failed_write_of_this_stream_reads_as_a_persistence_failure() {
    let fx = Fixture::new();
    let mut run = driven(&fx);
    run.logs.note_failed(
        logs::GraduationLogFailure::write(
            logs::index::CODE_APPEND_FAILED,
            GraduationLogStream::Structured,
            "the stream could not be appended to",
            vec!["r-1".to_string()],
        ),
        1,
    );

    let page = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        None,
        None,
    );
    assert_eq!(page.status, STATUS_PERSISTENCE_FAILED);
    let failure = page.failure.expect("a typed write failure");
    assert_eq!(failure.kind, "write");
    assert_eq!(failure.pending_record_ids, vec!["r-1".to_string()]);

    // GRS-FR-CTQI: the other stream is unaffected by it and carries its own
    // status.
    let other = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Source,
        None,
        None,
        None,
    );
    assert_ne!(other.status, STATUS_PERSISTENCE_FAILED);
}

/// GRS-FR-MBED: a storage version this build does not recognise renders no log
/// and is neither a read failure nor a refusal of the run.
#[test]
fn an_unrecognised_storage_version_renders_no_log() {
    let fx = Fixture::new();
    let mut run = driven(&fx);
    run.logs.log_storage_version = 2;

    let page = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        None,
        None,
    );
    assert_eq!(page.status, STATUS_EMPTY);
    assert!(page.failure.is_none());
}

/// GRS-FR-DYPS: every read is bounded to one page, and the page
/// states the whole scope's `matched_total` rather than what it holds.
#[test]
fn a_page_is_bounded_and_still_counts_the_whole_scope() {
    let fx = Fixture::new();
    let run = driven(&fx);

    let page = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        Some(1),
        None,
    );
    assert_eq!(page.entries.len(), 1);
    assert!(page.matched_total > 1, "the scope holds more than the page");
    assert_eq!(page.latest_sequence, run.logs.structured.latest_sequence);
}

/// GRS-FR-MQEQ, GRS-FR-KYWE: a partial **final** line is the tail of a write
/// that did not finish, and it is not a damaged file.
#[test]
fn a_partial_final_line_is_a_tail_rather_than_corruption() {
    let fx = Fixture::new();
    let run = driven(&fx);
    let path = logs::paths_for(&fx.app, &run.id)
        .expect("the paths")
        .stream(GraduationLogStream::Structured);
    let mut text = std::fs::read_to_string(&path).expect("the stream");
    // The tail of a record whose write the process did not finish.
    text.push_str("{\"schema_version\":1,\"record_id\":\"half");
    std::fs::write(&path, text).expect("the partial tail");

    let page = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        Some(1000),
        None,
    );
    assert_eq!(page.status, STATUS_AVAILABLE);
    assert!(page.failure.is_none(), "a tail is not a read failure");
    assert!(!page.entries.is_empty(), "and the records before it still read");
}

/// GRS-FR-GSUY, GRS-FR-OWDT: an absent cursor returns the **newest** page, and
/// the backward walk reaches the scope's first record and stops there.
#[test]
fn the_backward_walk_reaches_the_beginning_and_covers_the_whole_scope() {
    let fx = Fixture::new();
    let run = driven(&fx);

    let whole = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        Some(1000),
        None,
    );
    let every = sequences(&whole);
    assert!(every.len() > 2, "the scope is worth paging");

    let newest = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        Some(1),
        None,
    );
    assert_eq!(
        sequences(&newest),
        vec![*every.last().unwrap()],
        "an absent cursor answers with the newest page"
    );

    let mut walked = sequences(&newest);
    let mut page = newest;
    while !page.oldest_reached {
        let cursor = page.older_cursor.clone().expect("an older cursor");
        page = read(
            &fx,
            &run,
            "working",
            GraduationLogPassScope::Phase,
            GraduationLogStream::Structured,
            Some(cursor),
            Some(1),
            None,
        );
        let older = sequences(&page);
        assert!(
            older.iter().all(|s| !walked.contains(s)),
            "the pages are disjoint"
        );
        walked.extend(older);
    }
    assert!(page.older_cursor.is_none(), "null exactly at the beginning");
    walked.sort_unstable();
    assert_eq!(walked, every, "the walk covered the whole scope");
}

/// GRS-FR-UEIL: a cursor of the other stream, and one whose direction is
/// neither of the two, are refused rather than answered from the wrong records.
#[test]
fn a_cursor_of_the_other_stream_or_of_no_direction_is_refused() {
    let fx = Fixture::new();
    let run = driven(&fx);
    let fs = crate::graduation::store_fs(&fx.app).expect("the store");
    let paths = logs::paths_for(&fx.app, &run.id).expect("the paths");

    let refuse = |cursor: GraduationLogCursor| {
        logs::read::read_page(
            &fs,
            &paths,
            &run.logs,
            &run.id,
            "working",
            &GraduationLogPassScope::Phase,
            GraduationLogStream::Structured,
            Some(cursor),
            None,
            None,
        )
        .unwrap_err()
    };
    assert_eq!(
        refuse(GraduationLogCursor {
            run_id: run.id.clone(),
            stream: GraduationLogStream::Source,
            direction: DIRECTION_AFTER.to_string(),
            sequence: 1,
        }),
        logs::read::ERR_CURSOR_NOT_FOR_THIS_SCOPE
    );
    assert_eq!(
        refuse(GraduationLogCursor {
            run_id: run.id.clone(),
            stream: GraduationLogStream::Structured,
            direction: "sideways".to_string(),
            sequence: 1,
        }),
        logs::read::ERR_CURSOR_NOT_FOR_THIS_SCOPE
    );
}

/// GRS-FR-OYIC: every displayed source field is searchable, so nothing a reader
/// can see is unmatchable.
#[test]
fn every_displayed_source_field_is_searchable() {
    let fx = Fixture::new();
    let run = driven(&fx);

    let page = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Source,
        None,
        Some(1000),
        None,
    );
    let chunk = page.entries[0].record.clone();
    let sequence = chunk["sequence"].as_u64().expect("a sequence");
    for needle in [
        chunk["run_id"].as_str().unwrap().to_string(),
        chunk["phase_id"].as_str().unwrap().to_string(),
        chunk["origin"].as_str().unwrap().to_string(),
        chunk["producer"].as_str().unwrap().to_string(),
        chunk["source"].as_str().unwrap().to_string(),
        chunk["at"].as_str().unwrap().to_string(),
        sequence.to_string(),
        chunk["pass"].as_u64().unwrap().to_string(),
    ] {
        let found = read(
            &fx,
            &run,
            "working",
            GraduationLogPassScope::Phase,
            GraduationLogStream::Source,
            None,
            Some(1000),
            Some(&needle),
        );
        assert_eq!(found.search, SEARCH_MATCHED, "“{needle}” is searchable");
    }
}

/// GRS-FR-OYIC: a structured search matches the event and every field the
/// surface renders, keys and values alike.
#[test]
fn a_structured_search_matches_the_fields_the_surface_renders() {
    let fx = Fixture::new();
    let run = driven(&fx);

    let page = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Structured,
        None,
        Some(1000),
        None,
    );
    let with_fields = page
        .entries
        .iter()
        .find(|entry| {
            entry.record["fields"]
                .as_object()
                .is_some_and(|fields| !fields.is_empty())
        })
        .expect("a record carrying fields");
    let fields = with_fields.record["fields"].as_object().unwrap();
    let (key, value) = fields.iter().next().unwrap();
    for needle in [key.clone(), value.to_string().trim_matches('"').to_string()] {
        let found = read(
            &fx,
            &run,
            "working",
            GraduationLogPassScope::Phase,
            GraduationLogStream::Structured,
            None,
            Some(1000),
            Some(&needle),
        );
        assert_eq!(found.search, SEARCH_MATCHED, "“{needle}” is searchable");
    }
}

/// GRS-FR-QVTA: decoding for display is UTF-8 with replacement characters, and
/// the stored base64 is unchanged by a read.
#[test]
fn an_invalid_byte_sequence_is_searched_with_replacement_characters() {
    use base64::Engine as _;
    let fx = Fixture::new();
    let run = driven(&fx);
    let path = logs::paths_for(&fx.app, &run.id)
        .expect("the paths")
        .stream(GraduationLogStream::Source);
    let text = std::fs::read_to_string(&path).expect("the stream");
    let mut first: serde_json::Value =
        serde_json::from_str(text.lines().next().unwrap()).expect("a chunk");
    // `0xFF` is no part of any UTF-8 sequence, wrapped in text either side.
    let bytes = [b"before".as_slice(), &[0xFF], b"after".as_slice()].concat();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    first["data_base64"] = serde_json::Value::String(encoded.clone());
    let phase = first["phase_id"].as_str().expect("a phase").to_string();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    lines[0] = first.to_string();
    std::fs::write(&path, format!("{}\n", lines.join("\n"))).expect("the damaged chunk");

    let page = read(
        &fx,
        &run,
        &phase,
        GraduationLogPassScope::Phase,
        GraduationLogStream::Source,
        None,
        Some(1000),
        Some("before"),
    );
    assert_eq!(page.search, SEARCH_MATCHED, "the decoded text is searchable");
    let after = read(
        &fx,
        &run,
        &phase,
        GraduationLogPassScope::Phase,
        GraduationLogStream::Source,
        None,
        Some(1000),
        None,
    );
    assert_eq!(
        after.entries[0].record["data_base64"].as_str(),
        Some(encoded.as_str()),
        "the stored base64 is returned unchanged"
    );
}

/// GRS-FR-DYPS: `limit` bounds one page, whatever the caller asks for.
#[test]
fn a_limit_is_bounded_at_both_ends() {
    let fx = Fixture::new();
    let run = driven(&fx);
    let of = |limit: Option<u32>| {
        read(
            &fx,
            &run,
            "working",
            GraduationLogPassScope::Phase,
            GraduationLogStream::Structured,
            None,
            limit,
            None,
        )
    };
    assert_eq!(of(Some(0)).entries.len(), 1, "a limit of none is one record");
    let whole = of(Some(logs::read::MAX_LIMIT));
    assert_eq!(
        of(Some(logs::read::MAX_LIMIT * 10)).entries.len(),
        whole.entries.len(),
        "a limit past the bound is the bound"
    );
    assert!(of(None).entries.len() <= logs::read::DEFAULT_LIMIT as usize);
}

/// GRS-FR-CTQI, GRS-FR-THZA: a failed write of one stream leaves the other
/// reading its own records under its own status.
#[test]
fn a_failed_write_of_one_stream_leaves_the_other_readable() {
    let fx = Fixture::new();
    let mut run = driven(&fx);
    run.logs.note_failed(
        logs::GraduationLogFailure::write(
            logs::index::CODE_APPEND_FAILED,
            GraduationLogStream::Structured,
            "the stream could not be appended to",
            vec!["r-1".to_string()],
        ),
        1,
    );

    let other = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Source,
        None,
        Some(1000),
        None,
    );
    assert_eq!(other.status, STATUS_AVAILABLE);
    assert!(!other.entries.is_empty());
    assert!(other.failure.is_none());
}

/// GRS-FR-EYNU, GRS-FR-IQUA: a stream that cannot be read at all is
/// `unavailable` with its own typed code, and never `empty`.
#[test]
fn a_stream_that_cannot_be_read_at_all_is_unavailable() {
    let fx = Fixture::new();
    let run = driven(&fx);
    let path = logs::paths_for(&fx.app, &run.id)
        .expect("the paths")
        .stream(GraduationLogStream::Source);
    // A directory where the stream should be: it exists, and it is no file.
    std::fs::remove_file(&path).expect("the stream");
    std::fs::create_dir(&path).expect("something unreadable in its place");

    let page = read(
        &fx,
        &run,
        "working",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Source,
        None,
        None,
        None,
    );
    assert_eq!(page.status, STATUS_UNAVAILABLE);
    let failure = page.failure.expect("a typed read failure");
    assert_eq!(failure.code, logs::read::CODE_STREAM_UNREADABLE);
    assert_eq!(failure.kind, "read");
}

/// GRS-FR-TYQK: a run this module holds no storage for reads back `empty`
/// rather than as an error.
#[test]
fn a_run_with_no_storage_reads_back_empty() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");
    let paths = logs::paths_for(&fx.app, &run.id).expect("the paths");
    if paths.directory.is_dir() {
        std::fs::remove_dir_all(&paths.directory).expect("no storage at all");
    }

    let page = read(
        &fx,
        &run,
        "queued",
        GraduationLogPassScope::Phase,
        GraduationLogStream::Source,
        None,
        None,
        None,
    );
    assert_eq!(page.status, STATUS_EMPTY);
    assert!(page.failure.is_none());
}
