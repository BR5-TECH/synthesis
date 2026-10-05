//! RFT-FR-03, RFT-FR-04, RFT-FR-05: project-relative resolution and the two bounds.

use super::*;

// ---------------------------------------------------------------------------
// RFT-FR-03 — project-relative resolution (RFT-FR-03, RFT-FR-04)
// ---------------------------------------------------------------------------

#[test]
fn a_project_relative_path_is_read_however_it_is_spelled() {
    let project = project();

    assert_eq!(project.at("src/a.ts").unwrap(), "export const a = 1;\n");
    assert_eq!(
        project.at("src/./nested/../a.ts").unwrap(),
        "export const a = 1;\n",
        "a path that climbs out and back in is judged by where it ends \
         (RFT-FR-04)",
    );
    assert_eq!(
        project.at("  src/a.ts  ").unwrap(),
        "export const a = 1;\n",
        "a padded path means the path (TLC-FR-07)",
    );
}

// ---------------------------------------------------------------------------
// RFT-FR-04 — the two bounds (RFT-FR-04, RFT-FR-05)
// ---------------------------------------------------------------------------

#[test]
fn the_sessions_own_temp_directory_is_outside_what_this_tool_reads() {
    let project = project();
    // A real, readable file inside the instance's *other* allowlisted root.
    std::fs::write(project.temp.join("scratch.txt"), "session scratch").unwrap();

    // Reached the only way a model could: by climbing out of the project.
    let climb = pathdiff_climb(&project.root, &project.temp.join("scratch.txt"));
    let refusal = project.at(&climb).expect_err(
        "the session temp directory is allowlisted for the instance and still \
         out of bounds for this tool (RFT-FR-04)",
    );
    assert_eq!(refusal, ToolRefusal::PathOutsideProject);
    assert_eq!(refusal.kind(), ToolErrorKind::PermissionDenied);
    assert!(refusal.retryable());
    assert_eq!(refusal.to_string(), crate::tools::PATH_OUTSIDE_PROJECT);

    // And the file is still there, unread by this tool.
    assert_eq!(
        std::fs::read_to_string(project.temp.join("scratch.txt")).unwrap(),
        "session scratch",
    );
}

#[test]
fn the_agent_instance_holds_no_root_an_app_data_path_could_land_in() {
    let project = project();
    // FSA-FR-29: the profile's whole point. Nothing in the allowlist is
    // `app_data_dir()`, so a path aimed there cannot resolve into any root.
    let app_data = crate::fs::app_data_dir().expect("resolvable");
    assert!(
        project.access.roots().iter().all(|root| !app_data.starts_with(root)),
        "an agent instance must not reach app_data_dir (FSA-FR-29)",
    );

    let climb = pathdiff_climb(&project.root, &app_data.join("synthesis.toml"));
    let refusal = project
        .at(&climb)
        .expect_err("app_data is outside the project (RFT-FR-04)");
    assert_eq!(refusal, ToolRefusal::PathOutsideProject);
}

/// A `../`-laden project-relative spelling of `target`, which is the only way a
/// model could aim outside the root at all.
fn pathdiff_climb(root: &std::path::Path, target: &std::path::Path) -> String {
    let ups = "../".repeat(root.components().count());
    format!("{ups}{}", target.to_string_lossy().trim_start_matches('/'))
}

#[test]
fn the_read_goes_through_the_filesystem_helper() {
    // RFT-FR-05 in the source: every read is a method on the instance this tool
    // was constructed with, and there is no direct filesystem call to bypass it.
    const SOURCE: &str = include_str!("../../file_read.rs");
    // The suite lives in its own file, so SOURCE is the module and nothing else.
    let body = SOURCE;
    for forbidden in [
        "std::fs::",
        "fs::read_to_string",
        "File::open",
        "read_dir",
    ] {
        assert!(
            !body.contains(forbidden),
            "`{forbidden}` bypasses the helper that owns containment (RFT-FR-05)",
        );
    }
    assert!(
        body.contains("access.read_text"),
        "the read is the instance's own (RFT-FR-05)",
    );
    assert_eq!(
        body.matches("access.read_text").count(),
        1,
        "exactly one read of the file (RFT-FR-05, and the spec's I/O posture)",
    );
}

