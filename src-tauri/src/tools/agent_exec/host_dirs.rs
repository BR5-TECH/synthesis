//! The host directories a launch mounts: the session state of EAC-FR-31,
//! the repository mask of EAC-FR-41, the roots of EAC-FR-40, the repository
//! access of EAC-FR-FNFV, and the launch files of EAC-FR-BOXF.
//!
//! Split out of `agent_exec.rs` for size alone. Everything here is a method of
//! [`AgentCliExecutor`] and reads the module's own private items through
//! `super`.

use std::path::{Path, PathBuf};

use super::*;

impl AgentCliExecutor {
    /// EAC-FR-40: every user-scoped root the application owns, which is what a
    /// supplied bundle must sit under.
    ///
    /// There are **two** of them and a bundle inside either one satisfies the
    /// check: the application data directory, and the short root of
    /// `FSA-filesystem-access.md` FSA-FR-KVWD — which is where
    /// `GSU-graduation-start.md` GSU-FR-RJRF puts a graduation run's store, and
    /// therefore every semantic-rebase bundle this mount exists to carry.
    /// Naming only the first is what made every semantic-rebase turn refuse to
    /// launch.
    ///
    /// A build under test roots it at the harness's own directory, exactly as it
    /// roots session state there, and that one root **replaces** both — a test
    /// exercises the root it named rather than the machine's.
    pub(super) fn data_roots(&self) -> Result<Vec<PathBuf>, AgentExecutionError> {
        if let Some(root) = &self.session_state_root {
            return Ok(vec![root.clone()]);
        }
        let roots: Vec<PathBuf> = [crate::fs::short_data_dir(), crate::fs::app_data_dir()]
            .into_iter()
            .flatten()
            .collect();
        if roots.is_empty() {
            return Err(AgentExecutionError::SupplementaryMountInvalid(
                "no application data directory".to_string(),
            ));
        }
        Ok(roots)
    }

    /// EAC-FR-FNFV: the read-only repository mounts and Git environment for one
    /// launch, or `None` where this turn reaches no repository.
    ///
    /// **All of it or none of it.** A store mounted without the mask that covers
    /// its `config`, or a linked worktree whose pointer could not be pinned, is
    /// worse than no access: the first discloses a credential and the second
    /// lets a turn redirect the next turn's mount. So a part that cannot be
    /// expressed as a mount takes the whole grant with it, and the turn reads
    /// files instead — which is what it did before this existed.
    /// `directory` is the **canonical** execution directory of EAC-FR-02, which
    /// is the one spelling everything below is composed from: the shape was
    /// decided from it, and Git reports the repository in the same form.
    pub(super) fn repository_mounts(
        &self,
        fs: &FsAccess,
        directory: &Path,
        workspace: &str,
        aligned: bool,
    ) -> Option<RepositoryMounts> {
        let access = repository_access(directory)?;
        let mask = self.empty_mask_dir(fs).ok()?;
        // What the generated files below are named by. They are one execution's
        // own: a name shared with a concurrent launch is one that launch can
        // replace between this write and the moment the runtime resolves the
        // mount, which would stand this turn on another working copy's `.git`.
        let slug = descriptor::path_slug(directory);
        // EAC-FR-ZKMR: where the store stands, and what the worktree resolves
        // it through. Where the container names the host's paths the store is
        // at its own, the records the worktree already carries name paths the
        // container has, and nothing is generated — the pin is then the
        // worktree's own file. In the fallback shape the store stands somewhere
        // those records do not name, and both of them are generated.
        let (store, pin, record) = match &access {
            RepositoryAccess::Plain { .. } => (String::new(), mask.file.clone(), None),
            RepositoryAccess::Linked { common_dir, .. } if aligned => (
                // Withheld rather than half-granted: a store this launch cannot
                // express is the part EAC-FR-FNFV takes the whole grant with,
                // and the container keeps the paths it has already agreed on.
                descriptor::container_path(common_dir)?,
                directory.join(".git"),
                None,
            ),
            RepositoryAccess::Linked { .. } => {
                let pointer =
                    mask.dir.with_file_name(format!("{REPOSITORY_POINTER_FILENAME}-{slug}"));
                fs.write_text_atomic(&pointer, &access.pointer_text()?).ok()?;
                let commondir =
                    mask.dir.with_file_name(format!("{REPOSITORY_COMMONDIR_FILENAME}-{slug}"));
                fs.write_text_atomic(&commondir, &access.commondir_text()?).ok()?;
                (GIT_COMMON_DIR_TARGET.to_string(), pointer, Some(commondir))
            }
        };
        let mut out: Vec<BindMount> = Vec::new();
        for (source, target) in
            access.mounts(workspace, &store, &pin, record.as_deref(), &mask.file)
        {
            out.push(BindMount {
                source: descriptor::mount_source(&source)?,
                target,
                // Read-only is the whole of the grant: the turn reads history,
                // and it commits, checks out, stages, fetches, and rewrites
                // nothing.
                read_only: true,
            });
        }
        let env = access.env().into_iter().map(|(name, value)| (name.to_string(), value)).collect();
        Some((out, env, access.linked()))
    }

    /// EAC-FR-41: an empty, read-only directory to cover a `.git` path with, so a
    /// read of one fails and a write to one fails.
    ///
    /// It lives beside the executor's own session state rather than in either
    /// worktree: it is machine state this module owns, and it must exist on every
    /// machine a semantic-rebase turn can run on — a masking that could not be
    /// established is a refusal to launch rather than a launch without it.
    pub(super) fn empty_mask_dir(
        &self,
        fs: &FsAccess,
    ) -> Result<RepositoryMask, AgentExecutionError> {
        use crate::fs::FsError;

        let root = match &self.session_state_root {
            Some(root) => root.clone(),
            None => crate::fs::app_data_dir()
                .map_err(|_| {
                    AgentExecutionError::RepositoryMaskingUnavailable(
                        "no application data directory".to_string(),
                    )
                })?
                .join(SESSION_STATE_DIRNAME),
        };
        let dir = root.join(REPOSITORY_MASK_DIRNAME);
        // Routed through `FsAccess` like every other backend filesystem operation.
        if let Err(err) = fs.create_dir(&dir) {
            match err {
                FsError::AlreadyExists { .. } => {}
                other => {
                    return Err(AgentExecutionError::RepositoryMaskingUnavailable(
                        other.to_string(),
                    ));
                }
            }
        }
        // EAC-FR-41: a `.git` is not always a directory. A **linked worktree**
        // — which is what every graduation run is given (`Repository::worktree`)
        // — carries a `.git` **file** holding a `gitdir:` pointer at its root,
        // and a bind mount of a directory onto a file is refused by the kernel
        // with `ENOTDIR`. So the mask is a pair, and each entry is masked by
        // the one of its own kind.
        let file = root.join(REPOSITORY_MASK_FILENAME);
        // Truncating, so a file left from an earlier launch is empty again
        // rather than trusted to still be: what this mounts must hold nothing.
        if let Err(err) = fs.create_file(&file, crate::fs::CreateMode::Truncate) {
            return Err(AgentExecutionError::RepositoryMaskingUnavailable(err.to_string()));
        }
        Ok(RepositoryMask { dir, file })
    }

    /// The directory this launch's session state lives in, created if absent.
    ///
    /// Three outcomes, kept distinct on purpose. `Ok(None)` means this vendor has
    /// no dedicated mount because its credential mount is also its session
    /// directory (EAC-FR-16). `Ok(Some(..))` means the directory is there and
    /// mountable. An `Err` means it should have been there and is not — which is
    /// a failure to report rather than a mount to quietly omit, because a turn
    /// that runs without it looks entirely successful while every transcript it
    /// writes dies with the container (EAC-FR-31).
    ///
    /// `mount` is the session-state mount the launch credential leaves the
    /// vendor (EAC-FR-QHIU), never [`descriptor::SessionStateMount::ByCredential`].
    pub(super) fn session_state_for(
        &self,
        fs: &FsAccess,
        vendor: &str,
        mount: Option<descriptor::SessionStateMount>,
        project_key: &str,
    ) -> Result<Option<(PathBuf, &'static str, &'static str)>, AgentExecutionError> {
        use crate::fs::FsError;

        let Some(descriptor::SessionStateMount::Dedicated { env, target }) = mount else {
            return Ok(None);
        };

        let root = match &self.session_state_root {
            Some(root) => root.clone(),
            None => crate::fs::app_data_dir()
                .map_err(|_| {
                    AgentExecutionError::SessionStateUnavailable(
                        SessionStateProblem::RootUnresolved,
                    )
                })?
                .join(SESSION_STATE_DIRNAME),
        };
        let dir = descriptor::session_state_dir(&root, vendor, project_key);

        // Routed through `FsAccess` like every other backend filesystem
        // operation. The shared instance already allowlists `app_data_dir()`,
        // which is what this directory lives under, so nothing here needs a
        // bootstrap exemption.
        match fs.create_dir(&dir) {
            Ok(()) => {}
            // `create_dir` refuses to adopt an existing entry, so the second and
            // every later launch for a project lands here. That is the normal
            // path, not a failure — but only where what is already there is a
            // directory.
            Err(FsError::AlreadyExists { .. }) => {
                let info = fs.file_info(&dir).map_err(|_| {
                    AgentExecutionError::SessionStateUnavailable(SessionStateProblem::Unusable)
                })?;
                if info.kind != EntryKind::Dir {
                    return Err(AgentExecutionError::SessionStateUnavailable(
                        SessionStateProblem::Unusable,
                    ));
                }
            }
            Err(_) => {
                return Err(AgentExecutionError::SessionStateUnavailable(
                    SessionStateProblem::NotCreatable,
                ));
            }
        }

        Ok(Some((dir, env, target)))
    }

    /// EAC-FR-BOXF: the directory that holds `files`, written once and named by
    /// the SHA-256 of what it holds, under `agent-exec/vendor-files/`.
    ///
    /// A directory already at that name holds the same bytes, so it is used as
    /// it is and never written again. A new one is written under a name of its
    /// own and then moved to its final name, so a launch never mounts a
    /// directory that another launch is still writing.
    pub(super) fn launch_files_dir(
        &self,
        fs: &FsAccess,
        files: &[descriptor::LaunchFile],
    ) -> Result<PathBuf, AgentExecutionError> {
        use crate::fs::FsError;
        let failed =
            |what: &str| AgentExecutionError::LaunchFailed(format!("launch files: {what}"));

        let base = match &self.session_state_root {
            Some(root) => root.join(LAUNCH_FILES_DIRNAME),
            None => crate::fs::app_data_dir()
                .map_err(|_| failed("no application data directory"))?
                .join(LAUNCH_FILES_PARENT)
                .join(LAUNCH_FILES_DIRNAME),
        };
        let digest = descriptor::launch_files_digest(files);
        let dir = base.join(&digest);
        if fs.file_info(&dir).is_ok_and(|info| info.kind == EntryKind::Dir) {
            return Ok(dir);
        }

        let staging_name = format!(".{digest}.{}", descriptor::unique_container_name());
        let staging = base.join(&staging_name);
        match fs.create_dir(&staging) {
            Ok(()) | Err(FsError::AlreadyExists { .. }) => {}
            Err(_) => return Err(failed("the directory cannot be created")),
        }
        for file in files {
            if fs.write_bytes_atomic(staging.join(file.name), &file.contents).is_err() {
                // Leave no half-written staging directory behind.
                let _ = fs.delete_path(&staging, true);
                return Err(failed("a file cannot be written"));
            }
        }
        match fs.rename_path(&staging, &digest) {
            Ok(()) => Ok(dir),
            // Another launch moved the same bytes into place first. Its
            // directory is the one to use, and this copy is not needed.
            Err(_) if fs.file_info(&dir).is_ok_and(|info| info.kind == EntryKind::Dir) => {
                let _ = fs.delete_path(&staging, true);
                Ok(dir)
            }
            Err(_) => {
                let _ = fs.delete_path(&staging, true);
                Err(failed("the directory cannot be moved into place"))
            }
        }
    }
}
