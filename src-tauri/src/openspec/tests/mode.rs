//! Detection, the mode, and the status (OSM-FR-LLRV, OSM-FR-QNFI,
//! OSM-FR-JGYD, OSM-FR-UILN, OSM-FR-RMUD, OSM-FR-YXUC).

use super::*;

// OSM-FR-LLRV: the config file alone marks an OpenSpec project.
#[test]
fn the_config_file_marks_an_openspec_project() {
    let (_dir, root) = openspec_project();
    assert!(detection::uses_openspec(&root));
}

// OSM-FR-LLRV: a `spec.md` at any depth below `openspec/specs/` marks one too.
#[test]
fn a_nested_capability_spec_marks_an_openspec_project() {
    let (dir, root) = plain_project();
    write(&dir, "openspec/specs/ui/theme/spec.md", "# Theme\n");
    assert!(detection::uses_openspec(&root));
}

// OSM-FR-LLRV: an empty `openspec/` folder, a folder named like the config
// file, a folder named `spec.md`, and other files do not.
#[test]
fn neither_folders_nor_other_files_mark_an_openspec_project() {
    let (dir, root) = plain_project();
    assert!(!detection::uses_openspec(&root));
    std::fs::create_dir_all(dir.path().join("openspec/config.yaml")).unwrap();
    std::fs::create_dir_all(dir.path().join("openspec/specs/auth/spec.md")).unwrap();
    write(&dir, "openspec/specs/auth/notes.md", "notes\n");
    write(&dir, "openspec/changes/x/specs/auth/spec.md", "delta\n");
    assert!(!detection::uses_openspec(&root));
}

// OSM-FR-JGYD: nothing is kept between two reads.
#[test]
fn detection_follows_the_folder_at_each_read() {
    let (dir, root) = plain_project();
    assert!(!OpenSpec::mode_active(Some(&root)));
    write(&dir, "openspec/config.yaml", "schema: spec-driven\n");
    assert!(OpenSpec::mode_active(Some(&root)));
    std::fs::remove_file(dir.path().join("openspec/config.yaml")).unwrap();
    assert!(!OpenSpec::mode_active(Some(&root)));
}

// OSM-FR-UILN: the setting `off` keeps the mode inactive, and no project means
// no mode.
#[test]
fn off_and_no_project_keep_the_mode_inactive() {
    let (dir, root) = openspec_project();
    assert!(OpenSpec::mode_active(Some(&root)));
    write(&dir, ".synthesis/project.toml", "openspecMode = \"off\"\n");
    assert!(!OpenSpec::mode_active(Some(&root)));
    assert!(!OpenSpec::mode_active(None));
}

// OSM-FR-UILN, PSS-FR-JXKK: a malformed stored setting reads as `auto`.
#[test]
fn a_malformed_setting_reads_as_auto() {
    let (dir, root) = openspec_project();
    write(&dir, ".synthesis/project.toml", "openspecMode = \"sometimes\"\n");
    assert!(OpenSpec::mode_active(Some(&root)));
}

// OSM-FR-RMUD, CVL-FR-DPWP: a project that does not use OpenSpec gets no read
// tools, and starts no CLI process for the decision.
#[test]
fn a_plain_project_gets_no_read_tools_and_runs_nothing() {
    let (seam, runner, _) = seam_with(FakeRunner::default());
    let (_dir, root) = plain_project();
    assert!(!seam.read_tools_attached(&Silent, buffer(), Some(&root)));
    assert!(runner.calls().is_empty());
}

// CVL-FR-DPWP: an active mode with an available CLI gets the tools; a CLI that
// is not found or not supported keeps them away.
#[test]
fn the_read_tools_need_an_active_mode_and_an_available_cli() {
    let (_dir, root) = openspec_project();
    let (seam, _, probe) = seam_with(FakeRunner::default());
    assert!(seam.read_tools_attached(&Silent, buffer(), Some(&root)));
    probe.executables.lock().unwrap().clear();
    assert!(!seam.read_tools_attached(&Silent, buffer(), Some(&root)));

    let runner = Arc::new(FakeRunner::default().json("--version", "0.9.0"));
    let old = OpenSpec::new(runner, Arc::new(FakeProbe::default().with_executable(CLI)), env());
    assert!(!old.read_tools_attached(&Silent, buffer(), Some(&root)));
}

// OSM-FR-YXUC: the status names the setting, the detection, the mode, and the
// CLI state, and checks the CLI also while the mode is off.
#[test]
fn the_status_checks_the_cli_also_while_the_mode_is_off() {
    let (dir, root) = openspec_project();
    write(&dir, ".synthesis/project.toml", "openspecMode = \"off\"\n");
    let (seam, runner, _) = seam_with(FakeRunner::default());
    let status = seam.status(&Silent, buffer(), Some(&root));
    assert_eq!(status.setting, "off");
    assert!(status.uses_openspec);
    assert!(!status.active);
    assert!(status.cli.is_available());
    assert_eq!(runner.calls_of("--version"), 1);

    let (_plain, plain) = plain_project();
    let status = seam.status(&Silent, buffer(), Some(&plain));
    assert_eq!(status.setting, "auto");
    assert!(!status.uses_openspec);
    assert!(!status.active);
}

// OSM-FR-YXUC: the status serialises with the field names of the contract, in
// the camel case of the wire.
#[test]
fn the_status_serialises_as_the_contract_names_it() {
    let (_dir, root) = openspec_project();
    let (seam, _, _) = seam_with(FakeRunner::default());
    let value = serde_json::to_value(seam.status(&Silent, buffer(), Some(&root))).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "setting": "auto",
            "usesOpenspec": true,
            "active": true,
            "cli": { "state": "available", "path": CLI, "version": "1.17.0" },
        })
    );
}

// CVL-FR-DPWP: on the path of a turn, a CLI with no kept check gets no tools
// and starts no process; the caller is told to start the check, and once it is
// kept the tools are attached.
#[test]
fn the_turn_path_reads_only_the_kept_check() {
    let (_dir, root) = openspec_project();
    let (seam, runner, probe) = seam_with(FakeRunner::default());
    let mut asked = 0;
    assert!(!seam.read_tools_attached_now(Some(&root), || asked += 1));
    assert_eq!(asked, 1);
    assert!(runner.calls().is_empty());

    seam.cli_state(&Silent, buffer());
    assert!(seam.read_tools_attached_now(Some(&root), || panic!("a kept check needs no new one")));
    assert_eq!(runner.calls_of("--version"), 1);

    // A CLI that changed has no kept check any more.
    probe.touch(CLI, SystemTime::UNIX_EPOCH + Duration::from_secs(5));
    let mut again = 0;
    assert!(!seam.read_tools_attached_now(Some(&root), || again += 1));
    assert_eq!(again, 1);

    // A CLI that is not found needs no check, and no tools follow.
    probe.executables.lock().unwrap().clear();
    assert!(!seam.read_tools_attached_now(Some(&root), || panic!("not_found needs no check")));
}

// CVL-FR-DPWP: an inactive mode asks for nothing and attaches nothing.
#[test]
fn the_turn_path_of_an_inactive_mode_asks_for_no_check() {
    let (_dir, root) = plain_project();
    let (seam, runner, _) = seam_with(FakeRunner::default());
    assert!(!seam.read_tools_attached_now(Some(&root), || panic!("no check for an inactive mode")));
    assert!(runner.calls().is_empty());
}
