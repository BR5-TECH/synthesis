//! EAC-FR-07 … EAC-FR-17 — the task document, the credentials in the invocation, and the runtime's distinct failures.

use super::*;

// ---------------------------------------------------------------------------
// EAC-FR-07 / EAC-FR-08 / EAC-FR-09 — the task document
// ---------------------------------------------------------------------------

/// EAC-FR-07 — strict decoding, and validation before any container.
#[test]
fn an_invalid_task_document_is_rejected_before_launch() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying("");

    let mut wrong_version = task("go");
    wrong_version.protocol_version = 2;
    let mut empty = task("   ");
    empty.instruction = "   ".into();

    for bad in [wrong_version, empty] {
        let error = run(&harness, runtime.clone(), bad).unwrap_err();
        assert!(matches!(error, AgentExecutionError::TaskInvalid(_)));
    }
    assert_eq!(runtime.launched(), 0);

    // An undefined field anywhere in the document, and a cancellation this
    // build does not recognise, are rejections rather than ignored keys.
    for document in [
        r#"{"protocol_version":1,"instruction":"go","surprise":1,
            "execution":{"timeout_ms":1,"cancellation":"caller_controlled"}}"#,
        r#"{"protocol_version":1,"instruction":"go",
            "execution":{"timeout_ms":1,"cancellation":"executor_controlled"}}"#,
        r#"{"protocol_version":1,"instruction":"go",
            "execution":{"timeout_ms":1,"cancellation":"caller_controlled","extra":true}}"#,
    ] {
        assert!(
            serde_json::from_str::<AgentTaskRequest>(document).is_err(),
            "must not decode: {document}"
        );
    }
}

/// EAC-FR-08 — the named byte limits, checked before Docker.
#[test]
fn an_oversized_request_names_the_limit_it_exceeded() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying("");

    let mut huge_instruction = task("go");
    huge_instruction.instruction = "x".repeat(LIMIT_INSTRUCTION + 1);
    assert_eq!(
        run(&harness, runtime.clone(), huge_instruction).unwrap_err(),
        AgentExecutionError::RequestTooLarge(LimitName::Instruction)
    );

    // The whole-document bound is not redundant with the field bounds, and this
    // is why: a field is measured raw, but it travels JSON-escaped. An
    // instruction of quote characters is inside its own limit and doubles in
    // size on the wire, which is exactly the case a per-field check alone would
    // wave through.
    let mut huge_task = task(&"\"".repeat(LIMIT_INSTRUCTION));
    let mut input = serde_json::Map::new();
    input.insert("blob".into(), json!("y".repeat(LIMIT_INPUT - 64)));
    huge_task.input = Some(input);
    assert!(huge_task.check_limits().is_ok(), "every field is within bounds");
    assert_eq!(
        run(&harness, runtime.clone(), huge_task).unwrap_err(),
        AgentExecutionError::RequestTooLarge(LimitName::Task)
    );

    let mut huge_resume = task("go");
    huge_resume.resume = Some(SessionRef {
        session_id: Some("z".repeat(LIMIT_RESUME_FIELD + 1)),
        continuation_token: None,
    });
    assert_eq!(
        run(&harness, runtime.clone(), huge_resume).unwrap_err(),
        AgentExecutionError::RequestTooLarge(LimitName::ResumeField)
    );

    assert_eq!(runtime.launched(), 0);
}

/// EAC-FR-18, CCP-FR-07, EAC-FR-35 — the response contract reaches every vendor, states the decoder's
/// own rules, and agrees with the schema the validating vendor is handed.
#[test]
fn every_vendor_is_told_the_response_contract_in_the_same_words() {
    // Both vendors, and the same text: the vendor whose CLI validates the answer
    // against a schema is not excused from being told, because a caller cannot
    // choose its vendor (EAC-FR-03) and a guarantee holding for one of them is
    // one nobody could rely on.
    let mut contracts = Vec::new();
    for vendor in ["claude_code", "codex"] {
        let harness = harness_for(vendor);
        let stdout = match vendor {
            "codex" => codex_stdout(&envelope_json("success")),
            _ => valid_claude_stdout(),
        };
        let runtime = RecordingRuntime::replying(&stdout);
        run(&harness, runtime.clone(), task("go")).expect("runs");
        let (_, contract) = split_task_document(&runtime.only_run().stdin);
        contracts.push(contract);
    }
    assert_eq!(
        contracts[0], contracts[1],
        "both vendors are told the same contract, byte for byte",
    );
    assert_eq!(contracts[0], *RESPONSE_CONTRACT);

    // --- The words against the decoder ---------------------------------------
    //
    // Every expectation below is derived from the envelope type and from the
    // decoder's own answers rather than written out here. A list of literals
    // would let the contract be rewritten into anything that still happened to
    // contain the right words, which is the one failure this requirement is
    // about: EAC-FR-35's claim is that the *rules* agree, not that the text
    // mentions the right nouns.
    let text = RESPONSE_CONTRACT.as_str();

    // The field set, read off a fully populated envelope. A field added to the
    // type fails here until the words name it.
    let fields = envelope_field_names();
    assert_eq!(fields.len(), 8, "the envelope's field set changed shape");
    for field in &fields {
        assert!(
            text.contains(&format!("`{field}`")),
            "the contract must name the `{field}` field (EAC-FR-35)",
        );
    }
    // Backticked bullets, one per field and no more, so prose that merely
    // contains the words does not pass for a field list. The exclusivity bullets
    // open with a quoted outcome rather than a bare field name, which is what
    // separates the two lists.
    //
    // EAC-FR-35 states the escalation object to its own depth, and its bullets
    // name that object's fields rather than the envelope's, so the envelope's
    // list is the part before that section begins.
    let (envelope_part, escalation_part) = text
        .split_once("The `escalation` object carries")
        .expect("the contract states the escalation object's own shape (EAC-FR-35)");
    let bullets: Vec<&str> = envelope_part
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("- `") && !line.starts_with("- `\""))
        .collect();
    assert_eq!(
        bullets.len(),
        fields.len(),
        "one bullet per envelope field, and no others: {bullets:#?}",
    );

    // CCP-FR-07, EAC-FR-35 / EAC-FR-18: the escalation is stated to its full depth — its
    // own field set, the question count, the option count each question allows,
    // and each of a proposed response's three fields with its limit. An agent
    // that must guess how many questions it may ask asks one and loses the rest.
    for named in [
        "`reason`",
        "`questions`",
        "`question`",
        "`options`",
        "`answer`",
        "`summary`",
        "`description`",
    ] {
        assert!(
            escalation_part.contains(named),
            "the escalation's shape must name {named} (EAC-FR-35)",
        );
    }
    // Each bound is looked for in the clause that states it rather than
    // anywhere in the section. A bare substring sweep is vacuous here: the byte
    // limits alone put "1", "2", "5", "8" and "12" into the text (128, 2048,
    // 512, 256), so a contract that said "at most fifty words" would pass one.
    let clause = |after: &str| -> String {
        let from = escalation_part
            .find(after)
            .unwrap_or_else(|| panic!("the contract states no rule for {after:?} (EAC-FR-35)"));
        let rest = &escalation_part[from..];
        let end = rest.find("\n-").unwrap_or(rest.len());
        rest[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let states = |text: &str, bound: usize| {
        let digits = bound.to_string();
        text.contains(&digits) || text.contains(&spelled(&digits))
    };
    let questions_clause = clause("`questions` —");
    assert!(
        states(&questions_clause, MIN_ESCALATION_QUESTIONS)
            && states(&questions_clause, MAX_ESCALATION_QUESTIONS),
        "the question count is stated where the questions are: {questions_clause}",
    );
    let options_clause = clause("`options` —");
    assert!(
        states(&options_clause, MAX_QUESTION_OPTIONS),
        "the option count is stated where the options are: {options_clause}",
    );
    let summary_clause = clause("`summary` —");
    assert!(
        states(&summary_clause, MAX_SUMMARY_WORDS)
            && states(&summary_clause, LIMIT_OPTION_SUMMARY),
        "a summary's word and byte bounds are stated together: {summary_clause}",
    );
    let description_clause = clause("`description` —");
    assert!(
        states(&description_clause, MAX_DESCRIPTION_SENTENCES)
            && states(&description_clause, MAX_DESCRIPTION_WORDS)
            && states(&description_clause, LIMIT_OPTION_DESCRIPTION),
        "a description's sentence, word, and byte bounds likewise: {description_clause}",
    );
    assert!(
        states(&clause("`reason` —"), LIMIT_ESCALATION_REASON),
        "and the reason's byte bound",
    );
    assert!(
        states(&clause("`question` —"), LIMIT_ESCALATION_QUESTION),
        "and each question's",
    );
    assert!(
        states(&clause("`answer` —"), LIMIT_OPTION_ANSWER),
        "and a proposed response's answer's",
    );

    // EAC-FR-18, CCP-FR-07, EAC-FR-35: and the vendor schema states the same structural rules, so an
    // agent reading both is told one contract rather than two. Each literal is
    // bound to the Rust constant the decoder enforces, rather than to a number
    // written here twice.
    let schema: serde_json::Value = serde_json::from_str(
        crate::tools::agent_exec::descriptor::claude_code::ENVELOPE_SCHEMA,
    )
    .expect("the vendor schema parses");
    let escalation_schema = &schema["properties"]["escalation"];
    let questions_schema = &escalation_schema["properties"]["questions"];
    assert_eq!(questions_schema["minItems"], json!(MIN_ESCALATION_QUESTIONS));
    assert_eq!(questions_schema["maxItems"], json!(MAX_ESCALATION_QUESTIONS));
    let question_schema = &questions_schema["items"];
    assert_eq!(question_schema["additionalProperties"], json!(false));
    let options_schema = &question_schema["properties"]["options"];
    assert_eq!(options_schema["maxItems"], json!(MAX_QUESTION_OPTIONS));
    let option_schema = &options_schema["items"];
    assert_eq!(option_schema["additionalProperties"], json!(false));
    // CCP-FR-07: a non-blank value is structural and exactly expressible, so
    // the schema states it — with a pattern rather than `minLength` alone,
    // which would accept the whitespace the decoder trims and refuses.
    for field in ["answer", "summary", "description"] {
        assert_eq!(option_schema["properties"][field]["pattern"], json!(r"\S"));
    }
    assert_eq!(
        escalation_schema["properties"]["reason"]["pattern"],
        json!(r"\S"),
    );
    assert_eq!(
        question_schema["properties"]["question"]["pattern"],
        json!(r"\S"),
    );
    // CCP-FR-07: and the two classes it cannot express faithfully stay with the
    // decoder rather than being transcribed more freely than they are enforced.
    // A byte bound written as a character bound, or a word count written as a
    // pattern, would pass an agent's answer here and refuse it afterwards.
    let no_such_key = |value: &serde_json::Value, key: &str| {
        assert!(
            value.get(key).is_none(),
            "the schema states no {key} for the escalation (CCP-FR-07): {value}",
        );
    };
    for field in ["answer", "summary", "description"] {
        no_such_key(&option_schema["properties"][field], "maxLength");
    }
    no_such_key(&escalation_schema["properties"]["reason"], "maxLength");
    no_such_key(&question_schema["properties"]["question"], "maxLength");

    // The outcome names, read off the enum rather than spelled here.
    for outcome in ALL_OUTCOMES {
        let name = outcome_name(outcome);
        assert!(
            text.contains(&format!("`\"{name}\"`")),
            "the contract must name the {name:?} outcome (EAC-FR-35)",
        );
    }

    // The exclusivity between an outcome and its payload, checked rule by rule
    // against what the decoder actually does. `carried` and `excluded` are
    // discovered by probing the decoder, so a change to EAC-FR-18 makes this
    // fail until the words are brought back into line — and deleting a bullet
    // fails it too, because the clause is then not found at all.
    for outcome in ALL_OUTCOMES {
        let name = outcome_name(outcome);
        let (carried, excluded) = decoder_payload_rule(outcome);
        let clause = exclusivity_clause(&text, &name);
        let (says_carries, says_neither) = clause_payloads(&clause);
        assert_eq!(
            says_carries, carried,
            "the contract says {name:?} carries {says_carries:?}, the decoder \
             accepts {carried:?} — one contract, not two (EAC-FR-35, EAC-FR-18)\n\
             clause: {clause}",
        );
        assert_eq!(
            says_neither, excluded,
            "the contract says {name:?} excludes {says_neither:?}, the decoder \
             refuses {excluded:?} (EAC-FR-35, EAC-FR-18)\nclause: {clause}",
        );
    }

    // The rules that are not about exclusivity, each paired with the decoder
    // behaviour it describes, so the sentence cannot be dropped while the rule
    // it states still holds.
    let bullet_for = |field: &str| {
        bullets
            .iter()
            .find(|line| line.starts_with(&format!("- `{field}`")))
            .unwrap_or_else(|| panic!("no bullet for `{field}`"))
            .to_string()
    };
    assert!(
        bullet_for("summary").contains("never empty"),
        "the contract must say `summary` is never empty, which the decoder enforces",
    );
    assert!(
        decode_envelope(&minimal_envelope(AgentOutcome::Success, Some("")).to_string()).is_err(),
        "and the decoder does enforce it",
    );
    assert!(
        bullet_for("metadata").contains("carries nothing"),
        "the contract must say `metadata` carries nothing (EAC-FR-20)",
    );
    assert!(
        text.contains("no others") && text.contains("refused"),
        "the contract must say the field set is closed and a stray field refused",
    );
    assert!(
        decode_envelope(
            &json!({"protocol_version":1,"outcome":"success","summary":"s","surprise":1})
                .to_string()
        )
        .is_err(),
        "and the decoder does refuse a field the contract does not name",
    );

    // The contract and the schema the validating vendor receives describe one
    // contract rather than two (CCP-FR-07), over the same derived field set.
    let schema = descriptor::claude_code::ENVELOPE_SCHEMA;
    for field in &fields {
        assert!(
            schema.contains(&format!("\"{field}\"")),
            "{field} is stated by the schema as well as by the words",
        );
    }
    assert!(
        schema.contains(r#""maxProperties":0"#),
        "the schema states the empty metadata allowlist the words also state",
    );
    assert!(
        schema.contains(r#""minLength":1"#),
        "the schema states the non-empty summary the words also state",
    );

    // Two turns carrying different tasks are told the identical contract, and it
    // carries no part of either. The second task carries input and resume as
    // well, so "no project material" is checked against a task that has some.
    const MARKER: &str = "MARKER-TASK-5b2c";
    const IN_INPUT: &str = "MARKER-INPUT-9d41";
    const IN_RESUME: &str = "MARKER-RESUME-0c7e";
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let mut loaded = task(MARKER);
    let mut input = serde_json::Map::new();
    input.insert("context".into(), json!(IN_INPUT));
    loaded.input = Some(input);
    loaded.resume = Some(SessionRef {
        session_id: Some(IN_RESUME.to_string()),
        continuation_token: None,
    });
    run(&harness, runtime.clone(), loaded).expect("runs");
    let (_, second) = split_task_document(&runtime.only_run().stdin);
    assert_eq!(second, contracts[0], "the contract does not vary with the task");
    for marker in [MARKER, IN_INPUT, IN_RESUME] {
        assert!(
            !second.contains(marker),
            "the contract carries no part of a task: {marker}",
        );
    }
}

/// The three outcomes, so a test iterates the enum rather than a list of words.
const ALL_OUTCOMES: [AgentOutcome; 3] = [
    AgentOutcome::Success,
    AgentOutcome::Failure,
    AgentOutcome::EscalationRequired,
];

/// An outcome as it appears on the wire, read off its own serialization.
fn outcome_name(outcome: AgentOutcome) -> String {
    serde_json::to_value(outcome)
        .expect("an outcome serializes")
        .as_str()
        .expect("as a string")
        .to_string()
}

/// The envelope's field names, read off a fully populated value.
///
/// From the type rather than from a list here, so a field added to
/// `AgentResponseEnvelope` is a field the contract has to start naming.
fn envelope_field_names() -> Vec<String> {
    let populated = AgentResponseEnvelope {
        protocol_version: PROTOCOL_VERSION,
        outcome: AgentOutcome::Success,
        summary: "s".into(),
        result: Some(serde_json::Map::new()),
        failure: Some(AgentFailure {
            code: "c".into(),
            message: "m".into(),
            retryable: true,
        }),
        escalation: Some(AgentEscalation {
            reason: "r".into(),
            questions: vec![AgentEscalationQuestion {
                question: "q".into(),
                options: Vec::new(),
            }],
        }),
        session: Some(SessionRef {
            session_id: None,
            continuation_token: None,
        }),
        metadata: Some(serde_json::Map::new()),
    };
    serde_json::to_value(populated)
        .expect("an envelope serializes")
        .as_object()
        .expect("into an object")
        .keys()
        .cloned()
        .collect()
}

/// The three payload fields an outcome is exclusive over.
const PAYLOADS: [&str; 3] = ["result", "failure", "escalation"];

/// A decodable envelope of `outcome`, carrying `payload` and nothing else.
fn envelope_with(outcome: AgentOutcome, payload: Option<&str>) -> serde_json::Value {
    let mut document = minimal_envelope(outcome, None);
    if let Some(name) = payload {
        let value = match name {
            "result" => json!({}),
            "failure" => json!({"code":"c","message":"m","retryable":true}),
            _ => json!({"reason":"r","questions":[{"question":"q"}]}),
        };
        document[name] = value;
    }
    document
}

fn minimal_envelope(outcome: AgentOutcome, summary: Option<&str>) -> serde_json::Value {
    json!({
        "protocol_version": PROTOCOL_VERSION,
        "outcome": outcome_name(outcome),
        "summary": summary.unwrap_or("s"),
    })
}

/// Which payload the decoder accepts beside `outcome`, and which two it refuses.
///
/// Discovered by asking the decoder rather than by restating EAC-FR-18, so this
/// stays true if the rule changes and the words do not.
fn decoder_payload_rule(outcome: AgentOutcome) -> (Vec<String>, Vec<String>) {
    let mut carried = Vec::new();
    let mut excluded = Vec::new();
    for payload in PAYLOADS {
        let document = envelope_with(outcome, Some(payload));
        if decode_envelope(&document.to_string()).is_ok() {
            carried.push(payload.to_string());
        } else {
            excluded.push(payload.to_string());
        }
    }
    (carried, excluded)
}

/// The contract's own sentence about which payloads accompany `outcome`.
///
/// Whitespace is flattened first, because the sentence wraps in the file and a
/// line-based reading would miss half of it. A missing clause panics rather than
/// returning empty: a deleted rule must fail the test that reads it.
fn exclusivity_clause(contract: &str, outcome: &str) -> String {
    let flat = contract.split_whitespace().collect::<Vec<_>>().join(" ");
    let opening = format!("`\"{outcome}\"` carries");
    let start = flat.find(&opening).unwrap_or_else(|| {
        panic!("the contract states no rule for the {outcome:?} outcome (EAC-FR-35)")
    });
    let rest = &flat[start..];
    let end = rest.find('.').map(|at| at + 1).unwrap_or(rest.len());
    rest[..end].to_string()
}

/// The payloads a clause says are carried, and the ones it says are not.
///
/// Everything backticked before the word `neither` is carried; everything after
/// it is excluded. Only the three payload names are counted, so a clause that
/// mentions another field does not shift the reading.
fn clause_payloads(clause: &str) -> (Vec<String>, Vec<String>) {
    let split = clause.find("neither").unwrap_or(clause.len());
    let named = |text: &str| {
        PAYLOADS
            .iter()
            .filter(|payload| text.contains(&format!("`{payload}`")))
            .map(|payload| payload.to_string())
            .collect::<Vec<_>>()
    };
    (named(&clause[..split]), named(&clause[split..]))
}

/// EAC-FR-08, EAC-FR-35, EAC-FR-07 — the contract is the executor's alone, and the size bound is
/// applied to the document the agent actually receives.
#[test]
fn the_response_contract_is_the_executors_and_no_callers() {
    // A caller cannot supply one, three ways over. The type it hands in defines
    // no such field, so nothing it serializes carries the key...
    let plain = serde_json::to_value(task("go")).expect("a task serializes");
    assert!(
        plain.get("response_contract").is_none(),
        "no field of AgentTaskRequest can express a contract (EAC-FR-35)",
    );
    // ...a document carrying the key does not decode into one (EAC-FR-07)...
    let document = r#"{"protocol_version":1,"instruction":"go","response_contract":"mine",
            "execution":{"timeout_ms":1000,"cancellation":"caller_controlled"}}"#;
    let error = serde_json::from_str::<AgentTaskRequest>(document)
        .expect_err("a caller-supplied contract is an undefined field");
    assert!(
        error.to_string().contains("response_contract"),
        "the refusal names the field: {error}",
    );
    // ...and a caller that smuggles one through the structured input, which is a
    // free-form object it does own, does not displace the executor's. This is the
    // assertion the two above cannot make: they hold with the whole of EAC-FR-35
    // deleted, because they are statements about a type that predates it.
    let harness = harness_for("claude_code");
    let smuggler_runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let mut smuggler = task("go");
    let mut input = serde_json::Map::new();
    input.insert("response_contract".into(), json!("answer however you like"));
    smuggler.input = Some(input);
    run(&harness, smuggler_runtime.clone(), smuggler).expect("runs");
    let (sent, contract) = split_task_document(&smuggler_runtime.only_run().stdin);
    assert_eq!(
        contract, *RESPONSE_CONTRACT,
        "the contract at the top of the document is the executor's",
    );
    assert_eq!(
        sent.input.as_ref().and_then(|i| i.get("response_contract")),
        Some(&json!("answer however you like")),
        "and the caller's own key stayed inside its own object, unaltered",
    );

    // A document that fits with the contract beside it still dispatches, so the
    // bound below is a bound rather than a blanket refusal.
    let fitting_runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let mut fitting = task("go");
    fitting.instruction = "x".repeat(LIMIT_INSTRUCTION);
    run(&harness, fitting_runtime.clone(), fitting).expect("a task inside the bound runs");
    assert_eq!(fitting_runtime.launched(), 1);

    // EAC-FR-35: the contract is added before the whole-document bound is
    // applied, so a task that fits on its own and does not fit with the contract
    // beside it is refused rather than dispatched. The instruction is sized to
    // land the pair just past the bound while every field bound still passes.
    // A character that costs six bytes escaped, so the document can be walked up
    // to the whole-document bound while the raw instruction stays well inside its
    // own — which is the only way the two bounds can be told apart.
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying("");
    let mut straddling = task("go");
    let measure = |text: &str| {
        let mut probe = straddling.clone();
        probe.instruction = text.to_string();
        serde_json::to_vec(&probe).expect("serializes").len()
    };
    let per = measure("\u{1}\u{1}") - measure("\u{1}");
    let overhead = measure("\u{1}") - per;
    straddling.instruction = "\u{1}".repeat((LIMIT_TASK - overhead) / per);

    assert!(
        straddling.check_limits().is_ok(),
        "every field bound passes; only the document as sent is too large",
    );
    let callers_half = serde_json::to_vec(&straddling).expect("serializes").len();
    assert!(
        callers_half <= LIMIT_TASK,
        "the caller's half alone is inside the bound — the contract is what tips it",
    );
    assert!(
        callers_half + RESPONSE_CONTRACT.len() > LIMIT_TASK,
        "the pair is over it",
    );
    assert_eq!(
        run(&harness, runtime.clone(), straddling).unwrap_err(),
        AgentExecutionError::RequestTooLarge(LimitName::Task)
    );
    assert_eq!(runtime.launched(), 0, "the bound is checked before any container");
}

/// EAC-FR-09 — sensitive caller data reaches stdin and nothing else.
#[test]
fn task_data_appears_only_on_stdin() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());

    const INSTRUCTION: &str = "MARKER-INSTRUCTION-e3f1";
    const INPUT_VALUE: &str = "MARKER-INPUT-91ab";
    const RESUME_TOKEN: &str = "MARKER-RESUME-77cd";

    let mut request = task(INSTRUCTION);
    let mut input = serde_json::Map::new();
    input.insert("secret_context".into(), json!(INPUT_VALUE));
    request.input = Some(input);
    request.resume = Some(SessionRef {
        session_id: Some(RESUME_TOKEN.to_string()),
        continuation_token: None,
    });

    let outcome = run(&harness, runtime.clone(), request).expect("runs");
    let recorded = runtime.only_run();

    let stdin = String::from_utf8(recorded.stdin.clone()).expect("utf8");
    for marker in [INSTRUCTION, INPUT_VALUE] {
        assert!(stdin.contains(marker), "{marker} must reach stdin");
    }

    // The argv carries the resume *session id* by contract (it is a vendor
    // handle, not caller content), but never the instruction or the input.
    let argv = recorded.argv.join(" ");
    for marker in [INSTRUCTION, INPUT_VALUE] {
        assert!(!argv.contains(marker), "{marker} leaked into the argv");
    }
    for (_, value) in &recorded.env {
        for marker in [INSTRUCTION, INPUT_VALUE, RESUME_TOKEN] {
            assert!(!value.contains(marker), "{marker} leaked into the environment");
        }
    }
    let rendered = format!("{outcome:?}");
    for marker in [INSTRUCTION, INPUT_VALUE] {
        assert!(!rendered.contains(marker), "{marker} leaked into the result");
    }
}

// ---------------------------------------------------------------------------
// EAC-FR-15, EAC-FR-29 / EAC-FR-ZKMR, EAC-FR-40, EAC-FR-13, EAC-FR-16, EAC-FR-31 — credentials in the invocation
// ---------------------------------------------------------------------------

/// EAC-FR-15, EAC-FR-29 — the token is in one container's environment and nowhere else.
#[test]
fn the_oauth_token_reaches_one_container_environment_only() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let outcome = run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();

    assert_eq!(
        recorded.env.get("CLAUDE_CODE_OAUTH_TOKEN").map(String::as_str),
        Some(SAMPLE_TOKEN)
    );
    assert_eq!(recorded.env.len(), 1, "no other variable carries it");

    // Not in the argv: the vector names the *variable*, which is not a secret.
    assert!(!recorded.argv.iter().any(|a| a.contains(SAMPLE_TOKEN)));
    assert!(recorded.argv.contains(&"CLAUDE_CODE_OAUTH_TOKEN".to_string()));
    // Not on stdin, and not in the returned result.
    assert!(!String::from_utf8_lossy(&recorded.stdin).contains(SAMPLE_TOKEN));
    assert!(!format!("{outcome:?}").contains(SAMPLE_TOKEN));

    // A Codex launch inherits nothing: the variable is absent entirely.
    let codex = harness_for("codex");
    let codex_runtime = RecordingRuntime::replying(&codex_stdout(&envelope_json("success")));
    run(&codex, codex_runtime.clone(), task("go")).expect("runs");
    assert!(codex_runtime.only_run().env.is_empty());
}

/// EAC-FR-ZKMR, EAC-FR-40, EAC-FR-13, EAC-FR-16, EAC-FR-31 — the Codex mount is read/write and addressed by `CODEX_HOME`, and
/// nothing else is mounted.
#[test]
fn the_codex_configuration_mount_is_writable_and_nothing_else_is_mounted() {
    let harness = harness_for("codex");
    let runtime = RecordingRuntime::replying(&codex_stdout(&envelope_json("success")));
    run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();

    let mounts: Vec<&String> = recorded
        .argv
        .iter()
        .enumerate()
        .filter(|(i, _)| *i > 0 && recorded.argv[i - 1] == "--mount")
        .map(|(_, value)| value)
        .collect();

    assert_eq!(mounts.len(), 2, "the worktree and the login directory, only");
    assert!(mounts[0].ends_with(&format!(",target={}", harness.workspace_target())));
    assert!(!mounts[0].contains("readonly"));
    assert!(mounts[1].contains("target=/home/agent/.codex"));
    // CDX-FR-23: this vendor writes its session rollout files beneath the same
    // directory it reads its login from, so a read-only mount would leave every
    // session unresumable and an expired credential unrefreshable.
    assert!(
        !mounts[1].contains("readonly"),
        "the login mount is also the session directory and must be writable"
    );
    // And it is named to the CLI, which is what makes it the login directory
    // rather than an unused mount.
    assert!(recorded
        .argv
        .iter()
        .any(|a| a == "CODEX_HOME=/home/agent/.codex"));

    // Nothing dangerous is mounted or enabled.
    let argv = recorded.argv.join(" ");
    for forbidden in [
        "/var/run/docker.sock",
        "--privileged",
        "--publish",
        "-p 0",
        "source=/,",
    ] {
        assert!(!argv.contains(forbidden), "{forbidden} must not appear");
    }
    assert!(argv.contains("--cap-drop ALL"));
    assert!(argv.contains("--security-opt no-new-privileges"));
}

// ---------------------------------------------------------------------------
// EAC-FR-17 — the runtime's three distinct failures
// ---------------------------------------------------------------------------

/// EAC-FR-17. (EAC-FR-14, host ownership of agent-created files, needs a real
/// container and is exercised by `docker/agent-images/verify.sh` plus a manual
/// run rather than here — the seam has no filesystem to own anything.)
#[test]
fn runtime_image_and_launch_failures_stay_distinct() {
    let harness = harness_for("claude_code");

    let no_runtime = RecordingRuntime::broken(false, true, None);
    assert_eq!(
        run(&harness, no_runtime.clone(), task("go")).unwrap_err(),
        AgentExecutionError::RuntimeUnavailable
    );
    assert_eq!(no_runtime.launched(), 0);

    let no_image = RecordingRuntime::broken(true, false, None);
    assert_eq!(
        run(&harness, no_image.clone(), task("go")).unwrap_err(),
        AgentExecutionError::ImageUnavailable("claude_code".into())
    );
    assert_eq!(no_image.launched(), 0);

    let refused = RecordingRuntime::broken(
        true,
        true,
        Some(RuntimeError::LaunchFailed("permission denied".into())),
    );
    assert_eq!(
        run(&harness, refused.clone(), task("go")).unwrap_err(),
        AgentExecutionError::LaunchFailed("permission denied".into())
    );
    // Even a launch that failed after the container was named is cleaned up.
    assert_eq!(refused.removed.lock().unwrap().len(), 1);
}
