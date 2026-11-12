//! OAT: detection, the version check, and each read of the seam against the
//! real CLI.

use super::*;
use crate::openspec::project_validation::ValidationScope;
use crate::openspec::spec_index::{ChipLocation, IndexLocation};

// OAT-FR-MSXV, OSM-FR-LLRV, OSM-FR-ZNVZ, OSM-FR-SKUP: the fixture uses
// OpenSpec, and the CLI state is `available` with the path and the version
// of the CLI under test.
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_detection_and_version_check() {
    let fx = Fixture::new();
    let root = fx.root();
    let says = fx.says("detection", "--version");
    assert!(
        detection::uses_openspec(&root),
        "{says}: `openspec init` makes a project that uses OpenSpec"
    );
    assert!(OpenSpec::mode_active(Some(&root)), "{says}: the mode is active by default");
    match seam().cli_state_in(&Silent, buffer(), Some(&root)) {
        OpenSpecCliState::Available { path, version } => {
            assert_eq!(PathBuf::from(&path), fx.cli.path, "{says}: the path");
            assert_eq!(version, fx.cli.version, "{says}: the version");
        }
        other => panic!("{says}: the CLI state is {other:?}"),
    }
}

// OAT-FR-DJYT, OSM-FR-JTOQ, OSM-FR-SIBT: the capabilities, ordered by id, a
// capability with its requirements and scenarios, and a capability that is
// not there.
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_capability_reads() {
    let fx = Fixture::new();
    let root = fx.root();
    let os = seam();
    let says = fx.says("capabilities", "list --specs");
    let listed = os.list_capabilities(&Silent, buffer(), Some(&root)).expect(&says);
    let ids: Vec<(&str, u64)> =
        listed.iter().map(|c| (c.id.as_str(), c.requirement_count)).collect();
    assert_eq!(ids, [("auth", 2), ("billing", 1), ("ui/theme", 2)], "{says}");

    let says = fx.says("capabilities", "show auth --type spec");
    let auth = os.read_capability(&Silent, buffer(), Some(&root), "auth").expect(&says);
    let names: Vec<&str> = auth.requirements.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Sign in", "Session expiry"], "{says}");
    assert!(
        auth.overview.starts_with("Users sign in"),
        "{says}: the overview is {:?}",
        auth.overview
    );
    let scenario = &auth.requirements[1].scenarios[0];
    assert_eq!(scenario.name, "Idle session", "{says}");
    assert!(
        scenario.text.contains("30 minutes"),
        "{says}: the scenario text is {:?}",
        scenario.text
    );

    let says = fx.says("capabilities", "show ui/theme --type spec");
    let theme = os.read_capability(&Silent, buffer(), Some(&root), "ui/theme").expect(&says);
    assert_eq!(theme.requirements.len(), 2, "{says}");

    let missing = os.read_capability(&Silent, buffer(), Some(&root), "nope");
    assert_eq!(
        missing,
        Err(OpenSpecReadError::CapabilityNotFound),
        "{}",
        fx.says("capabilities", "show nope")
    );
}

// OAT-FR-DJYT, OSM-FR-OPZF, OSM-FR-FCMY: the active changes with their task
// counts and the capabilities of their delta files.
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_change_list() {
    let fx = Fixture::new();
    let root = fx.root();
    let says = fx.says("changes", "list --changes + status --all");
    let changes = seam().list_changes(&Silent, buffer(), Some(&root)).expect(&says);
    let good = changes.iter().find(|c| c.name == GOOD).unwrap_or_else(|| panic!("{says}: {GOOD}"));
    assert_eq!((good.completed_tasks, good.total_tasks), (1, 3), "{says}: the task counts");
    assert!(good.is_complete, "{says}: every artifact of {GOOD} is done, open tasks aside");
    assert!(good.artifacts.iter().all(|a| a.status == "done"), "{says}: {:?}", good.artifacts);
    assert_eq!(good.capabilities, ["auth", "ui/theme"], "{says}: the capabilities");
    assert_eq!(good.schema, "spec-driven", "{says}: the schema");
    let ids: Vec<&str> = good.artifacts.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(
        ids,
        ["proposal", "specs", "design", "tasks"],
        "{says}: the artifacts in build order"
    );
    let bad = changes.iter().find(|c| c.name == BAD).unwrap_or_else(|| panic!("{says}: {BAD}"));
    assert_eq!((bad.completed_tasks, bad.total_tasks), (0, 0), "{says}: {BAD} has no tasks");
    assert!(bad.capabilities.is_empty(), "{says}: {BAD} has no delta files");
    assert!(!bad.is_complete, "{says}: {BAD} holds the proposal alone");
}

// OAT-FR-DJYT, OSM-FR-LLIO, OSM-FR-NMHS, OSM-FR-PYHP: a change read with its
// artifacts, each delta kind, and the live requirement of each delta.
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_change_read() {
    let fx = Fixture::new();
    let root = fx.root();
    let says = fx.says("change", "status --change + show --deltas-only");
    let detail = seam()
        .read_change(&Silent, buffer(), Some(&root), &root, ChangeLocation::ActiveWorktree, GOOD)
        .expect(&says);
    assert_eq!(detail.name, GOOD, "{says}");
    assert_eq!((detail.completed_tasks, detail.total_tasks), (1, 3), "{says}");
    let artifacts: Vec<&str> = detail.artifacts.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(artifacts, ["proposal", "specs", "design", "tasks"], "{says}");
    let spec_files: Vec<&str> = detail
        .artifacts
        .iter()
        .find(|a| a.id == "specs")
        .map(|a| a.files.iter().map(|f| f.path.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        spec_files,
        ["specs/auth/spec.md", "specs/ui/theme/spec.md"],
        "{says}: the delta files"
    );

    let kinds: Vec<(&str, &str)> =
        detail.deltas.iter().map(|d| (d.capability.as_str(), d.operation.as_str())).collect();
    assert_eq!(
        kinds,
        [("auth", "ADDED"), ("auth", "MODIFIED"), ("auth", "RENAMED"), ("ui/theme", "REMOVED")],
        "{says}: the deltas"
    );
    let delta = |op: &str| detail.deltas.iter().find(|d| d.operation == op).unwrap();
    assert!(delta("ADDED").live.is_none(), "{says}: an ADDED requirement has no live one");
    let modified = delta("MODIFIED");
    assert!(modified.requirement.as_ref().unwrap().text.contains("15 minutes"), "{says}");
    assert!(modified.live.as_ref().unwrap().text.contains("30 minutes"), "{says}: the live text");
    let renamed = delta("RENAMED");
    let rename = renamed.rename.as_ref().unwrap();
    assert_eq!(
        (rename.from.as_str(), rename.to.as_str()),
        ("Sign in", "Sign in with password"),
        "{says}"
    );
    assert_eq!(renamed.live.as_ref().map(|r| r.name.as_str()), Some("Sign in"), "{says}");
    assert_eq!(
        delta("REMOVED").live.as_ref().map(|r| r.name.as_str()),
        Some("Old toggle"),
        "{says}"
    );
    assert!(detail.deltas.iter().all(|d| d.capability_exists), "{says}: every capability is live");
}

// OAT-FR-RYAS: each read gives the same result where the location path is
// the form the temporary folder gives (a symbolic link on macOS) and where
// it is the resolved form.
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_link_and_resolved_paths_read_the_same() {
    let fx = Fixture::new();
    let link = fx.root();
    let resolved_path = std::fs::canonicalize(fx.path()).unwrap();
    let resolved = RootFs::for_root(&resolved_path);
    let os = seam();
    let says =
        fx.says("paths", &format!("{} and {}", fx.path().display(), resolved_path.display()));

    #[cfg(target_os = "macos")]
    assert_ne!(fx.path(), resolved_path, "{says}: the temporary folder is a link on macOS");
    assert_eq!(
        os.cli_state_in(&Silent, buffer(), Some(&link)),
        os.cli_state_in(&Silent, buffer(), Some(&resolved)),
        "{says}: the CLI state"
    );
    let caps = |at: &RootFs| os.list_capabilities(&Silent, buffer(), Some(at)).expect(&says);
    assert_eq!(caps(&link), caps(&resolved), "{says}: list --specs");
    let changes = |at: &RootFs| os.list_changes(&Silent, buffer(), Some(at)).expect(&says);
    let (a, b) = (changes(&link), changes(&resolved));
    assert_eq!(a, b, "{says}: list --changes");
    let read = |at: &RootFs| {
        os.read_change(&Silent, buffer(), Some(at), at, ChangeLocation::ActiveWorktree, GOOD)
            .expect(&says)
    };
    assert_eq!(read(&link), read(&resolved), "{says}: the change read");
    let apply =
        |at: &RootFs| os.apply_instructions(&Silent, buffer(), Some(at), at, GOOD).expect(&says);
    assert_eq!(apply(&link), apply(&resolved), "{says}: instructions apply");
    let capability =
        |at: &RootFs| os.read_capability(&Silent, buffer(), Some(at), "ui/theme").expect(&says);
    assert_eq!(capability(&link), capability(&resolved), "{says}: show ui/theme");
    let validate =
        |at: &RootFs| os.validate_change(&Silent, buffer(), Some(at), at, BAD).expect(&says);
    assert_eq!(validate(&link), validate(&resolved), "{says}: validate bad-change");
    let project = |at: &RootFs| {
        os.validate_project(&Silent, buffer(), Some(at), at, ValidationScope::All).expect(&says)
    };
    assert_eq!(project(&link), project(&resolved), "{says}: validate --all");
    let index = |at: &RootFs| {
        let locations = [IndexLocation {
            location: ChipLocation::ActiveWorktree,
            at: Some(RootFs::for_root(at.path())),
            validated: true,
        }];
        let index = os.load_spec_index(&Silent, buffer(), at, &locations).expect(&says);
        serde_json::to_value(index).unwrap()
    };
    assert_eq!(index(&link), index(&resolved), "{says}: the spec index");
}
