//! The worktree a rework run revises its change in (`OCR-openspec-change-runs.md`
//! OCR-FR-QUSP, OCR-FR-SSAI, OCR-FR-FJFI).
//!
//! A work turn of a rework run stands here and nowhere else, so no turn of the
//! run writes the stream's working copy or the stream branch. The worktree is a
//! linked worktree of the project's repository, at the run's **seed commit**:
//! the stream branch head with the change folder as it stands in the stream's
//! working copy. An accepted hunk can stand uncommitted there (CEP-FR-IYHP), so
//! a seed of the branch head alone would make the finish propose to undo it.
//!
//! The worktree outlives a rest, so Continue resumes the saved work. It is
//! removed when the run ends.

use std::path::{Path, PathBuf};

use super::*;

/// OCR-FR-QUSP: the scratch branch the registration needs a reference of its
/// own for, so it never borrows a live branch.
fn scratch_branch(run_id: &str) -> String {
    format!("synthesis/rework-run/{run_id}")
}

/// OCR-FR-QUSP: the registration Git holds the worktree under.
fn registration(run_id: &str) -> String {
    format!("{run_id}-cw")
}

/// The project's repository, opened at its primary worktree.
fn primary<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<git2::Repository, String> {
    let root = app
        .try_state::<crate::project::ProjectState>()
        .and_then(|state| state.root())
        .ok_or_else(|| crate::graduation::ERR_NO_PROJECT_OPEN.to_string())?;
    crate::streams::primary_repo_of(&root)
}

/// OCR-FR-QUSP, OCR-FR-SSAI: the rework worktree of a run and its seed
/// commit, created when it does not stand yet.
///
/// A worktree that stands is kept as it is: it holds the work of the earlier
/// passes. One the registration names but whose directory is gone is created
/// again, at the recorded seed where that commit still stands.
pub(crate) fn ensure<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) -> Result<(PathBuf, String), String> {
    let change = run
        .openspec
        .as_ref()
        .filter(|_| run.is_change_rework())
        .map(|data| data.change.clone())
        .ok_or_else(|| "the run is not a rework run".to_string())?;
    let target = crate::graduation::store_base(app)?.rework_worktree(&run.id);
    let fs = crate::graduation::store_fs(app)?;
    let main = primary(app)?;
    let name = registration(&run.id);

    let stands = crate::graduation::is_directory(&fs, &target);
    let registered = crate::streams::registers_worktree(&main, &name);
    if let (true, true, Some(seed)) = (stands, registered, run.base_commit.clone()) {
        return Ok((target, seed));
    }
    if stands || registered {
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation created a rework worktree again",
            log_fields! { "run_id" => run.id.clone() },
        );
        crate::streams::reclaim_linked_worktree(
            &fs,
            &main,
            &target,
            &name,
            Some(&scratch_branch(&run.id)),
        )?;
    }
    if let Some(parent) = target.parent() {
        if !crate::graduation::is_directory(&fs, parent) {
            fs.create_dir(parent).map_err(|e| e.to_string())?;
        }
    }

    // OCR-FR-SSAI: the recorded seed where it still stands, so the work of the
    // run is measured from the same commit; a new seed otherwise.
    let recorded = run
        .base_commit
        .as_deref()
        .and_then(|seed| git2::Oid::from_str(seed).ok())
        .filter(|oid| main.find_commit(*oid).is_ok());
    let seed = match recorded {
        Some(oid) => oid,
        None => seed_commit(app, &main, run, &change)?,
    };
    let commit = main.find_commit(seed).map_err(|e| e.to_string())?;
    let branch = scratch_branch(&run.id);
    main.branch(&branch, &commit, true).map_err(|e| e.to_string())?;
    let reference = main
        .find_branch(&branch, git2::BranchType::Local)
        .map_err(|e| e.to_string())?
        .into_reference();
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(&reference));
    main.worktree(&name, &target, Some(&opts)).map_err(|e| e.to_string())?;
    crate::worktree::normalize_worktree_commondir(&main, &name).map_err(|e| e.to_string())?;

    // Detached: the worktree reads one revision and owns no branch, so nothing a
    // turn writes can move the scratch branch either.
    let checkout = git2::Repository::open(&target).map_err(|e| e.to_string())?;
    checkout.set_head_detached(seed).map_err(|e| e.to_string())?;
    let mut builder = git2::build::CheckoutBuilder::new();
    builder.force();
    checkout.checkout_head(Some(&mut builder)).map_err(|e| e.to_string())?;
    logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "graduation created a rework worktree",
        log_fields! { "run_id" => run.id.clone(), "stream_id" => run.stream_id.clone() },
    );
    Ok((target, seed.to_string()))
}

/// OCR-FR-QUSP, GRD-FR-YBUM: the rework worktree of a rework run and its seed
/// commit as the base commit, before its first turn of a dispatch.
///
/// `None` where the run came to rest instead, having recorded why.
pub(super) fn prepare<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
) -> Option<PathBuf> {
    match ensure(app, run) {
        Ok((path, seed)) => {
            if run.base_commit.as_deref() != Some(seed.as_str()) {
                run.base_commit = Some(seed);
                let _ = crate::graduation::save_run(app, run);
            }
            Some(path)
        }
        Err(reason) => {
            let resume_part =
                run.checkpoint.resume_part.clone().unwrap_or_else(|| phases::PART_WORK.to_string());
            logging::log_error(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation could not prepare a rework worktree",
                log_fields! { "run_id" => run.id.clone(), "reason" => reason.clone() },
            );
            super::turns::note_blocked(app, run, "rework_worktree_failed");
            let _ = transitions::block(
                app,
                run,
                "rework_worktree_failed",
                reason,
                "Continue the run to prepare the rework worktree again.",
                &resume_part,
            );
            None
        }
    }
}

/// OCR-FR-SSAI: the stream branch head with each file of the change folder as
/// it stands in the stream's working copy, committed as one commit.
///
/// The tree is built from the files read through `FSA-filesystem-access.md`,
/// so nothing is copied on disk and the stream's working copy is not written.
/// A file of the head that the working copy no longer holds is left out, so
/// the change folder of the seed is the one of the working copy.
fn seed_commit<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    main: &git2::Repository,
    run: &GraduationRun,
    change: &str,
) -> Result<git2::Oid, String> {
    let stream = crate::streams::stream_of(app, &run.stream_id)
        .ok_or_else(|| crate::graduation::ERR_UNKNOWN_STREAM.to_string())?;
    let access = crate::graduation::store_fs(app)?;
    let copy = crate::fs::RootFs::new(stream.worktree(), access);
    let head = git2::Repository::open(stream.worktree())
        .and_then(|repo| repo.head().and_then(|h| h.peel_to_commit()).map(|c| c.id()))
        .map_err(|e| e.to_string())?;
    let head = main.find_commit(head).map_err(|e| e.to_string())?;
    let base_tree = head.tree().map_err(|e| e.to_string())?;

    let folder = format!("openspec/changes/{change}");
    let mut files = Vec::new();
    collect_files(&copy, &folder, &mut files)?;
    let mut update = git2::build::TreeUpdateBuilder::new();
    for path in tree_files(main, &base_tree, &folder) {
        if !files.iter().any(|(kept, _)| kept == &path) {
            update.remove(path.as_str());
        }
    }
    for (path, bytes) in &files {
        let blob = main.blob(bytes).map_err(|e| e.to_string())?;
        let mode = base_tree
            .get_path(Path::new(path))
            .ok()
            .filter(|entry| entry.filemode() == i32::from(git2::FileMode::BlobExecutable))
            .map_or(git2::FileMode::Blob, |_| git2::FileMode::BlobExecutable);
        update.upsert(path.as_str(), blob, mode);
    }
    let tree = update.create_updated(main, &base_tree).map_err(|e| e.to_string())?;
    let tree = main.find_tree(tree).map_err(|e| e.to_string())?;
    let signature = main.signature().map_err(|e| e.to_string())?;
    let message = format!("Seed of the rework run {}", run.id);
    main.commit(None, &signature, &signature, &message, &tree, &[&head]).map_err(|e| e.to_string())
}

/// Every regular file below `dir` of the working copy, with its
/// repository-relative path and its bytes. A link and a special file are left
/// out: neither is a file of the change.
fn collect_files(
    copy: &crate::fs::RootFs,
    dir: &str,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    let entries = match copy.list_dir(dir) {
        Ok(entries) => entries,
        Err(crate::fs::FsError::NotFound { .. }) => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    for entry in entries {
        let path = format!("{dir}/{}", entry.name);
        match entry.kind {
            crate::fs::EntryKind::Dir => collect_files(copy, &path, out)?,
            crate::fs::EntryKind::File => {
                let bytes = copy.read_bytes(&path).map_err(|e| e.to_string())?;
                out.push((path, bytes));
            }
            _ => {}
        }
    }
    Ok(())
}

/// The paths of every blob below `dir` of a tree.
fn tree_files(repo: &git2::Repository, tree: &git2::Tree<'_>, dir: &str) -> Vec<String> {
    let Some(sub) = tree
        .get_path(Path::new(dir))
        .ok()
        .and_then(|entry| entry.to_object(repo).ok())
        .and_then(|object| object.into_tree().ok())
    else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    let _ = sub.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            if let Ok(name) = entry.name() {
                paths.push(format!("{dir}/{root}{name}"));
            }
        }
        git2::TreeWalkResult::Ok
    });
    paths
}

/// OCR-FR-FJFI: remove the rework worktree, its registration, and its scratch
/// branch. Every step is idempotent, so a release that stopped half way is
/// finished by the next one.
pub(crate) fn release<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) -> Result<(), String> {
    let target = crate::graduation::store_base(app)?.rework_worktree(&run.id);
    let fs = crate::graduation::store_fs(app)?;
    let main = primary(app)?;
    crate::streams::reclaim_linked_worktree(
        &fs,
        &main,
        &target,
        &registration(&run.id),
        Some(&scratch_branch(&run.id)),
    )
}

/// OCR-FR-FJFI: whether anything of a rework run's own still stands, so a
/// queue read that finds an ended run reclaims it once and then costs nothing.
pub(crate) fn stands<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) -> bool {
    let Ok(base) = crate::graduation::store_base(app) else {
        return false;
    };
    let worktree = base.rework_worktree(&run.id);
    if crate::graduation::store_fs(app)
        .is_ok_and(|fs| crate::graduation::is_directory(&fs, &worktree))
    {
        return true;
    }
    let Ok(main) = primary(app) else {
        return false;
    };
    crate::streams::registers_worktree(&main, &registration(&run.id))
        || main.find_branch(&scratch_branch(&run.id), git2::BranchType::Local).is_ok()
}

/// The path the scratch branch of a run takes, for a test that checks it.
#[cfg(test)]
pub(crate) fn scratch_branch_for_test(run_id: &str) -> String {
    scratch_branch(run_id)
}
