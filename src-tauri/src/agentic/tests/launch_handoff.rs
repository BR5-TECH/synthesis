//! The executor-only launch handoff (AIC-FR-30, AIC-FR-31), and the agent
//! this module names (AIC-FR-33).

use super::*;

// -----------------------------------------------------------------------
// The executor-only launch handoff (AIC-FR-30, AIC-FR-31)
// -----------------------------------------------------------------------

/// AIC-FR-30 — the handoff answers only with launch material, and only for
/// a vendor this application can execute.
#[test]
fn the_launch_handoff_answers_only_for_executable_clis() {
    let h = claude_harness();
    verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(SAMPLE_TOKEN)))
        .expect("claude verifies");

    match resolve_agent_launch_credential(&h.ai, "claude_code").expect("handoff") {
        AgentLaunchCredential::ClaudeOauthToken(token) => {
            assert_eq!(token.expose(), SAMPLE_TOKEN);
        }
        other => panic!("expected a token, got {other:?}"),
    }

    // Codex resolves to its own login directory, with the container target
    // its pinned CLI expects.
    let home = tempfile::tempdir().expect("temp home");
    std::fs::create_dir_all(home.path().join(".codex")).expect("login dir");
    let codex = harness(
        FakeFs::with_executable(&["/usr/bin/codex"]),
        FakeRunner::saying("/usr/bin/codex", "codex 0.147.0"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    let codex = Harness {
        ai: codex.ai.with_home(home.path()),
        ..codex
    };
    verify_integration_impl(&codex.store, &codex.ai, "codex", &cli_config("/usr/bin/codex"))
        .expect("codex verifies");

    match resolve_agent_launch_credential(&codex.ai, "codex").expect("handoff") {
        AgentLaunchCredential::CodexConfigMount { source, target } => {
            assert_eq!(source, home.path().join(".codex"));
            assert_eq!(target, PathBuf::from("/home/agent/.codex"));
        }
        other => panic!("expected a config mount, got {other:?}"),
    }

    // OpenCode is CLI-kind and still refused: no execution protocol is
    // pinned for it. Neither API-kind vendor is executable either.
    for vendor in ["opencode", "claude_agent_api", "custom_agent_api"] {
        assert_eq!(
            resolve_agent_launch_credential(&h.ai, vendor).unwrap_err(),
            ERR_NOT_AN_EXECUTABLE_CLI,
            "{vendor} must not be executable"
        );
    }
    // And nothing it returns describes an integration — the variants carry
    // launch material alone, with no vendor, model, effort, or binary path.
    assert_eq!(
        resolve_agent_launch_credential(&h.ai, "no_such_vendor").unwrap_err(),
        ERR_NOT_AN_EXECUTABLE_CLI
    );
}

/// AIC-FR-30, AIC-FR-31 — the handoff type cannot be serialised or rendered, and every
/// way it can refuse names a cause without disclosing one.
#[test]
fn the_handoff_type_never_renders_and_every_refusal_is_typed() {
    // A stored token, rendered: the wrapper is what makes an accidental
    // `{:?}` in a log field harmless rather than a disclosure.
    let secret = SecretString::new(SAMPLE_TOKEN.to_string());
    let rendered = format!("{secret:?}");
    assert!(!rendered.contains(SAMPLE_TOKEN));
    assert!(!rendered.contains("sk-ant-oat01-"));
    assert!(rendered.contains("<redacted>"));

    let credential = AgentLaunchCredential::ClaudeOauthToken(SecretString::new(
        SAMPLE_TOKEN.to_string(),
    ));
    let rendered = format!("{credential:?}");
    assert!(!rendered.contains(SAMPLE_TOKEN));

    // The other variant holds no secret, but it does hold the machine
    // user's home directory — which names the person at the keyboard, and
    // which AIC-FR-31 forbids a rendering from carrying just as firmly.
    let mount = AgentLaunchCredential::CodexConfigMount {
        source: PathBuf::from("/Users/somebody/.codex"),
        target: PathBuf::from("/home/agent/.codex"),
    };
    let rendered = format!("{mount:?}");
    assert!(!rendered.contains("somebody"), "rendered the host user");
    assert!(!rendered.contains('/'), "rendered a path");
    assert!(rendered.contains("<redacted>"));

    // Claude Code verified, then its token removed from under it.
    let h = claude_harness();
    verify_integration_impl(&h.store, &h.ai, "claude_code", &claude_config(Some(SAMPLE_TOKEN)))
        .expect("verifies");
    h.keys.wipe("claude_code");
    assert_eq!(
        resolve_agent_launch_credential(&h.ai, "claude_code").unwrap_err(),
        ERR_TOKEN_MISSING
    );

    // A keychain that will not answer is its own distinction, because the
    // correction is different: unlock it rather than sign in again.
    let locked = claude_harness();
    verify_integration_impl(
        &locked.store,
        &locked.ai,
        "claude_code",
        &claude_config(Some(SAMPLE_TOKEN)),
    )
    .expect("verifies");
    locked.keys.lock_it();
    assert_eq!(
        resolve_agent_launch_credential(&locked.ai, "claude_code").unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );

    // Codex with no login directory on this machine.
    let empty_home = tempfile::tempdir().expect("temp home");
    let codex = harness(
        FakeFs::with_executable(&["/usr/bin/codex"]),
        FakeRunner::saying("/usr/bin/codex", "codex 0.147.0"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    let codex_ai = codex.ai.with_home(empty_home.path());
    let error = resolve_agent_launch_credential(&codex_ai, "codex").unwrap_err();
    assert_eq!(error, ERR_CODEX_CONFIG_MISSING);
    // AIC-FR-31: no refusal carries a credential-derived value or a
    // filesystem path — a home directory names the machine's user.
    for vendor in ["claude_code", "codex", "opencode"] {
        if let Err(message) = resolve_agent_launch_credential(&codex_ai, vendor) {
            assert!(!message.contains('/'), "{message} leaked a path");
            assert!(!message.contains(SAMPLE_TOKEN));
            assert!(!message.contains("sk-ant-oat01-"));
        }
    }
}

/// AIC-FR-30 (AIC-FR-33): the vendor this module resolves is the identity a
/// project's committed image entry is selected by, and this module holds no
/// image, Dockerfile, or Docker backend value of its own.
#[test]
fn aic_ts38_this_module_names_the_agent_and_holds_no_image_or_backend() {
    // Nothing this module returns, and nothing in the store it owns, can
    // carry an image reference, a digest, a Dockerfile path, or a Docker
    // endpoint: there is no field for one.
    const SOURCE: &str = include_str!("../../agentic.rs");
    // The production half alone: this suite names the very tokens it is
    // asserting the absence of, so scanning itself would always find them.
    // The test module is a file of its own, so the declaration that brings it
    // in is where production ends.
    let production = SOURCE
        .split_once("\n#[cfg(test)]\nmod tests;")
        .map(|(before, _)| before)
        .expect("the test module marks where production ends");
    let code: String = production
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for absent in [
        "image_ref",
        "image_digest",
        "image_name",
        "dockerfile",
        "docker_endpoint",
        "DockerBackend",
    ] {
        assert!(
            !code.contains(absent),
            "AIC-FR-33: this module names no {absent}",
        );
    }

    // The record shape carries none of them either, whatever a vendor is
    // configured with.
    let record = AgenticRecord {
        vendor: "codex".to_string(),
        binary_path: Some("/usr/local/bin/codex".to_string()),
        ..Default::default()
    };
    let json = serde_json::to_string(&record).unwrap();
    for absent in ["image", "docker", "Dockerfile"] {
        assert!(
            !json.to_lowercase().contains(&absent.to_lowercase()),
            "a record carries no {absent}: {json}",
        );
    }

    // And the executable vendors are Claude Code and Codex alone: OpenCode
    // may be configured like any other CLI-kind vendor and resolves no
    // launch, so a project that configures an image for it has configured
    // one nothing can yet run.
    assert_eq!(
        crate::project_settings::images::EXECUTABLE_VENDORS,
        ["claude_code", "codex"]
    );
    assert!(!crate::project_settings::images::is_executable_vendor("opencode"));
    for api_kind in ["claude_agent_api", "custom_agent_api"] {
        assert!(!crate::project_settings::images::is_executable_vendor(api_kind));
    }
}
