//! The debug information of a `dev` and a `test` build
//! (`specifications/infra/RDP-rust-dev-profile.md`).

const MANIFEST: &str = include_str!("../Cargo.toml");

fn manifest() -> toml::Value {
    toml::from_str(MANIFEST).expect("Cargo.toml is malformed")
}

// RDP-FR-RFMT: the workspace crates keep the line tables, so a panic and a
// backtrace show the file and the line of each workspace frame.
#[test]
fn the_dev_profile_keeps_line_tables_for_the_workspace() {
    let manifest = manifest();
    let debug = manifest["profile"]["dev"].get("debug").and_then(|v| v.as_str());
    assert_eq!(debug, Some("line-tables-only"));
}

// RDP-FR-RQEL: a dependency carries no debug information.
#[test]
fn the_dev_profile_gives_dependencies_no_debug_information() {
    let manifest = manifest();
    let debug = manifest["profile"]["dev"]["package"]["*"].get("debug");
    assert_eq!(debug.and_then(|v| v.as_bool()), Some(false));
}

// RDP-FR-RFMT, RDP-FR-RQEL: nothing replaces the two tables. Cargo reads a
// profile only from the manifest of the workspace root, a profile in a Cargo
// configuration file has a higher precedence than one in the manifest, and a
// table for one dependency has a higher precedence than the `"*"` table.
#[test]
fn nothing_overrides_the_dev_debug_setting() {
    let manifest = manifest();
    assert!(manifest.get("workspace").is_some(), "Cargo.toml is not the workspace root");
    let config: toml::Value = toml::from_str(include_str!("../.cargo/config.toml")).unwrap();
    assert!(config.get("profile").is_none(), ".cargo/config.toml sets a profile");
    let packages = manifest["profile"]["dev"]["package"].as_table().unwrap();
    for (name, table) in packages {
        if name != "*" {
            assert!(table.get("debug").is_none(), "[profile.dev.package.{name:?}] sets debug");
        }
    }
}

// RDP-FR-RFMT, RDP-FR-RQEL: the `test` profile takes its debug setting from
// `dev`. A `[profile.test]` table that sets its own `debug` would give the
// test binary, which each check links, its full size again.
#[test]
fn the_test_profile_takes_the_dev_debug_setting() {
    let manifest = manifest();
    let test = manifest["profile"].get("test");
    let sets_debug = |table: Option<&toml::Value>| table.and_then(|t| t.get("debug")).is_some();
    assert!(!sets_debug(test), "[profile.test] sets its own debug");
    let packages = test.and_then(|t| t.get("package")).and_then(|p| p.as_table());
    for (name, table) in packages.into_iter().flatten() {
        assert!(!sets_debug(Some(table)), "[profile.test.package.{name:?}] sets its own debug");
    }
}

// RDP-FR-EKRY: the comment above the two tables tells a developer how to get
// full debug information for one build without a change to the file.
#[test]
fn the_profile_comment_names_the_full_debug_override() {
    let comment: String = MANIFEST
        .split("[profile.dev]")
        .next()
        .unwrap()
        .lines()
        .rev()
        .take_while(|line| line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(comment.contains("CARGO_PROFILE_DEV_DEBUG=full"), "{comment}");
    assert!(comment.contains("--config 'profile.dev.package.\"*\".debug=\"full\"'"), "{comment}");
}
