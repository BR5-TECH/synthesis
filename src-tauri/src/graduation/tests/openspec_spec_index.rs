//! `"load specification map"` on the production path
//! (`OSI-openspec-spec-index.md` OSI-FR-OSWD, OSI-FR-QELX, OSI-FR-ALQI,
//! OSI-FR-PJJO, OSI-FR-VTAD, OSI-FR-QLCS), with the OpenSpec CLI faked.

use std::sync::Arc;

use super::*;
use crate::openspec::spec_index::{load_specification_map_now, ChipLocation, SpecificationMapLoad};
use crate::openspec::tests::{env, ok, FakeProbe, FakeRunner, CLI};
use crate::openspec::{OpenSpec, OpenSpecRunner, ProcessError, ProcessOutput};

const LIST_SPECS: &str = r#"{ "specs": [ { "id": "auth", "requirementCount": 1 } ] }"#;
const SHOW_AUTH: &str = r#"{ "id": "auth", "title": "Auth", "overview": "Sign in.",
  "requirements": [ { "name": "Login", "text": "Users SHALL log in.", "scenarios": [] } ] }"#;
const LIST_CHANGES: &str = r#"{ "changes": [
  { "name": "add-logout", "completedTasks": 0, "totalTasks": 1, "lastModified": "2026-11-10T10:00:00Z" } ] }"#;

/// The fake CLI that answers the reads of a load, with `validate` answering
/// `validation`.
fn index_runner(validation: &str) -> FakeRunner {
    index_runner_listing(validation, Ok(ok(LIST_SPECS)))
}

/// The same, with `list --specs` answering `specs`.
fn index_runner_listing(
    validation: &str,
    specs: Result<ProcessOutput, ProcessError>,
) -> FakeRunner {
    FakeRunner::default()
        .json("--version", "1.17.0\n")
        .answering_prefix(&["list", "--changes"], Ok(ok(LIST_CHANGES)))
        .answering_prefix(&["list", "--specs"], specs)
        .json("show", SHOW_AUTH)
        .json("status", r#"{ "changes": [] }"#)
        .answering(
            "validate",
            Ok(ProcessOutput {
                exit_status: Some(1),
                stdout: validation.into(),
                stderr: Vec::new(),
            }),
        )
}

/// A `list --specs` that exits with status 2.
fn failing_list() -> FakeRunner {
    index_runner_listing(
        r#"{ "items": [] }"#,
        Ok(ProcessOutput { exit_status: Some(2), stdout: Vec::new(), stderr: Vec::new() }),
    )
}

/// A project in OpenSpec mode with one live capability.
fn openspec_fixture() -> Fixture {
    let fx = Fixture::new();
    std::fs::create_dir_all(fx.root().join("openspec/specs/auth")).unwrap();
    std::fs::write(fx.root().join("openspec/config.yaml"), "schema: spec-driven\n").unwrap();
    std::fs::write(fx.root().join("openspec/specs/auth/spec.md"), "# Auth\n").unwrap();
    super::change_runs::commit_all(&fx.root(), "Add OpenSpec");
    fx
}

/// The same, whose seam runs `runner`, with the CLI found where `found`.
fn fixture_with(runner: impl OpenSpecRunner + 'static, found: bool) -> Fixture {
    let fx = openspec_fixture();
    let probe =
        if found { FakeProbe::default().with_executable(CLI) } else { FakeProbe::default() };
    fx.app.manage(OpenSpec::new(Arc::new(runner), Arc::new(probe), env()));
    fx
}

/// A project in OpenSpec mode with one live capability, whose fake CLI
/// answers the reads of a load.
fn index_fixture() -> (Fixture, Arc<FakeRunner>) {
    let runner = Arc::new(index_runner(r#"{ "items": [] }"#));
    let fx = fixture_with(Shared(runner.clone()), true);
    (fx, runner)
}

/// A runner shared with the test, which reads its calls.
struct Shared(Arc<FakeRunner>);

impl OpenSpecRunner for Shared {
    fn run(
        &self,
        executable: &Path,
        args: &[&str],
        cwd: &Path,
        timeout: std::time::Duration,
    ) -> Result<ProcessOutput, ProcessError> {
        self.0.run(executable, args, cwd, timeout)
    }
}

/// A runner whose `list --changes` fails in one working copy.
struct FailingIn {
    inner: Arc<FakeRunner>,
    cwd: PathBuf,
}

impl OpenSpecRunner for FailingIn {
    fn run(
        &self,
        executable: &Path,
        args: &[&str],
        cwd: &Path,
        timeout: std::time::Duration,
    ) -> Result<ProcessOutput, ProcessError> {
        if args.starts_with(&["list", "--changes"]) && canonical(cwd) == self.cwd {
            return Ok(ProcessOutput {
                exit_status: Some(2),
                stdout: Vec::new(),
                stderr: Vec::new(),
            });
        }
        self.inner.run(executable, args, cwd, timeout)
    }
}

/// A stream whose working copy holds the change `add-logout` with a delta of
/// `capability`.
fn stream_touching(fx: &Fixture, name: &str, capability: &str) -> crate::streams::WorkStream {
    let stream = fx.stream(name);
    let delta = stream.worktree().join("openspec/changes/add-logout/specs").join(capability);
    std::fs::create_dir_all(&delta).unwrap();
    std::fs::write(delta.join("spec.md"), "## ADDED Requirements\n").unwrap();
    stream
}

fn chips(load: &SpecificationMapLoad, capability: &str) -> Vec<ChipLocation> {
    let SpecificationMapLoad::Openspec { index } = load else { panic!("an openspec index") };
    let value = serde_json::to_value(index).unwrap();
    let mut found = Vec::new();
    for group in value["roots"].as_array().unwrap() {
        for node in group["children"].as_array().unwrap() {
            if node["id"] == format!("capability:{capability}") {
                for chip in node["changes"].as_array().cloned().unwrap_or_default() {
                    found.push(match chip["location"]["kind"].as_str().unwrap() {
                        "active_worktree" => ChipLocation::ActiveWorktree,
                        _ => ChipLocation::Stream {
                            stream_id: chip["location"]["streamId"].as_str().unwrap().into(),
                            stream_name: chip["location"]["streamName"].as_str().unwrap().into(),
                        },
                    });
                }
            }
        }
    }
    found
}

// OSI-FR-OSWD: a project that is not in OpenSpec mode answers `demo` and
// starts no process.
#[test]
fn a_project_outside_the_mode_loads_the_demo_index() {
    let fx = Fixture::new();
    let runner = Arc::new(FakeRunner::default().json("--version", "1.17.0\n"));
    let seam =
        OpenSpec::new(runner.clone(), Arc::new(FakeProbe::default().with_executable(CLI)), env());
    fx.app.manage(seam);
    assert_eq!(load_specification_map_now(&fx.app), Ok(SpecificationMapLoad::Demo));
    assert!(runner.calls().is_empty());
}

// OSI-FR-PJJO: with no project open the load fails with `no_project_open`.
#[test]
fn a_load_with_no_project_open_fails() {
    let app = tauri::test::mock_app();
    app.handle().manage(crate::project::ProjectState::default());
    let failure = load_specification_map_now(app.handle()).unwrap_err();
    assert_eq!(failure.code, "no_project_open");
}

// OSI-FR-QELX, OSI-FR-ALQI, OSI-FR-VOOS: the changes are read in the active
// worktree and in the working copy of each live stream, and each gives a chip
// on the capabilities its own working copy holds deltas for.
#[test]
fn the_changes_of_every_live_stream_give_chips() {
    let (fx, runner) = index_fixture();
    let editor = stream_touching(&fx, "editor", "auth");
    let billing = stream_touching(&fx, "billing", "billing");

    let load = load_specification_map_now(&fx.app).expect("a load");

    assert_eq!(
        chips(&load, "auth"),
        [ChipLocation::Stream { stream_id: editor.id.clone(), stream_name: "editor".into() }]
    );
    assert_eq!(
        chips(&load, "billing"),
        [ChipLocation::Stream { stream_id: billing.id.clone(), stream_name: "billing".into() }],
        "a planned capability node holds the chip"
    );
    let lists = runner
        .calls()
        .into_iter()
        .filter(|c| c.args.starts_with(&["list".to_string(), "--changes".to_string()]))
        .count();
    assert_eq!(lists, 3, "the active worktree and two streams");
}

// OSI-FR-QELX: an active worktree that is the working copy of a stream is
// read once, as that stream's location.
#[test]
fn an_active_worktree_that_is_a_stream_is_read_once() {
    let (fx, runner) = index_fixture();
    let editor = stream_touching(&fx, "editor", "auth");
    let access = fx.app.state::<crate::fs::FsAccessState>().get().unwrap();
    fx.app
        .state::<crate::project::ProjectState>()
        .set_root_with_access(canonical(&editor.worktree()), Some(access));

    let load = load_specification_map_now(&fx.app).expect("a load");

    assert_eq!(
        chips(&load, "auth"),
        [ChipLocation::Stream { stream_id: editor.id.clone(), stream_name: "editor".into() }]
    );
    let lists = runner
        .calls()
        .into_iter()
        .filter(|c| c.args.starts_with(&["list".to_string(), "--changes".to_string()]))
        .count();
    assert_eq!(lists, 1);
}

/// The issues of each chip of a capability, by its location: `active` or the
/// stream name.
fn chip_issues(load: &SpecificationMapLoad, capability: &str) -> Vec<(String, usize)> {
    let SpecificationMapLoad::Openspec { index } = load else { panic!("an openspec index") };
    let value = serde_json::to_value(index).unwrap();
    let mut found = Vec::new();
    for group in value["roots"].as_array().unwrap() {
        for node in group["children"].as_array().unwrap() {
            if node["id"] != format!("capability:{capability}") {
                continue;
            }
            for chip in node["changes"].as_array().cloned().unwrap_or_default() {
                let at = chip["location"]["streamName"].as_str().unwrap_or("active").to_string();
                let issues = chip["issues"].as_array().map(Vec::len).unwrap_or(0);
                found.push((at, issues));
            }
        }
    }
    found
}

const BAD_CHANGE: &str = r#"{ "items": [ { "id": "add-logout", "type": "change", "valid": false,
  "issues": [ { "level": "ERROR", "path": "deltas[0]", "message": "Bad delta" } ] } ] }"#;

// OSI-FR-GRCV, OSI-FR-QELX: where the active worktree is a stream's working
// copy, the issue of a change item of the validation belongs to that
// stream's chip, and a chip of the same change on another stream holds none.
#[test]
fn the_issues_of_a_change_go_to_the_stream_the_active_worktree_is() {
    let fx = fixture_with(index_runner(BAD_CHANGE), true);
    let editor = stream_touching(&fx, "editor", "auth");
    stream_touching(&fx, "billing", "auth");
    let access = fx.app.state::<crate::fs::FsAccessState>().get().unwrap();
    fx.app
        .state::<crate::project::ProjectState>()
        .set_root_with_access(canonical(&editor.worktree()), Some(access));

    let load = load_specification_map_now(&fx.app).expect("a load");

    assert_eq!(chip_issues(&load, "auth"), [("billing".to_string(), 0), ("editor".to_string(), 1)]);
}

// OSI-FR-GRCV: where the active worktree is no stream's working copy, the
// validation ran in the active worktree, so no stream chip holds an issue.
#[test]
fn no_stream_chip_holds_an_issue_where_the_active_worktree_is_no_stream() {
    let fx = fixture_with(index_runner(BAD_CHANGE), true);
    stream_touching(&fx, "editor", "auth");
    stream_touching(&fx, "billing", "auth");

    let load = load_specification_map_now(&fx.app).expect("a load");

    assert_eq!(chip_issues(&load, "auth"), [("billing".to_string(), 0), ("editor".to_string(), 0)]);
}

// OSI-FR-DENT: a working copy whose `list_changes` fails and a stream whose
// working copy is gone give no chip, are counted in `unreadLocations`, and do
// not fail the load; the gone working copy costs no process.
#[test]
fn a_location_that_cannot_be_read_is_counted() {
    let inner = Arc::new(index_runner(r#"{ "items": [] }"#));
    let fx = openspec_fixture();
    let failing = stream_touching(&fx, "failing", "auth");
    let gone = stream_touching(&fx, "gone", "auth");
    stream_touching(&fx, "editor", "auth");
    std::fs::remove_dir_all(gone.worktree()).unwrap();
    let runner = FailingIn { inner: inner.clone(), cwd: canonical(&failing.worktree()) };
    fx.app.manage(OpenSpec::new(
        Arc::new(runner),
        Arc::new(FakeProbe::default().with_executable(CLI)),
        env(),
    ));

    let load = load_specification_map_now(&fx.app).expect("a load");

    let SpecificationMapLoad::Openspec { index } = &load else { panic!("{load:?}") };
    assert_eq!(index.unread_locations, 2);
    assert_eq!(chip_issues(&load, "auth"), [("editor".to_string(), 0)]);
    assert!(
        inner.calls().iter().all(|c| !c.cwd.starts_with(gone.worktree())),
        "no process in a gone working copy"
    );
}

// OSI-FR-PJJO, OFP-FR-QLQJ: with no usable CLI the load fails with
// `cli_unavailable` and the CLI state, and a failing `list --specs` fails it
// with the failure of that read.
#[test]
fn a_load_with_no_cli_or_a_failing_list_fails_with_its_code() {
    let fx = fixture_with(index_runner(r#"{ "items": [] }"#), false);
    let failure = load_specification_map_now(&fx.app).unwrap_err();
    assert_eq!(failure.code, "cli_unavailable");
    assert_eq!(failure.detail.as_deref(), Some("not_found"));

    let fx = fixture_with(failing_list(), true);
    let failure = load_specification_map_now(&fx.app).unwrap_err();
    assert_eq!(failure.code, "cli_failed");
    assert_eq!(failure.detail.as_deref(), Some("2"));
}

/// The records of the application's diagnostic buffer. nextest runs each
/// test in its own process, so no other test adds to them.
fn all_records() -> Vec<crate::logging::LogRecord> {
    let filter = crate::logging::LogFilter {
        min_level: crate::logging::LogLevel::Debug,
        ..crate::logging::LogFilter::default()
    };
    crate::logging::BUFFER.query(&filter, None, 5000).expect("a page").records
}

// OSI-FR-YVZW: a load is told by one INFO record with the counts and the
// duration; a load that fails by a WARN record with its code; and no record
// holds a label, a summary, an issue message, a change name, or a capability
// id.
#[test]
fn a_load_is_told_with_counts_and_no_text() {
    let fx = fixture_with(index_runner(BAD_CHANGE), true);
    stream_touching(&fx, "editor", "auth");
    stream_touching(&fx, "billing", "billing");

    load_specification_map_now(&fx.app).expect("a load");

    let records = all_records();
    let loaded: Vec<_> =
        records.iter().filter(|r| r.message == "specification map loaded").collect();
    assert_eq!(loaded.len(), 1);
    let fields = &loaded[0].fields;
    assert_eq!(loaded[0].level, crate::logging::LogLevel::Info);
    assert_eq!(
        (
            &fields["groups"],
            &fields["capabilities"],
            &fields["requirements"],
            &fields["chips"],
            &fields["issues"],
            &fields["unreadLocations"],
        ),
        (
            &serde_json::json!(1),
            &serde_json::json!(2),
            &serde_json::json!(1),
            &serde_json::json!(2),
            &serde_json::json!(0),
            &serde_json::json!(0),
        )
    );
    assert!(fields["durationMs"].is_u64());
    assert!(!records.is_empty());
    for record in &records {
        let text = serde_json::to_string(record).unwrap();
        for word in
            ["Auth", "Sign in", "Users SHALL", "Login", "add-logout", "Bad delta", "\"auth\""]
        {
            assert!(!text.contains(word), "{word} in a record: {text}");
        }
    }

    let fx = fixture_with(failing_list(), true);
    load_specification_map_now(&fx.app).unwrap_err();
    let failed: Vec<_> = all_records()
        .into_iter()
        .filter(|r| r.message == "specification map load failed")
        .collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].level, crate::logging::LogLevel::Warn);
    assert_eq!(failed[0].fields["code"], "cli_failed");
}
