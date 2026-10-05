//! Construction of an `FsAccess` instance (FSA-FR-18) and the session
//! scratch directory an instance owns (FSA-FR-20).

use std::io;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::access::FsAccess;
use super::error::FsAccessBuildError;

// ---------------------------------------------------------------------------
// FSA-FR-18: construction
// ---------------------------------------------------------------------------

/// Builder for [`FsAccess`] (FSA-FR-18). The defaults are the safe ones — links
/// refused, no temporary directory — so every permissive setting is one a
/// caller had to name.
#[derive(Debug, Default, Clone)]
pub struct FsAccessBuilder {
    pub(super) roots: Vec<PathBuf>,
    /// FSA-FR-ZUCF: the subset of `roots` the application itself owns.
    pub(super) owned: Vec<PathBuf>,
    /// FSA-FR-OWVT: the subset of `roots` an author's tree may be removed from.
    pub(super) author: Vec<PathBuf>,
    /// FSA-FR-KAQV: files granted read-only (the documents profile).
    pub(super) read_only_files: Vec<PathBuf>,
    /// FSA-FR-KAQV: folders granted read-only, with everything below them.
    pub(super) read_only_folders: Vec<PathBuf>,
    pub(super) follow_symlinks: bool,
    pub(super) session_temp: bool,
}

impl FsAccessBuilder {
    /// Add a directory this instance may reach. Repeatable; at least one is
    /// required. Order carries no meaning — containment is decided against the
    /// whole set (FSA-FR-10).
    pub fn allow_root(mut self, path: impl Into<PathBuf>) -> Self {
        self.roots.push(path.into());
        self
    }

    /// Whether symbolic links are resolved (with per-hop validation, FSA-FR-27)
    /// or refused (FSA-FR-17). Defaults to `false`.
    pub fn follow_symlinks(mut self, follow: bool) -> Self {
        self.follow_symlinks = follow;
        self
    }

    /// Whether to create and allowlist a session scratch directory this
    /// instance owns for its whole life (FSA-FR-20). Defaults to `false`.
    pub fn session_temp(mut self, enabled: bool) -> Self {
        self.session_temp = enabled;
        self
    }

    /// Validate and build.
    ///
    /// Every root is canonicalised here — the one symbolic-link resolution that
    /// happens whatever the instance's policy, because it establishes the
    /// root's own identity rather than traversing to a target. Without it a
    /// root reached through a link (`/tmp` → `/private/tmp` on macOS) would
    /// contain none of the paths later resolved beneath it.
    pub fn build(self) -> Result<FsAccess, FsAccessBuildError> {
        // FSA-FR-UKAE: a read-only grant selects the documents profile, which is
        // built apart from the read-write one and never mixed with it.
        if !self.read_only_files.is_empty() || !self.read_only_folders.is_empty() {
            return super::access_readonly::build_read_only(self);
        }
        if self.roots.is_empty() {
            return Err(FsAccessBuildError::NoRoots);
        }
        let mut roots = Vec::with_capacity(self.roots.len() + 1);
        for raw in &self.roots {
            let canonical = canonicalize_root(raw)?;
            if !canonical.is_dir() {
                return Err(FsAccessBuildError::RootNotADirectory { path: raw.clone() });
            }
            // Both spellings of the same directory are allowlisted when they
            // differ, and that is not a loosening: they denote one directory,
            // and the canonical form alone would refuse every path a caller
            // composes from the spelling it was given. `/tmp/session/a.md` on
            // macOS is under `/private/tmp` in canonical form and under `/tmp`
            // in the form the caller holds; refusing the latter would reject
            // ordinary correct paths while closing no hole, since the two
            // resolve to the same bytes. The symlink policy is unaffected: it
            // walks only the components *below* whichever root matched, and a
            // root's own links were resolved at build for identity.
            if raw.is_absolute() && *raw != canonical {
                roots.push(raw.clone());
            }
            roots.push(canonical);
        }

        let owned_roots = super::owned_tree::canonicalize_owned(&self.owned)?;
        let author_roots = super::owned_tree::canonicalize_owned(&self.author)?;

        let session_temp = if self.session_temp {
            Some(SessionTemp::create()?)
        } else {
            None
        };
        if let Some(t) = &session_temp {
            roots.push(t.path.clone());
        }

        Ok(FsAccess {
            roots,
            owned_roots,
            author_roots,
            follow_symlinks: self.follow_symlinks,
            session_temp,
            read_only: false,
            file_grants: Vec::new(),
        })
    }
}

/// FSA-FR-18: establish a root's own identity.
///
/// The one symbolic-link resolution that happens whatever the instance's policy,
/// because it names the root rather than traversing to a target. A path that
/// does not exist and one that cannot be resolved for another reason are both
/// "we cannot establish what this root means", but the first is worth naming on
/// its own since it is overwhelmingly the common mistake.
pub(super) fn canonicalize_root(raw: &Path) -> Result<PathBuf, FsAccessBuildError> {
    fs::canonicalize(raw).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            FsAccessBuildError::RootNotADirectory {
                path: raw.to_path_buf(),
            }
        } else {
            FsAccessBuildError::RootNotCanonical {
                path: raw.to_path_buf(),
                source,
            }
        }
    })
}

/// A scratch directory an instance owns outright (FSA-FR-20): empty when the
/// instance is built, removed with everything under it when the instance is
/// dropped. Allowlisted for the instance that made it and for no other.
#[derive(Debug)]
pub(super) struct SessionTemp {
    pub(super) path: PathBuf,
}

impl SessionTemp {
    pub(super) fn create() -> Result<SessionTemp, FsAccessBuildError> {
        // Uniqueness comes from pid + a monotonic per-process counter +
        // nanoseconds. `create_dir` (not `create_dir_all`) so a collision is an
        // error rather than silent adoption of somebody else's directory.
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let base = std::env::temp_dir();
        for _ in 0..16 {
            let nanos = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let candidate = base.join(format!(
                "synthesis-session-{}-{}-{}",
                std::process::id(),
                n,
                nanos
            ));
            match fs::create_dir(&candidate) {
                Ok(()) => {
                    // Canonicalise for the same reason the roots are: on macOS
                    // `std::env::temp_dir()` is under `/var`, itself a link to
                    // `/private/var`, so the un-canonicalised path would contain
                    // nothing that later resolves beneath it.
                    let path = fs::canonicalize(&candidate)
                        .map_err(FsAccessBuildError::SessionTemp)?;
                    return Ok(SessionTemp { path });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(FsAccessBuildError::SessionTemp(e)),
            }
        }
        Err(FsAccessBuildError::SessionTemp(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not find an unused session temp directory name",
        )))
    }
}

impl Drop for SessionTemp {
    fn drop(&mut self) {
        // Best-effort: a session that cannot clean up must not panic on the way
        // out, and there is nothing a caller could do about it either.
        let _ = fs::remove_dir_all(&self.path);
    }
}
