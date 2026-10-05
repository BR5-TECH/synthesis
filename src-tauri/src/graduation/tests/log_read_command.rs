//! Reading a run's log streams back through the `read_graduation_logs` command
//! (`GRS-graduation-run-log-storage.md` GRS-FR-RZXA and the refusals, the
//! corrupt-read record, and the append announcement around it).

use super::*;

use super::log_reads::driven;
use crate::graduation::logs::read::{
    GraduationLogPassScope, STATUS_AVAILABLE, STATUS_UNAVAILABLE,
};
use crate::graduation::logs::{self, GraduationLogPage, GraduationLogStream};

// ---------------------------------------------------------------------------
// The command itself
// ---------------------------------------------------------------------------

/// Call `read_graduation_logs` as the frontend does.
fn command(
    fx: &Fixture,
    run_id: &str,
    phase_id: &str,
    scope: serde_json::Value,
    stream: &str,
) -> Result<GraduationLogPage, String> {
    crate::graduation::read_graduation_logs(
        fx.app.clone(),
        run_id.to_string(),
        phase_id.to_string(),
        scope,
        stream.to_string(),
        None,
        Some(1000),
        None,
    )
}

/// GRS contract surface: every refusal is typed and names what it was given.
#[test]
fn the_read_command_refuses_a_phase_a_stream_a_scope_and_a_run_it_does_not_hold() {
    let fx = Fixture::new();
    let run = driven(&fx);
    let pass = serde_json::json!({ "kind": "phase" });

    let phase = command(&fx, &run.id, "authoring", pass.clone(), "structured")
        .expect_err("an unknown phase");
    assert!(phase.starts_with(logs::read::ERR_UNKNOWN_PHASE));
    assert!(phase.contains("authoring"), "it names the value it was given");

    assert_eq!(
        command(&fx, &run.id, "working", pass.clone(), "diagnostics")
            .expect_err("an unknown stream"),
        crate::graduation::ERR_UNKNOWN_STREAM
    );
    assert_eq!(
        command(
            &fx,
            &run.id,
            "working",
            serde_json::json!({ "kind": "iteration", "iteration": 1 }),
            "structured"
        )
        .expect_err("an unknown scope"),
        logs::read::ERR_UNKNOWN_SCOPE
    );
    assert_eq!(
        command(&fx, "no-such-run", "working", pass, "structured")
            .expect_err("a run that does not exist"),
        crate::graduation::ERR_UNKNOWN_RUN
    );
}

/// GRS-FR-RZXA: the command answers for the run, the phase, the scope and the
/// stream it names, and writes neither file.
#[test]
fn the_read_command_answers_for_the_scope_it_names() {
    let fx = Fixture::new();
    let run = driven(&fx);
    let before = std::fs::read_to_string(
        logs::paths_for(&fx.app, &run.id)
            .expect("the paths")
            .stream(GraduationLogStream::Structured),
    )
    .expect("the stream");

    let page = command(
        &fx,
        &run.id,
        "working",
        serde_json::json!({ "kind": "pass", "pass": 1 }),
        "structured",
    )
    .expect("a page");
    assert_eq!(page.run_id, run.id);
    assert_eq!(page.phase_id, "working");
    assert_eq!(page.stream, GraduationLogStream::Structured);
    assert_eq!(page.scope, GraduationLogPassScope::Pass { pass: 1 });
    assert!(!page.entries.is_empty());

    let after = std::fs::read_to_string(
        logs::paths_for(&fx.app, &run.id)
            .expect("the paths")
            .stream(GraduationLogStream::Structured),
    )
    .expect("the stream");
    assert_eq!(before, after, "a read writes neither file");
}

/// GRS-FR-EYNU, GRS-FR-MRKO: a corrupt read is recorded on the run record, is
/// recorded once however many times it is met, leaves the persistence status and
/// the run state alone, and is cleared by a later read that succeeds.
#[test]
fn a_corrupt_read_is_recorded_on_the_run_and_cleared_by_a_later_one() {
    let fx = Fixture::new();
    let run = driven(&fx);
    let path = logs::paths_for(&fx.app, &run.id)
        .expect("the paths")
        .stream(GraduationLogStream::Structured);
    let whole = std::fs::read_to_string(&path).expect("the stream");
    let mut lines: Vec<&str> = whole.lines().collect();
    lines.insert(2, "{ this is not json");
    std::fs::write(&path, format!("{}\n", lines.join("\n"))).expect("the damaged stream");

    let pass = serde_json::json!({ "kind": "phase" });
    let page = command(&fx, &run.id, "working", pass.clone(), "structured").expect("a page");
    assert_eq!(page.status, STATUS_UNAVAILABLE);

    let recorded = crate::graduation::get_graduation_run(fx.app.clone(), run.id.clone())
        .expect("the run");
    let failure = recorded
        .logs
        .last_read_failure
        .clone()
        .expect("the damage is on the run record");
    assert_eq!(failure.code, logs::index::CODE_STREAM_CORRUPT);
    // GRS-FR-MRKO: it interrupts no run and moves no run state.
    assert!(recorded.logs.persistence.is_healthy());
    assert_eq!(recorded.state, run.state);

    // Met again, the record is left exactly as it stands.
    let _ = command(&fx, &run.id, "working", pass.clone(), "structured").expect("a page");
    let again = crate::graduation::get_graduation_run(fx.app.clone(), run.id.clone())
        .expect("the run");
    assert_eq!(again.logs.last_read_failure, Some(failure));

    // GRS-FR-EYNU: it stays readable until a later read of that stream succeeds.
    std::fs::write(&path, whole).expect("the repaired stream");
    let repaired = command(&fx, &run.id, "working", pass, "structured").expect("a page");
    assert_eq!(repaired.status, STATUS_AVAILABLE);
    let cleared = crate::graduation::get_graduation_run(fx.app.clone(), run.id.clone())
        .expect("the run");
    assert!(cleared.logs.last_read_failure.is_none());
}

/// GRS-FR-UCZL, GRS-FR-KCAK: records that became durable are announced, once
/// per stream, naming the run and the stream and carrying no record content.
#[test]
fn every_stream_announces_that_it_grew_and_carries_no_record() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tauri::Listener;

    let fx = Fixture::new();
    let source = Arc::new(AtomicUsize::new(0));
    let structured = Arc::new(AtomicUsize::new(0));
    let payloads = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    {
        let (source, structured, payloads) =
            (source.clone(), structured.clone(), payloads.clone());
        fx.app.listen(
            crate::graduation::GRADUATION_LOG_RECORDS_APPENDED,
            move |event| {
                let payload: serde_json::Value =
                    serde_json::from_str(event.payload()).expect("a payload");
                match payload["stream"].as_str() {
                    Some("source") => source.fetch_add(1, Ordering::SeqCst),
                    Some("structured") => structured.fetch_add(1, Ordering::SeqCst),
                    _ => panic!("every event names one of the two streams"),
                };
                payloads.lock().expect("the payloads").push(payload);
            },
        );
    }

    let run = driven(&fx);

    assert!(
        structured.load(Ordering::SeqCst) > 0,
        "the structured stream announced that it grew"
    );
    assert!(
        source.load(Ordering::SeqCst) > 0,
        "and so did the source stream, which is what a following surface reads"
    );
    let held = payloads.lock().expect("the payloads");
    for payload in held.iter() {
        assert_eq!(payload["runId"].as_str(), Some(run.id.as_str()));
        assert!(payload["latestSequence"].as_u64().is_some());
        // No record content: the three fields and no other.
        let object = payload.as_object().expect("an object");
        assert_eq!(object.len(), 3, "{payload}");
        for key in ["record", "records", "data_base64", "event", "fields"] {
            assert!(object.get(key).is_none(), "the event carries no {key}");
        }
    }
    // GRS-FR-UCZL: told after the records are durable, so a consumer that
    // re-reads on it reads the result rather than racing it.
    let newest = held
        .iter()
        .filter(|p| p["stream"] == "structured")
        .filter_map(|p| p["latestSequence"].as_u64())
        .max()
        .expect("a structured event");
    assert_eq!(newest, run.logs.structured.latest_sequence);
}
