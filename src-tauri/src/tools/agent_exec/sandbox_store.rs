//! The repository access of a sandbox launch (EAC-FR-TMQS, EAC-FR-WBDE).
//!
//! The sandbox runtime cannot bind a file inside a read-only mount, so the
//! Docker shape of EAC-FR-FNFV, a read-only store with an empty file over its
//! `config`, cannot boot. A sandbox gets the store as a directory the executor
//! generates instead: it holds an empty `config`, and each other store entry of
//! a fixed set is mounted read-only inside it under its own name.

use std::path::Path;

use super::repository::GIT_MASKED_ENTRY;
use super::*;
use crate::fs::{CreateMode, FsError};

/// EAC-FR-TMQS: the stem of the generated store directory, beside the masks.
/// The execution directory's slug follows it, as it follows the pointer's.
pub(super) const REPOSITORY_STORE_DIRNAME: &str = "repository-store";

/// EAC-FR-TMQS: the store entries a sandbox presents, and no other. `hooks` is
/// not one of them: a read-only turn runs no hook.
pub(super) const PRESENTED_STORE_ENTRIES: [&str; 11] = [
    "objects",
    "refs",
    "worktrees",
    "info",
    "logs",
    "lfs",
    "modules",
    "packed-refs",
    "shallow",
    "HEAD",
    "index",
];

impl AgentCliExecutor {
    /// EAC-FR-BJLI / EAC-FR-TMQS: the repository mounts and Git environment of
    /// one sandbox launch, or `None` where this turn reaches no repository.
    ///
    /// The same resolution as [`Self::repository_mounts`], so a turn in a VM
    /// reaches the repository a turn in a container reaches. A sandbox host
    /// names the host's paths in every case (EAC-FR-ZKMR), so a launch that is
    /// not aligned reaches none.
    pub(super) fn sandbox_repository_mounts<S>(
        &self,
        sink: &S,
        fs: &FsAccess,
        directory: &Path,
        workspace: &str,
        aligned: bool,
    ) -> Option<RepositoryMounts>
    where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        let reason = if aligned {
            match self.compose_sandbox_repository(sink, fs, directory, workspace) {
                Ok(granted) => return granted,
                Err(reason) => reason,
            }
        } else {
            "unaligned_paths"
        };
        // A repository the turn belongs to and cannot reach in the VM. The
        // reason names which part failed, and names no path.
        logging::log_warn(
            sink,
            &BUFFER,
            &[Domain::Ai, Domain::Backend],
            "sandbox execution cannot reach its repository",
            log_fields! { "reason" => reason },
        );
        None
    }

    /// [`Self::sandbox_repository_mounts`], with the reason a part failed.
    /// `Ok(None)` where the directory belongs to no repository, which is no
    /// failure (EAC-FR-FNFV).
    fn compose_sandbox_repository<S>(
        &self,
        sink: &S,
        fs: &FsAccess,
        directory: &Path,
        workspace: &str,
    ) -> Result<Option<RepositoryMounts>, &'static str>
    where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        let Some(access) = repository_access(directory) else {
            return Ok(None);
        };
        let mask = self.empty_mask_dir(fs).map_err(|_| "mask_dir_failed")?;
        let slug = descriptor::path_slug(directory);
        let generated = mask.dir.with_file_name(format!("{REPOSITORY_STORE_DIRNAME}-{slug}"));
        let (store, target, pin) = match &access {
            RepositoryAccess::Plain { git_dir } => {
                (git_dir.clone(), format!("{workspace}/.git"), None)
            }
            RepositoryAccess::Linked { common_dir, .. } => (
                common_dir.clone(),
                descriptor::container_path(common_dir).ok_or("unmountable_path")?,
                // EAC-FR-WBDE: the worktree's own `.git`, pinned read-only over
                // itself, so the turn cannot redirect the next turn's mount.
                Some(directory.join(".git")),
            ),
        };
        let entries: Vec<(&str, bool)> = PRESENTED_STORE_ENTRIES
            .iter()
            .filter_map(|name| crate::fs::probe_entry_is_dir(&store.join(name)).map(|d| (*name, d)))
            .collect();
        if let Err(error) = prepare_generated_store(fs, &generated, &entries) {
            // A turn can leave what `FsAccess` refuses to remove, such as a
            // symbolic link. The directory is set aside and made again, so the
            // new one has another identity and the VM reboots onto it
            // (EAC-FR-HXDN). What was set aside holds nothing of a repository.
            logging::log_warn(
                sink,
                &BUFFER,
                &[Domain::Ai, Domain::Backend],
                "sandbox repository store set aside and made again",
                log_fields! { "error" => error.to_string() },
            );
            let aside = format!(
                "{REPOSITORY_STORE_DIRNAME}-{slug}-discarded-{}",
                descriptor::unique_container_name()
            );
            fs.rename_path(&generated, &aside).map_err(|_| "store_set_aside_failed")?;
            prepare_generated_store(fs, &generated, &entries)
                .map_err(|_| "store_prepare_failed")?;
        }

        let mut out = vec![BindMount {
            source: descriptor::mount_source(&generated).ok_or("unmountable_path")?,
            target: target.clone(),
            // EAC-FR-WBDE: the one writable part. It reaches no host repository.
            read_only: false,
        }];
        for (name, _) in &entries {
            out.push(BindMount {
                source: descriptor::mount_source(&store.join(name)).ok_or("unmountable_path")?,
                target: format!("{target}/{name}"),
                read_only: true,
            });
        }
        if let Some(pin) = pin {
            out.push(BindMount {
                source: descriptor::mount_source(&pin).ok_or("unmountable_path")?,
                target: format!("{workspace}/.git"),
                read_only: true,
            });
        }
        let env = access.env().into_iter().map(|(name, value)| (name.to_string(), value)).collect();
        Ok(Some((out, env, access.linked())))
    }
}

/// EAC-FR-TMQS / EAC-FR-WBDE: make `generated` hold an empty `config` and one
/// mount point of the right kind for each presented entry, and remove
/// everything else a turn added to it.
///
/// The directory itself and its mount points are kept rather than made again:
/// a VM that is reused holds them, and a directory made again would stand empty
/// in that VM (EAC-FR-HXDN).
pub(super) fn prepare_generated_store(
    fs: &FsAccess,
    generated: &Path,
    entries: &[(&str, bool)],
) -> Result<(), FsError> {
    match fs.create_dir(generated) {
        Ok(()) | Err(FsError::AlreadyExists { .. }) => {}
        Err(other) => return Err(other),
    }
    for entry in fs.list_dir(generated)? {
        // Kept only where the kind is exactly a regular file or a directory. A
        // turn can put a symbolic link here, and the truncation below must
        // never follow one to a file outside this directory.
        let kind_of = |dir: bool| if dir { EntryKind::Dir } else { EntryKind::File };
        let kept = (entry.name == GIT_MASKED_ENTRY && entry.kind == EntryKind::File)
            || entries.iter().any(|(name, dir)| *name == entry.name && entry.kind == kind_of(*dir));
        if !kept {
            fs.delete_path(generated.join(&entry.name), true)?;
        }
    }
    // Emptied at every launch, so what a turn wrote there reaches no later one.
    fs.create_file(generated.join(GIT_MASKED_ENTRY), CreateMode::Truncate)?;
    for (name, is_dir) in entries {
        let point = generated.join(name);
        let made = if *is_dir {
            fs.create_dir(&point)
        } else {
            fs.create_file(&point, CreateMode::Exclusive)
        };
        match made {
            Ok(()) | Err(FsError::AlreadyExists { .. }) => {}
            Err(other) => return Err(other),
        }
    }
    Ok(())
}
