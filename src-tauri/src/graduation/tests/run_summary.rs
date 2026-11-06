//! The run summary a `ready` review gives a draft run
//! (`../ai/GRL-graduation-loop.md` GRL-FR-FRNJ, GRL-FR-ECES; `GRD-graduation.md`
//! GRD-FR-UUGP).

use super::*;

/// A `ready` verdict that carries `summary` as its run summary.
pub(super) fn ready_with(summary: serde_json::Value) -> Turn {
    Turn::answering(Answer::RawResult(serde_json::json!({
        "verdict": "ready",
        "rationale": "The work answers the prompt.",
        "findings": [],
        "summary": summary,
    })))
}

// GRL-FR-FRNJ, GRD-FR-UUGP: a draft run that completes on a `ready` review keeps
// the review's run summary, a trimmed title of one line and a description.
#[test]
fn a_completed_draft_run_keeps_the_run_summary_of_its_ready_review() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        ready_with(serde_json::json!({
            "title": " Show an empty state ",
            "description": "## Summary\n\nThe panel says nothing is queued.\n",
        })),
    ]);

    let done = fx.drive(&run, dispatch);

    assert_eq!(done.state, GraduationRunState::Completed);
    let summary = done.run_summary.as_ref().expect("a run summary");
    assert_eq!(summary.title, "Show an empty state");
    assert_eq!(summary.description, "## Summary\n\nThe panel says nothing is queued.");
    let wire = serde_json::to_value(&done).unwrap();
    assert_eq!(wire["runSummary"]["title"], "Show an empty state");
}

// GRL-FR-ECES, GRD-FR-UUGP: a summary that is absent, blank in a field, or of
// another shape is dropped. The verdict, its refusal count and the run's next
// step do not change.
#[test]
fn a_summary_that_cannot_be_read_is_dropped_and_the_verdict_stands() {
    for (case, summary) in [
        ("a blank title", serde_json::json!({ "title": "  ", "description": "d" })),
        ("a blank description", serde_json::json!({ "title": "t", "description": "\n" })),
        ("a title of two lines", serde_json::json!({ "title": "a\nb", "description": "d" })),
        ("an extra field", serde_json::json!({ "title": "t", "description": "d", "body": "b" })),
        ("text", serde_json::json!("A title")),
        ("null", serde_json::Value::Null),
    ] {
        let fx = Fixture::new();
        let stream = fx.stream("editor");
        let run = fx.enqueue(&stream, "Add the empty state to the panel.");
        let dispatch = ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            ready_with(summary),
        ]);

        let done = fx.drive(&run, dispatch.clone());

        assert_eq!(done.state, GraduationRunState::Completed, "{case}");
        assert_eq!(done.commits.len(), 1, "{case}: the work is committed");
        assert!(done.run_summary.is_none(), "{case}: no run summary");
        assert_eq!(done.checkpoint.verdict_refusals, 0, "{case}: the verdict was read");
        assert_eq!(dispatch.turns_taken(), 2, "{case}: no review was asked again");
    }

    // A `ready` verdict with no summary at all completes the same way.
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");
    let done = fx.drive(&run, ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]));
    assert_eq!(done.state, GraduationRunState::Completed);
    assert!(done.run_summary.is_none());
}

// GRL-FR-FRNJ, GRL-FR-ECES: a `revise` verdict carries no summary, so a run
// that ends on an advisory `revise` keeps none, and a summary sent beside a
// `revise` changes nothing about the pass it starts.
#[test]
fn a_revise_verdict_gives_no_run_summary() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");
    let revise_with_summary = Turn::answering(Answer::RawResult(serde_json::json!({
        "verdict": "revise",
        "rationale": "A minor thing.",
        "findings": [{ "severity": "minor", "description": "d", "correction": "c",
            "affected_files": ["src/panel.ts"] }],
        "summary": { "title": "t", "description": "d" },
    })));

    let done = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            revise_with_summary,
        ]),
    );

    assert_eq!(done.state, GraduationRunState::Completed, "an advisory end");
    assert!(done.run_summary.is_none());
}

// GRD-FR-UUGP: a direct run records no run summary.
#[test]
fn a_direct_run_records_no_run_summary() {
    let fx = Fixture::new();
    let draft = fx.committed_draft("direct");
    let run = fx.direct("Add the empty state.", &draft);
    let done = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            ready_with(serde_json::json!({ "title": "t", "description": "d" })),
        ]),
    );
    assert_eq!(done.state, GraduationRunState::Completed);
    assert!(done.run_summary.is_none());
}
