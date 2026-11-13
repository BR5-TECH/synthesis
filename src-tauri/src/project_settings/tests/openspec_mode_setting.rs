//! The OpenSpec mode setting (PSS-FR-IMDN, PSS-FR-JXKK, PSS-FR-HVUZ), read and
//! written through the committed store.
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

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

fn mode(value: &str) -> ProjectConfig {
    ProjectConfig { openspec_mode: Some(value.to_string()), ..Default::default() }
}

// PSS-FR-IMDN: an unconfigured project reads `auto`.
#[test]
fn an_unconfigured_project_reads_auto() {
    let dir = TempDir::new().unwrap();
    let config = load_project_config_from(&root(&dir)).unwrap();
    assert_eq!(config.openspec_mode.as_deref(), Some("auto"));
}

// PSS-FR-IMDN, PSS-FR-HVUZ: `off` is stored under `openspecMode`, reads back,
// and a later `auto` removes the key.
#[test]
fn off_round_trips_and_auto_removes_the_key() {
    let dir = TempDir::new().unwrap();
    save_project_config_to(&root(&dir), mode("off")).unwrap();
    assert!(committed(&dir).contains("openspecMode = \"off\""), "{}", committed(&dir));
    let config = load_project_config_from(&root(&dir)).unwrap();
    assert_eq!(config.openspec_mode.as_deref(), Some("off"));

    save_project_config_to(&root(&dir), mode("auto")).unwrap();
    assert!(!committed(&dir).contains("openspecMode"), "{}", committed(&dir));
    let config = load_project_config_from(&root(&dir)).unwrap();
    assert_eq!(config.openspec_mode.as_deref(), Some("auto"));
}

// PSS-FR-HVUZ: a payload that names no mode leaves a stored `off` alone.
#[test]
fn a_payload_that_names_no_mode_keeps_off() {
    let dir = TempDir::new().unwrap();
    write_raw(&dir, "openspecMode = \"off\"\n");
    let config = ProjectConfig { draft_template: Some("T".to_string()), ..Default::default() };
    save_project_config_to(&root(&dir), config).unwrap();
    assert!(committed(&dir).contains("openspecMode = \"off\""), "{}", committed(&dir));
}

// PSS-FR-JXKK: a malformed stored value reads `auto`, and the next write of
// the store drops it.
#[test]
fn a_malformed_stored_value_reads_auto_and_the_next_write_drops_it() {
    let dir = TempDir::new().unwrap();
    write_raw(&dir, "openspecMode = \"never\"\n");
    let config = load_project_config_from(&root(&dir)).unwrap();
    assert_eq!(config.openspec_mode.as_deref(), Some("auto"));

    let unrelated = ProjectConfig { draft_template: Some("T".to_string()), ..Default::default() };
    save_project_config_to(&root(&dir), unrelated).unwrap();
    assert!(!committed(&dir).contains("openspecMode"), "{}", committed(&dir));
}

// PSS-FR-HVUZ: another value is refused whole with `openspec_mode_invalid`, and
// nothing is written.
#[test]
fn another_value_is_refused_and_nothing_is_written() {
    let dir = TempDir::new().unwrap();
    write_raw(&dir, "openspecMode = \"off\"\n");
    let before = committed(&dir);
    let config = ProjectConfig {
        openspec_mode: Some("on".to_string()),
        draft_template: Some("T".to_string()),
        ..Default::default()
    };
    assert_eq!(
        save_project_config_to(&root(&dir), config),
        Err("openspec_mode_invalid".to_string())
    );
    assert_eq!(committed(&dir), before);
}
