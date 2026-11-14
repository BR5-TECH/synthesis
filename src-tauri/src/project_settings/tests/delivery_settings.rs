//! The delivery settings (PSS-FR-KZGG, PSS-FR-KZYD).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;
use crate::delivery::settings::{
    CreateTypes, DeliverySettings, DeliverySourceChoice, MappedKind, ReadEntry,
};

fn fresh() -> (TempDir, crate::fs::RootFs) {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    (dir, root)
}

fn project_toml(dir: &TempDir) -> String {
    std::fs::read_to_string(dir.path().join(".synthesis/project.toml")).unwrap()
}

fn write_toml(dir: &TempDir, text: &str) {
    std::fs::write(dir.path().join(".synthesis/project.toml"), text).unwrap();
}

fn custom() -> DeliverySettings {
    DeliverySettings {
        source: DeliverySourceChoice::File { flow_id: "plans/beta.flow".into() },
        create_types: CreateTypes {
            epic: "Initiative".into(),
            feature: "Feature".into(),
            task: "Chore".into(),
            defect: "Defect".into(),
        },
        read_entries: vec![
            ReadEntry { type_name: "Story".into(), kind: MappedKind::Feature },
            ReadEntry { type_name: "Spike".into(), kind: MappedKind::Exclude },
        ],
        untyped: Some(MappedKind::Task),
    }
}

/// PSS-FR-KZGG: an unset store reads as the source `automatic`, the create
/// Types `Epic`, `Feature`, `Task`, and `Bug`, no read entry, and an unset
/// untyped choice.
#[test]
fn an_unset_delivery_store_reads_as_the_defaults() {
    let (_dir, root) = fresh();
    let settings = load_delivery_settings_from(&root).unwrap();
    assert_eq!(settings.source, DeliverySourceChoice::Automatic);
    assert_eq!(
        (
            settings.create_types.epic.as_str(),
            settings.create_types.feature.as_str(),
            settings.create_types.task.as_str(),
            settings.create_types.defect.as_str(),
        ),
        ("Epic", "Feature", "Task", "Bug")
    );
    assert!(settings.read_entries.is_empty());
    assert_eq!(settings.untyped, None);
}

/// PSS-FR-KZGG: the settings are stored in a table of their own in the
/// project-public store and read back as written. Nothing goes to the
/// project-local store.
#[test]
fn the_delivery_settings_round_trip_in_their_own_table() {
    let (dir, root) = fresh();
    save_delivery_settings_to(&root, &custom()).unwrap();
    assert_eq!(load_delivery_settings_from(&root).unwrap(), custom());
    let text = project_toml(&dir);
    let written: toml::Table = text.parse().unwrap_or_else(|e| panic!("{e}: {text}"));
    assert_eq!(written.keys().collect::<Vec<_>>(), vec!["delivery"], "{text}");
    assert_eq!(written["delivery"]["source"]["kind"].as_str(), Some("file"));
    assert_eq!(written["delivery"]["source"]["flowId"].as_str(), Some("plans/beta.flow"));
    assert_eq!(written["delivery"]["createTypes"]["task"].as_str(), Some("Chore"));
    assert!(!dir.path().join(".synthesis/local.toml").exists());

    // The `github` choice and an unset untyped choice round-trip too. An
    // unset choice is an absent key.
    let github = DeliverySettings {
        source: DeliverySourceChoice::Github,
        untyped: None,
        read_entries: Vec::new(),
        ..custom()
    };
    save_delivery_settings_to(&root, &github).unwrap();
    assert_eq!(load_delivery_settings_from(&root).unwrap(), github);

    // The untyped choice `exclude` round-trips.
    let exclude = DeliverySettings { untyped: Some(MappedKind::Exclude), ..custom() };
    save_delivery_settings_to(&root, &exclude).unwrap();
    assert_eq!(load_delivery_settings_from(&root).unwrap(), exclude);
    save_delivery_settings_to(&root, &github).unwrap();
    let text = project_toml(&dir);
    assert!(!text.contains("untyped") && !text.contains("flowId"), "{text}");
}

/// PSS-FR-KZGG: a value that is not valid reads as its default, one field at
/// a time: a blank or non-string create Type, a source of another kind, a
/// `file` choice with no artifact id, a read entry without a Type or with a
/// kind outside the enumeration, and an untyped choice outside it.
#[test]
fn an_invalid_delivery_value_reads_as_its_default() {
    let (dir, root) = fresh();
    write_toml(
        &dir,
        "[delivery]\nuntyped = \"story\"\n\
         [delivery.source]\nkind = \"file\"\n\
         [delivery.createTypes]\nepic = 3\nfeature = \"   \"\ntask = \" Chore \"\n\
         [[delivery.readEntries]]\ntype = \"Story\"\nkind = \"feature\"\n\
         [[delivery.readEntries]]\ntype = \"\"\nkind = \"task\"\n\
         [[delivery.readEntries]]\ntype = \"Spike\"\nkind = \"milestone\"\n",
    );
    let settings = load_delivery_settings_from(&root).unwrap();
    assert_eq!(settings.source, DeliverySourceChoice::Automatic);
    assert_eq!(settings.create_types.epic, "Epic");
    assert_eq!(settings.create_types.feature, "Feature");
    assert_eq!(settings.create_types.task, "Chore");
    assert_eq!(settings.create_types.defect, "Bug");
    assert_eq!(
        settings.read_entries,
        vec![ReadEntry { type_name: "Story".into(), kind: MappedKind::Feature }]
    );
    assert_eq!(settings.untyped, None);

    write_toml(&dir, "[delivery]\nsource = \"github\"\n");
    assert_eq!(load_delivery_settings_from(&root).unwrap().source, DeliverySourceChoice::Automatic);
    write_toml(&dir, "[delivery.source]\nkind = \"jira\"\n");
    assert_eq!(load_delivery_settings_from(&root).unwrap().source, DeliverySourceChoice::Automatic);
    write_toml(&dir, "delivery = 7\n");
    assert_eq!(load_delivery_settings_from(&root).unwrap(), DeliverySettings::default());
}

/// PSS-FR-KZYD: a write of the delivery settings carries every other
/// project-public section through, and a write of another section carries the
/// delivery settings through.
#[test]
fn the_delivery_settings_and_other_sections_carry_each_other_through() {
    let (dir, root) = fresh();
    write_toml(&dir, "lineEndings = \"crlf\"\n\n[plugins]\nenabled = [\"git-pr\"]\n");
    let publication =
        GithubPublicationSettings { sub_issue_type: "Bug".into(), ..Default::default() };
    save_github_publication_settings_to(&root, &publication).unwrap();
    let polling = GithubPollingSettings { interval_minutes: Some(15), ..Default::default() };
    save_github_polling_settings_to(&root, &polling).unwrap();

    save_delivery_settings_to(&root, &custom()).unwrap();
    assert_eq!(load_github_publication_settings_from(&root).unwrap(), publication);
    assert_eq!(load_github_polling_settings_from(&root).unwrap(), polling);
    let text = project_toml(&dir);
    assert!(text.contains("git-pr") && text.contains("crlf"), "{text}");

    save_github_publication_settings_to(&root, &GithubPublicationSettings::default()).unwrap();
    save_github_polling_settings_to(&root, &GithubPollingSettings::default()).unwrap();
    let mut config = load_project_config_from(&root).unwrap();
    config.line_endings = LineEndings::Lf;
    save_project_config_to(&root, config).unwrap();
    assert_eq!(load_delivery_settings_from(&root).unwrap(), custom());
}

/// PSS-FR-KZGG: a malformed project-public file is the typed error of
/// PSS-FR-10, and a write does not overwrite it.
#[test]
fn a_malformed_store_refuses_the_delivery_settings() {
    let (dir, root) = fresh();
    write_toml(&dir, "not = [valid");
    assert_eq!(load_delivery_settings_from(&root), Err(ERR_MALFORMED_PROJECT_CONFIG.to_string()));
    assert!(save_delivery_settings_to(&root, &custom()).is_err());
    assert_eq!(project_toml(&dir), "not = [valid");
}

/// PSS-FR-KZYD: the `[flowTracker]` table of an earlier build is carried
/// through as any other section, and is not read as delivery settings.
#[test]
fn an_earlier_flow_tracker_table_is_not_read() {
    let (dir, root) = fresh();
    write_toml(&dir, "[flowTracker]\nepicType = \"Saga\"\n");
    assert_eq!(load_delivery_settings_from(&root).unwrap(), DeliverySettings::default());
    save_delivery_settings_to(&root, &custom()).unwrap();
    assert!(project_toml(&dir).contains("epicType = \"Saga\""));
}

/// PSS-FR-KZGG: a Type with quotes, a backslash, and text that is not ASCII
/// is stored and read back unchanged, in a create Type and in a read entry.
#[test]
fn a_type_with_quotes_and_non_ascii_text_round_trips() {
    let (_dir, root) = fresh();
    let settings = DeliverySettings {
        create_types: CreateTypes {
            epic: "Épopée \"grande\"".into(),
            feature: "機能".into(),
            task: "Task \\ sub".into(),
            defect: "Ошибка ✓".into(),
        },
        read_entries: vec![ReadEntry {
            type_name: "Histoire \"x\"".into(),
            kind: MappedKind::Task,
        }],
        ..DeliverySettings::default()
    };
    save_delivery_settings_to(&root, &settings).unwrap();
    assert_eq!(load_delivery_settings_from(&root).unwrap(), settings);
}
