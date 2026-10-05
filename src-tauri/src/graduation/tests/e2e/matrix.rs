//! The coverage matrix and the suite agree (GTE-FR-CSEH, GTE-FR-WUAP).
//!
//! `README.md` beside this file lists a journey per line and the tests that
//! answer it. A journey nobody wrote a test for, and a test the matrix never
//! names, are each a gap the suite reports rather than a gap a reader has to
//! notice.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// This module's own directory, resolved from the manifest rather than from the
/// working directory a test runner happens to have.
fn here() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/graduation/tests/e2e")
}

fn read(name: &str) -> String {
    // Through the guarded handle rather than `std::fs`, on the terms FSA-FR-19
    // binds every read in the backend to.
    crate::fs::FsAccess::builder()
        .allow_root(here())
        .build()
        .expect("the suite's own directory")
        .read_text(here().join(name))
        .unwrap_or_else(|_| panic!("{name} is readable"))
}

/// The files of the scenario language itself. Every other `.rs` file of this
/// directory holds journeys, so a journey file added later is read without
/// being named here.
const HARNESS_FILES: [&str; 15] = [
    "acts.rs",
    "dispatch_assert.rs",
    "hooks.rs",
    "merge_acts.rs",
    "merge_arrange.rs",
    "merge_assert.rs",
    "matrix.rs",
    "mod.rs",
    "outcome.rs",
    "reconcile.rs",
    "repository_assert.rs",
    "scenario.rs",
    "script.rs",
    "settings.rs",
    "stop_assert.rs",
];

/// Every file of the suite that holds journeys.
fn journey_files() -> Vec<String> {
    let mut files: Vec<String> = crate::fs::FsAccess::builder()
        .allow_root(here())
        .build()
        .expect("the suite's own directory")
        .list_dir(here())
        .expect("the suite's directory is listable")
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| name.ends_with(".rs") && !HARNESS_FILES.contains(&name.as_str()))
        .collect();
    files.sort();
    assert!(
        files.iter().any(|name| name == "journeys.rs"),
        "the scan found no journey file, so it has collapsed"
    );
    files
}

/// Every `#[test]` function this suite declares, by name.
fn declared_tests() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for file in journey_files() {
        let source = read(&file);
        let mut expecting = false;
        for line in source.lines() {
            let trimmed = line.trim_start();
            if trimmed == "#[test]" {
                expecting = true;
                continue;
            }
            if expecting {
                if let Some(rest) = trimmed.strip_prefix("fn ") {
                    let name: String = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    names.insert(name);
                }
                expecting = false;
            }
        }
    }
    names
}

// GTE-FR-YAEB: a scenario that lets the production queue attempt a dispatch
// fails when it ends, so the tripwire is a check rather than a record nobody
// reads. The answers are recorded with the queue's dispatch on, which is the
// mistake the tripwire exists to catch.
#[test]
#[should_panic(expected = "the production queue tried to dispatch")]
fn a_scenario_that_lets_the_queue_dispatch_fails() {
    use super::scenario::{Scenario, TurnScript as Script};

    let outcome = Scenario::named("queue let loose")
        .work_turn(Script::escalates(&["Which panel is meant?"]))
        .run();
    crate::graduation::answer_graduation_escalation(
        outcome.fixture().app.clone(),
        outcome.run_id().to_string(),
        vec![crate::graduation::GraduationEscalationAnswer {
            position: 1,
            answer: "The editor's.".to_string(),
            summary: "The editor's.".to_string(),
        }],
    )
    .expect("the answers are recorded");
    drop(outcome);
}

// GTE-FR-CSEH, GTE-FR-WUAP: the matrix names every test the suite declares, and
// declares every test the matrix names.
#[test]
fn the_coverage_matrix_and_the_suite_name_the_same_tests() {
    let readme = read("README.md");
    let declared = declared_tests();
    assert!(
        declared.len() >= 20,
        "the scan found only {} tests, so it has collapsed",
        declared.len()
    );

    // The matrix cites `binary_protocol.rs` as a file rather than naming its
    // four tests one by one, that journey being the whole of that file.
    let file_cited = readme.contains("`binary_protocol.rs`");
    let missing: Vec<&String> = declared
        .iter()
        .filter(|name| {
            if file_cited && read("binary_protocol.rs").contains(&format!("fn {name}(")) {
                return false;
            }
            !readme.contains(name.as_str())
        })
        .collect();
    assert!(
        missing.is_empty(),
        "these tests are in the suite and in no line of the coverage matrix: {missing:?}"
    );

    // And every name the matrix cites is a test that exists. Only the matrix
    // section is read: the sections above it name the scenario language's own
    // methods, which are not tests and are not meant to be.
    let matrix = readme
        .split_once("## Coverage matrix")
        .expect("the README holds a coverage matrix")
        .1;
    let cited: BTreeSet<String> = matrix
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|token| {
            token.matches('_').count() >= 2
                && token
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
        .map(str::to_string)
        .collect();
    assert!(!cited.is_empty(), "the matrix cites no test at all");
    let unknown: Vec<&String> = cited.difference(&declared).collect();
    assert!(
        unknown.is_empty(),
        "the coverage matrix names tests this suite does not declare: {unknown:?}"
    );
}
