//! `specifications/core/DAS-draft-assets.md` — DAS-FR-01, DAS-FR-02, DAS-FR-05, DAS-FR-26 … DAS-FR-27.
//!
//! Everything here runs against a real temporary project through a real
//! [`crate::fs::RootFs`], because the requirements this module carries are
//! almost all statements about **what is on disk afterwards**: a refused store
//! leaves the folder byte-for-byte as it was, a sweep deletes a file only where
//! five conditions hold together, and a failure leaves the asset exactly as it
//! was. A test against a mocked filesystem could not see any of that.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use tempfile::TempDir;

use super::*;
use crate::drafts;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    dir
}

/// A draft holding one prompt, returning its id.
fn draft(root: &crate::fs::RootFs, name: &str) -> String {
    drafts::create_draft_impl(root, Some(name), None).unwrap().draft.id
}

fn assets_of(root: &crate::fs::RootFs, id: &str) -> PathBuf {
    drafts::draft_assets_dir(root, id).unwrap()
}

fn save_prompt(root: &crate::fs::RootFs, id: &str, body: &str) {
    let prompt = drafts::require_prompt(root, id).unwrap();
    drafts::save_draft_file_impl(root, id, &prompt, body).unwrap();
}

/// The smallest byte sequence that is a real PNG as far as [`bytes_are`] is
/// concerned, made distinguishable by a trailing tag so two assets can be told
/// apart by their content.
fn png(tag: &str) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(tag.as_bytes());
    bytes
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// A hold callback that records nothing — for the stores whose hold is not what
/// is being asserted.
fn no_hold() -> impl Fn(&str) {
    |_: &str| {}
}

/// Store one PNG and return the asset it produced.
fn store_png(root: &crate::fs::RootFs, id: &str, filename: &str, tag: &str) -> DraftAsset {
    store_image_impl(root, id, "image/png", Some(filename), &b64(&png(tag)), &no_hold()).unwrap()
}

/// Put a file directly under a draft's `assets/`, bypassing the store — for the
/// cases a store would refuse and a sweep still has to decide about.
fn place(root: &crate::fs::RootFs, id: &str, name: &str, bytes: &[u8]) {
    std::fs::write(assets_of(root, id).join(name), bytes).unwrap();
}

/// DAS-FR-27: whether an error is this typed value, with or without the safe
/// diagnostic category that follows it.
///
/// Every caller matches the value the same way — by its leading token — so this
/// is the shape a surface reads too (per `../ui/NAW-new-artifact.md`
/// NAW-FR-51), and the category is asserted separately where it is the point.
fn is_typed(error: &str, value: &str) -> bool {
    error == value || error.starts_with(&format!("{value}:"))
}

/// The safe diagnostic category an error carries, if it carries one.
fn category_of(error: &str) -> Option<&str> {
    error.split_once(':').map(|(_, category)| category)
}

/// Everything under a draft's `assets/`, the filename record included.
fn everything_in_assets(root: &crate::fs::RootFs, id: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(assets_of(root, id))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// The **assets** under a draft's `assets/`.
///
/// The filename record of DAS-FR-02 is a file of the folder's own rather than
/// an asset, so it is excluded here and asserted about on its own terms — a
/// sweep retains it (DAS-FR-12), and every count below is a count of pictures.
fn asset_names(root: &crate::fs::RootFs, id: &str) -> Vec<String> {
    everything_in_assets(root, id)
        .into_iter()
        .filter(|name| name != FILENAMES_FILE)
        .collect()
}

/// Every file under `dir`, with its bytes — for asserting that nothing at all
/// changed.
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

fn never_cancelled() -> AtomicBool {
    AtomicBool::new(false)
}

/// One pass over one draft, with nothing held and nothing locked.
fn sweep(root: &crate::fs::RootFs, id: &str) -> DraftAssetSweep {
    sweep_draft_impl(root, id, false, &|_: &str| false, &never_cancelled())
}

/// One pass with a set of held names (DAS-FR-14).
fn sweep_holding(root: &crate::fs::RootFs, id: &str, held: &[&str]) -> DraftAssetSweep {
    let held: HashSet<String> = held.iter().map(|n| n.to_string()).collect();
    sweep_draft_impl(root, id, false, &|name: &str| held.contains(name), &never_cancelled())
}

mod github_shadow;
mod holds;
mod internals;
mod reads;
mod references;
mod storing;
