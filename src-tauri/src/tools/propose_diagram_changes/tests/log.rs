//! What a call logs (PDM-FR-PEWN): under the `ai` and `backend` domains, the
//! tool, the origin, the path, and the counts, and never a source, a parse
//! error, the rationale, or a part of the file.

use super::*;

static SUCCESS_LOG: LogBuffer = LogBuffer::new();
static REFUSAL_LOG: LogBuffer = LogBuffer::new();
static PARSE_LOG: LogBuffer = LogBuffer::new();
static UPDATE_LOG: LogBuffer = LogBuffer::new();
static BOUND_LOG: LogBuffer = LogBuffer::new();

/// A text that no record may carry, put in a source, a rationale, and a file.
const SECRET: &str = "Zanzibarnode";

fn secret_body() -> String {
    BODY.replace("Intro line.", "Intro line about Zanzibarfile.")
}

fn secret_rationale() -> String {
    format!("Because of {SECRET}rationale. <spoken>{SECRET}spoken.</spoken>")
}

/// Every text a record of the call may not hold.
fn forbidden() -> Vec<String> {
    vec![
        "A --> B".to_string(),
        "A --> C".to_string(),
        "flowchart".to_string(),
        "Zanzibar".to_string(),
        "Intro line".to_string(),
    ]
}

fn assert_clean(record: &crate::logging::LogRecord) {
    let fields = serde_json::to_string(&record.fields).expect("fields");
    let whole = format!("{} {}", record.message, fields);
    for text in forbidden() {
        assert!(!whole.contains(&text), "the record carries {text:?}: {whole}");
    }
}

// PDM-FR-PEWN: a success logs an INFO record under both domains that names the
// tool, the origin, the conversation, the path, and the count of diagram
// changes, and a refusal logs a WARN record that names the tool, the reason,
// and the place. No record holds a source, the rationale, or file text.
#[test]
fn a_success_and_a_refusal_log_the_path_and_the_counts_and_no_text() {
    for subject in SUBJECTS {
        let f = Fixture::with_body(subject, &secret_body());
        SUCCESS_LOG.clear();
        let mut missing = f.args(vec![
            change(SRC_A, SRC_A2),
            change("flowchart TD\n  Zanzibarnode --> Q", SRC_A3),
        ]);
        missing.rationale = secret_rationale();
        block_on(f.tool().with_buffer(&SUCCESS_LOG).call(missing)).expect_err("refused");
        let mut good = f.args(vec![change(SRC_A, SRC_A2)]);
        good.rationale = secret_rationale();
        block_on(f.tool().with_buffer(&SUCCESS_LOG).call(good)).expect("recorded");

        for domain in [Domain::Ai, Domain::Backend] {
            let records = records(&SUCCESS_LOG, domain);
            assert_eq!(records.len(), 2, "one per call, under {domain:?}");
            let info = records.iter().find(|r| r.level == LogLevel::Info).expect("the success");
            let warn = records.iter().find(|r| r.level == LogLevel::Warn).expect("the refusal");

            let ok = serde_json::to_value(&info.fields).expect("fields");
            assert_eq!(ok["tool"], NAME);
            assert_eq!(ok["path"], f.file());
            assert_eq!(ok["diagrams"], 1);
            assert_eq!(ok["updates"], 0);
            assert_eq!(ok["discussionId"], f.thread_id.as_str());
            assert!(ok["originKind"].is_string());

            let refused = serde_json::to_value(&warn.fields).expect("fields");
            assert_eq!(refused["tool"], NAME);
            assert_eq!(refused["reason"], "invalid_arguments");
            assert_eq!(refused["refusal"], "diagram_not_found");
            assert_eq!(refused["refusalAt"], 2, "the place of the change");
            assert_eq!(refused["retryable"], true);
            assert_eq!(refused["path"], f.file());
            assert_eq!(refused["diagrams"], 2);

            for record in [info, warn] {
                assert_clean(record);
            }
        }
    }
}

// PDM-FR-PEWN: a refusal for a source that does not parse names the reason and
// the place and carries neither the source nor the parse error.
#[test]
fn a_parse_refusal_logs_its_code_and_no_parse_error() {
    for subject in SUBJECTS {
        let f = Fixture::with_body(subject, &secret_body());
        PARSE_LOG.clear();
        let bad = "sequenceDiagram\n  Zanzibarnode->>: oops ->>";
        let refusal =
            block_on(f.tool().with_buffer(&PARSE_LOG).call(f.args(vec![change(SRC_A, bad)])))
                .expect_err("refused");
        let ToolRefusal::NotValidMermaid { error, .. } = &refusal else {
            panic!("expected a parse refusal, got {refusal:?}");
        };

        for domain in [Domain::Ai, Domain::Backend] {
            let records = records(&PARSE_LOG, domain);
            let warn = records.iter().find(|r| r.level == LogLevel::Warn).expect("the refusal");
            let fields = serde_json::to_value(&warn.fields).expect("fields");
            assert_eq!(fields["refusal"], "not_valid_mermaid");
            assert_eq!(fields["refusalAt"], 1);
            let whole = format!("{} {}", warn.message, fields);
            assert!(!whole.contains(error.as_str()), "no parse error: {whole}");
            assert!(!whole.contains("Zanzibar"), "no source: {whole}");
            assert!(!whole.contains("sequenceDiagram"), "no source: {whole}");
        }
    }
}

// PDM-FR-PEWN: an update logs the number of updates of each action and no text.
#[test]
fn an_update_logs_its_action_counts_and_no_text() {
    for subject in SUBJECTS {
        let f = Fixture::with_body(subject, &secret_body());
        f.propose(vec![change(SRC_A, SRC_A2)]).expect("recorded");
        let id = f.one().hunks[0].id.clone();
        UPDATE_LOG.clear();
        let mut call = f.updating(vec![
            entry(
                "change",
                Some(&id),
                Some(SRC_A),
                Some("flowchart TD\n  A --> Zanzibarnode"),
                None,
            ),
            entry("add", None, None, Some(SRC_B), Some("End line.")),
            entry("add", None, None, Some("graph LR\n  P --> Q"), Some("# Spec")),
        ]);
        call.rationale = secret_rationale();
        block_on(f.tool().with_buffer(&UPDATE_LOG).call(call)).expect("updated");

        for domain in [Domain::Ai, Domain::Backend] {
            let records = records(&UPDATE_LOG, domain);
            let info = records.iter().find(|r| r.level == LogLevel::Info).expect("the success");
            let fields = serde_json::to_value(&info.fields).expect("fields");
            assert_eq!(fields["tool"], NAME);
            assert_eq!(fields["updates"], 3);
            assert_eq!(fields["updateChanges"], 1);
            assert_eq!(fields["updateAdds"], 2);
            assert_eq!(fields["updateWithdraws"], 0);
            assert_eq!(fields["diagrams"], 0);
            let whole = format!("{} {}", info.message, fields);
            for text in ["Zanzibar", "A --> ", "Intro line", "graph LR"] {
                assert!(!whole.contains(text), "the record carries {text:?}: {whole}");
            }
        }
    }
}

// PDM-FR-PEWN: `path` is the one loggable argument, and it is bounded to the
// length a path can take, so a long one cannot cost the record its tool and its
// reason.
#[test]
fn a_very_long_path_is_bounded_before_it_reaches_a_record() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        BOUND_LOG.clear();
        let mut args = f.args(vec![change(SRC_A, SRC_A2)]);
        args.path = "x".repeat(5_000);
        block_on(f.tool().with_buffer(&BOUND_LOG).call(args)).expect_err("refused");

        let emitted = records(&BOUND_LOG, Domain::Backend);
        let fields = serde_json::to_string(&emitted[0].fields).expect("fields");
        assert!(fields.len() < 1_000, "the argument is bounded: {} chars", fields.len());
        assert!(fields.contains(NAME));
        assert!(fields.contains("path_missing") || fields.contains("not_found"), "{fields}");
    }
}
