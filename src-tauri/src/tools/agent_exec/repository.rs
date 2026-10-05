//! Read-only repository access for one execution (EAC-FR-FNFV, EAC-FR-40,
//! EAC-FR-41).
//!
//! Split out of `agent_exec.rs` for size alone. The items keep the visibility
//! they had, widened to `pub(super)` only where the module root or a sibling
//! part calls them.

use std::path::{Path, PathBuf};

use super::*;

// ---------------------------------------------------------------------------
// The semantic-rebase capability (EAC-FR-40, EAC-FR-41)
// ---------------------------------------------------------------------------

/// EAC-FR-41: where the empty mask directory sits under the executor's own root.
/// EAC-FR-FNFV: the fixed container path a run's shared Git store is mounted at.
///
/// This module's own, and never a caller's to name. Used for a **linked
/// worktree** alone: an ordinary checkout keeps its Git directory inside the
/// working tree, so there is nothing to mount somewhere else.
pub(super) const GIT_COMMON_DIR_TARGET: &str = "/gitcommon";

/// EAC-FR-FNFV: the entry of a repository that stays masked even where the rest
/// of it is readable.
///
/// `config` holds a project's remote URLs, and a remote URL routinely carries a
/// credential inside it (`https://<token>@host/...`). Mounting a store whole
/// without covering this would hand every ordinary turn the project's stored
/// credentials, which is a disclosure rather than a convenience.
///
/// One entry rather than a list, and it is named rather than tested for: every
/// Git store holds a `config`, so the mask always has something to cover and
/// nothing here has to ask the disk whether it does.
pub(super) const GIT_MASKED_ENTRY: &str = "config";

/// EAC-FR-FNFV: the entry inside a linked worktree's Git directory that records
/// the store that Git directory shares.
///
/// Git reads it before it reads anything else about the repository, so what it
/// says decides whether a turn's first Git command answers or fails.
pub(super) const GIT_COMMON_DIR_RECORD: &str = "commondir";

/// EAC-FR-FNFV: what one execution's read-only repository access consists of.
///
/// The two shapes differ in what has to be mounted, and in nothing else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RepositoryAccess {
    /// An ordinary checkout: `.git` is a directory inside the working tree, and
    /// it is its own store. Pinning it read-only over itself is the whole of
    /// the mount — Git finds it where it always was.
    Plain { git_dir: PathBuf },
    /// A linked worktree — what every graduation run in `git` mode is given.
    /// Its `.git` is a *file* naming a Git directory outside the working tree,
    /// and that directory lives inside the store it shares, so mounting the
    /// store brings both.
    Linked {
        common_dir: PathBuf,
        /// Where this worktree's own Git directory sits inside that store,
        /// which is what the generated pointer names.
        git_dir_within: PathBuf,
    },
}

impl RepositoryAccess {
    /// Whether this is the linked shape. The one fact worth a log field.
    pub(super) fn linked(&self) -> bool {
        matches!(self, Self::Linked { .. })
    }

    /// EAC-FR-FNFV: what a linked worktree's `.git` is replaced by.
    ///
    /// The application generates this rather than mounting the worktree's own,
    /// because the real file names a **host** path that means nothing inside a
    /// container. Naming the container path instead is what lets Git resolve the
    /// working tree the ordinary way — by reading `.git` where it stands — and
    /// that is what keeps `GIT_DIR` out of the environment. A `GIT_DIR` exported
    /// to the whole container redirects **every** Git command in it, including
    /// the ones a build or a test suite makes about some other directory
    /// entirely.
    pub(super) fn pointer_text(&self) -> Option<String> {
        match self {
            Self::Plain { .. } => None,
            Self::Linked { git_dir_within, .. } => Some(format!(
                "gitdir: {GIT_COMMON_DIR_TARGET}/{}\n",
                git_dir_within.to_string_lossy().replace('\\', "/"),
            )),
        }
    }

    /// EAC-FR-FNFV: what a linked worktree's store record is replaced by.
    ///
    /// What the real file holds depends on the tool that made the working copy:
    /// the `git` command line writes the path relative to the Git directory it
    /// sits in, and libgit2 — which is what this application creates its
    /// worktrees with — writes the absolute **host** path. An absolute host
    /// path names nothing inside a container, and Git resolves this record
    /// before anything else, so a turn standing on one fails on its first Git
    /// command rather than on some later one. A working copy this application
    /// did not make is one it has no rule over, so the executor generates the
    /// relative form here and pins it, and what a turn resolves is a property
    /// of the mounts alone.
    pub(super) fn commondir_text(&self) -> Option<String> {
        match self {
            Self::Plain { .. } => None,
            // The Git directory is inside the store and is not the store
            // itself, both of which `repository_access` established, so there
            // is always at least one level to climb.
            Self::Linked { git_dir_within, .. } => Some(format!(
                "{}\n",
                vec![".."; git_dir_within.components().count()].join("/"),
            )),
        }
    }

    /// EAC-FR-FNFV: the read-only bind mounts this access is made of, in the
    /// order they must be applied — a store before the mask that covers an entry
    /// inside it.
    ///
    /// `pointer` is the generated `.git` for the linked shape and unused for the
    /// plain one; `empty` is the empty file the mask is made of.
    pub(super) fn mounts(
        &self,
        workspace: &str,
        store: &str,
        pin: &Path,
        record: Option<&Path>,
        empty: &Path,
    ) -> Vec<(PathBuf, String)> {
        match self {
            Self::Plain { git_dir } => vec![
                // Pinned over itself: the turn reads it, and cannot write it.
                // Its store is inside the working tree, so it goes wherever the
                // execution directory is mounted.
                (git_dir.clone(), format!("{workspace}/.git")),
                (
                    empty.to_path_buf(),
                    format!("{workspace}/.git/{GIT_MASKED_ENTRY}"),
                ),
            ],
            Self::Linked {
                git_dir_within,
                common_dir,
            } => {
                let mut out = vec![
                    (common_dir.clone(), store.to_string()),
                    (empty.to_path_buf(), format!("{store}/{GIT_MASKED_ENTRY}")),
                ];
                // The generated store record, where the shape needed one
                // generated. Pinned over the worktree's own for the reason the
                // pointer is: read-only, so this turn cannot rewrite what the
                // next one resolves through.
                if let Some(record) = record {
                    out.push((
                        record.to_path_buf(),
                        format!(
                            "{store}/{}/{GIT_COMMON_DIR_RECORD}",
                            git_dir_within.to_string_lossy().replace('\\', "/"),
                        ),
                    ));
                }
                // The pin. Where the container names the host's paths it is the
                // worktree's own `.git`, mounted over itself; where it does not
                // it is the generated pointer. It is read-only either way, so
                // the turn cannot rewrite the `gitdir:` line and redirect the
                // *next* turn's mount at some other repository on this machine.
                out.push((pin.to_path_buf(), format!("{workspace}/.git")));
                out
            }
        }
    }

    /// EAC-FR-FNFV: what the invocation tells Git, which is as little as it can.
    ///
    /// No `GIT_DIR`, no `GIT_WORK_TREE`, and no `GIT_COMMON_DIR`: Git resolves
    /// the working tree by reading `.git` where it stands, so a command about
    /// some other directory in the container is answered about that directory
    /// rather than about this run.
    ///
    /// `GIT_OPTIONAL_LOCKS=0` stops a plain read failing over a write nobody
    /// asked for: Git otherwise writes the index it just refreshed back into a
    /// directory it may not write. `safe.directory` is relaxed because Git
    /// refuses a repository whose owner it does not recognise, and a container
    /// launched for one agent, running as the owner of what it mounts, has no
    /// second user for that check to protect anything from.
    pub(super) fn env(&self) -> Vec<(&'static str, String)> {
        vec![
            ("GIT_OPTIONAL_LOCKS", "0".to_string()),
            ("GIT_CONFIG_COUNT", "1".to_string()),
            ("GIT_CONFIG_KEY_0", "safe.directory".to_string()),
            ("GIT_CONFIG_VALUE_0", "*".to_string()),
        ]
    }
}

/// EAC-FR-FNFV: the repository an execution directory belongs to.
///
/// Resolved through `git2`, which is how every other part of this application
/// reads a repository, and which is what tells the two shapes apart.
///
/// Going through `git2` rather than [`FsAccess`] is deliberate. `FsAccess`
/// guards the paths the *application* reads and writes on the author's behalf,
/// and its roots are the active worktree and `app_data_dir()`. A run's
/// repository is usually under neither — the author's active worktree is
/// frequently a linked one whose primary checkout sits somewhere else entirely
/// — so resolving through it would answer `None` for the ordinary case and
/// quietly leave every turn reading files whole. What stands in place of that
/// allowlist is the pin: the `.git` this resolution reads is mounted read-only,
/// so it says the same thing on the next turn as it does on this one.
///
/// `None` where the directory belongs to no repository, or where its Git
/// directory is not inside the store it names. Each is a launch-without-it
/// rather than a refusal: repository access is a convenience of a turn and never
/// a precondition of one, which is the opposite of EAC-FR-41's masking.
pub(crate) fn repository_access(execution_directory: &Path) -> Option<RepositoryAccess> {
    // `open` rather than `discover`: the execution directory is the working
    // tree, and a directory that is not one is not a repository this turn may
    // reach through some parent it never asked about.
    let repo = git2::Repository::open(execution_directory).ok()?;
    // Git records a directory with a trailing separator. Trimmed here rather
    // than at each use, so a path this module mounts, names as a target, and
    // compares is one string throughout (EAC-FR-ZKMR).
    let trim = |path: &Path| PathBuf::from(path.to_string_lossy().trim_end_matches('/'));
    let git_dir = trim(repo.path());
    let common_dir = trim(repo.commondir());

    if git_dir == common_dir {
        return Some(RepositoryAccess::Plain { git_dir });
    }
    // A worktree's Git directory is inside the store it shares, which is where
    // Git itself puts it. A pair that fails this is a shape this module has no
    // single mount for, and mounting it as two leaves every relative path Git
    // wrote inside it resolving somewhere neither mount is.
    let git_dir_within = git_dir.strip_prefix(&common_dir).ok()?.to_path_buf();
    Some(RepositoryAccess::Linked { common_dir, git_dir_within })
}

pub(super) const REPOSITORY_MASK_DIRNAME: &str = "repository-mask";

/// EAC-FR-FNFV: the generated `.git` a linked worktree is pinned with.
///
/// Beside the masks and owned by this module, for the reason they are: it
/// is machine state the executor writes, and it belongs in neither worktree.
/// The execution directory's slug follows this stem, so two launches standing
/// in two working copies generate two files rather than overwriting one. What
/// that costs is a pair of small files per working copy this application has
/// ever run a turn in, kept rather than cleared: clearing them on a launch would
/// remove the pair a concurrent launch is about to be mounted from, which is the
/// race the slug exists to close.
pub(super) const REPOSITORY_POINTER_FILENAME: &str = "repository-pointer";

/// EAC-FR-FNFV: the generated store record a linked worktree's Git directory is
/// pinned with. Beside the pointer and owned by this module, for the reason it
/// is.
pub(super) const REPOSITORY_COMMONDIR_FILENAME: &str = "repository-commondir";

/// EAC-FR-41: the empty regular file a `.git` **file** is masked by.
///
/// A linked worktree's root `.git` is a file, and only a file may be bind
/// mounted over one.
pub(super) const REPOSITORY_MASK_FILENAME: &str = "repository-mask-file";

/// EAC-FR-41: the two empty things a repository entry may be masked by, one per
/// kind of entry.
pub(super) struct RepositoryMask {
    pub(super) dir: PathBuf,
    pub(super) file: PathBuf,
}

impl RepositoryMask {
    /// The mask of the same kind as the entry being hidden.
    pub(super) fn source_for(&self, kind: EntryKind) -> &Path {
        match kind {
            EntryKind::Dir => &self.dir,
            _ => &self.file,
        }
    }
}

/// EAC-FR-40: every check a supplied bundle has to pass, before any container
/// is created.
///
/// Every filesystem call here goes through `../core/FSA-filesystem-access.md`
/// rather than a bare path predicate, and the instance is rooted at the
/// application's own data directory — which is what makes the containment check
/// below a property of the accessor rather than a string comparison anyone
/// could get subtly wrong.
pub(super) fn validate_supplementary_mount(
    fs: &FsAccess,
    data_roots: &[PathBuf],
    host: &Path,
    execution_source: &str,
    session_state_source: Option<&Path>,
    credential_paths: &[String],
) -> Result<(), AgentExecutionError> {
    let refuse = |reason: &str| AgentExecutionError::SupplementaryMountInvalid(reason.to_string());

    if host
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(refuse("path contains a parent segment"));
    }

    // The bundle is the application's own material, so it lives under one of the
    // application's own roots and nowhere else. Resolving first and comparing
    // the canonical form is what closes the symbolic-link escape: a link inside
    // one of those roots pointing out of it resolves to a path that fails this,
    // and a link that leaves one root without entering another fails it too.
    let roots: Vec<PathBuf> = data_roots
        .iter()
        .map(|root| crate::changes::canonicalize_lenient(root))
        .collect();
    let canonical = crate::changes::canonicalize_lenient(host);
    if !roots.iter().any(|root| canonical.starts_with(root)) {
        return Err(refuse(
            "bundle is outside the application's data directory",
        ));
    }

    let info = fs.file_info(&canonical).map_err(|err| match err {
        crate::fs::FsError::NotFound { .. } => refuse("bundle does not exist"),
        _ => refuse("bundle cannot be read"),
    })?;
    if info.kind != EntryKind::Dir {
        return Err(refuse("bundle is not a directory"));
    }

    // EAC-FR-40: it must not be, contain, or be contained by the execution
    // directory, the vendor's **session-state directory**, or its credential
    // source. A bundle overlapping any of the three would put a credential, a
    // session, or a change set behind a path nobody audited — and the
    // session-state directory is the one of the three that is also **writable**
    // in the container, so an overlap there would hand the agent a way to write
    // into what it was given read-only.
    let overlaps = |other: &str| -> bool {
        let other = crate::changes::canonicalize_lenient(Path::new(other));
        canonical == other || canonical.starts_with(&other) || other.starts_with(&canonical)
    };
    if overlaps(execution_source) {
        return Err(refuse("bundle overlaps the execution directory"));
    }
    if let Some(session_state) = session_state_source {
        if overlaps(&session_state.to_string_lossy()) {
            return Err(refuse(
                "bundle overlaps the vendor's session-state directory",
            ));
        }
    }
    for path in credential_paths {
        if overlaps(path) {
            return Err(refuse("bundle overlaps the vendor's credential directory"));
        }
    }

    // EAC-FR-08: 1 MiB for any one file. The bundle whole is not bounded and no
    // total over it is taken — a report the agent reads one file at a time is
    // not one a total could refuse without refusing what nobody reads at once.
    let mut stack = vec![canonical];
    while let Some(dir) = stack.pop() {
        let entries = fs
            .list_dir(&dir)
            .map_err(|_| refuse("bundle cannot be read"))?;
        for entry in entries {
            let path = dir.join(&entry.name);
            match entry.kind {
                EntryKind::Dir => stack.push(path),
                EntryKind::File => {
                    let size = fs.file_info(&path).map(|info| info.size).unwrap_or(0);
                    if size > SEMANTIC_REBASE_FILE_LIMIT {
                        return Err(refuse("a file in the bundle exceeds its limit"));
                    }
                }
                // A link inside the bundle is neither read nor followed: what
                // the agent is given is the report this application wrote.
                _ => return Err(refuse("bundle holds an entry that is not a file")),
            }
        }
    }
    Ok(())
}

/// EAC-FR-41: every repository-metadata path inside the execution directory —
/// the entry at its root, whether a directory or the file a linked worktree
/// carries, and every nested one beneath it.
/// EAC-FR-41: how many directories the mask walk may descend into.
///
/// High enough that no ordinary project reaches it — a dependency tree of a
/// large front end is tens of thousands of directories — and finite, so a
/// pathological tree bounds the walk rather than the launch.
pub(super) const REPOSITORY_MASK_WALK_LIMIT: usize = 200_000;

pub(super) fn repository_metadata_paths(
    fs: &FsAccess,
    execution_directory: &Path,
) -> Result<Vec<(String, EntryKind)>, AgentExecutionError> {
    repository_metadata_paths_within(fs, execution_directory, REPOSITORY_MASK_WALK_LIMIT)
}

/// [`repository_metadata_paths`] with the walk bound named rather than fixed,
/// so the refusal EAC-FR-41 requires at that bound is reachable from a test
/// without building a tree of two hundred thousand directories.
pub(super) fn repository_metadata_paths_within(
    fs: &FsAccess,
    execution_directory: &Path,
    limit: usize,
) -> Result<Vec<(String, EntryKind)>, AgentExecutionError> {
    let mut out: Vec<(String, EntryKind)> = Vec::new();
    let mut stack = vec![execution_directory.to_path_buf()];
    let mut visited: usize = 0;
    while let Some(dir) = stack.pop() {
        let entries = fs.list_dir(&dir).map_err(|err| {
            AgentExecutionError::RepositoryMaskingUnavailable(err.to_string())
        })?;
        for entry in entries {
            let path = dir.join(&entry.name);
            if entry.name == ".git" {
                let Ok(rel) = path.strip_prefix(execution_directory) else {
                    continue;
                };
                // The kind travels with the path: a `.git` file and a `.git`
                // directory are masked by different things.
                out.push((rel.to_string_lossy().replace('\\', "/"), entry.kind));
                continue;
            }
            if entry.kind == EntryKind::Dir {
                // EAC-FR-41: **every** nested `.git`, wherever it is. A
                // dependency directory is not a floor here as it is for a
                // change set: a package manager that vendored a checkout put a
                // whole repository under `node_modules`, and a repository left
                // unmasked is exactly what this rule exists to prevent —
                // whether the author wrote it or a tool installed it.
                //
                // The cost that floor was avoiding is bounded by count instead,
                // so a pathological tree cannot hold a launch open
                // indefinitely. Reaching that bound **refuses the launch**
                // rather than stopping the walk: a walk that stopped early has
                // not established the masking, and EAC-FR-41 has no
                // launch-anyway path. Masking part of a tree and starting the
                // container regardless is the one outcome that reads as a
                // guarantee while not being one.
                visited = visited.saturating_add(1);
                if visited > limit {
                    return Err(AgentExecutionError::RepositoryMaskingUnavailable(
                        "the execution directory holds more directories than the mask can walk"
                            .to_string(),
                    ));
                }
                stack.push(path);
            }
        }
    }
    out.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(out)
}
