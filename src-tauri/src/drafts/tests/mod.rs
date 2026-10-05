//! The tests of draft storage — `../../../specifications/ui/DRP-drafts-panel.md`
//! and `../../../specifications/ui/NAW-new-artifact.md`.
//!
//! This file holds what all of them share: the project fixture, the two views
//! of the hierarchy the assertions read, and the small helpers that make a
//! draft or look inside one. Each topic file beside it starts with
//! `use super::*;` and holds one subject.

use super::*;
use tempfile::TempDir;

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    dir
}

/// The drafts half of the hierarchy — what almost every assertion below is
/// about. The folders half has its own tests further down.
fn listed(root: &crate::fs::RootFs) -> Vec<DraftSummary> {
    list_drafts_impl(root).drafts
}

/// The folders half, by path.
fn folder_paths(root: &crate::fs::RootFs) -> Vec<String> {
    list_drafts_impl(root).folders.into_iter().map(|f| f.path).collect()
}

/// Write a record back where the draft already sits — what a test that has
/// hand-edited one wants. Production code always holds the directory
/// already (see [`write_record_at`]'s note), so only the tests need this.
fn write_record(root: &crate::fs::RootFs, record: &DraftRecord) -> Result<(), String> {
    let dir = draft_dir(root, &record.id)?;
    write_record_at(root, &dir, record)
}

/// Where a draft is filed right now.
fn folder_of(root: &crate::fs::RootFs, id: &str) -> String {
    listed(root)
        .into_iter()
        .find(|d| d.id == id)
        .map(|d| d.folder)
        .unwrap_or_else(|| panic!("draft {id} is not listed"))
}

/// DRS-FR-25: the file a draft named `d` — which is most of the drafts below
/// — is created holding.
const FIRST_FILE: &str = "d.md";

/// Create a draft and hand back its id — what almost every test below needs.
fn draft(root: &crate::fs::RootFs, name: &str) -> String {
    create_draft_at_root(root, Some(name)).unwrap().draft.id
}

/// The names in a draft's `files/`, sorted — what DRS-FR-11 is a claim about.
fn files_in(root: &crate::fs::RootFs, id: &str) -> Vec<String> {
    let dir = draft_dir(root, id).unwrap().join(FILES_DIR);
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Every file under a folder, as path → bytes, so a claim that nothing moved
/// is checked against the whole subtree rather than against one file.
fn tree_bytes(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.push((path, bytes));
            }
        }
    }
    out.sort();
    out
}

/// An archived draft. Archiving is a record change and nothing else, so this
/// exists to assert that every command answers the same for one (DRS-FR-10).
fn archived_draft(root: &crate::fs::RootFs, name: &str) -> String {
    let id = draft(root, name);
    set_draft_status_impl(root, &id, DraftStatus::Archived).unwrap();
    id
}

/// A husk holding `comments/<name>` with `content`, at drafts-root-relative
/// `at`. What a path composed from a draft's id leaves behind.
fn husk_with_log(root: &crate::fs::RootFs, at: &str, name: &str, content: &str) -> PathBuf {
    let husk = root.join(DRAFTS_REL).join(at);
    std::fs::create_dir_all(husk.join(PROPOSALS_DIR)).unwrap();
    std::fs::write(husk.join(PROPOSALS_DIR).join(name), content).unwrap();
    husk
}

/// A project holding one drafts folder `UI` and one draft filed in it.
fn project_with_filed_draft() -> (TempDir, String) {
    let dir = project();
    let id = {
        let root = &crate::fs::RootFs::for_root(dir.path());
        create_drafts_folder_impl(root, "", "UI").unwrap();
        create_draft_impl(root, Some("spec"), Some("UI")).unwrap().draft.id
    };
    (dir, id)
}

mod resolution;
mod registry;
mod creation;
mod status_rename;
mod prompt;
mod contents;
mod search;
mod deletion;
mod validation;
mod statistics_log;
mod template;
mod root_organisation;
mod storage;
mod prompt_activity;
mod github_shadow;
