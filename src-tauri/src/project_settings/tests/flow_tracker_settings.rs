//! The flow tracker settings (PSS-FR-VGWF, PSS-FR-DNVN).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

fn fresh() -> (TempDir, crate::fs::RootFs) {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    (dir, root)
}

fn project_toml(dir: &TempDir) -> String {
    std::fs::read_to_string(dir.path().join(".synthesis/project.toml")).unwrap()
}

fn custom() -> FlowTrackerSettings {
    FlowTrackerSettings {
        deliverable_type: "Initiative".into(),
        epic_type: "Epic".into(),
        feature_type: "Feature".into(),
        task_type: "Chore".into(),
        defect_type: "Defect".into(),
    }
}

/// PSS-FR-VGWF: an unset store reads as `Feature`, `Feature`, `Feature`,
/// `Task`, and `Bug`.
#[test]
fn an_unset_flow_tracker_store_reads_as_the_defaults() {
    let (_dir, root) = fresh();
    let settings = load_flow_tracker_settings_from(&root).unwrap();
    assert_eq!(settings.deliverable_type, "Feature");
    assert_eq!(settings.epic_type, "Feature");
    assert_eq!(settings.feature_type, "Feature");
    assert_eq!(settings.task_type, "Task");
    assert_eq!(settings.defect_type, "Bug");
}

/// PSS-FR-VGWF: the five Types are stored in a table of their own in the
/// project-public store and read back as written. Nothing goes to the
/// project-local store.
#[test]
fn the_flow_tracker_settings_round_trip_in_their_own_table() {
    let (dir, root) = fresh();
    save_flow_tracker_settings_to(&root, &custom()).unwrap();
    assert_eq!(load_flow_tracker_settings_from(&root).unwrap(), custom());
    let text = project_toml(&dir);
    assert!(text.contains("[flowTracker]"));
    assert!(text.contains("taskType = \"Chore\""));
    assert!(!dir.path().join(".synthesis/local.toml").exists());
}

/// PSS-FR-VGWF: a blank value, a value that is not a string, and an absent
/// value each read as their own default, one field at a time.
#[test]
fn an_invalid_flow_tracker_value_reads_as_its_default() {
    let (dir, root) = fresh();
    std::fs::write(
        dir.path().join(".synthesis/project.toml"),
        "[flowTracker]\ndeliverableType = \"   \"\nepicType = 3\ntaskType = \" Chore \"\n",
    )
    .unwrap();
    let settings = load_flow_tracker_settings_from(&root).unwrap();
    assert_eq!(settings.deliverable_type, "Feature");
    assert_eq!(settings.epic_type, "Feature");
    assert_eq!(settings.feature_type, "Feature");
    assert_eq!(settings.task_type, "Chore");
    assert_eq!(settings.defect_type, "Bug");
}

/// PSS-FR-DNVN: a write of the flow tracker settings carries every other
/// project-public section through, and a write of another section carries the
/// flow tracker settings through.
#[test]
fn the_flow_tracker_settings_and_other_sections_carry_each_other_through() {
    let (dir, root) = fresh();
    std::fs::write(
        dir.path().join(".synthesis/project.toml"),
        "lineEndings = \"crlf\"\n\n[plugins]\nenabled = [\"git-pr\"]\n",
    )
    .unwrap();
    let publication =
        GithubPublicationSettings { sub_issue_type: "Bug".into(), ..Default::default() };
    save_github_publication_settings_to(&root, &publication).unwrap();
    let polling = GithubPollingSettings { interval_minutes: Some(15), ..Default::default() };
    save_github_polling_settings_to(&root, &polling).unwrap();

    save_flow_tracker_settings_to(&root, &custom()).unwrap();
    assert_eq!(load_github_publication_settings_from(&root).unwrap(), publication);
    assert_eq!(load_github_polling_settings_from(&root).unwrap(), polling);
    let text = project_toml(&dir);
    assert!(text.contains("git-pr") && text.contains("crlf"));

    save_github_publication_settings_to(&root, &GithubPublicationSettings::default()).unwrap();
    save_github_polling_settings_to(&root, &GithubPollingSettings::default()).unwrap();
    let mut config = load_project_config_from(&root).unwrap();
    config.line_endings = LineEndings::Lf;
    save_project_config_to(&root, config).unwrap();
    assert_eq!(load_flow_tracker_settings_from(&root).unwrap(), custom());
}

/// PSS-FR-VGWF: a malformed project-public file is the typed error of
/// PSS-FR-10, and a write does not overwrite it.
#[test]
fn a_malformed_store_refuses_the_flow_tracker_settings() {
    let (dir, root) = fresh();
    std::fs::write(dir.path().join(".synthesis/project.toml"), "not = [valid").unwrap();
    assert_eq!(
        load_flow_tracker_settings_from(&root),
        Err(ERR_MALFORMED_PROJECT_CONFIG.to_string())
    );
    assert!(save_flow_tracker_settings_to(&root, &custom()).is_err());
    assert_eq!(project_toml(&dir), "not = [valid");
}
