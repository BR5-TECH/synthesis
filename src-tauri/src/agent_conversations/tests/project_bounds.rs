//! The bounds a turn is read from the project (CVL-FR-PDXK).
//!
//! The whole-turn timeout, the provider-call deadline and the retry budget are
//! the project's own settings (`PSS-project-settings-storage.md` PSS-FR-TQMV,
//! PSS-FR-HDBN, PSS-FR-WPKS), read before every turn. What is asserted here is
//! that a **turn** runs under them rather than that an accessor returns them:
//! a wiring that read the settings and then used the registry's own values
//! would pass the accessor's test and fail every one of these.

use super::*;

/// Open a project at the harness's own root, holding the settings named.
///
/// Written as bytes through the guarded handle rather than through
/// `save_project_config`, so a test can also put a value there that the typed
/// payload itself would refuse (PSS-FR-ZLCF).
fn open_project_with(h: &Harness, settings: &[(&str, i64)]) {
    let root = h.root().path().to_path_buf();
    let body: String = settings
        .iter()
        .map(|(key, value)| format!("{key} = {value}\n"))
        .collect();
    let fs = crate::fs::FsAccess::builder()
        .allow_root(&root)
        .build()
        .expect("a real directory");
    fs.write_text_atomic(root.join(".synthesis").join("project.toml"), &body)
        .expect("the settings file is writable");

    h.app
        .state::<crate::fs::FsAccessState>()
        .install_for_worktree(&root)
        .expect("a test root is a real directory");
    let access = h
        .app
        .state::<crate::fs::FsAccessState>()
        .get()
        .expect("an instance");
    h.app
        .state::<ProjectState>()
        .set_root_with_access(root, Some(access));
}

// CVL-FR-PDXK, CVL-FR-19 / PSS-FR-WPKS: a turn spends the project's retry
// budget of physical attempts and no more, which is this loop's own meaning for
// the one shared number.
#[test]
fn a_turn_spends_the_projects_retry_budget_of_attempts_and_no_more() {
    let h = Harness::with_retries(
        // More failures than any budget could take, so what stops the turn is
        // the budget rather than the script running out.
        vec![Err(FAIL_UNREACHABLE); 8],
        fast_retries(),
        0.5,
        Duration::from_secs(30),
    );
    open_project_with(&h, &[("retryBudget", 2)]);
    h.create_agent("arch", "");

    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_UNREACHABLE));
    // Two, not the three the module's own default would have spent.
    assert_eq!(
        h.seam.call_count(),
        2,
        "the turn spent the project's budget rather than the module's default"
    );
    assert_ne!(MAX_ATTEMPTS, 2, "the budget under test is not the default");
}

// CVL-FR-PDXK, CVL-FR-19: a project that configures no budget leaves the turn
// at this module's own, so an unconfigured project behaves exactly as it did.
#[test]
fn a_project_that_configures_no_budget_leaves_the_turn_at_the_modules_own() {
    let h = Harness::with_retries(
        vec![Err(FAIL_UNREACHABLE); 8],
        fast_retries(),
        0.5,
        Duration::from_secs(30),
    );
    open_project_with(&h, &[]);
    h.create_agent("arch", "");

    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(h.seam.call_count(), MAX_ATTEMPTS);
}

// CVL-FR-PDXK / PSS-FR-ZLCF: a stored budget outside its bounds is
// repaired to unset, so a turn runs under the module's own attempts rather than
// under a bound nobody could have meant.
#[test]
fn a_budget_outside_its_bounds_leaves_the_turn_at_the_modules_own() {
    let h = Harness::with_retries(
        vec![Err(FAIL_UNREACHABLE); 8],
        fast_retries(),
        0.5,
        Duration::from_secs(30),
    );
    open_project_with(&h, &[("retryBudget", 99)]);
    h.create_agent("arch", "");

    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(h.seam.call_count(), MAX_ATTEMPTS);
}

// CVL-FR-PDXK, CVL-FR-16 / PSS-FR-TQMV: the whole-turn timeout is the
// project's. A turn under a bound of a fraction of a second is abandoned on it
// and never retried, where the registry's own thirty seconds would have let
// every attempt run.
#[test]
fn the_whole_turn_timeout_is_the_one_the_project_configures() {
    let (seam, calls, _) = super::deadlines::counting_never_answers();
    let h = Harness::with_seam_and_retries(seam, fast_retries(), Duration::from_secs(30));
    open_project_with(&h, &[("executionTimeoutMs", 1_000)]);
    h.create_agent("arch", "");

    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_TIMED_OUT));
    // CVL-FR-18: the whole-turn deadline, whose budget is spent by definition,
    // so no retry follows it.
    assert!(!terminal.retry_permitted);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the turn was abandoned on the project's bound rather than the registry's"
    );
}

// CVL-FR-PDXK, CVL-FR-17 / PSS-FR-HDBN: the provider-call deadline is the
// project's, and each attempt is given it rather than the module's own five
// minutes.
#[test]
fn the_provider_call_deadline_is_the_one_the_project_configures() {
    let (seam, _, deadlines) = super::deadlines::counting_never_answers();
    let h = Harness::with_seam_and_retries(seam, fast_retries(), Duration::from_secs(30));
    open_project_with(&h, &[("providerCallDeadlineMs", 1_000)]);
    h.create_agent("arch", "");

    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_TIMED_OUT));
    // CVL-FR-18: the provider-call deadline is recoverable, so every attempt
    // of the budget was spent on it.
    assert!(terminal.retry_permitted);

    let given = deadlines.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(given.len(), MAX_ATTEMPTS);
    for deadline in given {
        assert_eq!(
            deadline,
            Duration::from_millis(1_000),
            "each call was given the project's deadline"
        );
    }
    assert!(
        PROVIDER_CALL_TIMEOUT > Duration::from_millis(1_000),
        "the deadline under test is not the module's own"
    );
}

/// AAP-FR-FGNK: store a turn timeout on the harness's verified provider.
fn store_provider_turn_timeout(h: &Harness, ms: u64) {
    store_turn_timeout_on(h, "openrouter", ms);
}

/// AAP-FR-FGNK: store a turn timeout on one seeded provider.
///
/// Written through the registry rather than through the command, so a test can
/// use a bound of a second, which the command would refuse.
fn store_turn_timeout_on(h: &Harness, provider: &str, ms: u64) {
    let (mut records, active) = h.store().load_ai_api_registry().expect("registry");
    let record = records
        .iter_mut()
        .find(|r| r.provider == provider)
        .expect("the provider is seeded");
    record.turn_timeout_ms = Some(ms);
    h.store().save_ai_api_registry(records, active).expect("registry");
}

// CVL-FR-16, AAP-FR-TXNM: the provider's turn timeout bounds the turn where no
// project is open, in place of the registry's own value.
#[test]
fn the_providers_turn_timeout_bounds_the_turn() {
    let (seam, calls, _) = super::deadlines::counting_never_answers();
    let h = Harness::with_seam_and_retries(seam, fast_retries(), Duration::from_secs(30));
    store_provider_turn_timeout(&h, 1_000);
    h.create_agent("arch", "");

    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_TIMED_OUT));
    assert!(!terminal.retry_permitted, "CVL-FR-18: the whole-turn bound is not retried");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

// CVL-FR-16, CVL-FR-PDXK, PSS-FR-TQMV: the provider's value comes before the
// project's execution timeout.
#[test]
fn the_providers_turn_timeout_comes_before_the_projects() {
    let (seam, calls, _) = super::deadlines::counting_never_answers();
    let h = Harness::with_seam_and_retries(seam, fast_retries(), Duration::from_secs(30));
    open_project_with(&h, &[("executionTimeoutMs", 30_000)]);
    store_provider_turn_timeout(&h, 1_000);
    h.create_agent("arch", "");

    let started = std::time::Instant::now();
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_TIMED_OUT));
    assert!(!terminal.retry_permitted);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the turn stopped on the provider's second, not the project's thirty",
    );
}

// AAP-FR-TXNM, CVL-FR-16: the turn reads the timeout of the provider that serves
// it, which is the provider the project resolves to.
#[test]
fn the_turn_reads_its_own_providers_timeout_and_not_the_active_ones() {
    let (seam, calls, _) = super::deadlines::counting_never_answers();
    let h = Harness::with_seam_and_retries(seam, fast_retries(), Duration::from_secs(30));
    h.seed_second_provider("anthropic");
    store_turn_timeout_on(&h, "anthropic", 1_000);
    h.create_plain_agent("arch");

    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.failure.as_deref(), Some(FAIL_TIMED_OUT));
    assert!(!terminal.retry_permitted);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

// AAP-FR-TXNM: another provider's timeout bounds nothing of this turn.
#[test]
fn another_providers_timeout_does_not_bound_this_turn() {
    let seam = Arc::new(
        ScriptedCompletion::new(vec![Ok("Late but welcome.".into())])
            .with_delay(Duration::from_millis(1_500)),
    );
    let h = Harness::build_full(
        Box::new(seam.clone()),
        seam,
        DEFAULT_CONCURRENCY,
        Box::new(FixedSecrets),
        Duration::from_secs(30),
        fast_retries(),
        0.5,
    );
    h.seed_second_provider("anthropic");
    store_turn_timeout_on(&h, "openrouter", 1_000);
    h.create_plain_agent("arch");

    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
}

// CVL-FR-16: precedence is not "the smaller of the two". A provider that allows
// more than the project lets an answer arrive after the project's bound.
#[test]
fn a_longer_provider_timeout_outlasts_a_shorter_project_one() {
    let seam = Arc::new(
        ScriptedCompletion::new(vec![Ok("Late but welcome.".into())])
            .with_delay(Duration::from_millis(1_500)),
    );
    let h = Harness::build_full(
        Box::new(seam.clone()),
        seam,
        DEFAULT_CONCURRENCY,
        Box::new(FixedSecrets),
        Duration::from_secs(30),
        fast_retries(),
        0.5,
    );
    open_project_with(&h, &[("executionTimeoutMs", 1_000)]);
    store_provider_turn_timeout(&h, 5_000);
    h.create_agent("arch", "");

    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
}

// CVL-FR-PDXK: the provider's timeout replaces the turn bound alone. The
// project's retry budget still applies beside it.
#[test]
fn the_projects_retry_budget_still_applies_beside_a_provider_timeout() {
    let h = Harness::with_retries(
        vec![Err(FAIL_UNREACHABLE); 8],
        fast_retries(),
        0.5,
        Duration::from_secs(30),
    );
    open_project_with(&h, &[("retryBudget", 2)]);
    store_provider_turn_timeout(&h, 60_000);
    h.create_agent("arch", "");

    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(h.seam.call_count(), 2);
}

// CVL-FR-16: five minutes where neither the provider nor the project sets one.
#[test]
fn the_default_turn_timeout_is_five_minutes() {
    assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(300));
}
