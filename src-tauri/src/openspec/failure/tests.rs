//! The tests of the failure classes, the failure object, and the
//! diagnostics (`OFP-openspec-failure-policy.md`).

use super::*;
use crate::openspec::OpenSpecCliState;

// OFP-FR-PHOW, OFP-FR-SQDK, OFP-FR-DGSC, OFP-FR-RQXW, OFP-FR-FKRV: every
// code of the class table has its class, and a code the table does not name
// has none.
#[test]
fn every_code_of_the_class_table_has_its_class() {
    let table = [
        ("openspec_mode_inactive", FailureClass::Environment),
        ("cli_unavailable", FailureClass::Environment),
        ("cli_output_unreadable", FailureClass::Environment),
        ("openspec_image_cli_unavailable", FailureClass::Environment),
        ("cli_timed_out", FailureClass::Transient),
        ("cli_failed", FailureClass::Transient),
        ("cli_rejected", FailureClass::Content),
        ("change_not_ready", FailureClass::Content),
        ("tasks_incomplete", FailureClass::Content),
        ("archive_spec_update_failed", FailureClass::Content),
        ("archive_spec_validation_failed", FailureClass::Content),
        ("archive_validation_failed", FailureClass::Content),
        ("archive_tasks_incomplete", FailureClass::Content),
        ("archive_target_exists", FailureClass::Content),
        ("archive_error", FailureClass::Content),
        ("archive_some_new_code", FailureClass::Content),
    ];
    for (code, class) in table {
        assert_eq!(classify_openspec_failure(code), class, "{code}");
    }
    for code in [
        "change_not_found",
        "change_archived",
        "invalid_change_name",
        "proposal_pending",
        "commit_failed",
        "unknown_stream",
        "archive_failed",
        "change_not_archivable",
        "apply_progress_unavailable",
        "openspec_validation_unavailable",
        "",
    ] {
        assert_eq!(classify_openspec_failure(code), FailureClass::None, "{code}");
    }
}

// OFP-FR-BYYT: the classifying code is the cause where there is one, and a
// wrapping refusal takes the class of its cause.
#[test]
fn a_wrapping_refusal_takes_the_class_of_its_cause() {
    let mut failure = OpenSpecFailure::new("archive_failed");
    failure.cause = Some("archive_spec_update_failed".into());
    assert_eq!(failure.classifying_code(), "archive_spec_update_failed");
    assert_eq!(failure.class(), FailureClass::Content);
    failure.cause = Some("cli_timed_out".into());
    assert_eq!(failure.class(), FailureClass::Transient);
    failure.cause = Some("commit_failed".into());
    assert_eq!(failure.class(), FailureClass::None);
    let plain = OpenSpecFailure::new("cli_unavailable");
    assert_eq!(plain.classifying_code(), "cli_unavailable");
    assert_eq!(plain.class(), FailureClass::Environment);
}

// OFP-FR-QLQJ, OFP-FR-ZDEE, OFP-FR-OFLU: the object is camelCase on the wire,
// leaves out every absent field, and always carries its diagnostics.
#[test]
fn the_failure_object_is_camel_case_and_always_carries_diagnostics() {
    let bare = serde_json::to_value(OpenSpecFailure::new("openspec_mode_inactive")).unwrap();
    assert_eq!(bare, serde_json::json!({ "code": "openspec_mode_inactive", "diagnostics": [] }));
    let full = OpenSpecFailure {
        code: "archive_failed".into(),
        cause: Some("archive_spec_update_failed".into()),
        change: Some("add-logout".into()),
        detail: None,
        thread_id: Some("t1".into()),
        repair_run_id: Some("g1".into()),
        repair_refusal: None,
        diagnostics: vec![CliDiagnostic {
            severity: Some("error".into()),
            code: Some("archive_spec_update_failed".into()),
            path: None,
            message: "MODIFIED not found".into(),
            fix: Some("Use ADDED".into()),
        }],
    };
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::json!({
            "code": "archive_failed",
            "cause": "archive_spec_update_failed",
            "change": "add-logout",
            "threadId": "t1",
            "repairRunId": "g1",
            "diagnostics": [{
                "severity": "error",
                "code": "archive_spec_update_failed",
                "message": "MODIFIED not found",
                "fix": "Use ADDED"
            }]
        })
    );
}

// OFP-FR-EOAD, OFP-FR-GPCO, OFP-FR-OFLU: the failure of a read carries the
// CLI state of `cli_unavailable`, the exit status of `cli_failed` (empty for
// a signal), the folder of `change_archived`, and no diagnostic for an
// environment failure.
#[test]
fn the_failure_of_a_read_carries_its_detail() {
    let state = OpenSpecCliState::Unsupported { path: "/x".into(), version: "0.9.0".into() };
    let failure = OpenSpecReadError::CliUnavailable(state).to_failure();
    assert_eq!(failure.code, "cli_unavailable");
    assert_eq!(failure.detail.as_deref(), Some("unsupported"));
    assert!(failure.diagnostics.is_empty());
    let failed = OpenSpecReadError::CliFailed { exit_status: Some(2), diagnostics: Vec::new() };
    assert_eq!(failed.to_failure().detail.as_deref(), Some("2"));
    let signal = OpenSpecReadError::CliFailed { exit_status: None, diagnostics: Vec::new() };
    assert_eq!(signal.to_failure().detail.as_deref(), Some(""));
    let archived = OpenSpecReadError::ChangeArchived { archived_as: "2026-11-10-x".into() };
    assert_eq!(archived.to_failure().code, "change_archived");
    assert_eq!(archived.to_failure().detail.as_deref(), Some("2026-11-10-x"));
}

// OFP-FR-ZDEE: a wrapping refusal names the read's code as its cause and
// keeps the read's detail and diagnostics.
#[test]
fn a_wrapping_failure_keeps_the_read_as_its_cause() {
    let error =
        OpenSpecReadError::CliRejected { diagnostics: vec![CliDiagnostic::of("error", "bad")] };
    let failure = OpenSpecFailure::wrapping("apply_progress_unavailable", &error).for_change("x");
    assert_eq!(failure.code, "apply_progress_unavailable");
    assert_eq!(failure.cause.as_deref(), Some("cli_rejected"));
    assert_eq!(failure.change.as_deref(), Some("x"));
    assert_eq!(failure.diagnostics.len(), 1);
    assert_eq!(failure.to_string(), "apply_progress_unavailable: cli_rejected");
    let mut archive = OpenSpecFailure::new("archive_failed").for_change("x");
    archive.cause = Some("archive_error".into());
    assert_eq!(archive.to_string(), "archive_failed: x: archive_error");
}

// OFP-FR-NJSO: the `status` entries, in order, each with its severity, code,
// message, and fix; an entry with no message gives its code.
#[test]
fn status_entries_are_the_diagnostics_in_the_order_of_the_cli() {
    let json = serde_json::json!({
        "items": [],
        "status": [
            { "severity": "error", "code": "a_code", "message": "First", "fix": "Do this" },
            { "severity": "warning", "code": "b_code" },
            { "severity": "info" }
        ]
    });
    let diagnostics = status_diagnostics(&json);
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].message, "First");
    assert_eq!(diagnostics[0].fix.as_deref(), Some("Do this"));
    assert_eq!(diagnostics[0].severity.as_deref(), Some("error"));
    assert_eq!(diagnostics[1].message, "b_code");
    assert_eq!(diagnostics[1].code.as_deref(), Some("b_code"));
    assert!(status_diagnostics(&serde_json::json!({ "items": [] })).is_empty());
}

// OFP-FR-RUOP: the issues of a validation, in order, with the level as the
// severity, the path, and the message.
#[test]
fn validation_issues_are_the_diagnostics() {
    let issues = vec![
        ValidationIssue { level: "ERROR".into(), path: "specs/a.md".into(), message: "M1".into() },
        ValidationIssue { level: "WARNING".into(), path: String::new(), message: "M2".into() },
    ];
    let diagnostics = issue_diagnostics(&issues);
    assert_eq!(diagnostics[0].severity.as_deref(), Some("error"));
    assert_eq!(diagnostics[0].path.as_deref(), Some("specs/a.md"));
    assert_eq!(diagnostics[0].message, "M1");
    assert_eq!(diagnostics[1].path, None);
    assert_eq!(diagnostics[1].severity.as_deref(), Some("warning"));
}

// OFP-FR-BWML: one error for each missing artifact naming it, one warning for
// each warning, and one info with the instruction.
#[test]
fn not_ready_diagnostics_name_the_missing_artifacts() {
    let diagnostics = not_ready_diagnostics(
        &["design".into(), "specs".into()],
        &["Tasks are empty".into()],
        Some("Write the design first."),
    );
    assert_eq!(diagnostics.len(), 4);
    assert_eq!(diagnostics[0].severity.as_deref(), Some("error"));
    assert!(diagnostics[0].message.contains("design"));
    assert!(diagnostics[1].message.contains("specs"));
    assert_eq!(diagnostics[2].severity.as_deref(), Some("warning"));
    assert_eq!(diagnostics[3].severity.as_deref(), Some("info"));
    assert_eq!(diagnostics[3].message, "Write the design first.");
    assert_eq!(not_ready_diagnostics(&[], &[], None), Vec::new());
}

// OFP-FR-WNVO: the last 4 KiB of standard error, with each invalid sequence
// replaced and no severity; empty standard error gives no entry.
#[test]
fn standard_error_gives_its_last_four_kib() {
    assert_eq!(stderr_diagnostic(b""), None);
    assert_eq!(stderr_diagnostic(b"  \n"), None);
    let mut long = vec![b'a'; 6000];
    long.extend_from_slice(b"\xffEND");
    let diagnostic = stderr_diagnostic(&long).unwrap();
    assert!(diagnostic.message.ends_with("\u{fffd}END"));
    // The last 4096 bytes are 4092 `a`, one invalid byte, and `END`. The
    // invalid byte becomes U+FFFD, which takes 3 bytes.
    assert_eq!(diagnostic.message.len(), 4098);
    assert_eq!(diagnostic.message.len(), STDERR_KEEP - 1 + '\u{fffd}'.len_utf8());
    assert!(diagnostic.message.starts_with(&"a".repeat(4092)));
    assert_eq!(diagnostic.severity, None);
}

// OFP-FR-QOET: at most 50 entries, the first in order, each message and fix
// cut to 2 KiB at a character boundary.
#[test]
fn a_list_of_diagnostics_is_bounded() {
    let many: Vec<CliDiagnostic> = (0..60)
        .map(|i| CliDiagnostic {
            message: format!("{i}{}", "é".repeat(2000)),
            fix: Some("x".repeat(3000)),
            ..CliDiagnostic::default()
        })
        .collect();
    let bounded = bounded(many);
    assert_eq!(bounded.len(), MAX_DIAGNOSTICS);
    assert!(bounded[0].message.starts_with('0'));
    assert!(bounded[49].message.starts_with("49"));
    // Each message keeps as much as fits: a cut at a character boundary loses
    // at most one byte of the 2 KiB.
    assert!(bounded.iter().all(|d| (MAX_TEXT - 1..=MAX_TEXT).contains(&d.message.len())));
    assert!(bounded.iter().all(|d| d.fix.as_ref().unwrap().len() == MAX_TEXT));
}

// OFP contract surface, fixed text: each diagnostic line leaves out each
// absent part and adds the fix.
#[test]
fn a_diagnostic_line_leaves_out_the_absent_parts() {
    let full = CliDiagnostic {
        severity: Some("error".into()),
        code: Some("archive_spec_update_failed".into()),
        path: Some("specs/auth".into()),
        message: "Not found".into(),
        fix: Some("Use ADDED".into()),
    };
    assert_eq!(
        full.line(),
        "- error archive_spec_update_failed specs/auth: Not found Fix: Use ADDED"
    );
    let bare = CliDiagnostic { message: "Just text".into(), ..CliDiagnostic::default() };
    assert_eq!(bare.line(), "- Just text");
}

// OFP-FR-QLQJ: a refusal is a typed code or an object on the wire, and reads
// as its line form, which holds no diagnostic.
#[test]
fn a_refusal_is_a_string_or_an_object_on_the_wire() {
    let code = Refusal::from("stream_busy");
    assert_eq!(serde_json::to_value(&code).unwrap(), serde_json::json!("stream_busy"));
    assert_eq!(code.code(), "stream_busy");
    let mut failure = OpenSpecFailure::new("change_not_ready").for_change("x");
    failure.detail = Some("design,specs".into());
    failure.diagnostics = vec![CliDiagnostic::of("error", "secret words of the CLI")];
    let refusal = Refusal::from(failure);
    assert_eq!(serde_json::to_value(&refusal).unwrap()["code"], "change_not_ready");
    assert_eq!(refusal.code(), "change_not_ready");
    assert_eq!(refusal, "change_not_ready: design,specs");
    assert!(!refusal.to_string().contains("secret"));
    assert_eq!(Refusal::from("openspec_run_active: g1").code(), "openspec_run_active");
}
