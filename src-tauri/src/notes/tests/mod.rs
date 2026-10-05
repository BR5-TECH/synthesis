//! The unit tests of `specifications/core/NTC-notes-context.md`.
//!
//! This module holds the helpers that every topic file uses. Each topic file
//! below covers one part of the module.

use super::*;

fn temp_root() -> tempfile::TempDir {
    tempfile::TempDir::new().unwrap()
}

fn entity(id: &str) -> NoteScope {
    NoteScope::Entity {
        entity_id: id.into(),
        entity_path: String::new(),
    }
}

fn touch(root: &Path, rel: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, b"x").unwrap();
}

fn create(root: &crate::fs::RootFs, scope: NoteScope, body: &str, at: &str) -> Note {
    create_note_in(root, scope, body.into(), None, None, at).unwrap()
}


/// Open a discussion on a note through the comment store, which is where
/// the association lives (CMS-FR-62).
fn discuss(root: &crate::fs::RootFs, note_id: &str, body: &str) -> String {
    crate::comments::get_or_create_note_discussion_in(
        root,
        root,
        note_id,
        body.into(),
        Vec::new(),
        &crate::comments::Participant::Human {
            login: "raver119".into(),
            display_name: None,
            email: None,
        },
        "2025-03-04T11:00:00Z",
    )
    .unwrap()
    .0
    .id
}


mod body_bound;
mod channel;
mod create_and_update;
mod deletion;
mod discussions;
mod malformed;
mod records;
mod renames;
mod scopes;
mod timestamps;
mod wire_shape;
mod worktrees;
