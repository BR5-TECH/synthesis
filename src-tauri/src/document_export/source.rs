//! The file an export reads: a DOC artifact of the active worktree
//! (DXP-FR-LNCQ), named by its path or by its Documents id (DXP-FR-PKDB).

use std::path::Path;

use tauri::Manager;

use super::ERR_OUTSIDE_PROJECT;
use super::{ERR_NOT_DOC_ARTIFACT, ERR_NOT_FOUND, ERR_NOT_MARKDOWN, ERR_NO_PROJECT_OPEN};
use crate::documents::{DocumentsCollection, DocumentsSnapshot};
use crate::fs::{self, EntryKind, RootFs};
use crate::scanning::ArtifactType;

/// DXP-FR-LNCQ: check that `path` names a DOC artifact of `root`, and return
/// it as a normalised project-relative path with `/` separators.
pub fn check_doc_artifact(root: &RootFs, path: &str) -> Result<String, &'static str> {
    let rel = normalise(path).ok_or(ERR_OUTSIDE_PROJECT)?;
    let abs = fs::resolve_under(root.path(), &rel).map_err(|_| ERR_OUTSIDE_PROJECT)?;
    match root.access().file_info(&abs) {
        Ok(info) if info.kind == EntryKind::File => {}
        Ok(_) => return Err(ERR_NOT_FOUND),
        Err(fs::FsError::NotFound { .. }) => return Err(ERR_NOT_FOUND),
        Err(_) => return Err(ERR_OUTSIDE_PROJECT),
    }
    let markdown = Path::new(&rel)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md"));
    if !markdown {
        return Err(ERR_NOT_MARKDOWN);
    }
    if crate::artifacts::resolved_type(root, &rel) != Some(ArtifactType::Doc) {
        return Err(ERR_NOT_DOC_ARTIFACT);
    }
    Ok(rel)
}

/// A project-relative path with `/` separators and no `.` or `..` segment that
/// leaves the root, or `None` for a path that is absolute or leaves the root.
fn normalise(path: &str) -> Option<String> {
    let path = path.trim().replace('\\', "/");
    if path.is_empty() || path.starts_with('/') || Path::new(&path).is_absolute() {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// DXP-FR-PKDB: resolve a path or a Documents id against `root` and the
/// Documents snapshot, then apply the checks of DXP-FR-LNCQ.
pub fn resolve_identifier(
    root: &RootFs,
    identifier: &str,
    documents: Option<&DocumentsSnapshot>,
) -> Result<String, &'static str> {
    let identifier = identifier.trim();
    if is_document_id(identifier) {
        let entry = documents
            .and_then(|s| s.documents.iter().find(|d| d.id == identifier))
            .ok_or(ERR_NOT_FOUND)?;
        let project_path = entry.project_path.as_deref().ok_or(ERR_OUTSIDE_PROJECT)?;
        return check_doc_artifact(root, project_path);
    }
    check_doc_artifact(root, identifier)
}

/// DCL-FR-QKJO: `doc-` and 32 lowercase hexadecimal characters.
fn is_document_id(text: &str) -> bool {
    text.strip_prefix("doc-").is_some_and(|hex| {
        hex.len() == 32 && hex.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    })
}

/// DXP-FR-PKDB: the project-relative path of the DOC artifact `identifier`
/// names, or one of the typed errors of `super`.
pub fn resolve_export_document<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    identifier: &str,
) -> Result<String, &'static str> {
    let root = app
        .try_state::<crate::project::ProjectState>()
        .and_then(|p| p.require_root().ok())
        .ok_or(ERR_NO_PROJECT_OPEN)?;
    let snapshot = app
        .try_state::<DocumentsCollection>()
        .filter(|c| c.is_open())
        .and_then(|c| c.snapshot().ok());
    resolve_identifier(&root, identifier, snapshot.as_ref())
}
