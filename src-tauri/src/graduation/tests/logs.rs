//! The two durable log streams a run owns
//! (`GRS-graduation-run-log-storage.md`, `GRD-graduation.md` GRD-FR-OSCG).

use super::*;

use crate::graduation::logs::{
    self, GraduationLogLevel, GraduationLogProducer, GraduationLogStream, StructuredEvent,
};

fn field<'a>(record: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    record.get(key).unwrap_or(&serde_json::Value::Null)
}

/// GRS-FR-KDOY, GRD-FR-OSCG: both streams exist from the moment the run does,
/// so a run that has emitted nothing still has two readable ones.
#[test]
fn a_run_owns_two_readable_streams_from_the_moment_it_exists() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Write the panel.");
    logs::initialize(&fx.app, &mut run);

    let paths = logs::paths_for(&fx.app, &run.id).expect("the paths");
    assert!(paths.stream(GraduationLogStream::Activity).is_file());
    assert!(paths.stream(GraduationLogStream::Structured).is_file());
    assert!(run.logs.persistence.is_healthy());
    assert_eq!(run.logs.log_storage_version, 1);
}

/// GRS-FR-KJVN, GRS-FR-URSZ: one run reads back as one sequence, each record
/// attributed by what produced it.
#[test]
fn a_whole_run_reads_back_as_one_attributed_sequence() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::revise(ReviewSeverity::Major, "Not yet."),
            Turn::work().writing("src/panel.ts", "2\n"),
            Turn::ready(),
        ]),
    );
    assert_eq!(run.state, GraduationRunState::Completed);

    let records = lines_of(&fx, &run.id, GraduationLogStream::Structured);
    assert!(records.len() >= 8, "the whole run is there: {}", records.len());

    // GRS-FR-SXNY: the sequence ascends within one run and one stream.
    let sequences: Vec<u64> = records
        .iter()
        .map(|r| field(r, "sequence").as_u64().unwrap_or_default())
        .collect();
    assert_eq!(
        sequences,
        (1..=records.len() as u64).collect::<Vec<u64>>(),
        "sequences ascend without a gap"
    );

    // GRS-FR-MEPZ: every phase id is one of the four, and no other value exists.
    for record in &records {
        let phase = field(record, "phase_id").as_str().unwrap_or_default();
        assert!(
            ["queued", "working", "review", "done"].contains(&phase),
            "a record carries the phase {phase:?}"
        );
        assert_eq!(field(record, "schema_version").as_u64(), Some(1));
        assert_eq!(field(record, "run_id").as_str(), Some(run.id.as_str()));
        assert!(!field(record, "record_id").as_str().unwrap_or_default().is_empty());
    }

    // The attribution table: a work turn is `working` and carries its pass; a
    // review turn is `review` and carries its pass; the queue wait and the
    // commit are run-level.
    let by_producer = |producer: &str| -> Vec<&serde_json::Value> {
        records
            .iter()
            .filter(|r| field(r, "producer").as_str() == Some(producer))
            .collect()
    };
    let work = by_producer("work_turn");
    assert_eq!(work.len(), 4, "two work turns, each started and ended");
    assert!(work.iter().all(|r| field(r, "phase_id").as_str() == Some("working")));
    assert_eq!(field(work[0], "pass").as_u64(), Some(1));
    assert_eq!(field(work[3], "pass").as_u64(), Some(2));

    let review = by_producer("review_turn");
    assert!(review.iter().all(|r| field(r, "phase_id").as_str() == Some("review")));

    let queued = by_producer("queue_wait");
    assert_eq!(queued.len(), 1);
    assert_eq!(field(queued[0], "phase_id").as_str(), Some("queued"));
    assert_eq!(field(queued[0], "pass"), &serde_json::Value::Null);

    let commit = by_producer("commit");
    assert_eq!(commit.len(), 1);
    assert_eq!(field(commit[0], "phase_id").as_str(), Some("done"));
    assert_eq!(field(commit[0], "pass"), &serde_json::Value::Null);
}

/// GRS-FR-WFWD, GRS-FR-MHJM: the index names what the file holds, and is
/// written in the same durable act.
#[test]
fn the_index_names_exactly_what_the_file_holds() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::ready(),
        ]),
    );

    let records = lines_of(&fx, &run.id, GraduationLogStream::Structured);
    let index = run.logs.stream(GraduationLogStream::Structured);
    assert_eq!(index.record_count, records.len() as u64);
    assert_eq!(index.latest_sequence, records.len() as u64);
    assert_eq!(index.durable_through_sequence, index.latest_sequence);

    let path = logs::paths_for(&fx.app, &run.id).unwrap().stream(GraduationLogStream::Structured);
    assert_eq!(
        index.byte_length,
        std::fs::metadata(&path).unwrap().len(),
        "the durable length matches the file"
    );

    // GRS-FR-WFWD: one segment per phase and pass, in first-sequence order.
    assert!(!index.segments.is_empty());
    let mut previous = 0u64;
    let mut counted = 0u64;
    for segment in &index.segments {
        assert!(segment.first_sequence > previous, "segments are in order");
        assert!(segment.last_sequence >= segment.first_sequence);
        previous = segment.last_sequence;
        counted += segment.record_count;
    }
    assert_eq!(counted, index.record_count, "the segments cover every record");
    assert_eq!(index.segments[0].phase_id, "queued");
}

/// GRS-FR-CGSP: the run record holds the indexes and no log payload.
#[test]
fn the_run_record_holds_no_log_payload() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");
    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "the work turn wrote this\n"),
            Turn::ready(),
        ]),
    );

    let record = std::fs::read_to_string(
        crate::graduation::store_base(&fx.app).unwrap().record(&run.id),
    )
    .expect("the run record");
    assert!(
        !record.contains("RAW PAYLOAD") && !record.contains("the scripted turn ran"),
        "no activity text reaches the run record"
    );
    assert!(
        !record.contains("the work turn wrote this"),
        "no payload reaches the run record"
    );
    assert!(record.contains("logStorageVersion"), "but the indexes do");
}

/// GRS-FR-JQOO, GRS-FR-SQGZ, GRS-FR-WJIA, GRS-FR-VZUZ, GXD-FR-IOZU,
/// GLG-FR-AVVZ, GLG-FR-YGCI, AGV-FR-ZSRV, EAC-FR-IRKZ: the durable activity stream stands beside the live panel and
/// holds the safe kinds alone, with no payload.
#[test]
fn the_activity_stream_holds_only_the_safe_activity_the_live_panel_showed() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");
    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::ready(),
        ]),
    );

    let records = lines_of(&fx, &run.id, GraduationLogStream::Activity);
    assert_eq!(records.len(), 4, "a message and a finished line per turn");
    let kinds: Vec<&str> = records.iter().filter_map(|r| field(r, "kind").as_str()).collect();
    assert_eq!(kinds, vec!["message", "finished", "message", "finished"]);
    for record in &records {
        let mut keys: Vec<&str> = record.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "at", "channel", "kind", "origin", "pass", "phase_id", "producer", "record_id",
                "run_id", "schema_version", "sequence", "summary"
            ],
            "a record holds the attribution and three fields, and no payload"
        );
    }
    let text = std::fs::read_to_string(
        logs::paths_for(&fx.app, &run.id)
            .unwrap()
            .stream(GraduationLogStream::Activity),
    )
    .unwrap();
    for excluded in ["RAW PAYLOAD", "the-secret-argument-vector", "the whole task document"] {
        assert!(!text.contains(excluded), "{excluded} is never persisted");
    }
    assert_eq!(field(&records[0], "producer").as_str(), Some("work_turn"));
    assert_eq!(field(&records[0], "origin").as_str(), Some("agent"));
    assert_eq!(field(&records[1], "origin").as_str(), Some("executor"));
    assert_eq!(field(&records[2], "producer").as_str(), Some("review_turn"));
    assert_eq!(field(&records[2], "phase_id").as_str(), Some("review"));
    assert_eq!(field(&records[0], "pass").as_u64(), Some(1));

    // AGV: the in-memory panel is a separate stream and still holds every kind.
    let store = fx
        .app
        .state::<std::sync::Arc<crate::agent_activity::ActivityStore>>()
        .inner()
        .clone();
    let live = store.read(&run.id, None, None).records;
    assert_eq!(live.len(), 8);
    assert!(live.iter().any(|record| record.kind == "invocation"));
}

/// GRS-FR-EIXS, GRS-FR-OVCO, GRD-FR-IKVE: a stream that cannot be written stops
/// the run before it does anything else, and Continue retries the pending writes
/// first, resuming the same pass.
#[test]
fn a_stream_that_cannot_be_written_rests_the_run_until_continue_repairs_it() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    // A directory standing where the stream's file belongs: an append to it
    // fails the way a full disk or a lost volume does.
    let path = logs::paths_for(&fx.app, &run.id)
        .unwrap()
        .stream(GraduationLogStream::Structured);
    std::fs::create_dir_all(&path).expect("the obstruction");

    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::Interrupted);
    assert_eq!(
        run.interruption.as_ref().map(|i| i.reason),
        Some(GraduationInterruptionReason::LogPersistenceFailed)
    );
    assert_eq!(
        dispatch.turns_taken(),
        0,
        "no agent action happens after a log stream fails"
    );
    assert!(!run.logs.persistence.is_healthy());
    let failure = run.logs.persistence.failure.as_ref().expect("the failure");
    assert_eq!(failure.kind, "write");
    assert_eq!(failure.stream, GraduationLogStream::Structured);
    assert!(
        !failure.pending_record_ids.is_empty(),
        "the records still to be written are named"
    );

    // GRD-FR-IKVE: Continue retries the pending writes first, and is refused
    // while they still cannot be written.
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    assert!(
        crate::graduation::continue_graduation_run(fx.app.clone(), run.id.clone()).is_err(),
        "Continue does not move a run whose logs still cannot be written"
    );

    // The obstruction goes, and Continue then replays what was pending.
    std::fs::remove_dir_all(&path).expect("clear the obstruction");
    let continued = crate::graduation::continue_graduation_run(fx.app.clone(), run.id.clone())
        .expect("Continue succeeds once the stream can be written");
    fx.app.state::<GraduationState>().set_loop_enabled(true);

    assert_eq!(continued.state, GraduationRunState::Queued);
    assert!(continued.logs.persistence.is_healthy());
    assert_eq!(continued.logs.persistence.pending_count, 0);

    // GRS-FR-MXDJ: the replay appended the pending record, once.
    let records = lines_of(&fx, &run.id, GraduationLogStream::Structured);
    assert_eq!(records.len(), 1, "the record that was pending is now there");
    let mut ids: Vec<&str> = records
        .iter()
        .filter_map(|r| field(r, "record_id").as_str())
        .collect();
    let held = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), held, "and no record was appended twice");
}

/// GRS-FR-MQEQ: the tail of a write the process did not finish is cut before
/// anything further is appended. It was never acknowledged, so nothing is lost.
#[test]
fn a_partial_final_line_is_cut_before_the_next_append() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Write the panel.");
    logs::initialize(&fx.app, &mut run);

    let path = logs::paths_for(&fx.app, &run.id)
        .unwrap()
        .stream(GraduationLogStream::Structured);
    std::fs::write(&path, "{\"record_id\":\"whole\",\"sequence\":1}\n{\"record_id\":\"cut off\"")
        .expect("a file with a partial tail");

    assert!(logs::emit(
        &fx.app,
        &mut run,
        StructuredEvent::new(GraduationLogProducer::QueueWait, "after the repair"),
    ));

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        !text.contains("cut off"),
        "the partial record is gone: {text}"
    );
    let records = lines_of(&fx, &run.id, GraduationLogStream::Structured);
    assert_eq!(records.len(), 2, "the whole record stands, and the new one");
    assert_eq!(
        field(&records[1], "event").as_str(),
        Some("after the repair")
    );
    // The record the file already held keeps its sequence, and the new one takes
    // the next: an index seeded from a stale record must not reuse a number the
    // file has already written.
    let sequences: Vec<u64> = records
        .iter()
        .map(|r| field(r, "sequence").as_u64().unwrap_or_default())
        .collect();
    assert_eq!(sequences, vec![1, 2]);
    assert_eq!(run.logs.stream(GraduationLogStream::Structured).record_count, 2);
    assert!(run.logs.persistence.is_healthy());
}

/// GRS-FR-SXNY, GRS-FR-MHJM: a run whose record was saved before the file was
/// written does not reuse the sequences the file already holds.
///
/// The record is saved at turn boundaries and a turn may run for hours, so a
/// process stopped inside one leaves exactly this disagreement behind.
#[test]
fn an_index_behind_its_file_is_brought_up_to_it_rather_than_reusing_sequences() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Write the panel.");
    logs::initialize(&fx.app, &mut run);

    // Three records the file holds, and a record that names none of them: what a
    // process killed inside a turn leaves.
    let path = logs::paths_for(&fx.app, &run.id)
        .unwrap()
        .stream(GraduationLogStream::Structured);
    let existing: String = (1..=3)
        .map(|sequence| {
            format!(
                "{{\"schema_version\":1,\"record_id\":\"r{sequence}\",\"sequence\":{sequence},\"phase_id\":\"working\",\"pass\":1}}\n"
            )
        })
        .collect();
    std::fs::write(&path, existing).expect("a file the record does not name");
    assert_eq!(run.logs.stream(GraduationLogStream::Structured).latest_sequence, 0);

    assert!(logs::emit(
        &fx.app,
        &mut run,
        StructuredEvent::new(GraduationLogProducer::QueueWait, "after the relaunch"),
    ));

    let records = lines_of(&fx, &run.id, GraduationLogStream::Structured);
    let sequences: Vec<u64> = records
        .iter()
        .map(|r| field(r, "sequence").as_u64().unwrap_or_default())
        .collect();
    assert_eq!(
        sequences,
        vec![1, 2, 3, 4],
        "the new record takes the next sequence, not one the file already holds"
    );
    let index = run.logs.stream(GraduationLogStream::Structured);
    assert_eq!(index.record_count, 4, "and the index names every record");
    assert_eq!(index.latest_sequence, 4);
    assert_eq!(
        index.byte_length,
        std::fs::metadata(&path).unwrap().len(),
        "and the file's real length"
    );
}

/// GRS-FR-NPIB: a structured record carries the level and the fields a surface
/// renders, and nothing it was not given.
#[test]
fn a_structured_record_carries_its_level_and_its_fields() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Write the panel.");

    assert!(logs::emit(
        &fx.app,
        &mut run,
        StructuredEvent::new(GraduationLogProducer::StageTransition, "run blocked")
            .at_level(GraduationLogLevel::Error)
            .leaving(GraduationVisualStage::Working)
            .with("code", "commit_failed"),
    ));

    let records = lines_of(&fx, &run.id, GraduationLogStream::Structured);
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(field(record, "level").as_str(), Some("error"));
    assert_eq!(field(record, "event").as_str(), Some("run blocked"));
    assert_eq!(field(record, "producer").as_str(), Some("stage_transition"));
    // GRS-FR-EPPM: a stage transition carries the phase being left.
    assert_eq!(field(record, "phase_id").as_str(), Some("working"));
    assert_eq!(field(record, "pass"), &serde_json::Value::Null);
    assert_eq!(
        field(record, "fields").get("code").and_then(|v| v.as_str()),
        Some("commit_failed")
    );
}

/// GRS-FR-SXNY: the activity stream's sequence ascends within the run, and the
/// index the run record holds names what the file holds.
#[test]
fn the_activity_streams_sequence_ascends_and_its_index_is_kept() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");
    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::revise(ReviewSeverity::Major, "Not yet."),
            Turn::work().writing("src/panel.ts", "2\n"),
            Turn::ready(),
        ]),
    );

    let chunks = lines_of(&fx, &run.id, GraduationLogStream::Activity);
    let sequences: Vec<u64> = chunks
        .iter()
        .map(|c| field(c, "sequence").as_u64().unwrap_or_default())
        .collect();
    assert_eq!(
        sequences,
        (1..=chunks.len() as u64).collect::<Vec<u64>>(),
        "every chunk has its own ascending sequence"
    );
    let index = run.logs.stream(GraduationLogStream::Activity);
    assert_eq!(index.record_count, chunks.len() as u64);
    assert_eq!(index.latest_sequence, chunks.len() as u64);
}

/// GRS-FR-EIXS, GRS-FR-ITWJ, GLG-FR-UCRL, GXD-FR-MMFM: the **activity** stream
/// is mandatory too. An activity the sink could not store stops the turn and
/// rests the run before it takes another turn.
#[test]
fn an_activity_stream_that_cannot_be_written_also_rests_the_run() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let path = logs::paths_for(&fx.app, &run.id)
        .unwrap()
        .stream(GraduationLogStream::Activity);
    std::fs::create_dir_all(&path).expect("the obstruction");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::Interrupted);
    assert_eq!(
        run.interruption.as_ref().map(|i| i.reason),
        Some(GraduationInterruptionReason::LogPersistenceFailed)
    );
    assert_eq!(
        run.logs.persistence.failure.as_ref().map(|f| f.stream),
        Some(GraduationLogStream::Activity),
        "the run names the stream that stopped"
    );
    assert_eq!(
        dispatch.of_part("review").len(),
        0,
        "no review turn is taken after the activity stream failed"
    );
}

/// GRD-FR-NHRY, GRD-FR-KRTU, GXD-FR-CYIW: every agent operation a run performs
/// is counted against the run's source draft, one line per turn.
#[test]
fn every_turn_is_counted_against_the_runs_source_draft() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let draft_id = fx.draft("the panel");
    let run = fx.enqueue_for(&stream, "Write the panel.", &draft_id);

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::ready(),
        ]),
    );
    // The append is asynchronous and never blocks a run (DSS-FR-TUMX), so the
    // test waits for the writer rather than for a duration.
    crate::statistics::wait_for_writer();

    let path = fx
        .root()
        .join("statistics")
        .join(format!("{}.jsonl", run.input.draft_id));
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let operations: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|v| field(v, "type").as_str() == Some("agent_operation"))
        .collect();

    assert_eq!(operations.len(), 2, "one line per turn: {text}");
    for operation in &operations {
        assert_eq!(field(operation, "runId").as_str(), Some(run.id.as_str()));
        let started = field(operation, "startedAt").as_str().unwrap_or_default();
        let ended = field(operation, "endedAt").as_str().unwrap_or_default();
        assert!(!started.is_empty() && !ended.is_empty());
        assert!(started <= ended, "the interval runs forwards");
    }
    // GRD-FR-NHRY: the work turn and the review turn are counted apart.
    let buckets: Vec<&str> = operations
        .iter()
        .filter_map(|o| field(o, "bucket").as_str())
        .collect();
    assert_eq!(buckets.len(), 2);
    assert_ne!(buckets[0], buckets[1], "a work turn and a review turn differ");
    // Nothing is estimated: the CLI executor reports no usage, so no token line
    // is written at all rather than one holding zeros.
    assert!(
        !text.lines().any(|line| line.contains("token_usage")),
        "a direction the provider did not report is absent rather than zero"
    );
}

/// GRS-FR-KQHY, GRS-FR-MXDJ: a pending queue holding records of both streams
/// replays each into its own stream, exactly once.
#[test]
fn a_pending_queue_of_both_streams_replays_each_into_its_own() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    // Both streams obstructed, so both a structured record and an activity record
    // fail to land and are queued.
    let paths = logs::paths_for(&fx.app, &run.id).unwrap();
    std::fs::create_dir_all(paths.stream(GraduationLogStream::Structured)).unwrap();
    std::fs::create_dir_all(paths.stream(GraduationLogStream::Activity)).unwrap();

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]),
    );
    assert_eq!(run.state, GraduationRunState::Interrupted);

    // A queue holding records for one stream that still cannot be written is
    // not cleared by the other one succeeding.
    std::fs::remove_dir_all(paths.stream(GraduationLogStream::Structured)).unwrap();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    assert!(
        crate::graduation::continue_graduation_run(fx.app.clone(), run.id.clone()).is_err()
            || crate::graduation::load_run(&fx.app, &run.id)
                .unwrap()
                .logs
                .persistence
                .is_healthy(),
        "Continue either repairs both streams or refuses"
    );

    std::fs::remove_dir_all(paths.stream(GraduationLogStream::Activity)).ok();
    let continued =
        crate::graduation::continue_graduation_run(fx.app.clone(), run.id.clone()).expect("continued");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    assert!(continued.logs.persistence.is_healthy());
    assert!(!paths.pending().exists(), "the queue is cleared once it is empty");

    // Every record landed in its own stream, once.
    for stream in [GraduationLogStream::Structured, GraduationLogStream::Activity] {
        let records = lines_of(&fx, &run.id, stream);
        let mut ids: Vec<&str> = records
            .iter()
            .filter_map(|r| field(r, "record_id").as_str())
            .collect();
        let held = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), held, "{stream:?} holds no record twice");
    }
}

/// GRS-FR-MQEQ: the activity stream's partial final line is cut too, and its
/// sequence continues rather than restarting.
#[test]
fn a_partial_final_line_in_the_activity_stream_is_cut_as_well() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Write the panel.");
    logs::initialize(&fx.app, &mut run);

    let path = logs::paths_for(&fx.app, &run.id)
        .unwrap()
        .stream(GraduationLogStream::Activity);
    std::fs::write(
        &path,
        "{\"schema_version\":1,\"record_id\":\"whole\",\"sequence\":1,\"phase_id\":\"working\",\"pass\":1}\n{\"record_id\":\"cut",
    )
    .expect("a file with a partial tail");

    let activity = crate::tools::agent_exec::DurableActivity {
        record_id: "after-the-repair".to_string(),
        at: crate::notes::now_rfc3339(),
        channel: "stdout",
        kind: "message",
        summary: "after the repair".to_string(),
    };
    logs::append_activity_records(
        &fx.app,
        &run.id,
        run.logs.clone(),
        vec![logs::activity_record(
            &run.id,
            GraduationLogProducer::WorkTurn,
            Some(run.pass()),
            &activity,
        )
        .expect("a safe kind")],
    )
    .expect("appended");

    let records = lines_of(&fx, &run.id, GraduationLogStream::Activity);
    assert_eq!(records.len(), 2);
    let sequences: Vec<u64> = records
        .iter()
        .map(|r| field(r, "sequence").as_u64().unwrap_or_default())
        .collect();
    assert_eq!(sequences, vec![1, 2], "the whole record keeps its sequence");
    assert!(!std::fs::read_to_string(&path).unwrap().contains("\"cut"));
}

/// GRS-FR-OVCO: a run that rested on a log failure inside its second pass
/// resumes that pass, and replays no record the file already holds.
#[test]
fn a_run_that_rested_on_a_log_failure_resumes_the_pass_it_was_in() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");
    let app = fx.app.clone();
    let run_id = run.id.clone();

    // The first pass runs whole; the stream is obstructed inside the second.
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        Turn::revise(ReviewSeverity::Major, "Not yet."),
        Turn::work().writing("src/panel.ts", "2\n").doing(move || {
            let path = logs::paths_for(&app, &run_id)
                .unwrap()
                .stream(GraduationLogStream::Structured);
            let held = std::fs::read_to_string(&path).unwrap_or_default();
            std::fs::remove_file(&path).ok();
            std::fs::create_dir_all(&path).unwrap();
            // What the file held before the obstruction is kept, so the replay
            // has something to be idempotent against.
            std::fs::write(path.join("held"), held).ok();
        }),
    ]);
    let run = fx.drive(&run, dispatch);

    assert_eq!(run.state, GraduationRunState::Interrupted);
    assert_eq!(
        run.interruption.as_ref().map(|i| i.reason),
        Some(GraduationInterruptionReason::LogPersistenceFailed)
    );
    assert_eq!(run.pass(), 2, "the run rested inside its second pass");

    // The obstruction goes, and Continue replays what was pending and resumes.
    let path = logs::paths_for(&fx.app, &run.id)
        .unwrap()
        .stream(GraduationLogStream::Structured);
    let held = std::fs::read_to_string(path.join("held")).unwrap_or_default();
    std::fs::remove_dir_all(&path).unwrap();
    std::fs::write(&path, held).unwrap();

    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let continued =
        crate::graduation::continue_graduation_run(fx.app.clone(), run.id.clone()).expect("continued");
    fx.app.state::<GraduationState>().set_loop_enabled(true);

    assert!(continued.logs.persistence.is_healthy());
    assert_eq!(continued.pass(), 2, "it resumes the pass it was in");

    let records = lines_of(&fx, &run.id, GraduationLogStream::Structured);
    let mut ids: Vec<&str> = records
        .iter()
        .filter_map(|r| field(r, "record_id").as_str())
        .collect();
    let held_count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), held_count, "no already-persisted record is replayed");
    let sequences: Vec<u64> = records
        .iter()
        .map(|r| field(r, "sequence").as_u64().unwrap_or_default())
        .collect();
    assert_eq!(
        sequences,
        (1..=records.len() as u64).collect::<Vec<u64>>(),
        "and the sequence still ascends without a gap or a repeat"
    );
}
