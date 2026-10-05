//! The test scenarios of `specifications/core/PSS-project-settings-storage.md`.
//!
//! Each test runs against a temporary project root, so no test reads or writes
//! the developer's own `.synthesis` directory. This module holds the fixtures
//! that build the persisted records; the topic modules hold the tests.

use super::*;
use tempfile::TempDir;

fn branch_state(target: &str) -> ChangesPanelState {
    ChangesPanelState {
        mode: ChangesMode::Branch,
        target_branch: Some(target.to_string()),
    }
}

fn library_state(
    expanded: &[&str],
    filter: ArtifactTypeFilter,
    text: &str,
) -> LibraryPanelState {
    LibraryPanelState {
        expanded_paths: expanded.iter().map(|s| s.to_string()).collect(),
        artifact_type_filter: filter,
        text_filter: text.to_string(),
    }
}

fn notes_state(position: NotesScopePosition, text: &str) -> NotesPanelState {
    NotesPanelState {
        scope_position: position,
        text_filter: text.to_string(),
    }
}

fn drafts_state(
    status: DraftsStatusFilter,
    text: &str,
    expanded: &[&str],
) -> DraftsPanelState {
    DraftsPanelState {
        status_filter: status,
        text_filter: text.to_string(),
        expanded_folders: expanded.iter().map(|s| s.to_string()).collect(),
    }
}

/// A `local.toml` as an earlier layout of this store left it: a
/// recently-edited MRU beside a real, still-supported section.
fn local_with_legacy_key(dir: &TempDir) -> crate::fs::RootFs {
    let root = crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(
        local_toml_path(&root),
        "recently_edited = [\"a.md\", \"b.md\", \"c.md\"]\n\
         \n\
         [notes_panel]\n\
         scopePosition = \"all\"\n\
         textFilter = \"handoff\"\n",
    )
    .unwrap();
    root
}

fn docker_root() -> (tempfile::TempDir, crate::fs::RootFs) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("docker")).unwrap();
    std::fs::write(dir.path().join("docker/agent.Dockerfile"), "FROM scratch\n").unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    (dir, root)
}

mod changes_panel;
mod line_endings;
mod draft_template;
mod library_panel;
mod notes_panel;
mod drafts_panel;
mod legacy_keys;
mod docker_images;
mod shared_loop_settings;
mod graduation_concurrency;
mod github_polling;
mod publication_settings;
