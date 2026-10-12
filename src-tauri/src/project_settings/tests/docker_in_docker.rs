//! The Docker-in-Docker settings (PSS-FR-DIND, PSS-FR-QWTN, PSS-FR-VRHC,
//! PSS-FR-MTJE, PSS-FR-16).

use super::*;

fn root_of(dir: &TempDir) -> crate::fs::RootFs {
    crate::fs::RootFs::for_root(dir.path())
}

fn settings(enabled: bool, socket_path: &str, host_gateway: bool) -> DockerInDockerSettings {
    DockerInDockerSettings { enabled, socket_path: socket_path.to_string(), host_gateway }
}

// PSS-FR-DIND: a worktree with nothing stored is disabled, with the default
// path and the gateway off.
#[test]
fn an_unconfigured_worktree_is_disabled() {
    let dir = TempDir::new().unwrap();
    let loaded = load_docker_in_docker_settings_from(&root_of(&dir));
    assert_eq!(loaded, settings(false, "", false));
    assert!(!dir.path().join(".synthesis/local.toml").exists());
}

// PSS-FR-DIND, PSS-FR-QWTN: the three values round trip through local.toml and
// never reach project.toml.
#[test]
fn the_three_values_round_trip_in_local_toml_only() {
    let dir = TempDir::new().unwrap();
    let saved = settings(true, "/run/user/1000/docker.sock", true);
    save_docker_in_docker_settings_to(&root_of(&dir), saved.clone()).unwrap();

    assert_eq!(load_docker_in_docker_settings_from(&root_of(&dir)), saved);
    let local = std::fs::read_to_string(dir.path().join(".synthesis/local.toml")).unwrap();
    assert!(local.contains("docker_in_docker"), "{local}");
    assert!(!dir.path().join(".synthesis/project.toml").exists(), "project.toml is untouched");
    assert!(dir.path().join(".synthesis/.gitignore").is_file(), "local.toml stays ignored");
}

// PSS-FR-DIND, PSS-FR-16: the values are per worktree.
#[test]
fn two_worktrees_keep_independent_values() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    save_docker_in_docker_settings_to(&root_of(&a), settings(true, "", false)).unwrap();

    assert_eq!(load_docker_in_docker_settings_from(&root_of(&b)), settings(false, "", false));
    assert!(!b.path().join(".synthesis/local.toml").exists());
    assert!(load_docker_in_docker_settings_from(&root_of(&a)).enabled);
}

// PSS-FR-QWTN: the save keeps every other section of local.toml.
#[test]
fn saving_keeps_every_other_project_local_section() {
    let dir = TempDir::new().unwrap();
    save_changes_panel_state_to(&root_of(&dir), branch_state("develop")).unwrap();
    save_docker_in_docker_settings_to(&root_of(&dir), settings(true, "", false)).unwrap();

    assert_eq!(load_changes_panel_state_from(&root_of(&dir)), branch_state("develop"));
    // And the other way round: a panel save keeps the Docker-in-Docker values.
    save_changes_panel_state_to(&root_of(&dir), branch_state("main")).unwrap();
    assert!(load_docker_in_docker_settings_from(&root_of(&dir)).enabled);
}

// PSS-FR-QWTN: one damaged value costs only itself.
#[test]
fn a_damaged_value_reads_as_its_default_and_costs_only_itself() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(
        dir.path().join(".synthesis/local.toml"),
        "[docker_in_docker]\nenabled = \"yes\"\nsocketPath = \"/a/b.sock\"\nhostGateway = true\n",
    )
    .unwrap();
    assert_eq!(
        load_docker_in_docker_settings_from(&root_of(&dir)),
        settings(false, "/a/b.sock", true)
    );
}

// PSS-FR-VRHC: the path is trimmed, and a broken path is refused with nothing
// written.
#[test]
fn a_broken_socket_path_is_refused_and_nothing_is_written() {
    let dir = TempDir::new().unwrap();
    for path in ["docker.sock", "relative/docker.sock", "/a,b/docker.sock", "/a\nb"] {
        let refused =
            save_docker_in_docker_settings_to(&root_of(&dir), settings(true, path, false));
        assert_eq!(refused, Err(ERR_DOCKER_SOCKET_PATH_INVALID.to_string()), "{path:?}");
    }
    assert!(!dir.path().join(".synthesis/local.toml").exists());

    save_docker_in_docker_settings_to(&root_of(&dir), settings(true, "  /a/d.sock  ", false))
        .unwrap();
    assert_eq!(load_docker_in_docker_settings_from(&root_of(&dir)).socket_path, "/a/d.sock");
}

// PSS-FR-VRHC: the save never checks that the socket exists.
#[test]
fn the_save_does_not_check_that_the_socket_exists() {
    let dir = TempDir::new().unwrap();
    save_docker_in_docker_settings_to(&root_of(&dir), settings(true, "/not/there.sock", false))
        .unwrap();
    assert!(load_docker_in_docker_settings_from(&root_of(&dir)).enabled);
}

// PSS-FR-MTJE, PSS-FR-QWTN: an empty path reads as the platform default.
#[cfg(unix)]
#[test]
fn an_empty_path_reads_as_the_platform_default() {
    let dir = TempDir::new().unwrap();
    save_docker_in_docker_settings_to(&root_of(&dir), settings(true, "", true)).unwrap();
    let read = read_docker_in_docker_settings(&root_of(&dir));
    assert_eq!(read.effective_socket_path, "/var/run/docker.sock");
    assert_eq!(read.socket_path, "");

    save_docker_in_docker_settings_to(&root_of(&dir), settings(true, "/x/d.sock", true)).unwrap();
    assert_eq!(read_docker_in_docker_settings(&root_of(&dir)).effective_socket_path, "/x/d.sock");
}

// PSS-FR-QWTN: the support level follows the platform.
#[test]
fn the_support_level_follows_the_platform() {
    let expected = if cfg!(target_os = "macos") {
        DockerInDockerSupport::BestEffort
    } else if cfg!(unix) {
        DockerInDockerSupport::Supported
    } else {
        DockerInDockerSupport::Unsupported
    };
    assert_eq!(platform_support(), expected);
}
