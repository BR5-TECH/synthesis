//! The graduation concurrency limit (PSS-FR-JRWC, PSS-FR-KMBT, PSS-FR-FHQU,
//! PSS-FR-ZVSD).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

fn root(dir: &TempDir) -> crate::fs::RootFs {
    crate::fs::RootFs::for_root(dir.path())
}

fn write_raw(dir: &TempDir, body: &str) {
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(dir.path().join(".synthesis").join("project.toml"), body).unwrap();
}

fn stored(dir: &TempDir) -> toml::Table {
    let text = std::fs::read_to_string(dir.path().join(".synthesis").join("project.toml")).unwrap();
    toml::from_str(&text).unwrap()
}

fn limit_of(dir: &TempDir) -> GraduationConcurrency {
    load_project_config_from(&root(dir))
        .expect("config")
        .graduation_concurrency_limit
        .expect("a read always names the limit")
}

fn save(dir: &TempDir, limit: Option<GraduationConcurrency>) {
    save_project_config_to(
        &root(dir),
        ProjectConfig {
            graduation_concurrency_limit: limit,
            ..Default::default()
        },
    )
    .expect("saved");
}

// PSS-FR-JRWC: the limit defaults to one where the project configures none.
#[test]
fn the_limit_defaults_to_one() {
    let dir = TempDir::new().unwrap();
    assert_eq!(limit_of(&dir), GraduationConcurrency::Limited(1));
}

// PSS-FR-JRWC: a configured limit is read back as it was written.
#[test]
fn a_configured_limit_round_trips() {
    let dir = TempDir::new().unwrap();
    save(&dir, Some(GraduationConcurrency::limited(4)));
    assert_eq!(limit_of(&dir), GraduationConcurrency::Limited(4));
}

// PSS-FR-KMBT: a positive integer the Graduation section does not list is read
// as it is, and a later write that names no limit keeps it.
#[test]
fn a_stored_positive_integer_outside_the_listed_choices_is_preserved() {
    let dir = TempDir::new().unwrap();
    write_raw(&dir, "graduationConcurrencyLimit = 3\n");
    assert_eq!(limit_of(&dir), GraduationConcurrency::Limited(3));
    save_project_config_to(
        &root(&dir),
        ProjectConfig {
            line_endings: LineEndings::Crlf,
            ..Default::default()
        },
    )
    .expect("saved");
    assert_eq!(limit_of(&dir), GraduationConcurrency::Limited(3));
}

// PSS-FR-FHQU: `unlimited` is stored as the named text, and read back as the
// named value.
#[test]
fn unlimited_is_stored_as_a_named_value() {
    let dir = TempDir::new().unwrap();
    save(&dir, Some(GraduationConcurrency::Unlimited));
    assert_eq!(
        stored(&dir).get("graduationConcurrencyLimit"),
        Some(&toml::Value::String("unlimited".to_string())),
    );
    assert_eq!(limit_of(&dir), GraduationConcurrency::Unlimited);
}

// PSS-FR-KMBT: a stored integer below one, a value that is not an integer, and a
// text other than `unlimited` are repaired to one.
#[test]
fn a_malformed_stored_limit_is_repaired_to_one() {
    for raw in ["0", "-1", "1.5", "\"two\"", "\"Unlimited\"", "true", "[2]"] {
        let dir = TempDir::new().unwrap();
        write_raw(&dir, &format!("graduationConcurrencyLimit = {raw}\n"));
        assert_eq!(
            limit_of(&dir),
            GraduationConcurrency::Limited(1),
            "{raw} should be repaired to one",
        );
    }
}

// PSS-FR-FHQU: an integer below one is written as one, so the store never holds
// a limit nothing could start under.
#[test]
fn an_integer_below_one_is_written_as_one() {
    let dir = TempDir::new().unwrap();
    save(&dir, Some(GraduationConcurrency::Limited(0)));
    assert_eq!(
        stored(&dir).get("graduationConcurrencyLimit"),
        Some(&toml::Value::Integer(1)),
    );
    let parsed: ProjectConfig =
        serde_json::from_str(r#"{"graduationConcurrencyLimit":-4}"#).unwrap();
    assert_eq!(parsed.graduation_concurrency_limit, Some(GraduationConcurrency::Limited(1)));
}

// PSS-FR-ZVSD: a payload that names no limit leaves the stored one as it stands,
// the unlimited value included.
#[test]
fn a_write_that_names_no_limit_leaves_the_stored_limit_alone() {
    let dir = TempDir::new().unwrap();
    save(&dir, Some(GraduationConcurrency::Unlimited));
    save(&dir, None);
    assert_eq!(limit_of(&dir), GraduationConcurrency::Unlimited);
    let parsed: ProjectConfig = serde_json::from_str(r#"{"lineEndings":"lf"}"#).unwrap();
    assert_eq!(parsed.graduation_concurrency_limit, None, "absent says nothing");
}

// PSS-FR-ZVSD: writing the limit carries the other project-public sections
// through unchanged.
#[test]
fn writing_the_limit_carries_the_other_sections_through() {
    let dir = TempDir::new().unwrap();
    save_project_config_to(
        &root(&dir),
        ProjectConfig {
            line_endings: LineEndings::Crlf,
            draft_template: Some("# Template".to_string()),
            ..Default::default()
        },
    )
    .expect("saved");
    save(&dir, Some(GraduationConcurrency::limited(8)));
    let config = load_project_config_from(&root(&dir)).expect("config");
    assert_eq!(config.draft_template.as_deref(), Some("# Template"));
}

// PSS-FR-FHQU: the limit crosses the seam as an integer or as the text
// `unlimited`, and a text that is neither is refused.
#[test]
fn the_limit_crosses_the_seam_as_an_integer_or_the_named_text() {
    let json = |limit| {
        serde_json::to_value(ProjectConfig {
            graduation_concurrency_limit: Some(limit),
            ..Default::default()
        })
        .unwrap()["graduationConcurrencyLimit"]
            .clone()
    };
    assert_eq!(json(GraduationConcurrency::Limited(2)), serde_json::json!(2));
    assert_eq!(json(GraduationConcurrency::Unlimited), serde_json::json!("unlimited"));
    let parsed: ProjectConfig =
        serde_json::from_str(r#"{"graduationConcurrencyLimit":"unlimited"}"#).unwrap();
    assert_eq!(parsed.graduation_concurrency_limit, Some(GraduationConcurrency::Unlimited));
    assert!(serde_json::from_str::<ProjectConfig>(r#"{"graduationConcurrencyLimit":"many"}"#).is_err());
}

// GRD-FR-KKKN: a finite limit permits a claim only while slots are free, and
// `unlimited` always does.
#[test]
fn a_limit_permits_a_claim_only_while_a_slot_is_free() {
    let two = GraduationConcurrency::limited(2);
    assert!(two.permits(1));
    assert!(!two.permits(2));
    assert!(!two.permits(5), "a limit lowered below the usage permits nothing");
    assert!(GraduationConcurrency::Unlimited.permits(1_000));
    assert!(!GraduationConcurrency::Unlimited.is_full(1_000));
}

// PSS-FR-KMBT: a stored positive integer is read as it is, a very large one
// included, on the terms the seam reads it.
#[test]
fn a_very_large_stored_integer_is_read_as_the_most_a_count_holds() {
    let dir = TempDir::new().unwrap();
    write_raw(&dir, "graduationConcurrencyLimit = 99999999999\n");
    assert_eq!(limit_of(&dir), GraduationConcurrency::Limited(u32::MAX));
}

// PSS-FR-LGKY: a limit stored only under the earlier stream key is read as the
// project-wide limit, and a malformed one is repaired to one on the terms of
// PSS-FR-KMBT.
#[test]
fn the_earlier_stream_key_is_read_where_the_current_key_is_absent() {
    let dir = TempDir::new().unwrap();
    write_raw(&dir, "streamConcurrencyLimit = 3\n");
    assert_eq!(limit_of(&dir), GraduationConcurrency::Limited(3));
    write_raw(&dir, "streamConcurrencyLimit = 0\n");
    assert_eq!(limit_of(&dir), GraduationConcurrency::Limited(1));
    write_raw(&dir, "streamConcurrencyLimit = \"many\"\n");
    assert_eq!(limit_of(&dir), GraduationConcurrency::Limited(1));
}

// PSS-FR-LGKY: the current key wins where both keys are stored.
#[test]
fn the_current_key_wins_over_the_earlier_stream_key() {
    let dir = TempDir::new().unwrap();
    write_raw(&dir, "streamConcurrencyLimit = 3\ngraduationConcurrencyLimit = 2\n");
    assert_eq!(limit_of(&dir), GraduationConcurrency::Limited(2));
    write_raw(&dir, "streamConcurrencyLimit = 3\ngraduationConcurrencyLimit = \"unlimited\"\n");
    assert_eq!(limit_of(&dir), GraduationConcurrency::Unlimited);
}

// PSS-FR-LGKY: a save that names a limit stores it under the current key and
// removes the earlier stream key.
#[test]
fn a_save_that_names_a_limit_removes_the_earlier_stream_key() {
    let dir = TempDir::new().unwrap();
    write_raw(&dir, "streamConcurrencyLimit = 3\n");
    save(&dir, Some(GraduationConcurrency::limited(4)));
    let table = stored(&dir);
    assert_eq!(table.get("graduationConcurrencyLimit"), Some(&toml::Value::Integer(4)));
    assert!(table.get("streamConcurrencyLimit").is_none());
    assert_eq!(limit_of(&dir), GraduationConcurrency::Limited(4));
}

// PSS-FR-LGKY, PSS-FR-ZVSD: a save that names no limit leaves both keys as they
// stand, so the earlier value is still the limit afterwards.
#[test]
fn a_save_that_names_no_limit_leaves_the_earlier_stream_key_alone() {
    let dir = TempDir::new().unwrap();
    write_raw(&dir, "streamConcurrencyLimit = 3\n");
    save(&dir, None);
    let table = stored(&dir);
    assert_eq!(table.get("streamConcurrencyLimit"), Some(&toml::Value::Integer(3)));
    assert!(table.get("graduationConcurrencyLimit").is_none());
    assert_eq!(limit_of(&dir), GraduationConcurrency::Limited(3));
}
