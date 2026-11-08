use super::*;
use crate::graduation::GraduationCheckCause as Cause;
use crate::tools::agent_exec::protocol::{AgentResponseEnvelope, PROTOCOL_VERSION};
use crate::tools::agent_exec::runtime::CapturedStream;
use serde_json::json;

fn answer(outcome: AgentOutcome, result: Option<serde_json::Value>) -> AgentExecution {
    AgentExecution {
        process_outcome: ProcessOutcome::Completed,
        exit_code: Some(0),
        response: Some(AgentResponseEnvelope {
            protocol_version: PROTOCOL_VERSION,
            outcome,
            summary: "done".into(),
            result: result.and_then(|value| value.as_object().cloned()),
            failure: None,
            escalation: None,
            session: None,
            metadata: None,
        }),
        stdout: CapturedStream::default(),
        stderr: CapturedStream::default(),
        duration_ms: 0,
        resume_unavailable: false,
        session: None,
        refusal: None,
        container_removed: true,
    }
}

fn success(result: serde_json::Value) -> AgentExecution {
    answer(AgentOutcome::Success, Some(result))
}

fn check(name: &str, finished: bool, exit_status: serde_json::Value) -> serde_json::Value {
    json!({ "name": name, "command": format!("run {name}"), "finished": finished, "exit_status": exit_status })
}

// GRL-FR-CPXI, GRL-FR-XWNQ: a success whose every check finished with exit
// status 0 is accepted, and the counts name each check.
#[test]
fn a_success_with_every_check_green_is_accepted() {
    let reading = judge(
        &success(
            json!({ "checks": [check("tests", true, json!(0)), check("lint", true, json!(0))] }),
        ),
        0,
    );
    assert_eq!(reading.cause, None);
    assert_eq!((reading.checks, reading.unfinished, reading.red, reading.waived), (2, 0, 0, 0));
    assert!(reading.failing.is_empty());
}

// GRL-FR-CPXI: the first cause that applies, in the order of the requirement.
#[test]
fn the_gate_refuses_with_the_first_cause_that_applies() {
    // An unfinished check comes before a red one.
    let both = success(json!({ "checks": [
        check("tests", true, json!(101)),
        check("build", false, json!(null)),
    ] }));
    let reading = judge(&both, 0);
    assert_eq!(reading.cause, Some(Cause::ChecksUnfinished));
    assert_eq!((reading.unfinished, reading.red), (1, 1));
    assert_eq!(
        reading.failing,
        vec![
            GraduationRestCheck { name: "tests".into(), finished: true, exit_status: Some(101) },
            GraduationRestCheck { name: "build".into(), finished: false, exit_status: None },
        ]
    );

    let red = success(json!({ "checks": [check("tests", true, json!(1))] }));
    assert_eq!(judge(&red, 0).cause, Some(Cause::ChecksRed));
    // A negative exit status is red too.
    let negative = success(json!({ "checks": [check("tests", true, json!(-9))] }));
    assert_eq!(judge(&negative, 0).cause, Some(Cause::ChecksRed));
    // A finished check with no exit status did not pass.
    let silent = success(json!({ "checks": [check("tests", true, json!(null))] }));
    assert_eq!(judge(&silent, 0).cause, Some(Cause::ChecksRed));
}

// GRL-FR-CPXI, GRL-FR-IJTT: a result that is not a valid `WorkResult`, and one
// that holds no check and no reason, are `record_missing`.
#[test]
fn a_record_that_is_not_a_valid_work_result_is_missing() {
    let long = "x".repeat(MAX_NAME + 1);
    let many: Vec<serde_json::Value> =
        (0..=MAX_CHECKS).map(|i| check(&format!("c{i}"), true, json!(0))).collect();
    let cases = [
        ("no result", answer(AgentOutcome::Success, None)),
        ("an empty object", success(json!({}))),
        ("no check and no reason", success(json!({ "checks": [] }))),
        ("a blank reason", success(json!({ "checks": [], "no_checks_reason": "  " }))),
        ("a key it does not define", success(json!({ "checks": [], "changed_paths": ["a"] }))),
        (
            "a check key it does not define",
            success(
                json!({ "checks": [{ "name": "t", "command": "c", "finished": true, "exit_status": 0, "log": "x" }] }),
            ),
        ),
        (
            "no exit status key",
            success(json!({ "checks": [{ "name": "t", "command": "c", "finished": true }] })),
        ),
        ("a blank name", success(json!({ "checks": [check(" ", true, json!(0))] }))),
        (
            "a blank command",
            success(
                json!({ "checks": [{ "name": "t", "command": " \n", "finished": true, "exit_status": 0 }] }),
            ),
        ),
        (
            "a command over its limit",
            success(json!({ "checks": [{
                "name": "t",
                "command": "c".repeat(MAX_COMMAND + 1),
                "finished": true,
                "exit_status": 0,
            }] })),
        ),
        (
            "a negative waiver",
            success(
                json!({ "checks": [{ "name": "t", "command": "c", "finished": true, "exit_status": 1, "waived_by": -1 }] }),
            ),
        ),
        ("a name over its limit", success(json!({ "checks": [check(&long, true, json!(0))] }))),
        ("more than 64 checks", success(json!({ "checks": many }))),
        (
            "a waiver of zero",
            success(
                json!({ "checks": [{ "name": "t", "command": "c", "finished": true, "exit_status": 1, "waived_by": 0 }] }),
            ),
        ),
        (
            "a fractional exit status",
            success(json!({ "checks": [check("tests", true, json!(1.5))] })),
        ),
    ];
    for (case, execution) in cases {
        assert_eq!(judge(&execution, 3).cause, Some(Cause::RecordMissing), "{case}");
    }
    // A completed turn with no readable envelope at all holds no record.
    let mut silent = success(json!({ "checks": [check("tests", true, json!(0))] }));
    silent.response = None;
    let reading = judge(&silent, 3);
    assert_eq!(reading.cause, Some(Cause::RecordMissing), "no response");
    assert_eq!((reading.checks, reading.failing.len()), (0, 0), "no response");
    // At the bound itself the record is valid.
    let at_bound: Vec<serde_json::Value> =
        (0..MAX_CHECKS).map(|i| check(&format!("c{i}"), true, json!(0))).collect();
    assert_eq!(judge(&success(json!({ "checks": at_bound })), 0).cause, None);
    let name = "n".repeat(MAX_NAME);
    assert_eq!(judge(&success(json!({ "checks": [check(&name, true, json!(0))] })), 0).cause, None);
    let command = json!({ "checks": [{
        "name": "t",
        "command": "c".repeat(MAX_COMMAND),
        "finished": true,
        "exit_status": 0,
    }] });
    assert_eq!(judge(&success(command), 0).cause, None, "a command at its limit");
}

// GRL-FR-CPXI: a record that holds checks and a reason is judged by its
// checks. The reason does not hide a red check.
#[test]
fn a_reason_beside_checks_does_not_hide_a_red_check() {
    let green = success(json!({
        "checks": [check("tests", true, json!(0))],
        "no_checks_reason": "The change is to a text file.",
    }));
    assert_eq!(judge(&green, 0).cause, None);
    let red = success(json!({
        "checks": [check("tests", true, json!(2))],
        "no_checks_reason": "The change is to a text file.",
    }));
    assert_eq!(judge(&red, 0).cause, Some(Cause::ChecksRed));
}

// GRL-FR-IJTT: a project that requires no check answers with an empty record
// and a reason, which the gate accepts.
#[test]
fn an_empty_record_with_a_reason_is_accepted() {
    let reading = judge(
        &success(json!({ "checks": [], "no_checks_reason": "The change is to a text file." })),
        0,
    );
    assert_eq!(reading.cause, None);
    let over = "r".repeat(MAX_REASON + 1);
    let reading = judge(&success(json!({ "checks": [], "no_checks_reason": over })), 0);
    assert_eq!(reading.cause, Some(Cause::RecordMissing));
}

// GRL-FR-HLNB: a red check whose `waived_by` names an entry of the decisions
// passes; one that names no entry stays red. An unfinished check is never
// waived.
#[test]
fn a_waiver_passes_a_red_check_only_where_its_decision_exists() {
    let waived = |by: u64| {
        success(json!({ "checks": [
            check("tests", true, json!(0)),
            { "name": "lint", "command": "run lint", "finished": true, "exit_status": 2, "waived_by": by },
        ] }))
    };
    let reading = judge(&waived(2), 2);
    assert_eq!(reading.cause, None);
    assert_eq!((reading.red, reading.waived), (0, 1));
    assert!(reading.failing.is_empty());

    let reading = judge(&waived(3), 2);
    assert_eq!(reading.cause, Some(Cause::ChecksRed));
    assert_eq!((reading.red, reading.waived), (1, 0));
    assert_eq!(judge(&waived(1), 0).cause, Some(Cause::ChecksRed), "no decision exists");

    let unfinished = success(json!({ "checks": [
        { "name": "lint", "command": "run lint", "finished": false, "exit_status": null, "waived_by": 1 },
    ] }));
    assert_eq!(judge(&unfinished, 1).cause, Some(Cause::ChecksUnfinished));
}

// GRL-FR-TYRV: a failure is refused whatever it carries, and its record, where
// it holds one, still names the checks that did not pass.
#[test]
fn a_failure_is_refused_as_work_failed() {
    let bare = answer(AgentOutcome::Failure, None);
    let reading = judge(&bare, 0);
    assert_eq!(reading.cause, Some(Cause::WorkFailed));
    assert!(reading.failing.is_empty());

    let with_record = answer(
        AgentOutcome::Failure,
        Some(json!({ "checks": [check("tests", true, json!(3)), check("lint", true, json!(0))] })),
    );
    let reading = judge(&with_record, 0);
    assert_eq!(reading.cause, Some(Cause::WorkFailed));
    assert_eq!((reading.checks, reading.red), (2, 1));
    assert_eq!(
        reading.failing,
        vec![GraduationRestCheck { name: "tests".into(), finished: true, exit_status: Some(3) }]
    );
}

// GRL-FR-XWNQ: a recorded escalation never reaches the gate, so an
// escalation answer the gate reads is one whose escalation was not recorded,
// and it is refused as `record_missing`, whatever its result holds.
#[test]
fn an_escalation_that_reaches_the_gate_was_not_recorded_and_is_refused() {
    assert_eq!(
        judge(&answer(AgentOutcome::EscalationRequired, None), 0).cause,
        Some(Cause::RecordMissing)
    );
    let with_green = answer(
        AgentOutcome::EscalationRequired,
        Some(json!({ "checks": [check("tests", true, json!(0))] })),
    );
    assert_eq!(judge(&with_green, 0).cause, Some(Cause::RecordMissing));
}

// GRL-FR-CHDT: each cause names its `refusals.md` section, and every section
// exists.
#[test]
fn each_cause_names_a_refusal_section_that_exists() {
    for (cause, section) in [
        (Cause::ChecksUnfinished, "checks_not_passing"),
        (Cause::ChecksRed, "checks_not_passing"),
        (Cause::RecordMissing, "checks_record_missing"),
        (Cause::WorkFailed, "work_failed"),
    ] {
        assert_eq!(cause.correction_section(), section);
        let text = prompts::prompt_section(&prompts::REFUSALS, section)
            .unwrap_or_else(|| panic!("refusals.md holds `{section}`"));
        assert!(text.contains("You may change files"), "{section} lets the turn change files");
    }
    let escalate = prompts::prompt_section(&prompts::REFUSALS, "checks_escalate").unwrap();
    assert!(escalate.contains("Change no file"));
    assert!(escalate.contains("You may not report success"));
    assert!(escalate.contains("`escalation_required`"));
}
