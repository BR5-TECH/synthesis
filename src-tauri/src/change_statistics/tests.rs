//! Tests for `CST-change-statistics.md`.

use std::sync::{Arc, Mutex};

use tauri::{Listener, Manager};
use tempfile::TempDir;

use super::*;
use crate::statistics::{
    Availability, Decision, ReportedTokens, Representation, UsageRecord, UsageScope,
};

const STREAM: &str = "cst-s1";
const CHANGE: &str = "add-auth";
const T: &str = "2026-03-14T12:00:00.000Z";

fn at(offset_ms: i64) -> String {
    let base = parse_instant_ms(T).expect("base");
    crate::notes::format_rfc3339_millis_utc(base + offset_ms)
}

fn interval(id: &str, from: i64, to: i64) -> Pending {
    Pending::now(EventBody::EditingInterval {
        interval_id: id.into(),
        started_at: at(from),
        ended_at: at(to),
    })
}

fn turn(id: &str, reached_model: bool) -> Pending {
    Pending::now(EventBody::ConversationTurn {
        turn_id: id.into(),
        discussion_id: "d1".into(),
        fragment_targeted: false,
        origin_kind: None,
        reached_model,
        started_at: at(1_000),
        ended_at: at(2_000),
        outcome: "delivered".into(),
    })
}

fn decision(id: &str, decision: Decision) -> Pending {
    Pending::at(at(3_000), EventBody::ProposalDecision { proposal_id: id.into(), decision })
}

fn store() -> (TempDir, fsa::RootFs) {
    let dir = TempDir::new().expect("tempdir");
    let root = fsa::RootFs::for_root(dir.path());
    (dir, root)
}

fn read(root: &fsa::RootFs) -> ChangeStatistics {
    read_change_statistics_impl(root, STREAM, CHANGE, Ok(4)).expect("read")
}

// CST-FR-KGGT / CST-FR-WSQI: the log of a change is in the change's own folder
// of the store; its first line is the activation boundary, and its lines carry
// the stream and the change, a working interval being `working_interval`.
#[test]
fn the_log_lives_in_the_change_folder_with_the_change_envelope() {
    let (_d, root) = store();
    append_change_events(&root, STREAM, CHANGE, &[interval("i1", 0, 60_000)]).expect("append");
    let path = root.path().join(format!("changes/{STREAM}/{CHANGE}/statistics.jsonl"));
    let text = std::fs::read_to_string(path).expect("log");
    let lines: Vec<serde_json::Value> =
        text.lines().map(|l| serde_json::from_str(l).expect("json")).collect();
    assert_eq!(lines[0]["type"], "telemetry_activated");
    assert_eq!(lines[0]["at"], at(0));
    assert_eq!(lines[1]["type"], "working_interval");
    assert_eq!(lines[1]["streamId"], STREAM);
    assert_eq!(lines[1]["change"], CHANGE);
    assert!(lines[1].get("draftId").is_none());
}

// CST-FR-WPHC / CST-FR-NCLF / CST-FR-WQKZ / CST-FR-HAMX / CST-FR-OXGK /
// CST-FR-PUAN: the totals fold on the terms of DSS, with the version count.
#[test]
fn the_totals_fold_on_the_terms_of_the_draft_statistics() {
    let (_d, root) = store();
    append_change_events(
        &root,
        STREAM,
        CHANGE,
        &[
            interval("i1", 0, 60_000),
            interval("i2", 120_000, 150_000),
            turn("t1", true),
            turn("t2", false),
            decision("p1", Decision::Accepted),
            decision("p2", Decision::Rejected),
        ],
    )
    .expect("append");
    let stats = read(&root);
    assert_eq!(stats.boundary_at.as_deref(), Some(at(0).as_str()));
    assert_eq!(stats.working_time_ms.value, Some(90_000));
    assert_eq!(stats.ai_interactions.value, Some(1));
    assert_eq!(stats.accepted_proposals.value, Some(1));
    assert_eq!(stats.rejected_proposals.value, Some(1));
    assert_eq!(stats.versions, Measure::of(4, Availability::Available));
    assert_eq!(stats.conversation_tokens.input.availability, Availability::Unavailable);
}

// CST-FR-NCLF / CST-FR-DCCV: a duplicated line folds once, and the tokens of
// the conversation are summed.
#[test]
fn a_duplicate_line_folds_once_and_tokens_are_summed() {
    let (_d, root) = store();
    let usage = crate::statistics::usage_events(
        &at(1_500),
        UsageScope::Conversation,
        crate::statistics::Bucket::Refinement,
        "t1",
        &[UsageRecord {
            usage_id: "t1:0:0".into(),
            round: 0,
            attempt: 0,
            representation: Representation::PerCall,
            tokens: ReportedTokens {
                input_tokens: Some(100),
                cached_input_tokens: None,
                uncached_input_tokens: None,
                output_tokens: Some(20),
            },
        }],
    );
    append_change_events(&root, STREAM, CHANGE, &usage).expect("append");
    let path = log_path(&root, STREAM, CHANGE).expect("path");
    let last = root.read_text(&path).expect("log").lines().last().expect("line").to_string();
    root.append_lines(&path, &[last]).expect("duplicate");
    let stats = read(&root);
    assert_eq!(stats.conversation_tokens.input.value, Some(100));
    assert_eq!(stats.conversation_tokens.output.value, Some(20));
}

// CST-FR-WPHC / CST-FR-BLIF / CST-FR-PUAN: a change with no log answers with
// no boundary and every statistic unavailable; a refused version count is
// unavailable; a bad name is refused.
#[test]
fn no_log_and_no_versions_read_as_unavailable() {
    let (_d, root) = store();
    let stats = read_change_statistics_impl(&root, STREAM, CHANGE, Err("unknown_stream".into()))
        .expect("read");
    assert_eq!(stats.boundary_at, None);
    for measure in
        [stats.working_time_ms, stats.ai_interactions, stats.versions, stats.accepted_proposals]
    {
        assert_eq!(measure.availability, Availability::Unavailable);
    }
    assert!(!root.path().join("changes").exists(), "a read creates nothing");
    assert_eq!(
        read_change_statistics_impl(&root, STREAM, "Bad Name", Ok(0)).map(|_| ()),
        Err(ERR_INVALID_CHANGE_NAME.to_string())
    );
    assert!(log_path(&root, "..", CHANGE).is_err());
}

// CST-FR-NCLF: an unreadable line is counted and left in place.
#[test]
fn an_unreadable_line_is_counted() {
    let (_d, root) = store();
    append_change_events(&root, STREAM, CHANGE, &[interval("i1", 0, 1_000)]).expect("append");
    let path = log_path(&root, STREAM, CHANGE).expect("path");
    root.append_lines(&path, &["{not json".to_string()]).expect("raw");
    assert_eq!(read(&root).unreadable_lines, 1);
}

// CST-FR-HDJD: a line holds the envelope and the event's own fields, and no
// text of any kind.
#[test]
fn a_line_holds_ids_and_numbers_alone() {
    let (_d, root) = store();
    append_change_events(
        &root,
        STREAM,
        CHANGE,
        &[turn("t1", true), decision("p1", Decision::Accepted)],
    )
    .expect("append");
    let text = root.read_text(log_path(&root, STREAM, CHANGE).expect("path")).expect("log");
    for line in text.lines() {
        let value: serde_json::Value = serde_json::from_str(line).expect("json");
        let keys: Vec<&String> = value.as_object().expect("object").keys().collect();
        for key in keys {
            assert!(
                [
                    "v",
                    "eventId",
                    "streamId",
                    "change",
                    "at",
                    "type",
                    "turnId",
                    "discussionId",
                    "fragmentTargeted",
                    "reachedModel",
                    "startedAt",
                    "endedAt",
                    "outcome",
                    "proposalId",
                    "decision",
                ]
                .contains(&key.as_str()),
                "unexpected field {key}"
            );
        }
    }
}

/// A mock app whose project has a store.
fn app_with_store() -> (TempDir, tauri::App<tauri::test::MockRuntime>) {
    // One governed directory serves as the root and the store.
    let dir = TempDir::new().expect("tempdir");
    let app = crate::tools::tests::mock_app();
    app.manage(crate::project::ProjectState::default());
    app.state::<crate::project::ProjectState>().set_root(dir.path().to_path_buf());
    (dir, app)
}

// CST-FR-QJGO / CST-FR-PSUF / CST-FR-TSOU: the interval command records one
// working interval on the writer, and the event names the stream and the
// change once it landed; a bad interval and a bad name are refused.
#[test]
fn the_interval_command_records_and_announces() {
    let (dir, app) = app_with_store();
    let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
    let sink = Arc::clone(&seen);
    app.listen(CHANGE_STATISTICS_CHANGED, move |event| {
        if let Ok(value) = serde_json::from_str(event.payload()) {
            sink.lock().unwrap().push(value);
        }
    });
    let handle = app.handle().clone();
    record_change_working_interval(handle.clone(), STREAM.into(), CHANGE.into(), at(0), at(5_000))
        .expect("recorded");
    crate::statistics::wait_for_writer();
    let store = fsa::RootFs::for_root(dir.path());
    assert_eq!(read(&store).working_time_ms.value, Some(5_000));
    assert_eq!(
        seen.lock().unwrap().clone(),
        vec![serde_json::json!({ "streamId": STREAM, "change": CHANGE })]
    );
    assert_eq!(
        record_change_working_interval(
            handle.clone(),
            STREAM.into(),
            CHANGE.into(),
            at(5_000),
            at(5_000)
        ),
        Err(ERR_INVALID_INTERVAL.to_string())
    );
    assert_eq!(
        record_change_working_interval(handle, STREAM.into(), "Bad".into(), at(0), at(1)),
        Err(ERR_INVALID_CHANGE_NAME.to_string())
    );
}

// CST-FR-BLIF: with no project open, the read is refused.
#[test]
fn no_project_is_refused() {
    let app = crate::tools::tests::mock_app();
    assert_eq!(
        read_change_statistics(app.handle().clone(), STREAM.into(), CHANGE.into()).map(|_| ()),
        Err(ERR_NO_PROJECT_OPEN.to_string())
    );
}

// CST-FR-OZBY / CST-FR-AHLL: nothing in the module deletes a log, and it
// writes through FSA alone.
#[test]
fn the_module_deletes_nothing_and_writes_through_fsa() {
    let source = include_str!("../change_statistics.rs");
    for forbidden in ["delete_under", "remove_file", "std::fs::", "reqwest"] {
        assert!(!source.contains(forbidden), "{forbidden}");
    }
}
