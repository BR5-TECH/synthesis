//! How a stream's row names its merge run, how the run order index carries the
//! facts that row needs, and what a live merge run protects
//! (`WKS-work-streams.md` WKS-FR-KHJS, WKS-FR-SGCM, WKS-FR-OVLQ, WKS-FR-WULF,
//! `GRD-graduation.md` GRD-FR-OYPY).

use std::sync::{Arc, Mutex};

use tauri::Listener;

use super::merge_support::*;
use super::*;
use crate::graduation::{GraduationRun, GraduationRunState as State};

/// A merge run of the stream, enqueued as the handoff enqueues one.
fn merge_run_of(fx: &Fixture, stream: &WorkStream) -> GraduationRun {
    let repo = fx.repo();
    let tip = tip_text(&repo, &stream.branch);
    crate::graduation::enqueue_merge_run(
        &fx.app,
        crate::graduation::MergeRunRequest {
            run_id: crate::graduation::new_run_id(),
            project_key: fx.app.state::<crate::project::ProjectState>().slot_key(),
            stream_id: stream.id.clone(),
            stream_name: stream.name.clone(),
            stream_branch: stream.branch.clone(),
            base_branch: stream.base_branch.clone(),
            base_tip: tip_text(&repo, &stream.base_branch),
            stream_tip: tip.clone(),
            merge_base: tip.clone(),
            snapshot_commit: tip,
            publication: StreamMergePublication::Uncommitted,
            changed_paths: vec!["README.md".into()],
            unresolved_paths: vec!["README.md".into()],
            conflicts: Vec::new(),
        },
    )
    .expect("a merge run")
}

fn resave(fx: &Fixture, run: &mut GraduationRun, change: impl FnOnce(&mut GraduationRun)) {
    change(run);
    crate::graduation::save_run(&fx.app, run).expect("saved");
}

// WKS-FR-KHJS, WKS-FR-SGCM: a stream's summary names its merge run with the
// run id, the title `Merge <stream name>` and the run state, and carries no
// `merge` field.
#[test]
fn the_summary_names_the_streams_merge_run() {
    let fx = Fixture::new();
    let stream = fx.create("linked", None).expect("created");
    assert!(summary_of(&fx, &stream.id).merge_run.is_none());
    let run = merge_run_of(&fx, &stream);

    let summary = summary_of(&fx, &stream.id);

    let link = summary.merge_run.clone().expect("a merge run link");
    assert_eq!(link.run_id, run.id);
    assert_eq!(link.name, "Merge linked");
    assert_eq!(link.state, State::Queued);
    let json = serde_json::to_value(&summary).unwrap();
    assert_eq!(
        json["mergeRun"],
        serde_json::json!({ "runId": run.id, "name": "Merge linked", "state": "queued" })
    );
    assert!(json.get("merge").is_none(), "the old record field is gone: {json}");
}

// WKS-FR-KHJS: a stream with no merge run carries no `mergeRun` value, and a
// run that is not a merge run is never offered as one.
#[test]
fn an_ordinary_run_is_not_a_merge_run_link() {
    let fx = Fixture::new();
    let stream = fx.create("plain", None).expect("created");
    save_plain_run(&fx, &stream, State::Queued);

    let summary = summary_of(&fx, &stream.id);

    assert!(summary.merge_run.is_none());
    let json = serde_json::to_value(&summary).unwrap();
    assert!(json.get("mergeRun").is_none(), "{json}");
}

// WKS-FR-KHJS: the newest merge run that is not discarded and not archived is
// the one the row names; a completed one is named, a discarded or archived one
// is not.
#[test]
fn the_row_names_the_newest_merge_run_that_is_neither_discarded_nor_archived() {
    let fx = Fixture::new();
    let stream = fx.create("history", None).expect("created");
    let mut older = merge_run_of(&fx, &stream);
    resave(&fx, &mut older, |run| run.state = State::Completed);
    let mut newer = merge_run_of(&fx, &stream);

    let named = |fx: &Fixture| summary_of(fx, &stream.id).merge_run.map(|l| (l.run_id, l.state));
    assert_eq!(named(&fx), Some((newer.id.clone(), State::Queued)));

    resave(&fx, &mut newer, |run| run.state = State::Discarded);
    assert_eq!(
        named(&fx),
        Some((older.id.clone(), State::Completed)),
        "a discarded run is skipped"
    );

    resave(&fx, &mut older, |run| {
        run.archived = true;
        run.archived_at = Some("t1".into());
    });
    assert_eq!(named(&fx), None, "an archived run is skipped too");
}

// WKS-FR-KHJS, WKS-FR-YBST: the row is read from the run order index alone, so
// it is named when no run record can be opened.
#[test]
fn the_row_is_read_from_the_index_and_opens_no_run_record() {
    let fx = Fixture::new();
    let stream = fx.create("indexed", None).expect("created");
    let run = merge_run_of(&fx, &stream);
    let records: Vec<std::path::PathBuf> = names_under(&canonical(fx.store.path()))
        .into_iter()
        .filter(|name| name.ends_with("/run.toml") || name == "run.toml")
        .map(|name| canonical(fx.store.path()).join(name))
        .collect();
    assert!(!records.is_empty(), "the run has a record");
    for record in &records {
        std::fs::write(record, "this is not a run record").unwrap();
    }

    let link = summary_of(&fx, &stream.id).merge_run.expect("named from the index");

    assert_eq!(link.run_id, run.id);
    assert_eq!(link.state, State::Queued);
}

// GRD-FR-OYPY, GRD-FR-MRNQ, WKS-FR-KHJS: a save of a run writes whether it is a
// merge run and whether it is archived into the index entry, and an index
// written before those fields existed reads back with both false.
#[test]
fn the_index_entry_records_whether_a_run_is_a_merge_run_and_archived() {
    let fx = Fixture::new();
    let stream = fx.create("entries", None).expect("created");
    let plain = save_plain_run(&fx, &stream, State::Completed);
    let mut merge = merge_run_of(&fx, &stream);
    resave(&fx, &mut merge, |run| {
        run.archived = true;
        run.archived_at = Some("t1".into());
    });

    let fs = fx.app.state::<crate::fs::FsAccessState>().get().expect("instance");
    let base = crate::graduation::StoreBase::new(canonical(fx.store.path()));
    let key = fx.app.state::<crate::project::ProjectState>().slot_key();
    let index = crate::graduation::read_queue_index(&fs, &base, &key);
    let entry = |id: &str| index.runs.iter().find(|e| e.run_id == id).expect("an entry").clone();
    assert!(!entry(&plain.id).merge);
    assert!(!entry(&plain.id).archived);
    assert!(entry(&merge.id).merge);
    assert!(entry(&merge.id).archived);

    let old: crate::graduation::QueueIndex = toml::from_str(
        "projectKey = \"p\"\n[[runs]]\nrunId = \"g1\"\nstreamId = \"w1\"\ndraftId = \"d1\"\nstate = \"queued\"\n",
    )
    .expect("an old index reads");
    assert!(!old.runs[0].merge);
    assert!(!old.runs[0].archived);
}

// WKS-FR-OVLQ, GRB-FR-UHFE: deleting a stream is refused with `stream_has_runs`
// while a merge run of it has not ended, and `force` does not reach that
// refusal. Nothing is removed.
#[test]
fn a_stream_with_a_live_merge_run_cannot_be_deleted() {
    let fx = Fixture::new();
    let stream = fx.create("kept", None).expect("created");
    let mut run = merge_run_of(&fx, &stream);

    // WKS-FR-OVLQ: neither `force` nor `discard_uncommitted` reaches the refusal.
    for force in [false, true] {
        let before = observe(&fx, &stream);
        let refusal = delete_work_stream(fx.app.clone(), stream.id.clone(), force, force)
            .expect_err("refused");
        assert_eq!(refusal, ERR_STREAM_HAS_RUNS);
        assert_eq!(observe(&fx, &stream), before);
    }

    resave(&fx, &mut run, |run| run.state = State::Discarded);
    delete_work_stream(fx.app.clone(), stream.id.clone(), false, false).expect("deleted once it ended");
}

// WKS-FR-WULF, GRD-FR-DLWB: a state change of a merge run is a change of the
// streams listing, and the event names the project alone.
#[test]
fn a_state_change_of_a_merge_run_tells_every_listing_to_reload() {
    let fx = Fixture::new();
    let stream = fx.create("announced", None).expect("created");
    let mut run = merge_run_of(&fx, &stream);
    let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    fx.app.listen(WORK_STREAMS_CHANGED, move |event| {
        if let Ok(value) = serde_json::from_str(event.payload()) {
            sink.lock().unwrap().push(value);
        }
    });

    crate::graduation::transition(&fx.app, &mut run, State::Discarded).expect("moved");

    let events = seen.lock().unwrap().clone();
    assert!(!events.is_empty(), "the change was announced");
    let key = fx.app.state::<crate::project::ProjectState>().slot_key();
    assert!(events.iter().all(|e| e == &serde_json::json!({ "projectKey": key })), "{events:?}");
}
