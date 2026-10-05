//! A read of a run this process writes answers with the live log indexes
//! (`GRS-graduation-run-log-storage.md` GRS-FR-MHJM, GRS-FR-FCRC, GRS-FR-IOHF,
//! GRS-FR-ZTCF).
//!
//! The run record is saved at turn boundaries, so inside a turn its saved copy
//! names fewer records than the files hold. What `run.toml` holds is read here
//! straight from the store, because every other read now answers with the live
//! indexes.

use super::*;

use crate::graduation::logs::{
    self, GraduationLogIndexes, GraduationLogPage, GraduationLogProducer, GraduationLogStream,
    StructuredEvent,
};

/// What `run.toml` holds, without the live indexes over it.
fn saved(fx: &Fixture, run_id: &str) -> GraduationRun {
    let fs = crate::graduation::store_fs(&fx.app).expect("a store");
    let base = crate::graduation::store_base(&fx.app).expect("a store");
    crate::graduation::store::read_run_record(&fs, &base, run_id).expect("the saved record")
}

/// Whether a stream's index names a phase and a pass.
fn names(logs: &GraduationLogIndexes, stream: GraduationLogStream, phase: &str, pass: Option<u32>) -> bool {
    logs.stream(stream)
        .segments
        .iter()
        .any(|segment| segment.phase_id == phase && segment.pass == pass)
}

/// Write one work-turn chunk per text to the source stream, as the executor does.
fn work_output(fx: &Fixture, run: &GraduationRun, texts: &[&str]) {
    logs::append_source_chunks(
        &fx.app,
        &run.id,
        run.logs.clone(),
        texts
            .iter()
            .map(|text| logs::source_chunk(run, GraduationLogProducer::WorkTurn, "stdout", text))
            .collect(),
    );
}

/// Call `read_graduation_logs` as the frontend does.
fn read(fx: &Fixture, run_id: &str, scope: serde_json::Value, stream: &str) -> GraduationLogPage {
    crate::graduation::read_graduation_logs(
        fx.app.clone(),
        run_id.to_string(),
        "working".to_string(),
        scope,
        stream.to_string(),
        None,
        Some(1000),
        None,
    )
    .expect("a page")
}

fn sequences(page: &GraduationLogPage) -> Vec<u64> {
    page.entries
        .iter()
        .map(|entry| entry.record.get("sequence").and_then(|v| v.as_u64()).unwrap_or_default())
        .collect()
}

/// A run at work, its streams created, saved with nothing written yet.
fn working_run(fx: &Fixture) -> GraduationRun {
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Write the panel.");
    logs::initialize(&fx.app, &mut run);
    run.state = GraduationRunState::Working;
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");
    run
}

// GRS-FR-ZTCF, GRS-FR-FCRC, GRU-FR-TQJW: inside a turn, the run record and the
// listing name the stage the turn writes, though the saved copy does not yet.
#[test]
fn the_run_record_and_the_listing_name_the_output_of_a_turn_that_is_still_running() {
    let fx = Fixture::new();
    let run = working_run(&fx);
    work_output(&fx, &run, &["first", "second"]);

    let on_disk = saved(&fx, &run.id);
    assert!(
        !names(&on_disk.logs, GraduationLogStream::Source, "working", Some(run.pass())),
        "the saved copy is behind the files inside a turn"
    );

    let got = crate::graduation::get_graduation_run(fx.app.clone(), run.id.clone()).expect("the run");
    assert_eq!(got.logs.source.record_count, 2);
    assert_eq!(got.logs.source.latest_sequence, 2);
    assert!(names(&got.logs, GraduationLogStream::Source, "working", Some(run.pass())));

    // The queue itself rather than the command: the command first sweeps a
    // working run no loop drives, and the sweep saves it.
    let listed = crate::graduation::project_queue(&fx.app).expect("the queue");
    let entry = listed
        .runs
        .iter()
        .find(|entry| entry.id == run.id)
        .expect("the run is listed");
    assert_eq!(entry.logs, got.logs, "the listing and the record report one index");
    assert_eq!(
        saved(&fx, &run.id).logs.source.record_count,
        0,
        "both answers came from the live indexes and not from a save"
    );
}

// GRS-FR-ZTCF, GRS-FR-MHJM, GRS-FR-WFWD: a read bounded by a saved segment of
// the same phase and pass still returns what the turn wrote after the save.
#[test]
fn a_log_read_returns_records_written_after_the_last_save() {
    let fx = Fixture::new();
    let mut run = working_run(&fx);
    work_output(&fx, &run, &["first"]);
    logs::refresh(&fx.app, &mut run);
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");
    work_output(&fx, &run, &["second", "third"]);

    let on_disk = saved(&fx, &run.id).logs;
    assert_eq!(on_disk.source.record_count, 1, "the save saw one record");
    assert!(names(&on_disk, GraduationLogStream::Source, "working", Some(run.pass())));

    for scope in [
        serde_json::json!({ "kind": "pass", "pass": run.pass() }),
        serde_json::json!({ "kind": "phase" }),
    ] {
        let page = read(&fx, &run.id, scope.clone(), "source");
        assert_eq!(sequences(&page), vec![1, 2, 3], "scope {scope} reads the whole turn");
        assert_eq!(page.latest_sequence, 3);
    }
}

// GRS-FR-ZTCF, GRS-FR-FCRC: a command that loads and saves a run inside a turn
// writes the live indexes, never an older copy over them.
#[test]
fn a_save_inside_a_turn_writes_the_live_indexes() {
    let fx = Fixture::new();
    let run = working_run(&fx);
    work_output(&fx, &run, &["first", "second"]);

    let mut loaded = crate::graduation::load_run(&fx.app, &run.id).expect("the run");
    crate::graduation::save_run(&fx.app, &mut loaded).expect("saved");

    assert_eq!(saved(&fx, &run.id).logs.source.record_count, 2);
}

// GRS-FR-ZTCF, GRS-FR-IOHF: a run this process does not write is read with its
// saved copy, and the read creates no live index that a later append would
// number from.
#[test]
fn a_read_of_a_run_nobody_writes_answers_with_the_saved_copy_and_holds_nothing() {
    let fx = Fixture::new();
    // A queued run, so the listing's sweep of abandoned working runs (which
    // reconciles them) leaves it alone.
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Write the panel.");
    logs::initialize(&fx.app, &mut run);
    assert!(logs::emit(
        &fx.app,
        &mut run,
        StructuredEvent::new(GraduationLogProducer::WorkTurn, "work turn started"),
    ));
    logs::refresh(&fx.app, &mut run);
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");
    logs::release(&fx.app, &run.id);

    // Two records a stopped process wrote after its last save.
    let path = logs::paths_for(&fx.app, &run.id)
        .expect("the paths")
        .stream(GraduationLogStream::Structured);
    let mut held = std::fs::read_to_string(&path).expect("the stream");
    for sequence in [2, 3] {
        held.push_str(&format!(
            "{{\"schema_version\":1,\"record_id\":\"raw-{sequence}\",\"sequence\":{sequence},\"phase_id\":\"working\",\"pass\":1}}\n"
        ));
    }
    std::fs::write(&path, held).expect("written");

    let got = crate::graduation::get_graduation_run(fx.app.clone(), run.id.clone()).expect("the run");
    assert_eq!(got.logs.structured.record_count, 1, "the saved copy, as saved");
    let listed = crate::graduation::list_graduation_queue(fx.app.clone()).expect("the queue");
    assert_eq!(
        listed.runs.iter().find(|entry| entry.id == run.id).map(|e| e.logs.structured.record_count),
        Some(1)
    );
    let _ = read(&fx, &run.id, serde_json::json!({ "kind": "phase" }), "structured");

    // Had a read seeded a live index from the saved copy, this append would
    // reuse sequence 2. Seeded first by the append, it reconciles with the file.
    assert!(logs::emit(
        &fx.app,
        &mut got.clone(),
        StructuredEvent::new(GraduationLogProducer::WorkTurn, "work turn ended"),
    ));
    let written: Vec<u64> = lines_of(&fx, &run.id, GraduationLogStream::Structured)
        .iter()
        .map(|record| record.get("sequence").and_then(|v| v.as_u64()).unwrap_or_default())
        .collect();
    assert_eq!(written, vec![1, 2, 3, 4]);
}

// GRS-FR-IOHF, GRU-FR-TQJW, GRD-FR-XVUD: a run the application stopped inside a
// turn is swept with its saved copy brought up to its files, so the stage of
// that turn opens.
#[test]
fn a_swept_run_names_what_its_stopped_turn_wrote() {
    let fx = Fixture::new();
    let mut run = working_run(&fx);
    work_output(&fx, &run, &["one", "two", "three"]);
    assert!(logs::emit(
        &fx.app,
        &mut run,
        StructuredEvent::new(GraduationLogProducer::WorkTurn, "work turn started"),
    ));
    // The process that wrote them is gone.
    logs::release(&fx.app, &run.id);
    let before = saved(&fx, &run.id).logs;
    assert_eq!(before.source.record_count, 0);
    assert_eq!(before.structured.record_count, 0);

    let queue = crate::graduation::project_queue(&fx.app).expect("the queue");
    crate::graduation::sweep_abandoned_runs(&fx.app, &queue);

    let swept = saved(&fx, &run.id);
    assert_eq!(swept.state, GraduationRunState::Interrupted);
    assert_eq!(
        swept.interruption.as_ref().map(|i| i.reason),
        Some(GraduationInterruptionReason::ExecutionAbandoned)
    );
    let source = &swept.logs.source;
    assert_eq!(source.record_count, 3);
    assert_eq!(source.latest_sequence, 3);
    assert_eq!(source.durable_through_sequence, 3);
    let path = logs::paths_for(&fx.app, &run.id)
        .expect("the paths")
        .stream(GraduationLogStream::Source);
    assert_eq!(
        source.byte_length,
        std::fs::metadata(&path).expect("the stream").len()
    );
    assert!(names(&swept.logs, GraduationLogStream::Source, "working", Some(1)));
    assert_eq!(
        swept.logs.structured.record_count as usize,
        lines_of(&fx, &run.id, GraduationLogStream::Structured).len()
    );
    assert_eq!(swept.logs.structured.record_count, 1);
    assert!(names(&swept.logs, GraduationLogStream::Structured, "working", Some(1)));
}

/// The source sequences the file holds, in file order.
fn source_sequences(fx: &Fixture, run_id: &str) -> Vec<u64> {
    lines_of(fx, run_id, GraduationLogStream::Source)
        .iter()
        .map(|record| record.get("sequence").and_then(|v| v.as_u64()).unwrap_or_default())
        .collect()
}

// GRS-FR-IOHF, GRS-FR-SXNY: when bringing a record up to the shared indexes is
// the first use of a run's indexes in a process, the seed is reconciled with
// the files, and the next append does not reuse a sequence.
#[test]
fn a_refresh_that_seeds_the_indexes_reconciles_them_with_the_files() {
    let fx = Fixture::new();
    let run = working_run(&fx);
    work_output(&fx, &run, &["one", "two", "three"]);
    logs::release(&fx.app, &run.id);

    let mut loaded = crate::graduation::load_run(&fx.app, &run.id).expect("the run");
    logs::refresh(&fx.app, &mut loaded);
    assert_eq!(loaded.logs.source.record_count, 3);
    work_output(&fx, &loaded, &["four"]);

    assert_eq!(source_sequences(&fx, &run.id), vec![1, 2, 3, 4]);
}

// GRS-FR-IOHF, GRS-FR-OVCO: a Continue in a new process that retries a run's
// pending writes first reconciles its indexes with the files.
#[test]
fn a_retry_that_seeds_the_indexes_reconciles_them_with_the_files() {
    let fx = Fixture::new();
    let run = working_run(&fx);
    work_output(&fx, &run, &["one", "two", "three"]);
    logs::release(&fx.app, &run.id);

    let mut loaded = crate::graduation::load_run(&fx.app, &run.id).expect("the run");
    loaded.logs.note_failed(
        logs::GraduationLogFailure::write(
            logs::index::CODE_APPEND_FAILED,
            GraduationLogStream::Source,
            "The stream could not be written.",
            Vec::new(),
        ),
        0,
    );
    assert!(logs::retry_pending(&fx.app, &mut loaded), "nothing is pending, so it is healthy");
    assert_eq!(loaded.logs.source.record_count, 3);
    work_output(&fx, &loaded, &["four"]);

    assert_eq!(source_sequences(&fx, &run.id), vec![1, 2, 3, 4]);
}

// GRS-FR-MHJM, GRS-FR-ZTCF: inside a real work turn, the record and the listing
// name the working stage from its first record, and the turn boundary still
// saves what the files hold.
#[test]
fn a_driven_work_turn_is_readable_before_it_ends() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");
    let app = fx.app.clone();
    let run_id = run.id.clone();
    let seen: Arc<Mutex<Option<(GraduationLogIndexes, GraduationLogIndexes, GraduationLogIndexes)>>> =
        Arc::new(Mutex::new(None));
    let into = seen.clone();

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n").doing(move || {
            let fs = crate::graduation::store_fs(&app).expect("a store");
            let base = crate::graduation::store_base(&app).expect("a store");
            let on_disk = crate::graduation::store::read_run_record(&fs, &base, &run_id)
                .expect("the saved record")
                .logs;
            let got = crate::graduation::get_graduation_run(app.clone(), run_id.clone())
                .expect("the run")
                .logs;
            let listed = crate::graduation::list_graduation_queue(app.clone())
                .expect("the queue")
                .runs
                .into_iter()
                .find(|entry| entry.id == run_id)
                .expect("listed")
                .logs;
            *into.lock().unwrap() = Some((on_disk, got, listed));
        }),
        Turn::ready(),
    ]);
    let driven = fx.drive(&run, dispatch);

    let (on_disk, got, listed) = seen.lock().unwrap().take().expect("the turn ran");
    assert!(
        !names(&on_disk, GraduationLogStream::Structured, "working", Some(1)),
        "the saved copy is behind inside the turn"
    );
    assert!(names(&got, GraduationLogStream::Structured, "working", Some(1)));
    assert!(names(&listed, GraduationLogStream::Structured, "working", Some(1)));

    let after = saved(&fx, &driven.id).logs;
    assert_eq!(
        after.structured.record_count as usize,
        lines_of(&fx, &driven.id, GraduationLogStream::Structured).len(),
        "the turn boundary saves what the files hold"
    );
    assert_eq!(
        after.source.record_count as usize,
        lines_of(&fx, &driven.id, GraduationLogStream::Source).len()
    );
}

// GRS-FR-ZTCF, GRS-FR-EYNU: the live indexes replace the stream indexes of the
// saved copy and keep the damaged read it records, which only a read writes.
#[test]
fn a_read_of_a_run_inside_a_turn_keeps_the_damaged_read_the_saved_copy_records() {
    let fx = Fixture::new();
    let mut run = working_run(&fx);
    let damaged = logs::GraduationLogFailure {
        kind: "read".to_string(),
        code: logs::index::CODE_STREAM_CORRUPT.to_string(),
        stream: GraduationLogStream::Source,
        message: "Repair the stream file.".to_string(),
        at: crate::notes::now_rfc3339(),
        stopped_sequence: Some(0),
        byte_offset: Some(0),
        pending_record_ids: Vec::new(),
    };
    run.logs.last_read_failure = Some(damaged.clone());
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");
    work_output(&fx, &run, &["first"]);

    let got = crate::graduation::get_graduation_run(fx.app.clone(), run.id.clone()).expect("the run");
    assert_eq!(got.logs.source.record_count, 1, "the live stream index");
    assert_eq!(got.logs.last_read_failure, Some(damaged), "the saved damaged read");
}
