//! The three graduation loop settings (PSS-FR-QBNE, PSS-FR-HVSM, PSS-FR-RKTW,
//! PSS-FR-YDCA, PSS-FR-OWGI, PSS-FR-LFUZ), read and written through the
//! committed store.
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;
use crate::project_settings::graduation_loop::GraduationLoopSettings;

fn root(dir: &TempDir) -> crate::fs::RootFs {
    crate::fs::RootFs::for_root(dir.path())
}

fn guard(dir: &TempDir) -> crate::fs::FsAccess {
    crate::fs::FsAccess::builder().allow_root(dir.path()).build().expect("a real directory")
}

fn write_raw(dir: &TempDir, body: &str) {
    guard(dir)
        .write_text_atomic(project_toml_path(&root(dir)), body)
        .expect("the settings file is writable");
}

fn committed(dir: &TempDir) -> String {
    guard(dir).read_text(project_toml_path(&root(dir))).expect("the settings file is readable")
}

// PSS-FR-QBNE, PSS-FR-HVSM, PSS-FR-RKTW, PSS-FR-YDCA: an unconfigured project
// holds none of the three, and the loop reads each as its default.
#[test]
fn an_unconfigured_project_reads_the_three_graduation_defaults() {
    let dir = TempDir::new().unwrap();
    let config = load_project_config_from(&root(&dir)).unwrap();
    assert_eq!(config.graduation_pass_budget, Some(None));
    assert_eq!(config.graduation_stage_time_limit_ms, Some(None));
    assert_eq!(config.graduation_total_time_limit_ms, Some(None));

    let settings = GraduationLoopSettings::try_read(&root(&dir)).unwrap();
    assert_eq!(settings.pass_budget, 2);
    assert_eq!(settings.stage_time_limit_ms, 10_800_000);
    assert_eq!(settings.total_time_limit_ms, 18_000_000);
}

// PSS-FR-OWGI, PSS-FR-JXOU, PSS-FR-WPKS, PSS-FR-TQMV: a valid value
// round-trips through the committed store under its own key, apart from the
// shared retry budget and execution timeout.
#[test]
fn a_configured_graduation_setting_round_trips_through_the_committed_store() {
    let dir = TempDir::new().unwrap();
    save_project_config_to(
        &root(&dir),
        ProjectConfig {
            graduation_pass_budget: Some(Some(5)),
            graduation_stage_time_limit_ms: Some(Some(5_400_000)),
            graduation_total_time_limit_ms: Some(Some(345_600_000)),
            retry_budget: Some(Some(3)),
            ..Default::default()
        },
    )
    .unwrap();
    let settings = GraduationLoopSettings::try_read(&root(&dir)).unwrap();
    assert_eq!(settings.pass_budget, 5);
    assert_eq!(settings.stage_time_limit_ms, 5_400_000);
    assert_eq!(settings.total_time_limit_ms, 345_600_000);
    let stored = committed(&dir);
    assert!(stored.contains("graduationPassBudget = 5"), "{stored}");
    assert!(stored.contains("retryBudget = 3"), "{stored}");

    // PSS-FR-OWGI: a payload that names nothing leaves the values, and `null`
    // clears one to unset.
    save_project_config_to(
        &root(&dir),
        ProjectConfig { graduation_pass_budget: Some(None), ..Default::default() },
    )
    .unwrap();
    let config = load_project_config_from(&root(&dir)).unwrap();
    assert_eq!(config.graduation_pass_budget, Some(None));
    assert_eq!(config.graduation_stage_time_limit_ms, Some(Some(5_400_000)));
    assert_eq!(config.retry_budget, Some(Some(3)));
}

// PSS-FR-LFUZ: a save with a value that is not valid is refused whole with the
// setting's typed error, and nothing is written.
#[test]
fn a_save_with_a_value_that_is_not_valid_writes_nothing() {
    let dir = TempDir::new().unwrap();
    write_raw(&dir, "graduationPassBudget = 3\n");
    let before = committed(&dir);
    for (config, code) in [
        (
            ProjectConfig { graduation_pass_budget: Some(Some(11)), ..Default::default() },
            "graduation_pass_budget_invalid",
        ),
        (
            ProjectConfig {
                graduation_stage_time_limit_ms: Some(Some(1_000)),
                draft_template: Some("A template".to_string()),
                ..Default::default()
            },
            "graduation_stage_time_limit_invalid",
        ),
        (
            ProjectConfig {
                graduation_total_time_limit_ms: Some(Some(345_600_000 + 1_800_000)),
                ..Default::default()
            },
            "graduation_total_time_limit_invalid",
        ),
    ] {
        assert_eq!(save_project_config_to(&root(&dir), config).unwrap_err(), code);
        assert_eq!(committed(&dir), before, "a refused save writes nothing");
    }
}

// PSS-FR-YDCA: a stored value outside its bounds, off its step, or not an
// integer is repaired to unset on read.
#[test]
fn a_stored_value_that_is_not_valid_is_repaired_to_unset_on_read() {
    let dir = TempDir::new().unwrap();
    write_raw(
        &dir,
        "graduationPassBudget = 0\ngraduationStageTimeLimitMs = 2000000\ngraduationTotalTimeLimitMs = 1.5\n",
    );
    let config = load_project_config_from(&root(&dir)).unwrap();
    assert_eq!(config.graduation_pass_budget, Some(None));
    assert_eq!(config.graduation_stage_time_limit_ms, Some(None));
    assert_eq!(config.graduation_total_time_limit_ms, Some(None));
    assert_eq!(
        GraduationLoopSettings::try_read(&root(&dir)).unwrap(),
        GraduationLoopSettings::default()
    );
}
