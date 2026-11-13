//! OAT: validation, apply progress, the spec index, the archive, and the
//! failure classes against the real CLI.

use super::*;
use crate::openspec::archive_cli::ArchiveCliFailure;
use crate::openspec::project_validation::ValidationScope;
use crate::openspec::spec_index::{ChipLocation, IndexLocation};

// OAT-FR-VTRA, OSM-FR-APVF, OSM-FR-XDRN, OFP-FR-RUOP: a valid change, a
// change that the CLI does not accept with its issues as diagnostics, and the
// project validation of `--all` and `--specs`.
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_validation() {
    let fx = Fixture::new();
    let root = fx.root();
    let os = seam();
    let says = fx.says("validation", "validate add-logout --type change --strict");
    let good = os.validate_change(&Silent, buffer(), Some(&root), &root, GOOD).expect(&says);
    assert!(good.valid, "{says}: the issues are {:?}", good.issues);

    let says = fx.says("validation", "validate bad-change --type change --strict");
    let bad = os.validate_change(&Silent, buffer(), Some(&root), &root, BAD).expect(&says);
    assert!(!bad.valid, "{says}");
    let first = bad.issues.first().unwrap_or_else(|| panic!("{says}: no issue"));
    assert_eq!(first.level, "ERROR", "{says}");
    assert!(!first.message.is_empty(), "{says}");
    let diagnostics = bad.diagnostics();
    assert_eq!(diagnostics.len(), bad.issues.len(), "{says}: one diagnostic for each issue");
    assert_eq!(
        diagnostics[0].severity.as_deref(),
        Some("error"),
        "{says}: the level, lowercase as a CLI severity"
    );
    assert_eq!(diagnostics[0].message, first.message, "{says}");

    for scope in [ValidationScope::All, ValidationScope::Specs] {
        let says = fx.says(
            "validation",
            if matches!(scope, ValidationScope::All) { "--all" } else { "--specs" },
        );
        let all = os.validate_project(&Silent, buffer(), Some(&root), &root, scope).expect(&says);
        let ids: Vec<(&str, &str)> =
            all.items.iter().map(|i| (i.kind.as_str(), i.id.as_str())).collect();
        for capability in ["auth", "billing", "ui/theme"] {
            assert!(ids.contains(&("spec", capability)), "{says}: {capability} in {ids:?}");
        }
        let has_changes = ids.iter().any(|(kind, _)| *kind == "change");
        assert_eq!(has_changes, matches!(scope, ValidationScope::All), "{says}: {ids:?}");
        assert!(!all.valid, "{says}: billing has a short purpose");
        for item in &all.items {
            let expect_issues = item.id == BAD || item.id == "billing";
            assert_eq!(item.valid, !expect_issues, "{says}: {} {}", item.kind, item.id);
            assert_eq!(!item.issues.is_empty(), expect_issues, "{says}: the issues of {}", item.id);
        }
    }
}

// OAT-FR-FOLQ, OSM-FR-QLRW, OSM-FR-WIQF, OSM-FR-PEAK: the state, the counts,
// and the next task equal what `tasks.md` holds; a tick in the file changes
// the next read.
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_apply_progress() {
    let fx = Fixture::new();
    let root = fx.root();
    let os = seam();
    let says = fx.says("apply", "instructions apply --change add-logout");
    let read = os.apply_instructions(&Silent, buffer(), Some(&root), &root, GOOD).expect(&says);
    assert_eq!(read.state, "ready", "{says}");
    assert_eq!((read.total, read.complete), (3, 1), "{says}");
    assert_eq!(read.next_task().as_deref(), Some("1.2 Remove the token from the store"), "{says}");
    assert!(read.missing_artifacts.is_empty(), "{says}: {:?}", read.missing_artifacts);

    let tasks = fx.read("openspec/changes/add-logout/tasks.md").replace("- [ ]", "- [x]");
    fx.write("openspec/changes/add-logout/tasks.md", &tasks);
    let done = os.apply_instructions(&Silent, buffer(), Some(&root), &root, GOOD).expect(&says);
    assert_eq!(done.state, "all_done", "{says}: after every tick");
    assert_eq!((done.total, done.complete), (3, 3), "{says}");
    assert_eq!(done.next_task(), None, "{says}");
}

// OAT-FR-FOLQ, OFP-FR-BWML: a change with missing artifacts is `blocked`, and
// its diagnostics name each missing artifact.
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_apply_on_a_change_with_missing_artifacts() {
    let fx = Fixture::new();
    let root = fx.root();
    let says = fx.says("apply", "instructions apply --change bad-change");
    let read = seam().apply_instructions(&Silent, buffer(), Some(&root), &root, BAD).expect(&says);
    assert_eq!(read.state, "blocked", "{says}");
    assert!(!read.missing_artifacts.is_empty(), "{says}");
    let diagnostics = read.not_ready_diagnostics();
    // The missing artifacts, compared with the raw answer of the CLI.
    let raw = quiet(Command::new(&fx.cli.path))
        .args(["instructions", "apply", "--change", BAD, "--json"])
        .current_dir(fx.path())
        .output()
        .expect("the CLI runs");
    let raw: serde_json::Value = serde_json::from_slice(&raw.stdout).expect(&says);
    let expected: Vec<String> = raw["missingArtifacts"]
        .as_array()
        .map(|list| list.iter().filter_map(|a| a.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    assert_eq!(read.missing_artifacts, expected, "{says}: the missing artifacts");
    for artifact in &read.missing_artifacts {
        assert!(
            diagnostics
                .iter()
                .any(|d| d.severity.as_deref() == Some("error")
                    && d.message.contains(artifact.as_str())),
            "{says}: no diagnostic names {artifact}: {diagnostics:?}"
        );
    }
}

// OAT-FR-CGIA, OSI-FR-MAUQ, OSI-FR-SJNC, OSI-FR-ALQI, OSI-FR-SBVF: the groups,
// the capabilities, and the requirements read from the CLI, a change chip on
// each capability that the change touches, and a validation issue as a gap.
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_spec_index() {
    let fx = Fixture::new();
    let root = fx.root();
    let says =
        fx.says("index", "list --specs + show + validate --all + list --changes + status --all");
    let locations = [IndexLocation {
        location: ChipLocation::ActiveWorktree,
        at: Some(fx.root()),
        validated: true,
    }];
    let index = seam().load_spec_index(&Silent, buffer(), &root, &locations).expect(&says);
    let json = serde_json::to_value(&index).unwrap();
    let groups: Vec<&str> =
        json["roots"].as_array().unwrap().iter().map(|g| g["id"].as_str().unwrap()).collect();
    assert_eq!(groups, ["group:/", "group:ui"], "{says}: the groups");
    let capability = |id: &str| {
        json["roots"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|g| g["children"].as_array().unwrap().iter())
            .find(|c| c["id"] == format!("capability:{id}"))
            .unwrap_or_else(|| panic!("{says}: no capability {id} in the index"))
            .clone()
    };
    let auth = capability("auth");
    assert_eq!(auth["children"].as_array().unwrap().len(), 2, "{says}: the requirements of auth");
    let chips = |node: &serde_json::Value| -> Vec<String> {
        node["changes"]
            .as_array()
            .map(|c| c.iter().map(|chip| chip["change"].as_str().unwrap().to_string()).collect())
            .unwrap_or_default()
    };
    assert_eq!(chips(&auth), [GOOD], "{says}: the chip on auth");
    assert_eq!(chips(&capability("ui/theme")), [GOOD], "{says}: the chip on ui/theme");
    assert!(chips(&capability("billing")).is_empty(), "{says}: no chip on billing");
    assert_eq!(index.validation, "invalid", "{says}: {BAD} is not valid");
    let issues = |node: &serde_json::Value| node["issues"].as_array().map_or(0, Vec::len);
    assert!(issues(&capability("billing")) > 0, "{says}: the short purpose of billing is a gap");
    assert_eq!(issues(&auth), 0, "{says}: auth has no issue");
}

// OAT-FR-WYEN, OAR-FR-IAKL, OAR-FR-VAGH, OAR-FR-XTLB: a change with all tasks
// done archives, the archive folder is valid, and the live spec holds the
// changed requirements.
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_archive_succeeds() {
    let fx = Fixture::new();
    let tasks = fx.read("openspec/changes/add-logout/tasks.md").replace("- [ ]", "- [x]");
    fx.write("openspec/changes/add-logout/tasks.md", &tasks);
    fx.commit("tick every task");
    let says = fx.says("archive", "archive add-logout --json --yes");
    let answer = seam()
        .archive_change(&Silent, buffer(), &fx.cli.path, fx.path(), GOOD)
        .unwrap_or_else(|failure| panic!("{says}: {failure:?}"));
    assert!(change::archive_folder_of(&answer.archived_as, GOOD), "{says}: {}", answer.archived_as);
    assert!(
        fx.exists(&format!("openspec/changes/archive/{}/proposal.md", answer.archived_as)),
        "{says}"
    );
    assert!(!fx.exists("openspec/changes/add-logout"), "{says}: the change folder moved");
    let auth = fx.read("openspec/specs/auth/spec.md");
    assert!(auth.contains("### Requirement: Sign out"), "{says}: the ADDED requirement");
    assert!(auth.contains("15 minutes"), "{says}: the MODIFIED requirement");
    assert!(
        auth.contains("### Requirement: Sign in with password"),
        "{says}: the RENAMED requirement"
    );
    assert!(!fx.read("openspec/specs/ui/theme/spec.md").contains("Old toggle"), "{says}: REMOVED");
}

// OAT-FR-WYEN, OAR-FR-EUXZ, OFP-FR-NJSO, OFP-FR-HQQN: an archive that the CLI
// refuses fails with the code of its first `status` entry, carries every
// entry as a diagnostic, and changes no file.
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_archive_refusal_carries_the_cli_code() {
    let fx = Fixture::new();
    fx.write(
        "openspec/changes/ghost/proposal.md",
        "## Why\nChange a requirement that the billing capability does not hold at all.\n\n## What Changes\n- Modify a requirement.\n",
    );
    fx.write("openspec/changes/ghost/tasks.md", "## 1. Work\n- [x] 1.1 Done\n");
    fx.write(
        "openspec/changes/ghost/specs/billing/spec.md",
        "## MODIFIED Requirements\n### Requirement: Yearly invoice\nThe system SHALL send an invoice each year.\n\n\
         #### Scenario: Year\n- **WHEN** the year starts\n- **THEN** the system sends an invoice\n",
    );
    fx.commit("a change that cannot archive");
    let before = fx.read("openspec/specs/billing/spec.md");
    let says = fx.says("archive", "archive ghost --json --yes");
    match seam().archive_change(&Silent, buffer(), &fx.cli.path, fx.path(), "ghost") {
        Err(ArchiveCliFailure::Status { code, diagnostics }) => {
            assert_eq!(code, "archive_spec_update_failed", "{says}");
            assert_eq!(
                diagnostics.first().and_then(|d| d.code.as_deref()),
                Some(code.as_str()),
                "{says}"
            );
            assert!(diagnostics[0].fix.is_some(), "{says}: the CLI gives a fix");
            assert_eq!(
                classify_openspec_failure(&code),
                FailureClass::Content,
                "{says}: the class"
            );
        }
        other => panic!("{says}: the answer is {other:?}"),
    }
    assert_eq!(
        fx.read("openspec/specs/billing/spec.md"),
        before,
        "{says}: the live spec is unchanged"
    );
    assert!(fx.exists("openspec/changes/ghost/proposal.md"), "{says}: the change stays");
}

// OAT-FR-OUBE, OFP-FR-SQDK, OFP-FR-GPCO, OFP-FR-EOAD: an executable that is
// not found is `cli_unavailable` with `not_found`, of the class
// `environment`, and starts no process.
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_missing_executable_is_an_environment_failure() {
    let fx = Fixture::new();
    let root = fx.root();
    let empty = TempDir::new().unwrap();
    let env = CliEnvironment {
        path_var: Some(empty.path().to_string_lossy().into_owned()),
        home: Some(empty.path().to_path_buf()),
    };
    let os = OpenSpec::new(Arc::new(RealRunner), Arc::new(RealCliProbe), env)
        .with_wait(Arc::new(RealWait));
    let says = fx.says("failure classes", "no executable");
    let error = os.list_capabilities(&Silent, buffer(), Some(&root)).expect_err(&says);
    let failure = error.to_failure();
    assert_eq!(failure.code, "cli_unavailable", "{says}");
    assert_eq!(failure.detail.as_deref(), Some("not_found"), "{says}");
    assert_eq!(failure.class(), FailureClass::Environment, "{says}");
}

// OAT-FR-OUBE, OSM-FR-WWWK, OFP-FR-RQXW, OFP-FR-NJSO: an exit status 1 with a
// `status` list is `cli_rejected`, of the class `content`, with the CLI's
// entries as diagnostics.
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_rejected_command_is_a_content_failure() {
    let fx = Fixture::new();
    fx.write("openspec/changes/odd/.openspec.yaml", "schema: no-such-schema\n");
    fx.write(
        "openspec/changes/odd/proposal.md",
        "## Why\nA change with a schema that does not exist.\n",
    );
    let root = fx.root();
    let says = fx.says("failure classes", "instructions apply --change odd");
    let error =
        seam().apply_instructions(&Silent, buffer(), Some(&root), &root, "odd").expect_err(&says);
    let failure = error.to_failure();
    assert_eq!(failure.code, "cli_rejected", "{says}: {error:?}");
    assert_eq!(failure.class(), FailureClass::Content, "{says}");
    let first = failure.diagnostics.first().unwrap_or_else(|| panic!("{says}: no diagnostic"));
    assert_eq!(first.severity.as_deref(), Some("error"), "{says}");
    assert_eq!(first.code.as_deref(), Some("change_error"), "{says}: the CLI code");
    assert!(first.message.contains("no-such-schema"), "{says}: {:?}", first.message);
}

// OAT-FR-DJYT, OSM-FR-JTOQ, OSM-FR-OPZF: a project right after
// `openspec init` holds no capability and no change, and each read answers
// an empty list.
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_empty_project_reads_empty_lists() {
    let fx = Fixture::empty();
    let root = fx.root();
    let os = seam();
    let says = fx.says("empty project", "list --specs + list --changes");
    assert!(detection::uses_openspec(&root), "{says}: config.yaml marks the project");
    assert_eq!(os.list_capabilities(&Silent, buffer(), Some(&root)).expect(&says), [], "{says}");
    assert_eq!(os.list_changes(&Silent, buffer(), Some(&root)).expect(&says), [], "{says}");
}

// OAT-FR-MSXV, OSM-FR-ZNVZ, OSM-FR-SKUP: an `openspec` that reports a
// version below 1, first in the search path, is `unsupported` with its
// version, and a read refuses with `cli_unavailable`.
#[cfg(unix)]
#[test]
#[ignore = "needs the OpenSpec CLI"]
fn openspec_uat_cli_old_cli_first_on_path_is_unsupported() {
    use std::os::unix::fs::PermissionsExt;
    let fx = Fixture::new();
    let root = fx.root();
    let shim = TempDir::new().unwrap();
    let path = shim.path().join("openspec");
    std::fs::write(&path, "#!/bin/sh\necho 0.9.0\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut env = CliEnvironment::from_process();
    env.path_var = Some(format!("{}:{}", shim.path().display(), env.path_var.unwrap_or_default()));
    let os = OpenSpec::new(Arc::new(RealRunner), Arc::new(RealCliProbe), env)
        .with_wait(Arc::new(RealWait));
    let says = fx.says("old CLI", "--version of a 0.9.0 shim");
    match os.cli_state_in(&Silent, buffer(), Some(&root)) {
        OpenSpecCliState::Unsupported { version, .. } => assert_eq!(version, "0.9.0", "{says}"),
        other => panic!("{says}: the CLI state is {other:?}"),
    }
    let failure =
        os.list_capabilities(&Silent, buffer(), Some(&root)).expect_err(&says).to_failure();
    assert_eq!(failure.code, "cli_unavailable", "{says}");
    assert_eq!(failure.detail.as_deref(), Some("unsupported"), "{says}");
}
