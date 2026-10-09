//! Claude Code's Custom Gateway launch environment at the executor
//! (EAC-FR-05, EAC-FR-15, EAC-FR-29, EAC-FR-OWPP).

use super::*;

const GATEWAY_TOKEN: &str = "gw-FAKE-TEST-TOKEN-NOT-A-CREDENTIAL-k2Qz";
const GATEWAY_URL: &str = "https://gateway.example.com";
const LONG_VALUE: &str = "proxy-secret-value-123";
const SHORT_VALUE: &str = "abc";

fn gateway_harness(token_var: Option<&str>, entries: &[&str]) -> Harness {
    let harness = harness_for("claude_code");
    let config = VerifyConfig {
        path: Some("/usr/bin/claude".into()),
        auth_mode: Some("custom_gateway".into()),
        gateway_base_url: Some(GATEWAY_URL.into()),
        gateway_token_var: token_var.map(str::to_string),
        gateway_token: Some(GATEWAY_TOKEN.into()),
        env_vars: Some(entries.iter().map(|e| e.to_string()).collect()),
        ..Default::default()
    };
    verify_integration_impl(&harness.store, &harness.ai, "claude_code", &config)
        .expect("gateway verifies");
    harness
}

/// EAC-FR-05, EAC-FR-15, CCP-FR-20 — gateway mode passes the URL, the token under the
/// author's name, and the author's entries; each by name, and none by value.
#[test]
fn a_gateway_launch_passes_every_variable_by_name() {
    let harness = gateway_harness(
        Some("ANTHROPIC_API_KEY"),
        &[&format!("HTTPS_PROXY={LONG_VALUE}")],
    );
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let outcome = run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();

    assert_eq!(recorded.env.get("ANTHROPIC_BASE_URL").map(String::as_str), Some(GATEWAY_URL));
    assert_eq!(recorded.env.get("ANTHROPIC_API_KEY").map(String::as_str), Some(GATEWAY_TOKEN));
    assert_eq!(recorded.env.get("HTTPS_PROXY").map(String::as_str), Some(LONG_VALUE));
    assert!(
        !recorded.env.contains_key("CLAUDE_CODE_OAUTH_TOKEN"),
        "the subscription token is not used in this mode"
    );
    assert_eq!(recorded.env.len(), 3);

    let argv = recorded.argv.join(" ");
    for name in ["ANTHROPIC_BASE_URL", "ANTHROPIC_API_KEY", "HTTPS_PROXY"] {
        assert!(recorded.argv.contains(&name.to_string()), "{name} is named in the argv");
    }
    for value in [GATEWAY_TOKEN, GATEWAY_URL, LONG_VALUE] {
        assert!(!argv.contains(value), "a value reached the argv");
    }
    assert!(!String::from_utf8_lossy(&recorded.stdin).contains(GATEWAY_TOKEN));
    assert!(!format!("{outcome:?}").contains(GATEWAY_TOKEN));

    // EAC-FR-OWPP: the variables are named in the order the handoff gives, and
    // the Claude Code mounts are the workspace and the session directory only —
    // no login directory and no configuration file (CCP-FR-20).
    let named: Vec<&str> = recorded
        .argv
        .windows(2)
        .filter(|pair| pair[0] == "--env" && !pair[1].contains('='))
        .map(|pair| pair[1].as_str())
        .collect();
    assert_eq!(named, vec!["ANTHROPIC_BASE_URL", "ANTHROPIC_API_KEY", "HTTPS_PROXY"]);
    assert_eq!(recorded.argv.iter().filter(|a| *a == "--mount").count(), 2);

    // EAC-FR-15, EAC-FR-29: nothing the launch logged carries a value.
    let page = super::super::log_buffer()
        .query(&crate::logging::LogFilter::default(), None, 20_000)
        .expect("query");
    let rendered = serde_json::to_string(&page).expect("serialise");
    for value in [GATEWAY_TOKEN, LONG_VALUE] {
        assert!(!rendered.contains(value), "a value reached the log");
    }
}

/// EAC-FR-29 (AIC-FR-ISOC) — the token and a long author value are masked in a
/// failure record, a short author value and the gateway URL are not.
#[test]
fn only_the_flagged_values_are_masked_in_a_failure_record() {
    let harness = gateway_harness(
        None,
        &[&format!("HTTPS_PROXY={LONG_VALUE}"), &format!("LEVEL={SHORT_VALUE}")],
    );
    let runtime = RecordingRuntime::refused(
        "",
        &format!("refused (token={GATEWAY_TOKEN}, proxy={LONG_VALUE}, level={SHORT_VALUE}, url={GATEWAY_URL})\n"),
        1,
    );
    let _ = run(&harness, runtime.clone(), task("go"));
    let record = failure_record(&container_name(&runtime.only_run().argv));
    let excerpt = record
        .fields
        .get("stderr_excerpt")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    assert!(!excerpt.contains(GATEWAY_TOKEN), "the token reached the record");
    assert!(!excerpt.contains(LONG_VALUE), "a long author value reached the record");
    assert!(excerpt.contains(&masked_form(GATEWAY_TOKEN)));
    assert!(excerpt.contains(&masked_form(LONG_VALUE)));
    assert!(excerpt.contains(&format!("level={SHORT_VALUE}")), "a short value is left readable");
    assert!(excerpt.contains(GATEWAY_URL), "the gateway URL is not a credential");
}

/// EAC-FR-OWPP — a variable the executor sets itself is not overridden, and the
/// name alone is reported.
#[test]
fn an_author_variable_cannot_replace_one_the_executor_sets() {
    use std::collections::BTreeSet;

    let mut env_names = vec!["GIT_OPTIONAL_LOCKS".to_string(), "KEPT".to_string()];
    let mut env = BTreeMap::new();
    env.insert("GIT_OPTIONAL_LOCKS".to_string(), SecretString::new("author-value-1".into()));
    env.insert("KEPT".to_string(), SecretString::new("value-one".into()));
    let mut masked: BTreeSet<String> = ["GIT_OPTIONAL_LOCKS".to_string()].into();
    let literals = vec![("GIT_OPTIONAL_LOCKS".to_string(), "0".to_string())];

    let dropped = super::super::launch::drop_executor_set_variables(
        &mut env_names,
        &mut env,
        &mut masked,
        &literals,
    );
    assert_eq!(dropped, vec!["GIT_OPTIONAL_LOCKS"]);
    assert_eq!(env_names, vec!["KEPT"]);
    assert!(env.contains_key("KEPT") && !env.contains_key("GIT_OPTIONAL_LOCKS"));
    assert!(masked.is_empty(), "a dropped value is not masked, because it is not passed");
}

/// EAC-FR-OWPP, AIC-FR-XZCS — a hand-edited registry entry that names a variable
/// the executor owns never reaches the container.
#[test]
fn a_hand_edited_entry_for_the_session_directory_never_reaches_the_container() {
    use crate::agentic::AgenticRecord;

    let harness = harness_for("claude_code");
    let (mut records, active) = harness.store.load_agentic_registry().expect("registry");
    let record: &mut AgenticRecord = records.iter_mut().find(|r| r.vendor == "claude_code").unwrap();
    record.env_vars = vec![
        "CLAUDE_CONFIG_DIR=/elsewhere".to_string(),
        "KEPT=value-one".to_string(),
    ];
    harness.store.save_agentic_registry(records, active).expect("saved");

    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();

    assert!(!recorded.env.contains_key("CLAUDE_CONFIG_DIR"));
    assert_eq!(recorded.env.get("KEPT").map(String::as_str), Some("value-one"));
    assert!(!recorded.argv.iter().any(|a| a.contains("/elsewhere")));
    assert_eq!(
        recorded.argv.iter().filter(|a| a.starts_with("CLAUDE_CONFIG_DIR=")).count(),
        1,
        "the executor's own value stands once"
    );
    assert_eq!(recorded.argv.iter().filter(|a| *a == "CLAUDE_CONFIG_DIR").count(), 0);
}

/// EAC-FR-05, EAC-FR-03 — a gateway record whose token is gone no longer resolves,
/// so the launch is refused before any container exists.
#[test]
fn a_gateway_launch_without_its_token_creates_no_container() {
    use crate::agentic::AuthMode;

    // A subscription token is stored and a gateway token is not.
    let harness = harness_for("claude_code");
    let (mut records, active) = harness.store.load_agentic_registry().expect("registry");
    let record = records.iter_mut().find(|r| r.vendor == "claude_code").unwrap();
    record.auth_mode = AuthMode::CustomGateway;
    record.gateway_base_url = Some(GATEWAY_URL.to_string());
    harness.store.save_agentic_registry(records, active).expect("saved");

    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let error = run(&harness, runtime.clone(), task("go")).expect_err("refused");
    assert!(
        matches!(error, AgentExecutionError::IntegrationUnresolved(_)),
        "{error:?}"
    );
    assert_eq!(runtime.launched(), 0, "no container was created");
}
