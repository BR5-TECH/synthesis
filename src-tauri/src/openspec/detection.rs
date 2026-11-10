//! Whether the open project uses OpenSpec (`OSM-openspec-mode.md`
//! OSM-FR-LLRV, OSM-FR-QNFI, OSM-FR-JGYD).
//!
//! Pure over the project's own `RootFs`: no program runs, and nothing is kept
//! between two calls.

use crate::fs::{EntryKind, RootFs};

/// The config file whose presence alone marks an OpenSpec project.
pub const CONFIG_FILE: &str = "openspec/config.yaml";

/// The folder of the live capability specs.
pub const SPECS_DIR: &str = "openspec/specs";

/// The file name of one capability spec.
pub const SPEC_FILE: &str = "spec.md";

/// OSM-FR-LLRV, OSM-FR-QNFI: `openspec/config.yaml` is a regular file, or a
/// regular `spec.md` stands somewhere below `openspec/specs/`. The walk stops
/// at the first match and follows no symbolic link.
pub fn uses_openspec(root: &RootFs) -> bool {
    if is_regular_file(root, CONFIG_FILE) {
        return true;
    }
    let mut pending = vec![std::path::PathBuf::from(SPECS_DIR)];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = root.list_dir(root.path().join(&dir)) else {
            continue;
        };
        for entry in entries {
            match entry.kind {
                EntryKind::File if entry.name == SPEC_FILE => return true,
                EntryKind::Dir => pending.push(dir.join(&entry.name)),
                _ => {}
            }
        }
    }
    false
}

/// A regular file at `rel` below the project root, through the project's own
/// instance.
pub fn is_regular_file(root: &RootFs, rel: &str) -> bool {
    root.file_info(root.path().join(rel)).map(|info| info.kind == EntryKind::File).unwrap_or(false)
}
