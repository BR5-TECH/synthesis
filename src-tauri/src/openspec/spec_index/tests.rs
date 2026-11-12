//! The tests of the spec index (`OSI-openspec-spec-index.md`): the shape of
//! the index on fixed reads, and the reads of one load through the fake
//! process seam of OSM.

use serde_json::json;

use super::*;
use crate::openspec::tests::{buffer, ok, openspec_project, records, seam_with, write, Silent};
use crate::openspec::{
    CapabilitySpec, ChangeSummary, ProcessOutput, ProjectValidation, ProjectValidationItem,
    Requirement, Scenario, ValidationIssue,
};

fn requirement(name: &str, scenarios: usize) -> Requirement {
    Requirement {
        name: name.into(),
        text: format!("The system SHALL {name}."),
        scenarios: (0..scenarios)
            .map(|i| Scenario { name: format!("s{i}"), text: "- WHEN x".into() })
            .collect(),
    }
}

fn capability(id: &str, title: &str, requirements: Vec<Requirement>) -> CapabilitySpec {
    CapabilitySpec {
        id: id.into(),
        title: title.into(),
        overview: format!("About {id}."),
        requirements,
    }
}

fn change(name: &str, capabilities: &[&str]) -> ChangeSummary {
    ChangeSummary {
        name: name.into(),
        schema: "spec-driven".into(),
        artifacts: vec![],
        is_complete: false,
        completed_tasks: 0,
        total_tasks: 0,
        capabilities: capabilities.iter().map(|c| c.to_string()).collect(),
        last_modified: "2026-11-10T10:00:00Z".into(),
    }
}

fn issue(level: &str, path: &str, message: &str) -> ValidationIssue {
    ValidationIssue { level: level.into(), path: path.into(), message: message.into() }
}

fn item(id: &str, kind: &str, issues: Vec<ValidationIssue>) -> ProjectValidationItem {
    ProjectValidationItem { id: id.into(), kind: kind.into(), valid: issues.is_empty(), issues }
}

fn stream(id: &str, name: &str) -> ChipLocation {
    ChipLocation::Stream { stream_id: id.into(), stream_name: name.into() }
}

fn caps() -> Vec<CapabilitySpec> {
    vec![
        capability("ui/theme", "Theme", vec![requirement("Dark mode", 2)]),
        capability("auth", "", vec![requirement("Login", 1), requirement("Logout", 0)]),
        capability("ui/layout", "Layout", vec![]),
        capability("core/store", "Store", vec![requirement("Persist", 3)]),
    ]
}

fn groups(index: &OpenSpecMapIndex) -> Vec<(String, String, Vec<String>)> {
    index
        .roots
        .iter()
        .map(|group| {
            let IndexChildren::Nodes(children) = &group.children else { panic!("a group") };
            (group.id.clone(), group.label.clone(), children.iter().map(|c| c.id.clone()).collect())
        })
        .collect()
}

fn node<'a>(index: &'a OpenSpecMapIndex, id: &str) -> &'a IndexNode {
    index
        .roots
        .iter()
        .flat_map(|group| match &group.children {
            IndexChildren::Nodes(children) => children.iter(),
            IndexChildren::Specs(_) => [].iter(),
        })
        .find(|node| node.id == id)
        .unwrap_or_else(|| panic!("no node {id}"))
}

fn specs(node: &IndexNode) -> &[SpecNode] {
    match &node.children {
        IndexChildren::Specs(specs) => specs,
        IndexChildren::Nodes(_) => panic!("a capability node"),
    }
}

// OSI-FR-XUJG, OSI-FR-MAUQ, OSI-FR-WFJU, OSI-FR-JIJM: three levels; the root
// group `group:/` labelled `specs` first, then the groups by segment; the
// capability nodes by id; no dependency and no hue.
#[test]
fn the_capabilities_stand_in_groups_by_their_first_segment() {
    let index = build_index(&caps(), None, &[]);
    let value = serde_json::to_value(&index).unwrap();
    assert_eq!(
        value["levels"],
        json!([
            { "plural": "groups", "singular": "group" },
            { "plural": "capabilities", "singular": "capability" },
            { "plural": "requirements", "singular": "requirement" }
        ])
    );
    assert_eq!(value["dependencies"], json!([]));
    assert!(!value.to_string().contains("hue"));
    assert_eq!(
        groups(&index),
        vec![
            ("group:/".to_string(), "specs".to_string(), vec!["capability:auth".to_string()]),
            ("group:core".into(), "core".into(), vec!["capability:core/store".into()]),
            (
                "group:ui".into(),
                "ui".into(),
                vec!["capability:ui/layout".into(), "capability:ui/theme".into()]
            ),
        ]
    );
    assert_eq!(index.roots[0].summary, "");
}

// OSI-FR-MAUQ: with no capability of its own, the root group is absent.
#[test]
fn no_root_group_stands_without_a_flat_capability() {
    let caps = vec![capability("ui/theme", "Theme", vec![])];
    let index = build_index(&caps, None, &[]);
    assert_eq!(groups(&index).iter().map(|g| g.0.as_str()).collect::<Vec<_>>(), ["group:ui"]);
}

// OSI-FR-OHJE, OSI-FR-SEYB, OSI-FR-CWLE: a capability node takes its title, or
// its id where the title is empty, and its overview; a requirement node has
// its code, badge, path, name, text, one requirement, and its scenario count.
#[test]
fn a_requirement_node_stands_for_one_requirement_in_file_order() {
    let index = build_index(&caps(), None, &[]);
    let auth = node(&index, "capability:auth");
    assert_eq!(auth.label, "auth");
    assert_eq!(auth.summary, "About auth.");
    assert_eq!(node(&index, "capability:ui/theme").label, "Theme");
    let value = serde_json::to_value(&specs(auth)[1]).unwrap();
    assert_eq!(
        value,
        json!({
            "code": "auth#2", "badge": "R2", "path": "openspec/specs/auth/spec.md",
            "label": "Logout", "summary": "The system SHALL Logout.",
            "requirements": 1, "scenarios": 0, "state": "built"
        })
    );
    assert_eq!(specs(node(&index, "capability:ui/theme"))[0].scenarios, 2);
    assert!(specs(node(&index, "capability:ui/layout")).is_empty());
}

// OSI-FR-DVLQ, OSI-FR-OJRE, OSI-FR-SBVF, OSI-FR-OIOB: an issue at
// `requirements[<i>]` belongs to that requirement, another issue of the spec
// item to its capability; an ERROR or WARNING makes the requirement, or every
// requirement of its capability, a gap; an INFO does not; an item that names
// no live capability gives no issue.
#[test]
fn the_issues_of_the_validation_mark_the_gaps() {
    let validation = ProjectValidation {
        valid: false,
        items: vec![
            item(
                "auth",
                "spec",
                vec![
                    issue("WARNING", "requirements[1].scenarios", "No scenario"),
                    issue("INFO", "requirements[0]", "Consider a scenario"),
                    issue("INFO", "overview", "Short overview"),
                ],
            ),
            item("core/store", "spec", vec![issue("ERROR", "purpose", "Too short")]),
            item("ui/theme", "spec", vec![issue("ERROR", "requirements[9]", "Out of range")]),
            item("gone", "spec", vec![issue("ERROR", "x", "Nobody")]),
        ],
    };
    let index = build_index(&caps(), Some(&validation), &[]);
    assert_eq!(index.validation, "invalid");
    let auth = node(&index, "capability:auth");
    let [login, logout] = specs(auth) else { panic!("two requirements") };
    assert_eq!(login.state, "built");
    assert_eq!(
        login.issues,
        [MapIssue { level: "INFO".into(), message: "Consider a scenario".into() }]
    );
    assert_eq!(logout.state, "gap");
    assert_eq!(logout.issues[0].message, "No scenario");
    assert_eq!(auth.issues, [MapIssue { level: "INFO".into(), message: "Short overview".into() }]);
    let store = node(&index, "capability:core/store");
    assert_eq!(store.issues[0].message, "Too short");
    assert_eq!(specs(store)[0].state, "gap");
    assert!(specs(store)[0].issues.is_empty());
    // A position outside the capability belongs to the capability.
    let theme = node(&index, "capability:ui/theme");
    assert_eq!(theme.issues[0].message, "Out of range");
    let text = serde_json::to_string(&index).unwrap();
    assert!(!text.contains("Nobody"));
    assert_eq!(index.counts().issues, 5);
}

// OSI-FR-OIOB, OSI-FR-OJRE: a validation that answers valid is `valid`; one
// that failed is `unavailable`, with every requirement `built` and no issue.
#[test]
fn the_validation_state_follows_the_answer() {
    let valid = ProjectValidation { valid: true, items: vec![item("auth", "spec", vec![])] };
    assert_eq!(build_index(&caps(), Some(&valid), &[]).validation, "valid");
    let index = build_index(&caps(), None, &[]);
    assert_eq!(index.validation, "unavailable");
    let text = serde_json::to_string(&index).unwrap();
    assert!(!text.contains("\"gap\"") && !text.contains("issues"));
    assert!(!text.contains("verified") && !text.contains("drafted"));
}

// OSI-FR-ALQI, OSI-FR-VOOS, OSI-FR-GRCV, OSI-FR-DENT: each change gives a chip
// on each capability it names, ordered by change name, then the active
// worktree first, then by stream name; an id with no live capability gives
// one planned node in its group; issues of a change item reach its chip in
// the active worktree alone; an unread location gives no chip and is counted.
#[test]
fn the_changes_give_chips_and_planned_nodes() {
    let locations = vec![
        LocationRead {
            location: stream("w2", "zeta"),
            validated: false,
            changes: Some(vec![change("add-logout", &["auth"])]),
        },
        LocationRead {
            location: ChipLocation::ActiveWorktree,
            validated: true,
            changes: Some(vec![
                change("add-logout", &["auth", "ui/sidebar"]),
                change("add-billing", &["billing"]),
            ]),
        },
        LocationRead {
            location: stream("w1", "alpha"),
            validated: false,
            changes: Some(vec![change("add-logout", &["auth"]), change("add-sso", &["auth"])]),
        },
        LocationRead { location: stream("w3", "broken"), changes: None, validated: false },
    ];
    let validation = ProjectValidation {
        valid: false,
        items: vec![item("add-logout", "change", vec![issue("ERROR", "deltas[0]", "Bad delta")])],
    };
    let index = build_index(&caps(), Some(&validation), &locations);
    assert_eq!(index.unread_locations, 1);
    let auth = node(&index, "capability:auth");
    let order: Vec<(String, String)> = auth
        .changes
        .iter()
        .map(|chip| {
            let at = match &chip.location {
                ChipLocation::ActiveWorktree => "active".to_string(),
                ChipLocation::Stream { stream_name, .. } => stream_name.clone(),
            };
            (chip.change.clone(), at)
        })
        .collect();
    assert_eq!(
        order,
        [
            ("add-logout".into(), "active".into()),
            ("add-logout".into(), "alpha".into()),
            ("add-logout".into(), "zeta".into()),
            ("add-sso".into(), "alpha".into()),
        ]
    );
    assert_eq!(auth.changes[0].issues[0].message, "Bad delta");
    assert!(auth.changes[1].issues.is_empty(), "a stream chip holds no issue");
    // OSI-FR-VOOS: planned nodes, one for each id, in their own groups.
    assert_eq!(
        groups(&index),
        vec![
            (
                "group:/".to_string(),
                "specs".to_string(),
                vec!["capability:auth".to_string(), "capability:billing".to_string()]
            ),
            ("group:core".into(), "core".into(), vec!["capability:core/store".into()]),
            (
                "group:ui".into(),
                "ui".into(),
                vec![
                    "capability:ui/layout".into(),
                    "capability:ui/sidebar".into(),
                    "capability:ui/theme".into()
                ]
            ),
        ]
    );
    let planned = serde_json::to_value(node(&index, "capability:ui/sidebar")).unwrap();
    assert_eq!(planned["planned"], true);
    assert_eq!(planned["label"], "ui/sidebar");
    assert_eq!(planned["summary"], "");
    assert_eq!(planned["children"], json!([]));
    assert_eq!(
        planned["changes"],
        json!([{ "change": "add-logout", "location": { "kind": "active_worktree" },
                 "issues": [{ "level": "ERROR", "message": "Bad delta" }] }])
    );
    let live = serde_json::to_value(auth).unwrap();
    assert!(live.get("planned").is_none());
    assert_eq!(
        live["changes"][1]["location"],
        json!({ "kind": "stream", "streamId": "w1", "streamName": "alpha" })
    );
    let counts = index.counts();
    assert_eq!((counts.groups, counts.capabilities, counts.chips), (3, 6, 6));
}

// OSI contract surface: the load answers `{ source: "demo" }` or
// `{ source: "openspec", index }`, camelCase on the wire.
#[test]
fn the_load_answers_its_source() {
    assert_eq!(
        serde_json::to_value(SpecificationMapLoad::Demo).unwrap(),
        json!({ "source": "demo" })
    );
    let index = build_index(&[], None, &[]);
    assert_eq!(
        serde_json::to_value(SpecificationMapLoad::Openspec { index: Box::new(index) }).unwrap(),
        json!({ "source": "openspec", "index": {
            "levels": [
                { "plural": "groups", "singular": "group" },
                { "plural": "capabilities", "singular": "capability" },
                { "plural": "requirements", "singular": "requirement" }
            ],
            "roots": [], "dependencies": [], "validation": "unavailable", "unreadLocations": 0
        } })
    );
}

const LIST_SPECS: &str = r#"{ "specs": [ { "id": "auth", "requirementCount": 1 },
                                          { "id": "ui/theme", "requirementCount": 0 } ] }"#;
const SHOW_AUTH: &str = r#"{ "id": "auth", "title": "Auth", "overview": "Sign in.",
  "requirements": [ { "name": "Login", "text": "Users SHALL log in.", "scenarios": [] } ] }"#;
const SHOW_THEME: &str =
    r#"{ "id": "ui/theme", "title": "Theme", "overview": "", "requirements": [] }"#;
const LIST_CHANGES: &str = r#"{ "changes": [
  { "name": "add-logout", "completedTasks": 0, "totalTasks": 1, "lastModified": "2026-11-10T10:00:00Z" } ] }"#;

fn index_project() -> (tempfile::TempDir, RootFs) {
    let (dir, root) = openspec_project();
    write(&dir, "openspec/specs/auth/spec.md", "# Auth\n");
    write(&dir, "openspec/specs/ui/theme/spec.md", "# Theme\n");
    write(&dir, "openspec/changes/add-logout/specs/auth/spec.md", "## ADDED\n");
    (dir, root)
}

fn index_runner() -> crate::openspec::tests::FakeRunner {
    crate::openspec::tests::FakeRunner::default()
        .answering_prefix(&["list", "--specs"], Ok(ok(LIST_SPECS)))
        .answering_prefix(&["list", "--changes"], Ok(ok(LIST_CHANGES)))
        .answering_prefix(&["show", "auth"], Ok(ok(SHOW_AUTH)))
        .answering_prefix(&["show", "ui/theme"], Ok(ok(SHOW_THEME)))
        .json("status", r#"{ "changes": [] }"#)
        .json("validate", r#"{ "items": [] }"#)
}

// OSI-FR-SJNC, OSI-FR-DVLQ, OSI-FR-QELX, OSI-FR-DENT, OSI-FR-YVZW: one load
// runs `list` once, `show` once for each capability, `validate --all` once,
// and two processes for each location; a location that cannot be read is
// counted; the records carry counts and no label, name, or id.
#[test]
fn a_load_reads_the_capabilities_the_validation_and_each_location() {
    let (seam, runner, _) = seam_with(index_runner());
    let (_dir, root) = index_project();
    let (_stream_dir, stream_root) = openspec_project();
    let locations = vec![
        IndexLocation {
            location: ChipLocation::ActiveWorktree,
            at: Some(root.clone()),
            validated: true,
        },
        IndexLocation {
            location: stream("w1", "editor"),
            at: Some(stream_root.clone()),
            validated: false,
        },
        IndexLocation { location: stream("w2", "gone"), at: None, validated: false },
    ];
    let buffer = buffer();
    let index = seam.load_spec_index(&Silent, buffer, &root, &locations).unwrap();
    assert_eq!(index.validation, "valid");
    assert_eq!(index.unread_locations, 1);
    assert_eq!(node(&index, "capability:auth").changes.len(), 1, "the stream holds no delta");
    let calls = runner.calls();
    let count = |prefix: &[&str]| {
        calls.iter().filter(|c| prefix.iter().zip(&c.args).all(|(p, a)| p == a)).count()
    };
    assert_eq!(count(&["list", "--specs"]), 1);
    assert_eq!(count(&["show"]), 2);
    assert_eq!(count(&["validate", "--all", "--strict", "--json", "--no-interactive"]), 1);
    assert_eq!(count(&["list", "--changes"]), 2);
    assert_eq!(count(&["status", "--all"]), 2);
    let records = records(buffer);
    assert!(!records.is_empty(), "the reads are told");
    for record in records {
        let text = serde_json::to_string(&record).unwrap();
        for word in ["Login", "Sign in", "add-logout", "ui/theme"] {
            assert!(!text.contains(word), "{word} in a record: {text}");
        }
    }
}

// OSI-FR-SJNC, OSI-FR-PJJO, OSI-FR-OIOB: a read of a capability that fails
// fails the load with its code; a validation that fails does not, and the
// index carries its `OpenSpecFailure` as `validationFailure`.
#[test]
fn a_failed_capability_read_fails_the_load_and_a_failed_validation_does_not() {
    let runner = index_runner().answering(
        "validate",
        Ok(ProcessOutput { exit_status: Some(9), stdout: vec![], stderr: Vec::new() }),
    );
    let (seam, _, _) = seam_with(runner);
    let (_dir, root) = index_project();
    let locations = [IndexLocation {
        location: ChipLocation::ActiveWorktree,
        at: Some(root.clone()),
        validated: true,
    }];
    let index = seam.load_spec_index(&Silent, buffer(), &root, &locations).unwrap();
    assert_eq!(index.validation, "unavailable");
    let failure = index.validation_failure.as_ref().expect("the validation failure");
    assert_eq!(failure.code, "cli_failed");
    assert_eq!(failure.detail.as_deref(), Some("9"));
    let wire = serde_json::to_value(&index).unwrap();
    assert_eq!(wire["validationFailure"]["code"], "cli_failed");
    let (seam2, _, _) = seam_with(
        crate::openspec::tests::FakeRunner::default()
            .answering_prefix(&["list", "--specs"], Ok(ok(LIST_SPECS)))
            .answering("show", Err(crate::openspec::ProcessError::timed_out())),
    );
    assert_eq!(
        seam2.load_spec_index(&Silent, buffer(), &root, &locations),
        Err(OpenSpecReadError::CliTimedOut { diagnostics: Vec::new() })
    );
}

// OSI-FR-GRCV: where the active worktree is a stream's working copy, the
// issues of a change item go to that stream's chip, and to no other.
#[test]
fn the_issues_of_a_change_go_to_the_validated_stream() {
    let locations = vec![
        LocationRead {
            location: stream("w1", "alpha"),
            changes: Some(vec![change("add-logout", &["auth"])]),
            validated: true,
        },
        LocationRead {
            location: stream("w2", "zeta"),
            changes: Some(vec![change("add-logout", &["auth"])]),
            validated: false,
        },
    ];
    let validation = ProjectValidation {
        valid: false,
        items: vec![item("add-logout", "change", vec![issue("ERROR", "deltas[0]", "Bad delta")])],
    };
    let index = build_index(&caps(), Some(&validation), &locations);
    let auth = node(&index, "capability:auth");
    let issues: Vec<(String, usize)> = auth
        .changes
        .iter()
        .map(|chip| match &chip.location {
            ChipLocation::Stream { stream_name, .. } => (stream_name.clone(), chip.issues.len()),
            ChipLocation::ActiveWorktree => ("active".to_string(), chip.issues.len()),
        })
        .collect();
    assert_eq!(issues, [("alpha".to_string(), 1), ("zeta".to_string(), 0)]);
}
