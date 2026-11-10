//! The tests of the three OpenSpec read tools (`LOS-list-openspec-specs-tool.md`,
//! `ROS-read-openspec-spec-tool.md`, `LOC-list-openspec-changes-tool.md`).
//!
//! Each runs against a mock app that manages an `OpenSpec` seam with a fake
//! runner, so no test needs Node or the OpenSpec CLI.

use std::sync::Arc;

use rig::tool::PortableTool;
use rig::tool::ToolErrorKind;
use tauri::Manager;
use tempfile::TempDir;

use crate::logging::{LogBuffer, LogFilter, LogLevel, LogRecord};
use crate::openspec::tests::{env, ok, FakeProbe, FakeRunner, CLI};
use crate::openspec::{OpenSpec, ProcessError, ProcessOutput};
use crate::tools::openspec_list_changes::{
    self as loc, ListOpenSpecChangesArgs, ListOpenSpecChangesTool,
};
use crate::tools::openspec_list_specs::{
    self as los, ListOpenSpecSpecsArgs, ListOpenSpecSpecsTool,
};
use crate::tools::openspec_read_spec::{self as ros, ReadOpenSpecSpecArgs, ReadOpenSpecSpecTool};
use crate::tools::tests::{agent_session, mock_app};
use crate::tools::ToolRefusal;

const SPECS: &str = r#"{ "specs": [ { "id": "ui/theme", "requirementCount": 1 }, { "id": "auth", "requirementCount": 2 } ] }"#;

const SHOW: &str = r#"{ "id": "auth", "title": "Auth", "overview": "Sign in and out.",
  "requirements": [
    { "name": "Session Expiration", "text": "Sessions SHALL expire after 30 minutes of no activity.",
      "scenarios": [ { "name": "Idle", "rawText": "- WHEN idle for 30 minutes\n- THEN the session ends" } ] },
    { "name": "Logout", "text": "Users SHALL log out.", "scenarios": [] }
  ] }"#;

const CHANGES: &str = r#"{ "changes": [
  { "name": "older", "completedTasks": 0, "totalTasks": 0, "lastModified": "2026-01-01T00:00:00.000Z" },
  { "name": "add-logout", "completedTasks": 1, "totalTasks": 4, "lastModified": "2026-11-10T10:00:00.000Z" }
] }"#;

const STATUS: &str = r#"{ "changes": [
  { "changeName": "add-logout", "schemaName": "spec-driven", "isComplete": false,
    "artifacts": [ { "id": "proposal", "status": "done" }, { "id": "tasks", "status": "ready" } ] },
  { "changeName": "older", "schemaName": "spec-driven", "isComplete": true, "artifacts": [] }
] }"#;

/// A mock app with an open OpenSpec project, an agent session over it, and a
/// seam whose CLI answers with `runner`.
struct Project {
    app: tauri::App<tauri::test::MockRuntime>,
    dir: TempDir,
    session: String,
    runner: Arc<FakeRunner>,
}

impl Project {
    fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.app.handle().clone()
    }

    fn write(&self, rel: &str, body: &str) {
        let path = self.dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
}

fn project_with(runner: FakeRunner, probe: FakeProbe, uses_openspec: bool) -> Project {
    let dir = TempDir::new().unwrap();
    if uses_openspec {
        std::fs::create_dir_all(dir.path().join("openspec")).unwrap();
        std::fs::write(dir.path().join("openspec/config.yaml"), "schema: spec-driven\n").unwrap();
    }
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = mock_app();
    app.state::<crate::bm25_index::Bm25Indexer>().mount(&crate::fs::RootFs::for_root(&root));
    let session = agent_session(&app, &root, "openspec-tools");
    let runner = Arc::new(runner.json("--version", "1.17.0\n"));
    app.manage(OpenSpec::new(runner.clone(), Arc::new(probe), env()));
    Project { app, dir, session, runner }
}

fn project(runner: FakeRunner) -> Project {
    project_with(runner, FakeProbe::default().with_executable(CLI), true)
}

fn full_runner() -> FakeRunner {
    FakeRunner::default().json("list", SPECS).json("show", SHOW).json("status", STATUS)
}

fn buffer() -> &'static LogBuffer {
    Box::leak(Box::new(LogBuffer::new()))
}

fn records(buffer: &'static LogBuffer) -> Vec<LogRecord> {
    buffer
        .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 200)
        .unwrap()
        .records
}

fn list_specs(
    p: &Project,
    buffer: &'static LogBuffer,
) -> Result<los::ListOpenSpecSpecsOutput, ToolRefusal> {
    tauri::async_runtime::block_on(
        ListOpenSpecSpecsTool::with_buffer(p.handle(), &p.session, buffer)
            .call(ListOpenSpecSpecsArgs::default()),
    )
}

fn read_spec(
    p: &Project,
    buffer: &'static LogBuffer,
    capability: &str,
    requirement: Option<&str>,
) -> Result<ros::ReadOpenSpecSpecOutput, ToolRefusal> {
    tauri::async_runtime::block_on(
        ReadOpenSpecSpecTool::with_buffer(p.handle(), &p.session, buffer).call(
            ReadOpenSpecSpecArgs {
                capability: capability.to_string(),
                requirement: requirement.map(str::to_string),
            },
        ),
    )
}

fn list_changes(
    p: &Project,
    buffer: &'static LogBuffer,
) -> Result<loc::ListOpenSpecChangesOutput, ToolRefusal> {
    tauri::async_runtime::block_on(
        ListOpenSpecChangesTool::with_buffer(p.handle(), &p.session, buffer)
            .call(ListOpenSpecChangesArgs::default()),
    )
}

fn assert_refusal(refusal: &ToolRefusal, kind: ToolErrorKind, retryable: bool, message: &str) {
    assert_eq!(format!("{:?}", refusal.kind()), format!("{kind:?}"));
    assert_eq!(refusal.retryable(), retryable);
    assert_eq!(refusal.to_string(), message);
}

// ---------------------------------------------------------------------------
// The definitions (LOS-FR-UPKL, ROS-FR-AZNY, LOC-FR-YVDP)
// ---------------------------------------------------------------------------

// LOS-FR-QKCD, LOS-FR-UPKL, ROS-FR-XPYM, ROS-FR-AZNY, LOC-FR-ZDJW, LOC-FR-YVDP:
// each tool has its name, its fixed description, and its schema.
#[test]
fn each_tool_has_its_name_description_and_schema() {
    let p = project(full_runner());
    let a = rig::tool::tool_definition(&ListOpenSpecSpecsTool::new(p.handle(), "s"));
    assert_eq!(a.name, "list_openspec_specs");
    assert_eq!(a.description, los::DESCRIPTION);
    assert_eq!(
        a.parameters,
        serde_json::json!({ "type": "object", "properties": {}, "required": [] })
    );

    let b = rig::tool::tool_definition(&ReadOpenSpecSpecTool::new(p.handle(), "s"));
    assert_eq!(b.name, "read_openspec_spec");
    assert_eq!(b.description, ros::DESCRIPTION);
    assert_eq!(b.parameters["required"], serde_json::json!(["capability"]));
    assert!(b.parameters["properties"]["capability"]["description"]
        .as_str()
        .unwrap()
        .starts_with("The capability id that `list_openspec_specs` returned"));
    assert!(b.parameters["properties"]["requirement"]["description"]
        .as_str()
        .unwrap()
        .contains("Letter case does not matter"));

    let c = rig::tool::tool_definition(&ListOpenSpecChangesTool::new(p.handle(), "s"));
    assert_eq!(c.name, "list_openspec_changes");
    assert_eq!(c.description, loc::DESCRIPTION);
    assert_eq!(
        c.parameters,
        serde_json::json!({ "type": "object", "properties": {}, "required": [] })
    );
}

// ---------------------------------------------------------------------------
// list_openspec_specs
// ---------------------------------------------------------------------------

// LOS-FR-RWGX, LOS-FR-XHLX: the capabilities of the seam, in its order, and the
// tool runs one CLI process of the seam and no other.
#[test]
fn list_openspec_specs_returns_the_capabilities_in_id_order() {
    let p = project(full_runner());
    let out = list_specs(&p, buffer()).unwrap();
    let ids: Vec<_> =
        out.capabilities.iter().map(|c| (c.id.as_str(), c.requirement_count)).collect();
    assert_eq!(ids, [("auth", 2), ("ui/theme", 1)]);
    assert_eq!(p.runner.calls_of("list"), 1);
}

// LOS-FR-GHCZ: no capability is a success with an empty list.
#[test]
fn list_openspec_specs_answers_an_empty_list() {
    let p = project(FakeRunner::default().json("list", r#"{ "specs": [] }"#));
    assert!(list_specs(&p, buffer()).unwrap().capabilities.is_empty());
}

// LOS-FR-VOAB, ROS-FR-QSEO, LOC-FR-XPNP: with no project open, each tool gives
// the shared refusal.
#[test]
fn each_tool_refuses_with_no_project_open() {
    let app = mock_app();
    app.manage(OpenSpec::new(Arc::new(full_runner()), Arc::new(FakeProbe::default()), env()));
    let handle = app.handle().clone();
    let buffer = buffer();
    let a = tauri::async_runtime::block_on(
        ListOpenSpecSpecsTool::with_buffer(handle.clone(), "none", buffer)
            .call(ListOpenSpecSpecsArgs::default()),
    );
    let b = tauri::async_runtime::block_on(
        ReadOpenSpecSpecTool::with_buffer(handle.clone(), "none", buffer)
            .call(ReadOpenSpecSpecArgs { capability: "auth".into(), requirement: None }),
    );
    let c = tauri::async_runtime::block_on(
        ListOpenSpecChangesTool::with_buffer(handle, "none", buffer)
            .call(ListOpenSpecChangesArgs::default()),
    );
    for refusal in [a.unwrap_err(), b.unwrap_err(), c.unwrap_err()] {
        assert_eq!(refusal, ToolRefusal::NoProjectOpen);
    }
}

// LOS-FR-SBNB, ROS-FR-MOFU, LOC-FR-YEQH, LOS-FR-GPCY, LOC-FR-UBZT: an inactive
// mode and a missing CLI each give the OpenSpec unavailable refusal, kind
// `Other`, not retryable.
#[test]
fn each_tool_refuses_as_unavailable_when_the_mode_or_the_cli_is_missing() {
    let inactive = project_with(full_runner(), FakeProbe::default().with_executable(CLI), false);
    let no_cli = project_with(full_runner(), FakeProbe::default(), true);
    for p in [&inactive, &no_cli] {
        let buffer = buffer();
        let message = crate::tools::openspec_shared::UNAVAILABLE;
        assert_refusal(&list_specs(p, buffer).unwrap_err(), ToolErrorKind::Other, false, message);
        assert_refusal(
            &read_spec(p, buffer, "auth", None).unwrap_err(),
            ToolErrorKind::Other,
            false,
            message,
        );
        assert_refusal(&list_changes(p, buffer).unwrap_err(), ToolErrorKind::Other, false, message);
    }
    // No read process starts for either.
    assert_eq!(inactive.runner.calls_of("list"), 0);
    assert_eq!(no_cli.runner.calls().len(), 0);
}

// LOS-FR-IEDQ, LOS-FR-GPCY, ROS-FR-MOFU, LOC-FR-YEQH, LOC-FR-UBZT: a timeout,
// a failed exit, and unreadable output each give the tool's read-failed
// refusal, and no CLI output reaches it.
#[test]
fn each_tool_refuses_a_failed_read_with_its_own_sentence() {
    for answer in [
        Err(ProcessError::TimedOut),
        Ok(ProcessOutput { exit_status: Some(1), stdout: b"SECRET-OUTPUT".to_vec() }),
        Ok(ok("SECRET-OUTPUT not json")),
    ] {
        let p = project(
            FakeRunner::default()
                .answering("list", answer.clone())
                .answering("show", answer.clone())
                .answering("status", answer),
        );
        p.write("openspec/specs/auth/spec.md", "# Auth\n");
        let buffer = buffer();
        let a = list_specs(&p, buffer).unwrap_err();
        assert_refusal(&a, ToolErrorKind::Other, false, los::READ_FAILED);
        let b = read_spec(&p, buffer, "auth", None).unwrap_err();
        assert_refusal(&b, ToolErrorKind::Other, false, ros::READ_FAILED);
        let c = list_changes(&p, buffer).unwrap_err();
        assert_refusal(&c, ToolErrorKind::Other, false, loc::READ_FAILED);
        let all = serde_json::to_string(&records(buffer)).unwrap();
        assert!(!all.contains("SECRET-OUTPUT"));
    }
}

// LOS-FR-PAFH: a call logs the tool and the count, and a refusal logs the tool
// and the reason.
#[test]
fn list_openspec_specs_logs_the_count_and_the_refusal() {
    let p = project(full_runner());
    let buffer = buffer();
    list_specs(&p, buffer).unwrap();
    let inactive = project_with(full_runner(), FakeProbe::default().with_executable(CLI), false);
    list_specs(&inactive, buffer).unwrap_err();
    let tool_records: Vec<_> = records(buffer)
        .into_iter()
        .filter(|r| r.fields.get("tool") == Some(&serde_json::json!(los::NAME)))
        .collect();
    assert_eq!(tool_records.len(), 2);
    assert_eq!(tool_records[0].level, LogLevel::Info);
    assert_eq!(tool_records[0].fields["capabilities"], 2);
    assert_eq!(tool_records[1].level, LogLevel::Warn);
    assert_eq!(tool_records[1].fields["reason"], "openspec_unavailable");
    assert!(!serde_json::to_string(&tool_records).unwrap().contains("ui/theme"));
}

// ---------------------------------------------------------------------------
// read_openspec_spec
// ---------------------------------------------------------------------------

// ROS-FR-XEHR, ROS-FR-BDJJ, ROS-FR-SSKT: without `requirement`, every
// requirement in file order, each with its whole text and all its scenarios.
#[test]
fn read_openspec_spec_returns_the_whole_capability() {
    let p = project(full_runner());
    p.write("openspec/specs/auth/spec.md", "# Auth\n");
    let out = read_spec(&p, buffer(), "auth", None).unwrap();
    assert_eq!(out.id, "auth");
    assert_eq!(out.title, "Auth");
    assert_eq!(out.overview, "Sign in and out.");
    let names: Vec<_> = out.requirements.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Session Expiration", "Logout"]);
    assert_eq!(out.requirements[0].text, "Sessions SHALL expire after 30 minutes of no activity.");
    assert_eq!(
        out.requirements[0].scenarios[0].text,
        "- WHEN idle for 30 minutes\n- THEN the session ends"
    );
    assert_eq!(p.runner.calls_of("show"), 1);
}

// ROS-FR-CSZP: `requirement` selects by name, trimmed and without regard to
// letter case, and the capability fields stay.
#[test]
fn read_openspec_spec_selects_a_requirement_by_name() {
    let p = project(full_runner());
    p.write("openspec/specs/auth/spec.md", "# Auth\n");
    let out = read_spec(&p, buffer(), "auth", Some("  session EXPIRATION ")).unwrap();
    assert_eq!(out.id, "auth");
    assert_eq!(out.title, "Auth");
    assert_eq!(out.requirements.len(), 1);
    assert_eq!(out.requirements[0].name, "Session Expiration");
}

// ROS-FR-GNFU: a name that matches no requirement is the retryable unknown
// requirement refusal.
#[test]
fn read_openspec_spec_refuses_an_unknown_requirement() {
    let p = project(full_runner());
    p.write("openspec/specs/auth/spec.md", "# Auth\n");
    let refusal = read_spec(&p, buffer(), "auth", Some("Password Reset")).unwrap_err();
    assert_refusal(&refusal, ToolErrorKind::NotFound, true, ros::UNKNOWN_REQUIREMENT);
}

// ROS-FR-FQHM: a blank capability and an id the seam refuses are the
// retryable invalid capability refusal; no process starts for either.
#[test]
fn read_openspec_spec_refuses_an_invalid_capability() {
    let p = project(full_runner());
    for capability in ["", "   ", "../etc", "/abs", "a b"] {
        let refusal = read_spec(&p, buffer(), capability, None).unwrap_err();
        assert_refusal(&refusal, ToolErrorKind::InvalidArgs, true, ros::INVALID_CAPABILITY);
    }
    assert_eq!(p.runner.calls_of("show"), 0);
}

// ROS-FR-RZRC: a capability with no spec file is the retryable unknown
// capability refusal.
#[test]
fn read_openspec_spec_refuses_an_unknown_capability() {
    let p = project(full_runner());
    let refusal = read_spec(&p, buffer(), "billing", None).unwrap_err();
    assert_refusal(&refusal, ToolErrorKind::NotFound, true, ros::UNKNOWN_CAPABILITY);
}

// ROS-FR-PRDW: a call logs the capability, whether a requirement was given,
// and the count; a long capability is bounded; the requirement name and the
// text never reach a record.
#[test]
fn read_openspec_spec_logs_the_capability_and_never_the_text() {
    let p = project(full_runner());
    p.write("openspec/specs/auth/spec.md", "# Auth\n");
    let buffer = buffer();
    read_spec(&p, buffer, "auth", Some("Logout")).unwrap();
    let long = "x".repeat(600);
    read_spec(&p, buffer, &long, None).unwrap_err();
    let tool_records: Vec<_> = records(buffer)
        .into_iter()
        .filter(|r| r.fields.get("tool") == Some(&serde_json::json!(ros::NAME)))
        .collect();
    assert_eq!(tool_records.len(), 2);
    assert_eq!(tool_records[0].fields["capability"], "auth");
    assert_eq!(tool_records[0].fields["requirementGiven"], true);
    assert_eq!(tool_records[0].fields["requirements"], 1);
    let logged = tool_records[1].fields["capability"].as_str().unwrap();
    assert_eq!(logged.chars().filter(|c| *c == 'x').count(), 512);
    assert!(logged.chars().count() < 600);
    let all = serde_json::to_string(&tool_records).unwrap();
    for leaked in ["Logout", "SHALL", "Sign in and out"] {
        assert!(!all.contains(leaked), "a record must not carry {leaked:?}");
    }
}

// ---------------------------------------------------------------------------
// list_openspec_changes
// ---------------------------------------------------------------------------

// LOC-FR-DSMK, LOC-FR-MGXX, LOC-FR-XTUV: each change with every field of its
// summary and its path, the most recent first.
#[test]
fn list_openspec_changes_returns_the_changes_most_recent_first() {
    let p = project(FakeRunner::default().json("list", CHANGES).json("status", STATUS));
    p.write("openspec/changes/add-logout/specs/auth/spec.md", "## ADDED Requirements\n");
    let out = list_changes(&p, buffer()).unwrap();
    let names: Vec<_> = out.changes.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["add-logout", "older"]);
    let first = &out.changes[0];
    assert_eq!(first.path, "openspec/changes/add-logout/");
    assert_eq!(first.schema, "spec-driven");
    assert_eq!(first.artifacts.len(), 2);
    assert_eq!(first.artifacts[0].id, "proposal");
    assert_eq!(first.artifacts[0].status, "done");
    assert!(!first.is_complete);
    assert_eq!((first.completed_tasks, first.total_tasks), (1, 4));
    assert_eq!(first.capabilities, ["auth"]);
    assert!(out.changes[1].is_complete);
    assert_eq!(p.runner.calls_of("list"), 1);
    assert_eq!(p.runner.calls_of("status"), 1);
}

// LOC-FR-UJOO: no active change is a success with an empty list.
#[test]
fn list_openspec_changes_answers_an_empty_list() {
    let p = project(
        FakeRunner::default()
            .json("list", r#"{ "changes": [] }"#)
            .json("status", r#"{ "changes": [] }"#),
    );
    assert!(list_changes(&p, buffer()).unwrap().changes.is_empty());
}

// LOC-FR-KZDF: a call logs the tool and the count of changes, and no change
// name.
#[test]
fn list_openspec_changes_logs_the_count() {
    let p = project(FakeRunner::default().json("list", CHANGES).json("status", STATUS));
    let buffer = buffer();
    list_changes(&p, buffer).unwrap();
    let record = records(buffer)
        .into_iter()
        .find(|r| r.fields.get("tool") == Some(&serde_json::json!(loc::NAME)))
        .unwrap();
    assert_eq!(record.level, LogLevel::Info);
    assert_eq!(record.fields["changes"], 2);
    assert!(!serde_json::to_string(&record).unwrap().contains("add-logout"));
}

// ---------------------------------------------------------------------------
// Read-only (LOS-FR-POAE, ROS-FR-PZWT, LOC-FR-IIQY)
// ---------------------------------------------------------------------------

// LOS-FR-POAE, ROS-FR-PZWT, LOC-FR-IIQY: a call of each tool leaves every file
// of the project as it was, and the seam ran only read subcommands.
#[test]
fn the_tools_change_no_file_of_the_project() {
    let p = project(full_runner().json("list", SPECS));
    p.write("openspec/specs/auth/spec.md", "# Auth\n");
    let snapshot = |dir: &std::path::Path| {
        let mut files = Vec::new();
        let mut pending = vec![dir.to_path_buf()];
        while let Some(d) = pending.pop() {
            for entry in std::fs::read_dir(&d).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    files.push((path.clone(), std::fs::read(&path).unwrap()));
                }
            }
        }
        files.sort();
        files
    };
    let before = snapshot(p.dir.path());
    let buffer = buffer();
    let _ = list_specs(&p, buffer);
    let _ = read_spec(&p, buffer, "auth", None);
    let _ = list_changes(&p, buffer);
    assert_eq!(snapshot(p.dir.path()), before);
    for call in p.runner.calls() {
        assert!(["--version", "list", "show", "status"].contains(&call.args[0].as_str()));
    }
}

// ROS-FR-SSKT: a long requirement text and a long scenario come back whole,
// byte for byte.
#[test]
fn read_openspec_spec_returns_long_text_whole() {
    let text = "The system SHALL keep every byte. ".repeat(6_000);
    let scenario = "- WHEN a long scenario\n- THEN nothing is cut\n".repeat(4_000);
    let show = serde_json::json!({
        "id": "auth", "title": "Auth", "overview": "",
        "requirements": [ { "name": "Long", "text": text, "scenarios": [ { "name": "Big", "rawText": scenario } ] } ],
    })
    .to_string();
    let p = project(FakeRunner::default().json("show", &show));
    p.write("openspec/specs/auth/spec.md", "# Auth\n");
    let out = read_spec(&p, buffer(), "auth", None).unwrap();
    assert!(text.len() > 200_000);
    assert_eq!(out.requirements[0].text, text);
    assert_eq!(out.requirements[0].scenarios[0].text, scenario);
}
