//! The Docker image configuration (PSS-FR-22 to PSS-FR-30).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// Docker image configuration (PSS-FR-22..PSS-FR-30)
// -----------------------------------------------------------------------

/// PSS-FR-23 (PSS-FR-22): three statuses whether or not anything is
/// configured, and a saved entry lands in the committed `project.toml`.
#[test]
fn every_vendor_has_a_status_and_a_saved_entry_is_committed() {
    let (dir, root) = docker_root();

    let statuses = load_project_docker_images_from(&root).unwrap();
    assert_eq!(
        statuses.iter().map(|s| s.vendor.as_str()).collect::<Vec<_>>(),
        vec!["claude_code", "codex", "opencode"]
    );
    assert!(statuses
        .iter()
        .all(|s| s.configuration == images::VendorImageConfiguration::Unset));

    save_project_vendor_image_to(
        &root,
        "claude_code",
        &images::ProjectVendorImage {
            image_name: "acme/agent".into(),
            tag: Some("2.1".into()),
            dockerfile: None,
        },
    )
    .unwrap();

    let statuses = load_project_docker_images_from(&root).unwrap();
    let claude = &statuses[0];
    assert_eq!(claude.configuration, images::VendorImageConfiguration::Configured);
    assert_eq!(claude.image_reference.as_deref(), Some("acme/agent:2.1"));
    assert_eq!(statuses[1].configuration, images::VendorImageConfiguration::Unset);

    // In `project.toml` (committed), never in `local.toml` (per-machine).
    let public = std::fs::read_to_string(dir.path().join(".synthesis/project.toml")).unwrap();
    assert!(public.contains("acme/agent"), "{public}");
    assert!(!dir.path().join(".synthesis/local.toml").exists());
}

/// PSS-FR-23 / PSS-FR-25 (PSS-FR-23, PSS-FR-24): a refused save writes
/// nothing and leaves the entry as it was.
#[test]
fn a_refused_docker_entry_leaves_the_store_untouched() {
    let (dir, root) = docker_root();
    save_project_vendor_image_to(
        &root,
        "codex",
        &images::ProjectVendorImage {
            image_name: "acme/agent".into(),
            tag: None,
            dockerfile: Some("docker/agent.Dockerfile".into()),
        },
    )
    .unwrap();
    let before = std::fs::read(dir.path().join(".synthesis/project.toml")).unwrap();

    for (dockerfile, expected) in [
        ("/etc/Dockerfile", images::ERR_DOCKERFILE_ABSOLUTE),
        (
            "../outside/Dockerfile",
            images::ERR_DOCKERFILE_ESCAPES_PROJECT_ROOT,
        ),
        ("docker", images::ERR_DOCKERFILE_NOT_A_REGULAR_FILE),
    ] {
        assert_eq!(
            save_project_vendor_image_to(
                &root,
                "codex",
                &images::ProjectVendorImage {
                    image_name: "acme/agent".into(),
                    tag: None,
                    dockerfile: Some(dockerfile.into()),
                },
            )
            .unwrap_err(),
            expected
        );
    }
    assert_eq!(
        save_project_vendor_image_to(
            &root,
            "codex",
            &images::ProjectVendorImage {
                image_name: "   ".into(),
                tag: None,
                dockerfile: None,
            },
        )
        .unwrap_err(),
        images::ERR_IMAGE_NAME_EMPTY
    );

    assert_eq!(
        std::fs::read(dir.path().join(".synthesis/project.toml")).unwrap(),
        before,
        "a refused save is byte-for-byte no write at all"
    );
}

/// PSS-FR-24 (PSS-FR-25): a Dockerfile deleted after it was configured
/// reads `invalid` rather than being removed from the entry.
#[test]
fn a_deleted_dockerfile_is_reported_rather_than_rewritten() {
    let (dir, root) = docker_root();
    save_project_vendor_image_to(
        &root,
        "codex",
        &images::ProjectVendorImage {
            image_name: "acme/agent".into(),
            tag: None,
            dockerfile: Some("docker/agent.Dockerfile".into()),
        },
    )
    .unwrap();
    assert_eq!(
        load_project_docker_images_from(&root).unwrap()[1].dockerfile_state,
        images::DockerfileState::Valid
    );

    std::fs::remove_file(dir.path().join("docker/agent.Dockerfile")).unwrap();
    let status = &load_project_docker_images_from(&root).unwrap()[1];
    assert_eq!(status.dockerfile_state, images::DockerfileState::Invalid);
    assert_eq!(
        status.dockerfile_problem,
        Some(images::DockerfileProblem::NotARegularFile)
    );
    assert_eq!(status.dockerfile.as_deref(), Some("docker/agent.Dockerfile"));
    assert_eq!(
        status.graduation_state,
        images::VendorGraduationState::DockerfileInvalid
    );
}

/// PSS-FR-23 (PSS-FR-22, PSS-FR-17): an image entry and every other
/// project-public section survive each other's writes.
#[test]
fn an_image_entry_and_the_rest_of_the_store_carry_each_other_through() {
    let (_dir, root) = docker_root();
    save_project_config_to(
        &root,
        ProjectConfig {
            line_endings: LineEndings::Crlf,
            draft_template: Some("# Context\n".to_string()),
                        ..Default::default()
        },
    )
    .unwrap();
    save_project_vendor_image_to(
        &root,
        "claude_code",
        &images::ProjectVendorImage {
            image_name: "acme/agent".into(),
            tag: None,
            dockerfile: None,
        },
    )
    .unwrap();

    let config = load_project_config_from(&root).unwrap();
    assert_eq!(config.line_endings, LineEndings::Crlf);
    assert_eq!(config.draft_template.as_deref(), Some("# Context\n"));

    // And the other direction: persisting the convention carries the image
    // entry through unchanged.
    save_project_config_to(
        &root,
        ProjectConfig {
            line_endings: LineEndings::Lf,
            draft_template: None,
                        ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        load_project_docker_images_from(&root).unwrap()[0]
            .image_reference
            .as_deref(),
        Some("acme/agent")
    );
}

/// PSS-FR-30 (PSS-FR-30): the single read path an execution takes, and its
/// refusal.
#[test]
fn an_execution_resolves_only_what_the_project_commits() {
    let (_dir, root) = docker_root();
    assert_eq!(
        resolve_project_vendor_image(&root, "claude_code").unwrap_err(),
        images::ERR_VENDOR_IMAGE_UNCONFIGURED
    );
    save_project_vendor_image_to(
        &root,
        "claude_code",
        &images::ProjectVendorImage {
            image_name: "acme/agent".into(),
            tag: Some("2.1".into()),
            dockerfile: None,
        },
    )
    .unwrap();
    assert_eq!(
        resolve_project_vendor_image(&root, "claude_code").unwrap(),
        "acme/agent:2.1"
    );
    // No fallback to any shipped image: another vendor still resolves
    // nothing.
    assert_eq!(
        resolve_project_vendor_image(&root, "codex").unwrap_err(),
        images::ERR_VENDOR_IMAGE_UNCONFIGURED
    );
}

/// PSS-FR-22 (PSS-FR-22, PSS-FR-16, PSS-FR-10): the entries resolve against
/// the active worktree, and a damaged store is a typed error rather than
/// three unset entries.
#[test]
fn image_entries_resolve_per_worktree_and_a_damaged_store_is_typed() {
    let (_a_dir, a) = docker_root();
    let (b_dir, b) = docker_root();
    save_project_vendor_image_to(
        &a,
        "codex",
        &images::ProjectVendorImage {
            image_name: "acme/agent".into(),
            tag: Some("2.1".into()),
            dockerfile: None,
        },
    )
    .unwrap();
    assert_eq!(
        load_project_docker_images_from(&b).unwrap()[1].configuration,
        images::VendorImageConfiguration::Unset,
        "nothing was copied from the other worktree"
    );

    std::fs::create_dir_all(b_dir.path().join(".synthesis")).unwrap();
    std::fs::write(b_dir.path().join(".synthesis/project.toml"), "not = = toml").unwrap();
    assert_eq!(
        load_project_docker_images_from(&b).unwrap_err(),
        ERR_MALFORMED_PROJECT_CONFIG
    );
    assert_eq!(
        resolve_project_vendor_image(&b, "codex").unwrap_err(),
        ERR_MALFORMED_PROJECT_CONFIG
    );
}

/// PSS-FR-23 (PSS-FR-22): a vendor id this build does not know is refused
/// rather than written into the store, so the three entries the contract
/// names are the only ones that reach `project.toml`.
#[test]
fn an_unknown_vendor_is_refused() {
    let (dir, root) = docker_root();
    assert_eq!(
        save_project_vendor_image_to(
            &root,
            "gemini",
            &images::ProjectVendorImage {
                image_name: "acme/agent".into(),
                tag: None,
                dockerfile: None,
            },
        )
        .unwrap_err(),
        images::ERR_UNKNOWN_VENDOR
    );
    assert!(!dir.path().join(".synthesis/project.toml").exists());
}
