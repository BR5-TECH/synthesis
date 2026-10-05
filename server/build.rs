//! Build script for the Synthesis backend microservice.
//!
//! Specification: `specifications/server/BMS-backend-microservice.md`.
//! Requirements: BMS-FR-14, BMS-FR-15, BMS-FR-16.
//!
//! It resolves the build version once, at compile time, and compiles it into
//! the binary as the `SYNTHESIS_SERVER_VERSION` environment value that
//! `src/version.rs` reads with `env!`. The running process therefore never
//! calls `git`, never reads a `.git` directory, and never reads an environment
//! variable to learn its version.

use std::path::{Path, PathBuf};

// The pure selection rule and the command probe, both shared with the crate so
// the crate's tests cover them (BMS-FR-26, BMS-FR-16).
include!("src/version_resolve.rs");
include!("src/build_probe.rs");

/// The build argument that carries the Git tag of the GitHub Release.
const BUILD_VERSION_ARGUMENT: &str = "SYNTHESIS_BUILD_VERSION";

fn main() {
    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("cargo always sets CARGO_MANIFEST_DIR"),
    );

    declare_rerun_conditions(&manifest_dir);

    let build_argument = std::env::var(BUILD_VERSION_ARGUMENT).ok();
    let exact_tag = capture(
        "git",
        &["describe", "--tags", "--exact-match"],
        &manifest_dir,
    );
    let short_hash = capture("git", &["rev-parse", "--short", "HEAD"], &manifest_dir);

    let version = resolve_version(
        build_argument.as_deref(),
        exact_tag.as_deref(),
        short_hash.as_deref(),
    );

    println!("cargo:rustc-env=SYNTHESIS_SERVER_VERSION={version}");
}

/// Tells cargo when to run this script again (BMS-FR-16).
///
/// A script that prints any rerun condition replaces cargo's own file scan, so
/// the sources of the crate are listed here as well. Without them, a changed
/// source file would not rebuild the crate.
fn declare_rerun_conditions(manifest_dir: &Path) {
    println!("cargo:rerun-if-env-changed={BUILD_VERSION_ARGUMENT}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=src");

    // A changed Git head must produce a new version rather than a stale one.
    if let Some(paths) = find_git_paths(manifest_dir) {
        for path in paths.watched_paths() {
            // Only a path that is there is watched. Cargo treats a path it
            // cannot read as a reason to run the script again, so a path that
            // is absent would rebuild the crate on every single build.
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
}

/// The two directories Git keeps a checkout's references in.
///
/// They are the same directory in a normal checkout. In a linked worktree the
/// head is private to the worktree while the branches and the tags are shared,
/// so the two are different and both must be watched.
struct GitPaths {
    /// The directory of this checkout, which holds its `HEAD` and its reflog.
    git_dir: PathBuf,
    /// The shared directory, which holds the branches, the tags, and the
    /// packed references.
    common_dir: PathBuf,
}

impl GitPaths {
    /// Every path whose change must produce a new version.
    fn watched_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![
            // Changes when the checkout moves to another commit with no branch,
            // which is how a continuous-integration runner checks out.
            self.git_dir.join("HEAD"),
            // Appended to by every operation that moves the head — a commit, a
            // checkout, a merge, a rebase. A commit on a branch changes neither
            // `HEAD`, which still names the branch, nor the tags, so this is the
            // path that catches the most common change of all.
            self.git_dir.join("logs/HEAD"),
            // A new tag adds a file here, which changes the directory itself.
            self.common_dir.join("refs/tags"),
            self.common_dir.join("packed-refs"),
        ];

        // The branch the head names, for a checkout that keeps no reflog.
        if let Some(reference) = self.head_reference() {
            paths.push(self.git_dir.join(&reference));
            paths.push(self.common_dir.join(&reference));
        }

        paths
    }

    /// The reference `HEAD` names, for example `refs/heads/main`.
    ///
    /// Returns `None` when the head is detached, because then `HEAD` holds the
    /// commit itself and is already watched.
    fn head_reference(&self) -> Option<String> {
        let head = std::fs::read_to_string(self.git_dir.join("HEAD")).ok()?;
        let reference = head.trim().strip_prefix("ref:")?.trim().to_string();

        // A reference that leaves the directory would be a malformed file.
        if reference.is_empty() || reference.contains("..") {
            return None;
        }

        Some(reference)
    }
}

/// Finds the Git directories of the checkout, if the build runs inside one.
///
/// The image build copies the crate without any Git metadata, so this returns
/// `None` there and the version falls to the build argument or to `undefined`.
fn find_git_paths(start: &Path) -> Option<GitPaths> {
    for directory in start.ancestors() {
        let candidate = directory.join(".git");

        // A normal checkout, where `.git` is the directory itself.
        if candidate.is_dir() {
            return Some(GitPaths {
                common_dir: common_dir_of(&candidate),
                git_dir: candidate,
            });
        }

        // A linked worktree or a submodule, where `.git` is a file that names
        // the directory.
        if candidate.is_file() {
            if let Some(git_dir) = read_git_dir_pointer(&candidate, directory) {
                return Some(GitPaths {
                    common_dir: common_dir_of(&git_dir),
                    git_dir,
                });
            }
        }
    }

    None
}

/// Reads the directory a `.git` file points at.
fn read_git_dir_pointer(pointer: &Path, base: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(pointer).ok()?;
    let target = text.trim().strip_prefix("gitdir:")?.trim();
    if target.is_empty() {
        return None;
    }

    let path = PathBuf::from(target);
    Some(if path.is_absolute() {
        path
    } else {
        base.join(path)
    })
}

/// Reads the shared directory a worktree's directory points at.
///
/// A checkout that is not a worktree holds no `commondir` file, and its shared
/// directory is its own.
fn common_dir_of(git_dir: &Path) -> PathBuf {
    let Ok(text) = std::fs::read_to_string(git_dir.join("commondir")) else {
        return git_dir.to_path_buf();
    };

    let target = text.trim();
    if target.is_empty() {
        return git_dir.to_path_buf();
    }

    let path = PathBuf::from(target);
    if path.is_absolute() {
        path
    } else {
        git_dir.join(path)
    }
}
