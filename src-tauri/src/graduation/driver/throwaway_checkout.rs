//! The throwaway checkouts of a run: a review turn's (GRL-FR-YKRI) and a PR
//! summary turn's (GRD-FR-TYVT).
//!
//! Each turn stands in a checkout at a path of its own (GRL-FR-BIQX). A path is
//! never deleted and made again for a later turn, because a container runtime
//! can keep a deleted host path as "does not exist" after it is made again:
//! Docker Desktop on macOS does, for minutes, and every launch that mounts that
//! path then fails.
//!
//! Before a checkout is made, and when its turn ends, every checkout of the
//! same kind that the run holds is removed (GRL-FR-PQLM). The name without a
//! nonce, which earlier versions of the application used, is removed too.

use std::path::{Path, PathBuf};

use crate::fs as fsa;
use crate::graduation::{is_checkout_name, new_checkout_nonce, StoreBase};

/// How many nonces a checkout draws before it gives up. A clash needs a
/// directory or a registration of the same 32-bit name to stand already.
const NONCE_DRAWS: usize = 8;

/// One kind of throwaway checkout, for one run.
pub(super) struct Throwaway<'a> {
    run_id: &'a str,
    /// The run home every checkout of the run stands directly under.
    home: PathBuf,
    base: StoreBase,
    /// The directory name stem: `rv` or `ps`.
    stem: &'static str,
    /// The scratch branch every checkout of this kind is made from.
    branch: String,
}

impl<'a> Throwaway<'a> {
    /// GRL-FR-YKRI: the review checkouts of a run, on the scratch branch
    /// `synthesis/review/<run-id>`.
    pub(super) fn review(base: StoreBase, run_id: &'a str) -> Self {
        Self::new(base, run_id, crate::graduation::REVIEW_DIR, format!("synthesis/review/{run_id}"))
    }

    /// GRD-FR-XKGC: the PR summary checkouts of a run, on the scratch branch
    /// `synthesis/pr-summary/<run-id>`.
    pub(super) fn pr_summary(base: StoreBase, run_id: &'a str) -> Self {
        Self::new(
            base,
            run_id,
            crate::graduation::PR_SUMMARY_DIR,
            format!("synthesis/pr-summary/{run_id}"),
        )
    }

    fn new(base: StoreBase, run_id: &'a str, stem: &'static str, branch: String) -> Self {
        Self { run_id, home: base.home(run_id), base, stem, branch }
    }

    /// The path of the checkout that has the nonce `nonce`.
    fn path(&self, nonce: &str) -> PathBuf {
        self.home.join(format!("{}-{nonce}", self.stem))
    }

    /// The registration Git holds the checkout of `nonce` under.
    fn registration(&self, nonce: &str) -> String {
        format!("{}-{}-{nonce}", self.run_id, self.stem)
    }

    /// GRL-FR-PQLM: whether `name` is a registration of a checkout of this kind
    /// for this run: `<run-id>-<stem>` or `<run-id>-<stem>-<nonce>`.
    fn is_registration(&self, name: &str) -> bool {
        name.strip_prefix(self.run_id)
            .and_then(|rest| rest.strip_prefix('-'))
            .is_some_and(|rest| is_checkout_name(self.stem, rest))
    }

    /// Every registration of a checkout of this kind, with the path Git
    /// recorded for it.
    fn registrations(&self, main: &git2::Repository) -> Vec<(String, PathBuf)> {
        let Ok(names) = main.worktrees() else {
            return Vec::new();
        };
        names
            .iter()
            .filter_map(|name| name.ok().flatten())
            .filter(|name| self.is_registration(name))
            .filter_map(|name| {
                let worktree = main.find_worktree(name).ok()?;
                Some((name.to_string(), worktree.path().to_path_buf()))
            })
            .collect()
    }

    /// Whether `path` is a checkout of this kind: directly under the run home,
    /// with a checkout name of this kind. Both sides are resolved first, so a
    /// link in the store root does not hide a match, and a link named like a
    /// checkout does not lead out of the run home.
    ///
    /// The name is part of the test because a registration can record any
    /// path. One that records the run's logs or its merge worktree must not
    /// take that directory with it.
    fn is_checkout_path(&self, path: &Path) -> bool {
        let home = crate::changes::canonicalize_lenient(&self.home);
        let resolved = crate::changes::canonicalize_lenient(path);
        resolved.parent() == Some(home.as_path())
            && resolved
                .file_name()
                .is_some_and(|name| is_checkout_name(self.stem, &name.to_string_lossy()))
    }

    /// GRL-FR-PQLM / GRL-FR-XJJU: whether anything of a checkout of this kind
    /// stands, a directory or a registration.
    pub(super) fn stands(&self, fs: &fsa::FsAccess, main: &git2::Repository) -> bool {
        !self.base.checkouts_under(fs, self.run_id, self.stem).is_empty()
            || !self.registrations(main).is_empty()
    }

    /// GRL-FR-PQLM: remove every checkout of this kind, its registration, and
    /// the scratch branch.
    ///
    /// Every step is tried, also after one fails, so one checkout that cannot
    /// go does not keep the others. Every step is idempotent: what is already
    /// gone is the state the caller wanted. The error names each step that
    /// failed.
    pub(super) fn reclaim(
        &self,
        fs: &fsa::FsAccess,
        main: &git2::Repository,
    ) -> Result<(), String> {
        let mut failures: Vec<String> = Vec::new();
        for (name, path) in self.registrations(main) {
            if self.is_checkout_path(&path) {
                if let Err(reason) =
                    crate::streams::reclaim_linked_worktree(fs, main, &path, &name, None)
                {
                    failures.push(format!("registration {name}: {reason}"));
                    // GRL-FR-LYXG: a directory that does not go must not keep
                    // its registration either. A registration can hold the
                    // scratch branch, and the next checkout needs that branch.
                    // The directory stays for a later reclaim to remove.
                    if let Err(reason) = prune_registration_only(main, &name) {
                        failures.push(format!("prune {name}: {reason}"));
                    }
                }
            } else if let Err(reason) = prune_registration_only(main, &name) {
                // A registration that records a path that is not a checkout of
                // this run is pruned, and whatever stands at that path stays.
                failures.push(format!("registration {name}: {reason}"));
            }
        }
        for dir in self.base.checkouts_under(fs, self.run_id, self.stem) {
            if let Err(reason) = fs.delete_owned_tree(&dir) {
                let name = dir.file_name().map(|n| n.to_string_lossy().into_owned());
                failures.push(format!("directory {}: {reason}", name.unwrap_or_default()));
            }
        }
        if let Err(reason) = delete_branch(main, &self.branch) {
            failures.push(format!("branch {}: {reason}", self.branch));
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    /// GRL-FR-BIQX: make a new checkout at a path no checkout of the run stands
    /// at, detached at `commit` and checked out in full.
    ///
    /// Returns the checkout, opened, with its path.
    pub(super) fn make(
        &self,
        fs: &fsa::FsAccess,
        main: &git2::Repository,
        commit: git2::Oid,
    ) -> Result<(git2::Repository, PathBuf), String> {
        if !crate::graduation::is_directory(fs, &self.home) {
            fs.create_dir(&self.home).map_err(|e| e.to_string())?;
        }
        let nonce = self.free_nonce(fs, main, new_checkout_nonce)?;
        let target = self.path(&nonce);
        let name = self.registration(&nonce);

        let found = main.find_commit(commit).map_err(|e| e.to_string())?;
        main.branch(&self.branch, &found, true).map_err(|e| e.to_string())?;
        let reference = main
            .find_branch(&self.branch, git2::BranchType::Local)
            .map_err(|e| e.to_string())?
            .into_reference();
        let mut opts = git2::WorktreeAddOptions::new();
        opts.reference(Some(&reference));
        main.worktree(&name, &target, Some(&opts)).map_err(|e| e.to_string())?;
        crate::worktree::normalize_worktree_commondir(main, &name).map_err(|e| e.to_string())?;

        let checkout = git2::Repository::open(&target).map_err(|e| e.to_string())?;
        // Detached: the turn reads one revision and owns no branch, so nothing
        // it writes can move the scratch branch.
        checkout.set_head_detached(commit).map_err(|e| e.to_string())?;
        let mut builder = git2::build::CheckoutBuilder::new();
        builder.force();
        checkout.checkout_head(Some(&mut builder)).map_err(|e| e.to_string())?;
        Ok((checkout, target))
    }

    /// GRL-FR-BIQX: a nonce whose directory and registration both do not stand.
    ///
    /// `draw` is the nonce source: `new_checkout_nonce` in production, and a
    /// fixed sequence in a test that needs a clash.
    fn free_nonce(
        &self,
        fs: &fsa::FsAccess,
        main: &git2::Repository,
        mut draw: impl FnMut() -> String,
    ) -> Result<String, String> {
        for _ in 0..NONCE_DRAWS {
            let nonce = draw();
            let taken = fs.file_info(self.path(&nonce)).is_ok()
                || main.find_worktree(&self.registration(&nonce)).is_ok();
            if !taken {
                return Ok(nonce);
            }
        }
        Err("no free name for a new checkout".to_string())
    }
}

/// Prune a registration and leave its working tree where it stands.
fn prune_registration_only(main: &git2::Repository, name: &str) -> Result<(), String> {
    let worktree = main.find_worktree(name).map_err(|e| e.to_string())?;
    let mut opts = git2::WorktreePruneOptions::new();
    opts.valid(true).locked(true).working_tree(false);
    worktree.prune(Some(&mut opts)).map_err(|e| e.to_string())
}

/// Delete a local branch. Already gone is the state the caller wanted.
fn delete_branch(main: &git2::Repository, branch: &str) -> Result<(), String> {
    match main.find_branch(branch, git2::BranchType::Local) {
        Ok(mut found) => found.delete().map_err(|e| e.to_string()),
        Err(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests;
