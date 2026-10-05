//! The three shared loop settings (PSS-FR-TQMV, PSS-FR-HDBN, PSS-FR-WPKS,
//! PSS-FR-ZLCF, PSS-FR-JXOU).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;
use crate::project_settings::loop_settings::SharedLoopSettings;

fn root(dir: &TempDir) -> crate::fs::RootFs {
    crate::fs::RootFs::for_root(dir.path())
}

/// A filesystem instance allowed at this test's root and nowhere else, on the
/// terms FSA-FR-19 binds every read and write in the backend to.
fn guard(dir: &TempDir) -> crate::fs::FsAccess {
    crate::fs::FsAccess::builder()
        .allow_root(dir.path())
        .build()
        .expect("a real directory")
}

/// The committed store, as bytes, so a test can put a value there that the
/// typed payload itself would refuse (PSS-FR-ZLCF).
fn write_raw(dir: &TempDir, body: &str) {
    // The write creates its own parents (FSA-FR-05).
    guard(dir)
        .write_text_atomic(project_toml_path(&root(dir)), body)
        .expect("the settings file is writable");
}

/// What the committed store holds, read back through the same instance.
fn committed(dir: &TempDir) -> String {
    guard(dir)
        .read_text(project_toml_path(&root(dir)))
        .expect("the settings file is readable")
}

// PSS-FR-TQMV, PSS-FR-HDBN, PSS-FR-WPKS, PSS-FR-ZLCF: an unconfigured project
// holds none of the three, which is the unset state each loop reads as its own
// default rather than as a bound of zero.
#[test]
fn an_unconfigured_project_holds_none_of_the_three_shared_bounds() {
    let dir = TempDir::new().unwrap();
    let config = load_project_config_from(&root(&dir)).unwrap();

    assert_eq!(config.execution_timeout_ms, Some(None));
    assert_eq!(config.provider_call_deadline_ms, Some(None));
    assert_eq!(config.retry_budget, Some(None));

    let shared = SharedLoopSettings::read(&root(&dir));
    assert_eq!(shared.execution_timeout_ms_or(7_200_000), 7_200_000);
    assert_eq!(shared.provider_call_deadline_ms_or(300_000), 300_000);
    assert_eq!(shared.retry_budget_or(2), 2);
}

// PSS-FR-TQMV, PSS-FR-HDBN, PSS-FR-WPKS, PSS-FR-JXOU: each configured bound
// round-trips through the committed store and is read back from it rather than
// from anything held in memory.
#[test]
fn a_configured_bound_round_trips_through_the_committed_store() {
    let dir = TempDir::new().unwrap();
    save_project_config_to(
        &root(&dir),
        ProjectConfig {
            execution_timeout_ms: Some(Some(60_000)),
            provider_call_deadline_ms: Some(Some(9_000)),
            retry_budget: Some(Some(4)),
            ..Default::default()
        },
    )
    .unwrap();

    // Re-read from disk, which is the "relaunch": nothing is held between.
    let config = load_project_config_from(&root(&dir)).unwrap();
    assert_eq!(config.execution_timeout_ms, Some(Some(60_000)));
    assert_eq!(config.provider_call_deadline_ms, Some(Some(9_000)));
    assert_eq!(config.retry_budget, Some(Some(4)));

    let shared = SharedLoopSettings::read(&root(&dir));
    assert_eq!(shared.execution_timeout_ms_or(7_200_000), 60_000);
    assert_eq!(shared.provider_call_deadline_ms_or(300_000), 9_000);
    assert_eq!(shared.retry_budget_or(2), 4);

    // The values sit in the committed file rather than in the per-machine one.
    let stored = committed(&dir);
    assert!(stored.contains("executionTimeoutMs"));
    assert!(stored.contains("providerCallDeadlineMs"));
    assert!(stored.contains("retryBudget"));
}

// PSS-FR-ZLCF: a stored value outside its bounds is repaired to unset on read
// rather than clamped, so a bound nobody chose never governs a loop.
#[test]
fn a_stored_bound_outside_its_range_reads_as_unset() {
    let dir = TempDir::new().unwrap();
    write_raw(
        &dir,
        "executionTimeoutMs = 999\nproviderCallDeadlineMs = 3600001\nretryBudget = 0\n",
    );
    let config = load_project_config_from(&root(&dir)).unwrap();
    assert_eq!(config.execution_timeout_ms, Some(None), "below the lower bound");
    assert_eq!(config.provider_call_deadline_ms, Some(None), "past the upper bound");
    assert_eq!(config.retry_budget, Some(None), "a budget of none is not a budget");

    // A negative value and a value that is not an integer read the same way.
    write_raw(&dir, "retryBudget = -3\nexecutionTimeoutMs = \"an hour\"\n");
    let config = load_project_config_from(&root(&dir)).unwrap();
    assert_eq!(config.retry_budget, Some(None));
    assert_eq!(config.execution_timeout_ms, Some(None));

    // The bounds themselves are inclusive at both ends.
    write_raw(
        &dir,
        "executionTimeoutMs = 1000\nproviderCallDeadlineMs = 3600000\nretryBudget = 10\n",
    );
    let config = load_project_config_from(&root(&dir)).unwrap();
    assert_eq!(config.execution_timeout_ms, Some(Some(1_000)));
    assert_eq!(config.provider_call_deadline_ms, Some(Some(3_600_000)));
    assert_eq!(config.retry_budget, Some(Some(10)));
}

// PSS-FR-ZLCF, PSS-FR-17: a write that says nothing about a shared bound
// carries the stored one through unchanged, and a value outside the bounds
// removes the key — which is how a project returns to each loop's own default.
#[test]
fn a_write_carries_a_bound_through_unchanged_and_an_out_of_range_value_clears_it() {
    let dir = TempDir::new().unwrap();
    save_project_config_to(
        &root(&dir),
        ProjectConfig {
            execution_timeout_ms: Some(Some(60_000)),
            retry_budget: Some(Some(4)),
            ..Default::default()
        },
    )
    .unwrap();

    // Another section is persisted and says nothing about the bounds.
    save_project_config_to(
        &root(&dir),
        ProjectConfig {
            line_endings: LineEndings::Crlf,
            ..Default::default()
        },
    )
    .unwrap();
    let config = load_project_config_from(&root(&dir)).unwrap();
    assert_eq!(config.line_endings, LineEndings::Crlf);
    assert_eq!(config.execution_timeout_ms, Some(Some(60_000)));
    assert_eq!(config.retry_budget, Some(Some(4)));

    // A value outside the bounds clears the key.
    save_project_config_to(
        &root(&dir),
        ProjectConfig {
            retry_budget: Some(Some(0)),
            ..Default::default()
        },
    )
    .unwrap();
    let config = load_project_config_from(&root(&dir)).unwrap();
    assert_eq!(config.retry_budget, Some(None), "the key is gone");
    assert_eq!(
        config.execution_timeout_ms,
        Some(Some(60_000)),
        "the other bounds are untouched"
    );
    let stored = committed(&dir);
    assert!(!stored.contains("retryBudget"));

    // A `null` in the payload clears a bound the same way, which is the route
    // the contract itself names.
    save_project_config_to(
        &root(&dir),
        ProjectConfig {
            execution_timeout_ms: Some(None),
            ..Default::default()
        },
    )
    .unwrap();
    let config = load_project_config_from(&root(&dir)).unwrap();
    assert_eq!(config.execution_timeout_ms, Some(None), "the key is gone");
    let stored = committed(&dir);
    assert!(!stored.contains("executionTimeoutMs"));
}

// PSS-FR-ZLCF, PSS-FR-10: a malformed **file** stays the typed error, and the
// best-effort read leaves the caller at its own defaults rather than failing a
// turn that would otherwise have run.
#[test]
fn a_malformed_store_is_the_typed_error_and_reads_as_no_configuration() {
    let dir = TempDir::new().unwrap();
    write_raw(&dir, "this is not = = toml\n");

    assert_eq!(
        load_project_config_from(&root(&dir)).unwrap_err(),
        ERR_MALFORMED_PROJECT_CONFIG
    );
    let shared = SharedLoopSettings::read(&root(&dir));
    assert_eq!(shared.execution_timeout_ms, None);
    assert_eq!(shared.provider_call_deadline_ms, None);
    assert_eq!(shared.retry_budget, None);
}
