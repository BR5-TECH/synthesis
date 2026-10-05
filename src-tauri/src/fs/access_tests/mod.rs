//! Scenario coverage for the `FsAccess` instance —
//! `specifications/core/FSA-filesystem-access.md` FSA-FR-10,
//! FSA-FR-14, FSA-FR-17 and FSA-FR-18 through FSA-FR-28.
//!
//! Every operation is exercised on both sides: the positive path that proves it
//! does its job, and the refusals that prove the gate is in front of it. The
//! refusal assertions consistently check *two* things — that the typed error
//! came back, and that the filesystem is byte-for-byte what it was — because an
//! error return that had already written something would satisfy the first
//! alone while breaking the containment the whole module exists for.

use super::access::*;
use super::{FsAccessBuildError, FsError};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::symlink;

/// A canonicalised temp root plus an instance over it — the shape almost every
/// test below wants.
fn rooted() -> (TempDir, FsAccess, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let access = FsAccess::builder().allow_root(&root).build().unwrap();
    (tmp, access, root)
}

fn write_raw(path: &Path, content: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, content).unwrap();
}

mod bounded;
mod construction;
mod hardening;
mod mechanics;
mod moving;
mod operations;
mod profiles;
mod readonly;
mod structural;
