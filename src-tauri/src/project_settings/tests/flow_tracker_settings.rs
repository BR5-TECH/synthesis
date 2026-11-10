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
        epic_type: "Initiative".into(),
        feature_type: "Feature".into(),
        task_type: "Chore".into(),
        defect_type: "Defect".into(),
    }
}

/// PSS-FR-VGWF / FTS-FR-LMBE: an unset store reads as `Epic`, `Feature`,
/// `Task`, and `Bug`.
#[test]
fn an_unset_flow_tracker_store_reads_as_the_defaults() {
    let (_dir, root) = fresh();
    let settings = load_flow_tracker_settings_from(&root).unwrap();
    assert_eq!(settings.epic_type, "Epic");
    assert_eq!(settings.feature_type, "Feature");
    assert_eq!(settings.task_type, "Task");
    assert_eq!(settings.defect_type, "Bug");
}

/// PSS-FR-VGWF: the four Types are stored in a table of their own in the
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
        "[flowTracker]\nepicType = 3\nfeatureType = \"   \"\ntaskType = \" Chore \"\n",
    )
    .unwrap();
    let settings = load_flow_tracker_settings_from(&root).unwrap();
    assert_eq!(settings.epic_type, "Epic");
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

/// PSS-FR-VGWF: a `deliverableType` that an earlier build stored is ignored on
/// read, and the next save writes the four keys alone.
#[test]
fn a_stored_deliverable_type_is_ignored() {
    let (dir, root) = fresh();
    std::fs::write(
        dir.path().join(".synthesis/project.toml"),
        "[flowTracker]\ndeliverableType = \"Initiative\"\nepicType = \"Saga\"\n",
    )
    .unwrap();
    let settings = load_flow_tracker_settings_from(&root).unwrap();
    assert_eq!(settings, FlowTrackerSettings { epic_type: "Saga".into(), ..Default::default() });
    save_flow_tracker_settings_to(&root, &settings).unwrap();
    let text = project_toml(&dir);
    assert!(!text.contains("deliverableType"), "{text}");
    for key in ["epicType", "featureType", "taskType", "defectType"] {
        assert!(text.contains(key), "{key} in {text}");
    }
}

/// PSS-FR-VGWF / PSS-FR-DNVN: a save where no `project.toml` exists yet
/// creates it with the settings table alone.
#[test]
fn a_save_without_a_project_file_creates_it() {
    let (dir, root) = fresh();
    assert!(!dir.path().join(".synthesis/project.toml").exists());
    save_flow_tracker_settings_to(&root, &custom()).unwrap();
    assert_eq!(load_flow_tracker_settings_from(&root).unwrap(), custom());
    assert!(project_toml(&dir).contains("[flowTracker]"));
}

/// PSS-FR-VGWF: a Type with quotes, a backslash, and text that is not ASCII
/// is stored and read back unchanged.
#[test]
fn a_type_with_quotes_and_non_ascii_text_round_trips() {
    let (_dir, root) = fresh();
    let settings = FlowTrackerSettings {
        epic_type: "Épopée \"grande\"".into(),
        feature_type: "機能".into(),
        task_type: "Task \\ sub".into(),
        defect_type: "Ошибка ✓".into(),
    };
    save_flow_tracker_settings_to(&root, &settings).unwrap();
    assert_eq!(load_flow_tracker_settings_from(&root).unwrap(), settings);
}
