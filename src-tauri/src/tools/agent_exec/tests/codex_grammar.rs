//! CDX at `0.172.0`: the optional parts of both vectors, the launch files they
//! name, the strict schema, and the reading of an answer written to it.

use serde_json::{json, Value};

use super::*;
use crate::tools::agent_exec::descriptor::{codex, ArgvRequest, LaunchFile};

const ALL_CONTRACTS: [ResultContract; 4] = [
    ResultContract::ReviewVerdict,
    ResultContract::GraduationVerdict,
    ResultContract::PullRequestSummary,
    ResultContract::WorkResult,
];

fn request<'a>(
    resume_session: Option<&'a str>,
    result_contract: Option<ResultContract>,
    milestone_reminder: bool,
) -> ArgvRequest<'a> {
    ArgvRequest {
        model_id: Some("gpt-6.1-sol"),
        effort_id: Some("max"),
        assigned_session_id: "IGNORED",
        resume_session,
        result_contract,
        milestone_reminder,
    }
}

const HOOK: &str = "hooks.PreToolUse=[{matcher=\"*\",hooks=[{type=\"command\",\
command=\"cat /opt/synthesis/vendor/milestone-reminder.json\"}]}]";

/// CDX-FR-02, CDX-FR-08, CDX-FR-14, CDX-FR-OQVA — every conditional part of a fresh
/// vector, in the order of the contract surface, with `-` last.
#[test]
fn a_fresh_codex_vector_with_every_optional_part() {
    let argv = descriptor::CODEX.argv(&request(None, Some(ResultContract::WorkResult), true));
    assert_eq!(
        argv,
        [
            "exec",
            "--json",
            "--dangerously-bypass-approvals-and-sandbox",
            "--skip-git-repo-check",
            "--output-schema",
            "/opt/synthesis/vendor/output-schema-work_result.json",
            "--dangerously-bypass-hook-trust",
            "-c",
            HOOK,
            "--model",
            "gpt-6.1-sol",
            "-c",
            "model_reasoning_effort=\"max\"",
            "-",
        ]
    );
}

/// CDX-FR-09, CDX-FR-10, CDX-FR-08, CDX-FR-21 — a resumed vector carries the same conditional parts under
/// the same conditions, before the session id and the `-` prompt.
#[test]
fn a_resumed_codex_vector_with_every_optional_part() {
    let argv =
        descriptor::CODEX.argv(&request(Some("thr"), Some(ResultContract::ReviewVerdict), true));
    assert_eq!(
        argv,
        [
            "exec",
            "resume",
            "--json",
            "--dangerously-bypass-approvals-and-sandbox",
            "--skip-git-repo-check",
            "--output-schema",
            "/opt/synthesis/vendor/output-schema-review_verdict.json",
            "--dangerously-bypass-hook-trust",
            "-c",
            HOOK,
            "--model",
            "gpt-6.1-sol",
            "-c",
            "model_reasoning_effort=\"max\"",
            "thr",
            "-",
        ]
    );
}

/// CDX-FR-14, CDX-FR-OQVA, CDX-FR-08 — each optional part is present only under its own
/// condition, on both vectors, and `-` stays last in each.
#[test]
fn each_optional_codex_part_appears_only_under_its_condition() {
    for resume in [None, Some("thr")] {
        for contract in [None, Some(ResultContract::PullRequestSummary)] {
            for reminder in [false, true] {
                let argv = descriptor::CODEX.argv(&request(resume, contract, reminder));
                assert_eq!(argv.last().map(String::as_str), Some("-"), "{argv:?}");
                assert_eq!(argv.iter().any(|a| a == "--output-schema"), contract.is_some());
                assert_eq!(argv.iter().any(|a| a == "--dangerously-bypass-hook-trust"), reminder);
                assert_eq!(argv.iter().any(|a| a == HOOK), reminder);
                if let Some(session) = resume {
                    assert_eq!(argv[argv.len() - 2], session);
                }
            }
        }
    }
}

/// CDX-FR-SDOV — the six levels are supported, and nothing else is.
#[test]
fn codex_supports_its_six_effort_levels_alone() {
    for level in ["low", "medium", "high", "xhigh", "max", "ultra"] {
        assert!(descriptor::CODEX.effort_supported(level), "{level}");
    }
    for level in ["minimal", "none", "persistent", "", "high\"", "ULTRA"] {
        assert!(!descriptor::CODEX.effort_supported(level), "{level:?}");
    }
}

/// CCP-FR-WZJS, EAC-FR-WBIF — Claude Code's vector with the reminder is the vector without it,
/// byte for byte, with the reminder appended.
#[test]
fn claude_s_vector_with_the_reminder_is_unchanged_but_for_the_reminder() {
    for resume in [None, Some("prior")] {
        let request = ArgvRequest {
            model_id: Some("opus"),
            effort_id: Some("high"),
            assigned_session_id: "SID",
            resume_session: resume,
            result_contract: Some(ResultContract::ReviewVerdict),
            milestone_reminder: true,
        };
        let mut expected = descriptor::CLAUDE_CODE.vendor_args(
            Some("opus"),
            Some("high"),
            "SID",
            resume,
            Some(ResultContract::ReviewVerdict),
        );
        expected.extend(descriptor::claude_code::milestone_reminder_args());
        assert_eq!(descriptor::CLAUDE_CODE.argv(&request), expected);
        // CCP-FR-07: its schema rides on the vector, so it names no file.
        assert!(descriptor::CLAUDE_CODE.launch_files().is_empty());
    }
}

/// EAC-FR-BOXF, EAC-FR-VTLG, CDX-FR-14, CDX-FR-WJZY — Codex declares one fixed set of launch
/// files: a strict schema for each result contract and the fixed reminder output. Every vector
/// names a file of that set only, and the set does not depend on the turn.
#[test]
fn codex_launch_files_are_one_fixed_set_for_every_turn() {
    let files = descriptor::CODEX.launch_files();
    let names: Vec<&str> = files.iter().map(|f| f.name).collect();
    assert_eq!(
        names,
        [
            "output-schema-review_verdict.json",
            "output-schema-graduation_verdict.json",
            "output-schema-pull_request_summary.json",
            "output-schema-work_result.json",
            "milestone-reminder.json",
        ]
    );
    for (file, contract) in files.iter().zip(codex::RESULT_CONTRACTS) {
        assert_eq!(file.name, codex::output_schema_file(contract));
        assert_eq!(file.contents, codex::output_schema(contract).into_bytes());
    }

    let reminder: Value = serde_json::from_slice(&files[4].contents).expect("the reminder is JSON");
    assert_eq!(
        reminder,
        json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext":
            "Milestone reminder: if you are now in a different milestone, write its start marker \
             in your next text message. A start marker marks every earlier milestone done."}})
    );
    assert_eq!(files, descriptor::CODEX.launch_files());

    // Every file a vector names is in the set.
    for contract in codex::RESULT_CONTRACTS {
        let argv = descriptor::CODEX.argv(&request(None, Some(contract), true));
        let at = argv.iter().position(|a| a == "--output-schema").expect("schema");
        let named = argv[at + 1].strip_prefix("/opt/synthesis/vendor/").expect("vendor dir");
        assert!(names.contains(&named), "{named} is a launch file");
    }
}

/// EAC-FR-BOXF — the directory name is the digest of what the files hold: the same set gives the
/// same name in any order, and a different set gives another.
#[test]
fn the_launch_files_digest_is_content_addressed() {
    let a = LaunchFile { name: "a.json", contents: b"{}".to_vec() };
    let b = LaunchFile { name: "b.json", contents: b"[]".to_vec() };
    let one = descriptor::launch_files_digest(&[a.clone(), b.clone()]);
    assert_eq!(one, descriptor::launch_files_digest(&[b.clone(), a.clone()]));
    assert_eq!(one.len(), 64);
    assert!(one.chars().all(|c| c.is_ascii_hexdigit()));
    let changed = LaunchFile { name: "b.json", contents: b"[1]".to_vec() };
    assert_ne!(one, descriptor::launch_files_digest(&[a.clone(), changed]));
    let renamed = LaunchFile { name: "c.json", contents: b"[]".to_vec() };
    assert_ne!(one, descriptor::launch_files_digest(&[a, renamed]));
}

/// Every object schema inside `schema`, with the JSON pointer it sits at.
fn walk<'a>(schema: &'a Value, at: String, out: &mut Vec<(String, &'a Value)>) {
    match schema {
        Value::Object(map) => {
            out.push((at.clone(), schema));
            for (key, value) in map {
                walk(value, format!("{at}/{key}"), out);
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                walk(item, format!("{at}/{i}"), out);
            }
        }
        _ => {}
    }
}

/// CDX-FR-XIGW, CDX-FR-BDZW — the strict schema of every contract keeps every rule of strict
/// structured output, and still describes the envelope.
#[test]
fn every_contract_s_strict_schema_keeps_the_strict_rules() {
    for contract in ALL_CONTRACTS {
        let text = codex::output_schema(contract);
        let schema: Value = serde_json::from_str(&text).expect("the schema is JSON");
        let mut nodes = Vec::new();
        walk(&schema, String::new(), &mut nodes);
        for (at, node) in nodes {
            let map = node.as_object().unwrap();
            // A `properties` map names fields; it is not a schema.
            if at.ends_with("/properties") {
                continue;
            }
            for keyword in ["oneOf", "if", "then", "else", "maxProperties", "$schema"] {
                assert!(!map.contains_key(keyword), "{contract:?}{at} keeps {keyword}");
            }
            let types: Vec<&str> = match map.get("type") {
                Some(Value::String(t)) => vec![t.as_str()],
                Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            };
            if types.contains(&"object") || map.contains_key("properties") {
                let props: Vec<&str> = map["properties"]
                    .as_object()
                    .map(|p| p.keys().map(String::as_str).collect())
                    .unwrap_or_default();
                let mut required: Vec<&str> =
                    map["required"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
                required.sort_unstable();
                let mut sorted = props.clone();
                sorted.sort_unstable();
                assert_eq!(required, sorted, "{contract:?}{at}: every property is required");
                assert_eq!(map["additionalProperties"], json!(false), "{contract:?}{at}");
            }
            if types.contains(&"array") {
                assert!(map.contains_key("items"), "{contract:?}{at}: an array names its items");
            }
            if let (Some(Value::Array(values)), true) = (map.get("enum"), types.contains(&"null")) {
                assert!(values.contains(&Value::Null), "{contract:?}{at}: a nullable enum");
            }
        }
        let properties = &schema["properties"];
        assert_eq!(properties["metadata"], json!({"type": "null"}), "{contract:?}");
        assert_eq!(properties["result"]["anyOf"][0], json!({"type": "null"}), "{contract:?}");
        assert_eq!(
            properties["result"]["anyOf"][1],
            codex::to_strict(contract.schema_value()),
            "{contract:?}"
        );
        let kind = &properties["escalation"]["properties"]["kind"];
        assert_eq!(kind["type"], json!(["string", "null"]), "{contract:?}");
        assert_eq!(kind["enum"], json!(["plan_update", null]), "{contract:?}");
        let description = kind["description"].as_str().expect("CDX-FR-BDZW");
        assert!(description.contains("null") && description.contains("plan_update"));
        // The envelope's own fields, and no other.
        let mut fields: Vec<&str> =
            properties.as_object().unwrap().keys().map(String::as_str).collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            [
                "escalation",
                "failure",
                "metadata",
                "outcome",
                "protocol_version",
                "result",
                "session",
                "summary"
            ]
        );
    }
}

/// CDX-FR-XIGW, CDX-FR-TPFM — the strict schema of each contract accepts a well-formed envelope
/// whose optional fields are `null`, and refuses one with a missing key or an undeclared key.
#[test]
fn every_contract_s_strict_schema_accepts_a_good_answer_and_refuses_a_bad_one() {
    for contract in ALL_CONTRACTS {
        let schema: Value = serde_json::from_str(&codex::output_schema(contract)).unwrap();
        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft7)
            .build(&schema)
            .expect("the strict schema compiles");
        let result = match contract {
            ResultContract::ReviewVerdict => {
                json!({"verdict": "ready", "rationale": "fine", "findings": [], "summary": null})
            }
            ResultContract::GraduationVerdict => {
                json!({"verdict": "ready", "rationale": "fine", "findings": []})
            }
            ResultContract::PullRequestSummary => json!({"title": "T", "description": "D"}),
            ResultContract::WorkResult => json!({"checks": [], "no_checks_reason": "none"}),
        };
        let good = json!({
            "protocol_version": 1, "outcome": "success", "summary": "done", "result": result,
            "failure": null, "escalation": null, "session": null, "metadata": null
        });
        assert!(validator.is_valid(&good), "{contract:?}: a good answer is refused");

        let mut missing = good.clone();
        missing.as_object_mut().unwrap().remove("failure");
        assert!(!validator.is_valid(&missing), "{contract:?}: a missing key is accepted");

        let mut extra = good.clone();
        extra["result"]["undeclared"] = json!(1);
        assert!(!validator.is_valid(&extra), "{contract:?}: an undeclared key is accepted");
    }
}

/// CDX-FR-XIGW — an optional property admits `null` and a required one does not; a property
/// with no `type` becomes `anyOf` of itself and `null`.
#[test]
fn the_strict_transform_makes_optional_properties_nullable_and_only_those() {
    let strict = codex::to_strict(&json!({
        "type": "object",
        "properties": {
            "needed": {"type": "string"},
            "optional": {"type": "integer"},
            "untyped": {"enum": [1, 2]},
            "free": {"description": "anything"}
        },
        "required": ["needed"]
    }));
    let props = &strict["properties"];
    assert_eq!(props["needed"]["type"], json!("string"));
    assert_eq!(props["optional"]["type"], json!(["integer", "null"]));
    assert_eq!(props["untyped"]["anyOf"][1], json!({"type": "null"}));
    assert_eq!(props["untyped"]["anyOf"][0]["enum"], json!([1, 2, null]));
    assert_eq!(props["free"], json!({"anyOf": [{"description": "anything"}, {"type": "null"}]}));
    assert_eq!(strict["required"], json!(["needed", "optional", "untyped", "free"]));
}

/// An escalation envelope whose answer leaves its optional fields out with `null`.
fn escalation_with_nulls(result: Value) -> String {
    json!({
        "protocol_version": 1,
        "outcome": "escalation_required",
        "summary": "needs a choice",
        "result": result,
        "failure": null,
        "escalation": {
            "kind": null,
            "reason": "ambiguous",
            "questions": [{ "question": "which one?", "options": null }]
        },
        "session": null,
        "metadata": null
    })
    .to_string()
}

/// CDX-FR-TPFM, EAC-FR-RBLI — under a contract, a `null` at an optional position reads as the
/// field absent, also inside the contract; without one, the same answer is refused.
#[test]
fn a_null_at_an_optional_position_reads_as_absent_under_a_contract() {
    let stream = codex_stdout(&escalation_with_nulls(Value::Null));
    let (envelope, _) = descriptor::CODEX
        .extract_for(&stream, None, Some(ResultContract::WorkResult))
        .expect("the nulls read as absent");
    let escalation = envelope.escalation.expect("an escalation");
    assert!(escalation.kind.is_none());
    assert!(escalation.questions[0].options.is_empty());
    // CDX-FR-14: a turn with no contract is held to no strict schema.
    assert_eq!(descriptor::CODEX.extract(&stream, None).unwrap_err(), EnvelopeInvalid::Malformed);

    // Inside the contract: `waived_by` and `no_checks_reason` are optional, and
    // `exit_status` is required and nullable, so it keeps its `null`.
    let text = json!({
        "protocol_version": 1, "outcome": "success", "summary": "done",
        "result": { "checks": [{ "name": "unit", "command": "cargo test", "finished": true,
            "exit_status": null, "waived_by": null }], "no_checks_reason": null },
        "failure": null, "escalation": null, "session": null, "metadata": null
    })
    .to_string();
    let stripped: Value =
        serde_json::from_str(&codex::strip_optional_nulls(&text, ResultContract::WorkResult))
            .unwrap();
    assert_eq!(
        stripped["result"],
        json!({ "checks": [{ "name": "unit", "command": "cargo test", "finished": true,
            "exit_status": null }] })
    );
    // The envelope's optional fields go too.
    for field in ["failure", "escalation", "session", "metadata"] {
        assert!(stripped.get(field).is_none(), "{field}");
    }
}

/// EAC-FR-RBLI — a `null` at a required position stays, so the answer is still refused.
#[test]
fn a_null_at_a_required_position_stays_a_violation() {
    let text = json!({
        "protocol_version": 1, "outcome": "success", "summary": null,
        "result": null, "failure": null, "escalation": null, "session": null, "metadata": null
    })
    .to_string();
    let stripped: Value =
        serde_json::from_str(&codex::strip_optional_nulls(&text, ResultContract::WorkResult))
            .unwrap();
    assert_eq!(stripped["summary"], Value::Null);
    let stream = codex_stdout(&text);
    assert!(descriptor::CODEX
        .extract_for(&stream, None, Some(ResultContract::WorkResult))
        .is_err());

    // In a verdict, `findings` is required in every branch, so its `null` stays.
    let verdict = json!({ "verdict": "ready", "rationale": "fine", "findings": null,
        "summary": null });
    let text = json!({ "protocol_version": 1, "outcome": "success", "summary": "s",
        "result": verdict })
    .to_string();
    let stripped: Value =
        serde_json::from_str(&codex::strip_optional_nulls(&text, ResultContract::ReviewVerdict))
            .unwrap();
    assert_eq!(
        stripped["result"],
        json!({ "verdict": "ready", "rationale": "fine",
        "findings": null })
    );
}

/// CDX-FR-TPFM — text that is not one JSON document reaches the decoder as it came, so it is
/// refused for what it is.
#[test]
fn text_that_is_not_one_document_is_left_to_the_decoder() {
    let two = format!("{}{}", envelope_json("success"), envelope_json("success"));
    assert_eq!(codex::strip_optional_nulls(&two, ResultContract::WorkResult), two);
    assert_eq!(
        descriptor::CODEX
            .extract_for(&codex_stdout(&two), None, Some(ResultContract::WorkResult))
            .unwrap_err(),
        EnvelopeInvalid::MultipleDocuments
    );
    assert_eq!(codex::strip_optional_nulls("prose", ResultContract::WorkResult), "prose");
}

/// CDX-FR-15 — an `error` item, which the bypass flags make the CLI write, is not fatal.
#[test]
fn an_error_item_does_not_fail_extraction() {
    let notice = json!({"type": "item.completed", "item": {"id": "item_0", "type": "error",
        "message": "`--dangerously-bypass-hook-trust` is enabled."}});
    let stream = format!("{notice}\n{}", codex_stdout(&envelope_json("success")));
    let (envelope, session) = descriptor::CODEX.extract(&stream, None).expect("extracts");
    assert_eq!(envelope.outcome, AgentOutcome::Success);
    assert_eq!(session.as_deref(), Some(CODEX_THREAD_ID));
    let activity = descriptor::CODEX.activity(&notice.to_string());
    assert_eq!(activity.kind, descriptor::ActivityKind::Error);
    assert!(activity.summary.contains("bypass-hook-trust"), "{}", activity.summary);
}

/// CDX-FR-28 — a `collab_tool_call` item reads as the tool and how many agents it reaches.
#[test]
fn a_collaboration_tool_call_reads_as_a_tool_call() {
    let line = json!({"type": "item.completed", "item": {"id": "i", "type": "collab_tool_call",
        "tool": "spawn_agent", "sender_thread_ids": "t0", "receiver_thread_ids": ["t1", "t2"],
        "status": "completed"}});
    let activity = descriptor::CODEX.activity(&line.to_string());
    assert_eq!(activity.kind, descriptor::ActivityKind::ToolCall);
    assert_eq!(activity.summary, "spawn_agent · 2 agents");
    let started = json!({"type": "item.started", "item": {"id": "i", "type": "collab_tool_call",
        "tool": "wait", "receiver_thread_ids": []}});
    assert_eq!(descriptor::CODEX.activity(&started.to_string()).summary, "wait · 0 agents…");
}
