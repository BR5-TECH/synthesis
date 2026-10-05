//! Project lifecycle + the open-project root state.
//!
//! `open_project_at_path` / `open_project_from_git_url` / `create_project`
//! derive a [`ProjectHandle`], append it to the user-global recents MRU
//! (GSS-FR-10 / PST-FR-06), and activate the project (set the root, reset the
//! per-project caches, start the watcher). [`ProjectState`] is the managed
//! root the Library/artifact commands read. The pure `*_impl` halves and the
//! `record_open` / `deactivate_project` seams are unit-tested without a runtime.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use crate::artifacts::ContentTracker;
use crate::progress;
use crate::global_settings::GlobalSettingsStore;
use crate::watcher::{start_watching, ProjectWatcher};

/// What an open/create returns. `path` is the project's **identity anchor**
/// (PST-FR-01 / `WTC-worktree-context.md` WTC-FR-18): the repository's primary
/// worktree when the project is inside a Git repository, and the opened root
/// itself when it is not. It is what the recent-projects list records, so a
/// repository with several worktrees is one entry rather than many
/// (`GSS-global-settings-storage.md` GSS-FR-18).
///
/// `active_worktree_path` is the **content root** — the checkout every path in
/// the project resolves against (WTC-FR-03, PST-FR-22). For a project outside a
/// repository the two are the same path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHandle {
    pub name: String,
    pub path: String,
    pub active_worktree_path: String,
    /// WTC-FR-17: the project resumed on its primary worktree because the
    /// worktree it was last working in no longer exists. Surfaced so the UI can
    /// say why it is not where the user left it.
    pub remembered_worktree_unavailable: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CreateMode {
    Standalone,
    Colocated,
}

/// The currently-open project root. Set on a successful open/create; read by
/// the Library tree commands (`load_project_tree`, `rescan_project_tree`,
/// `assign_artifact_type`, `clear_artifact_type`). `None` means no project is
/// open — the tree commands return a typed "no project open" error.
///
/// In the walking skeleton the opened path is treated as the project root
/// (open does not yet validate `.synthesis/project.toml`); the scanner walks
/// whatever root is set here.
#[derive(Default)]
pub struct ProjectState {
    /// The **content root**: the active worktree's directory (WTC-FR-03). Every
    /// path this application resolves — `.synthesis/`, the tree, artifact files
    /// — resolves against it (PST-FR-22).
    root: Mutex<Option<PathBuf>>,
    /// The project's **identity anchor**: the repository's primary worktree, or
    /// the project root when it is not in a repository. Stable across a change
    /// of active worktree, which is what makes it the key of the recents entry
    /// and of the user-global per-project slot (GSS-FR-18).
    anchor: Mutex<Option<String>>,
    /// The instance governing this root (FSA-FR-21). Set with the root, so the
    /// pair `require_root` hands out is always internally consistent.
    access: Mutex<Option<Arc<crate::fs::FsAccess>>>,
    /// The repository machine store, named rather than resolved.
    ///
    /// Test-only, and deliberately so: the application resolves the store from
    /// the identity anchor under `app_data_dir()` (RMS-FR-ZXHM), and a
    /// production override would be a way to put a conversation somewhere the
    /// repository identity does not select. A unit test names its own tempdir
    /// instead, so a suite never writes into the author's real application
    /// data.
    #[cfg(test)]
    store: Mutex<Option<PathBuf>>,
}

impl ProjectState {
    /// Set the content root together with the [`crate::fs::FsAccess`] that
    /// governs it (FSA-FR-19 / FSA-FR-21).
    ///
    /// The two are one setting rather than two because a root without its
    /// access is a path any caller could have composed, and access without its
    /// root governs nothing in particular. Storing them together is what makes
    /// `require_root` able to hand out a `RootFs` that cannot be stale in one
    /// half and current in the other.
    pub fn set_root_with_access(&self, root: PathBuf, access: Option<Arc<crate::fs::FsAccess>>) {
        if let Ok(mut guard) = self.root.lock() {
            *guard = Some(root);
        }
        if let Ok(mut guard) = self.access.lock() {
            *guard = access;
        }
    }

    /// Set the content root alone, leaving whatever access is installed.
    ///
    /// Only for the tests that exercise a module against a bare root; the
    /// application always goes through `set_root_with_access` via
    /// `remount_content_root`.
    #[cfg(test)]
    pub fn set_root(&self, root: PathBuf) {
        // The same directory serves as the repository machine store, which is
        // what lets a command-level test assert on one tree.
        if let Ok(mut guard) = self.store.lock() {
            *guard = Some(root.clone());
        }
        // A test that mounts a root gets it governed, over exactly that
        // directory — so `require_root` answers, and so the module under test
        // reaches the disk through the same gate it will in production.
        let access = crate::fs::FsAccess::builder()
            .allow_root(&root)
            .build()
            .ok()
            .map(Arc::new);
        if let Ok(mut guard) = self.root.lock() {
            *guard = Some(root);
        }
        if let Ok(mut guard) = self.access.lock() {
            *guard = access;
        }
    }

    /// Name the repository machine store rather than resolving it.
    ///
    /// Test-only, on the same terms the `store` field is (see it). A fixture
    /// that installs its root through `set_root_with_access` names its store
    /// here, so the suite writes into its own tempdir rather than into the
    /// author's real application data.
    #[cfg(test)]
    pub fn set_store(&self, dir: PathBuf) {
        if let Ok(mut guard) = self.store.lock() {
            *guard = Some(dir);
        }
    }

    /// Record the project's identity anchor. Set once per open; a worktree
    /// switch changes the root but never the anchor.
    pub fn set_anchor(&self, anchor: String) {
        if let Ok(mut guard) = self.anchor.lock() {
            *guard = Some(anchor);
        }
    }

    /// The open project's identity anchor, or `None` when no project is open.
    pub fn anchor(&self) -> Option<String> {
        self.anchor.lock().ok().and_then(|guard| guard.clone())
    }

    /// The key of the open project's user-global slot (GSS-FR-18), or the empty
    /// string when no project is open — which selects the default slot
    /// (PSS-FR-06).
    pub fn slot_key(&self) -> String {
        self.anchor().unwrap_or_default()
    }

    /// Forget the open project (SNV-FR-25 / PST-FR-14). After this the tree
    /// commands return the typed "no project open" error again, exactly as
    /// before the first open.
    pub fn clear_root(&self) {
        if let Ok(mut guard) = self.root.lock() {
            *guard = None;
        }
        #[cfg(test)]
        if let Ok(mut guard) = self.store.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.access.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.anchor.lock() {
            *guard = None;
        }
    }

    /// The current content root, or `None` when no project is open. Used where
    /// the absence is ordinary rather than an error — notably the progress
    /// teardown of PRG-FR-13, which has nothing to terminate in that case.
    pub fn root(&self) -> Option<PathBuf> {
        self.root.lock().ok().and_then(|guard| guard.clone())
    }

    /// The open project's **repository machine store**
    /// (`RMS-repository-machine-storage.md` RMS-FR-ZXHM): where the conversation
    /// logs of `CMS-comments-storage.md` and the statistics logs of
    /// `DSS-draft-statistics-storage.md` live.
    ///
    /// Resolved from the **identity anchor** rather than from the content root
    /// (RMS-FR-QJVT), which is what makes a change of active worktree select the
    /// same store (RMS-FR-TCAF). A project outside a Git repository has the
    /// content root for both, so it selects a store of its own like any other.
    ///
    /// The `FsAccess` is the project's own: `app_data_dir()` is one of the four
    /// roots the IDE instance is built with (`FSA-filesystem-access.md`
    /// FSA-FR-21), so the store is reachable through the same gate every other
    /// path is.
    pub fn require_store(&self) -> Result<crate::fs::RootFs, String> {
        let access = self
            .access
            .lock()
            .map_err(|e| format!("project state poisoned: {e}"))?
            .clone()
            .ok_or_else(|| "no project open".to_string())?;
        let identity = self
            .anchor()
            .or_else(|| self.root().map(|p| p.to_string_lossy().into_owned()))
            .ok_or_else(|| "no project open".to_string())?;
        #[cfg(test)]
        if let Some(named) = self.store.lock().ok().and_then(|guard| guard.clone()) {
            return Ok(crate::fs::RootFs::new(named, access));
        }
        let normalized =
            crate::repository_store::normalize_identity(std::path::Path::new(&identity));
        let dir = crate::repository_store::ensure_store(&normalized, &access)?;
        Ok(crate::fs::RootFs::new(dir, access))
    }

    /// The current root, or a typed "no project open" error string for the
    /// tree commands (ASC-FR-05 / PST-FR-07).
    pub fn require_root(&self) -> Result<crate::fs::RootFs, String> {
        let root = self
            .root
            .lock()
            .map_err(|e| format!("project state poisoned: {e}"))?
            .clone()
            .ok_or_else(|| "no project open".to_string())?;
        let access = self
            .access
            .lock()
            .map_err(|e| format!("project state poisoned: {e}"))?
            .clone()
            .ok_or_else(|| {
                // A root with no access installed is not a usable project: every
                // operation would have to reach the disk unguarded, which is the
                // one thing FSA-FR-19 exists to make impossible.
                "no filesystem access for the open project".to_string()
            })?;
        Ok(crate::fs::RootFs::new(root, access))
    }
}

pub fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Whether re-rooting is the step that discards the session's diagnostic
/// buffer, or whether its caller already did (LGC-FR-15).
///
/// The distinction exists because the clear must come *before* the operation
/// that performs the switch, and one switch performs work before it re-roots: a
/// branch checkout runs — and reports itself — and only then re-roots onto the
/// content it just replaced. Clearing at the re-rooting would discard exactly
/// the records explaining the checkout that caused it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Diagnostics {
    /// This call is the first step of the switch.
    Discard,
    /// The caller discarded the buffer before doing work whose records must
    /// survive into the fresh buffer.
    AlreadyDiscarded,
}

/// LGC-FR-15: discard the session's diagnostic buffer for a switch that is about
/// to happen.
///
/// Gated on there being an outgoing root, because the re-rooting seam is also
/// the *first* open of a session (`activate_project`). Clearing there would
/// discard exactly the records LGC-FR-20 exists to keep: application startup, a
/// plugin install, and a project open that failed before this one succeeded.
pub fn discard_diagnostics_for_switch<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project: &ProjectState,
    buffer: &'static crate::logging::LogBuffer,
) {
    if project.root().is_some() {
        crate::logging::clear_and_publish(app, buffer);
    }
}

/// Mount `root` as the project's content root: set it, drop the previous root's
/// in-memory caches, and (re)start the filesystem watcher on it.
///
/// This is the single re-rooting seam. A project open runs it for the worktree
/// the open resolved to, and a worktree switch runs the *same* sequence for the
/// new content root (PST-FR-14 / `WTC-worktree-context.md` WTC-FR-08 /
/// ASC-FR-16 / CHC-FR-19) — teardown then remount, never a merge, so no scan
/// state, watcher registration, or cached classification from the previous root
/// survives into the new one. It modifies no file in either root.
pub fn remount_content_root<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project: &ProjectState,
    watcher: &ProjectWatcher,
    // Deliberately a plain path: this is the function that BUILDS the guarded
    // root, so it is the one place that cannot already have one.
    root: &Path,
    // Taken rather than reached for, as `logging::log` itself takes it, so the
    // ordering this function is half of — discard, then the switch's own
    // records — is exercisable against a buffer of a test's own.
    buffer: &'static crate::logging::LogBuffer,
    diagnostics: Diagnostics,
) {
    let root = root.to_path_buf();
    if diagnostics == Diagnostics::Discard {
        discard_diagnostics_for_switch(app, project, buffer);
    }
    // The first record of the fresh buffer says what it is fresh *for*, so the
    // Logs panel after a switch is not simply blank. The path is a project path,
    // not a credential — LGC-FR-16 puts that judgement here, at the emit site.
    crate::logging::log(
        app,
        buffer,
        crate::logging::LogLevel::Info,
        &[crate::logging::Domain::Backend],
        "content root changed",
        crate::log_fields! { "root" => root.to_string_lossy().into_owned() },
    );
    // `../core/GRD-graduation.md` GRD-FR-TWMA: a change of active worktree
    // stops **nothing**. A run works in its work stream's own working copy,
    // which the author's active worktree is not, so moving the author changes
    // nothing the run is reading or writing.
    // PRG-FR-13: terminate every operation still running against the *outgoing*
    // content root before the root moves, so none is left in flight against a
    // root the application is no longer reading. Read before `set_root`, since
    // afterwards the outgoing root is no longer knowable.
    if let Some(outgoing) = project.root() {
        progress::terminate_scope_and_publish(
            app,
            &app.state::<progress::ProgressRegistry>(),
            &outgoing,
        );
    }
    // SCC-FR-12 / SCC-FR-04: a search still sweeping the outgoing root has
    // nothing left to answer for. Stopped before the root moves, so it ends with
    // its one terminal event rather than streaming hits from a checkout the
    // application is no longer showing.
    app.state::<crate::search::SearchRegistry>().cancel_running();
    // FSA-FR-21: the shared filesystem instance is REPLACED, not widened — the
    // outgoing worktree stops being reachable at the moment it stops being
    // active, because the new instance's allowlist simply does not contain it.
    // This runs on the first open of a session as well as on every switch,
    // since both arrive here; before it the pre-project instance is in force,
    // carrying `app_data_dir()` and a session temp directory alone.
    //
    // A build failure leaves the *previous* instance in force rather than
    // leaving the application with none. That is the safe direction: the old
    // allowlist cannot reach the new root, so every operation against it
    // refuses, which is a visible failure rather than an unguarded one.
    let mut installed_access = None;
    let mut access_install_failed = false;
    if let Some(access) = app.try_state::<crate::fs::FsAccessState>() {
        if let Err(e) = access.install_for_worktree(&root) {
            access_install_failed = true;
            crate::logging::log(
                app,
                &crate::logging::BUFFER,
                crate::logging::LogLevel::Error,
                &[crate::logging::Domain::Backend],
                "filesystem access could not be re-rooted",
                crate::log_fields! {
                    "root" => root.to_string_lossy().into_owned(),
                    "reason" => e.to_string(),
                },
            );
        }
        if !access_install_failed {
            installed_access = access.get();
        }
    }
    // FSA-FR-19: the root and the access governing it are set as one, so no
    // window exists in which a module could obtain one without the other.
    //
    // On a failed install `installed_access` stays `None` rather than carrying
    // the *previous* instance forward: that instance is rooted at the outgoing
    // worktree, and pairing it with the incoming root would produce a handle
    // whose two halves disagree — `require_root` would answer `Ok`, and every
    // read and write through it would then fail with "escapes allowed roots"
    // far from the cause. `None` makes `require_root` say what actually
    // happened.
    project.set_root_with_access(root.clone(), installed_access);
    // PST-FR-14: switching projects unmounts the previous project's in-memory
    // artifact checksum baselines so a stale id cannot collide across projects.
    // The same applies across worktrees: an id is a project-relative path, so
    // the identical id names different bytes in a different checkout.
    app.state::<ContentTracker>().clear();
    // ASC-FR-16 / ASC-FR-17: the candidate list belongs to the content root, so
    // it is torn down with it rather than carried into the new one.
    app.state::<crate::scanning::CandidateStore>().clear();
    // ASC-FR-16: likewise the attribution baseline — the incoming worktree has
    // its own `.synthesis/library.toml`, so the outgoing root's checksum would
    // name different bytes and could suppress the first genuine change here.
    app.state::<crate::scanning::AttributionBaseline>().clear();
    // PCP-FR-14 / PCP-FR-20: an acceptance interrupted by a crash or a kill is
    // settled here, **before any surface has been served that artifact**, and
    // the record and candidate of a decided proposal whose artifact the project
    // no longer holds are swept. Run against the incoming root and after the
    // root has been set, so both reach the tree the window is about to show.
    if let Ok(rooted) = project.require_root() {
        // The sweep needs the worktree alone, so it runs whatever the store
        // does. Only the reconciliation writes a decision comment, and only it
        // is bound to a store that resolved.
        match project.require_store() {
            Ok(store) => crate::prompt_proposals::on_content_root_mounted(app, &rooted, &store),
            Err(reason) => {
                // RMS-FR-WGQS: recorded once per open project, here at the one
                // mount every open and every worktree change passes through.
                // The reason and nothing of the author's: no path, no identity.
                crate::logging::log_warn(
                    app,
                    &crate::logging::BUFFER,
                    &[crate::logging::Domain::Backend],
                    "the repository store could not be resolved, so conversations \
                     and statistics are unavailable for this project",
                    crate::log_fields! { "reason" => reason },
                );
                crate::prompt_proposals::sweep_on_content_root_mounted(app, &rooted);
            }
        }
    }
    // PST-FR-MHZB / RMS-FR-XLTR: opening a project and changing the active
    // worktree each schedule one import pass, which copies the conversation and
    // statistics records an earlier build committed into the project across into
    // the repository machine store. Scheduled from the mount rather than from
    // the open, so a worktree change reaches it too — the pass reads the
    // *incoming* worktree's legacy folders. It removes, stages, and commits
    // nothing (RMS-FR-XRPT), and it runs in the background so it blocks
    // neither operation.
    crate::repository_store::schedule_import(app);
    // ASC-FR-14 / ASC-FR-16 / CHC-FR-19: replacing the debouncer drops the
    // previous one, so the outgoing root emits no further events.
    // The watcher thread checksums files on every batch, so it takes the
    // guarded root for the same reason the index pass does.
    match project.require_root() {
        Ok(rooted) => start_watching(app, watcher, &rooted),
        // Nothing to watch that could be read safely.
        Err(_) => watcher.stop(),
    }
    // DRS-FR-42: `.synthesis/drafts/` is pruned from the watcher above
    // (ASC-FR-09), so the drafts root carries a watch of its own — mounted on
    // the incoming root here and torn down with the outgoing one, exactly as
    // the project watcher is. It is what keeps the Dashboard's Active
    // workstreams widget current when an external editor writes a prompt
    // (PST-FR-35).
    if let Some(drafts_watcher) = app.try_state::<crate::draft_watcher::DraftsWatcher>() {
        match project.require_root() {
            Ok(rooted) => crate::draft_watcher::start_watching(app, &drafts_watcher, &rooted),
            Err(_) => drafts_watcher.stop(),
        }
    }
    // BMI-FR-13 / BMI-FR-25: the BM25 indexes belong to the content root. The
    // outgoing root's are discarded rather than merged into the new one, and a
    // full build of the new root is requested. It runs in the background, so
    // the project is usable throughout (BMI-FR-12).
    if let Some(indexer) = app.try_state::<crate::bm25_index::Bm25Indexer>() {
        // Mounted with the guarded root rather than a bare path: an index pass
        // runs on its own thread, and this is what keeps its reads inside the
        // same allowlist the command that scheduled it was bound by.
        match project.require_root() {
            Ok(rooted) => indexer.mount(&rooted),
            // No access installed means no project is usable, and there is
            // nothing valid for a pass to run against.
            Err(_) => {}
        }
    }
    crate::bm25_index::request_pass(app, crate::bm25_index::PassScope::ALL);
    // DCL-FR-QGLH / DCL-FR-XHSJ: the Documents collection opens with the project
    // and, on a change of active worktree, keeps its sources and runs one
    // refresh. Mounted after the index, whose `documents` part it feeds.
    crate::documents::lifecycle::on_content_root_mounted(app, &project.slot_key());
}

/// DRS-FR-04: whether Git still excludes the active worktree's drafts root.
///
/// Asked of Git rather than of the ignore file this module writes, because the
/// rule that excludes it may be anywhere Git reads one from. A content root in
/// no repository excludes nothing, and is not a state to report.
fn drafts_are_ignored(root: &crate::fs::RootFs) -> bool {
    crate::changes::open_repo(root.path())
        .ok()
        .and_then(|repo| {
            repo.status_should_ignore(Path::new(crate::storage_floor::DRAFTS_REL))
                .ok()
        })
        .unwrap_or(false)
}

/// Mark `path` as the open project and (re)start the filesystem watcher on it.
/// Shared by every open/create command (PST-FR-06 records recents; this seam
/// activates the Library scan/watch surface).
fn activate_project<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project: &ProjectState,
    watcher: &ProjectWatcher,
    handle: &ProjectHandle,
) {
    project.set_anchor(handle.path.clone());
    // SWN-FR-16 / PPK-FR-15: the Project settings menu entry is offered only
    // while a project is open. Put back here, before the frontend has drawn
    // anything, so the entry and the project appear together.
    crate::menu::set_project_settings_present(app, true);
    remount_content_root(
        app,
        project,
        watcher,
        Path::new(&handle.active_worktree_path),
        &crate::logging::BUFFER,
        Diagnostics::Discard,
    );
    // PST-FR-CDYM: opening a project runs `ensure_gitignored` against the active
    // worktree's `.synthesis/` folder (FSA-FR-08), which removes an existing
    // project's `drafts/` ignore entry — the whole of the migration draft
    // storage becoming committed asks for (per `DRS-draft-storage.md`
    // DRS-FR-ISPI). It writes `<synthesis_dir>/.gitignore` and no other file, so
    // not one draft file is read, deleted, moved, rewritten, staged, or
    // committed by it; the existing drafts reach the repository through the next
    // ordinary commit of DRS-FR-QHHY rather than through a migration commit
    // (DRS-FR-NRQQ). An **open** and not a remount, because changing the active
    // worktree modifies no file of the project (PST-FR-14). Best-effort: a
    // project whose ignore file cannot be written still opens, exactly as one
    // whose scaffold is incomplete does.
    if let Ok(rooted) = project.require_root() {
        if let Err(e) = rooted.ensure_gitignored(rooted.path().join(".synthesis")) {
            crate::logging::log(
                app,
                &crate::logging::BUFFER,
                crate::logging::LogLevel::Warn,
                &[crate::logging::Domain::Backend],
                "the project ignore file could not be brought up to date",
                crate::log_fields! { "reason" => e.to_string() },
            );
        }
        // DRS-FR-04: no ignore rule covers the drafts root. The entry this
        // module owns has just gone, but an ignore rule the author wrote — in
        // the project's own `.gitignore`, in `.git/info/exclude`, or in their
        // global excludes file — is theirs to remove and is not read or
        // rewritten here. An ignored path is invisible to a working-tree status,
        // so a draft-event commit of PST-FR-DQZT would silently carry no draft;
        // this is what makes that state visible in the Logs panel instead of
        // being reported as nothing to commit. The fact alone, and no pattern
        // and no path of the author's.
        if drafts_are_ignored(&rooted) {
            crate::logging::log(
                app,
                &crate::logging::BUFFER,
                crate::logging::LogLevel::Warn,
                &[crate::logging::Domain::Backend],
                "draft storage is still covered by an ignore rule, so no draft \
                 will be committed",
                crate::log_fields! { "folder" => crate::storage_floor::DRAFTS_REL },
            );
        }
    }
    // PST-FR-30: opening a project schedules the draft-asset housekeeping pass
    // of `DAS-draft-assets.md` (DAS-FR-19). Scheduled after the content root is
    // mounted, so the drafts it addresses are the incoming worktree's; it runs
    // asynchronously and blocks the open at no point, and a pass that fails
    // leaves the open untouched and is reported through the logging path alone
    // (DAS-FR-21).
    crate::draft_assets::sweep_project_draft_assets(app);
    // DAS-FR-19 / PST-FR-30: and the application's own start pass, once, after
    // the active project has been loaded. Where it coincides with the open
    // above — the shell reopening the last project at launch — the request is
    // coalesced into the pass already scheduled rather than running a second.
    crate::draft_assets::sweep_at_application_start(app);
}

/// Tear down everything tied to the open project (SNV-FR-25): forget the root
/// (PST-FR-14), stop the two filesystem watchers (ASC-FR-14, DRS-FR-42), and
/// drop the per-project in-memory caches (the content-checksum baselines,
/// PST-FR-14). Pure over the managed stores — no Tauri `State`/`AppHandle` — so
/// the teardown is unit-testable without a runtime. Touches no file on disk,
/// satisfying SNV-FR-25's "no file is modified".
///
/// There is no recently-edited list to drop: none is kept anywhere (PSS-FR-12),
/// and the Dashboard's Recently edited widget is read from the project's
/// artifacts and their modification times at the moment it is asked
/// (PST-FR-31).
pub fn deactivate_project(
    project: &ProjectState,
    watcher: &ProjectWatcher,
    tracker: &ContentTracker,
    drafts_watcher: &crate::draft_watcher::DraftsWatcher,
    candidates: &crate::scanning::CandidateStore,
    attribution: &crate::scanning::AttributionBaseline,
) {
    project.clear_root();
    watcher.stop();
    tracker.clear();
    // DRS-FR-42: no event fires for a closed project, so the drafts watch goes
    // down with the project watcher rather than outliving it.
    drafts_watcher.stop();
    // ASC-FR-14 / ASC-FR-17: the candidate list is torn down with the scan, so a
    // search started after the close finds nothing rather than the closed
    // project's files.
    candidates.clear();
    // ASC-FR-14: and the attribution baseline, on the same terms. A checksum
    // carried across a close would name the closed project's bytes, and the next
    // project's first attribution write could match it by coincidence and be
    // suppressed as though this application had made it.
    attribution.clear();
}

/// SNV-FR-25: close the open project. Invoked by the frontend as the last step
/// of File -> Close project, *after* it has written every artifact's pending
/// changes (EDT-FR-33) — a torn-down project has no root, so `save_artifact_contents`
/// would fail from here on. Writes no file itself, and is idempotent.
#[tauri::command]
pub fn close_project<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
    watcher: State<'_, ProjectWatcher>,
    tracker: State<'_, ContentTracker>,
    drafts_watcher: State<'_, crate::draft_watcher::DraftsWatcher>,
    progress_registry: State<'_, progress::ProgressRegistry>,
    candidates: State<'_, crate::scanning::CandidateStore>,
    searches: State<'_, crate::search::SearchRegistry>,
    turns: State<'_, crate::agent_conversations::TurnRegistry>,
) {
    // LGC-FR-15: the session's diagnostic buffer is discarded in full when the
    // project closes. First, before anything else the close does, so every
    // record the teardown itself emits lands in the fresh buffer rather than
    // being discarded by a clear that followed it.
    crate::logging::clear_and_publish(&app, &crate::logging::BUFFER);
    crate::logging::log(
        &app,
        &crate::logging::BUFFER,
        crate::logging::LogLevel::Info,
        &[crate::logging::Domain::Backend],
        "project closed",
        crate::logging::Fields::new(),
    );
    // SCC-FR-14: stop a search still sweeping the project being closed, so it
    // ends with its one terminal event instead of streaming into a closed
    // project's overlay.
    searches.cancel_running();
    // AGC-FR-20: a turn in flight when the project closes terminates
    // `cancelled` and appends nothing. Runs before the teardown, while the root
    // the reply would be written into is still knowable.
    crate::agent_conversations::cancel_all_turns(&app, &turns, &progress_registry);
    // `../core/GRD-graduation.md` GRD-FR-TWMA: a graduation the loop is holding
    // stops here and rests as `interrupted` — durable, still blocking its
    // queue, and offering Continue. Nothing is discarded: in Git mode the
    // worktree and its change set survive, in place the edits already made do.
    // Runs before the teardown, while the run's record is still readable.
    crate::graduation::interrupt_running(
        &app,
        crate::graduation::GraduationInterruptionReason::ProjectChanged,
    );
    // GPP-FR-DATH: the polling session ends with the project, so a project
    // opened again starts a new session with nothing reported.
    crate::github_polling::end_session(&app);
    // PRG-FR-13: nothing may be left in flight against a root the application
    // has stopped reading. Runs before the teardown, while the root is still
    // knowable. Operations with no project scope (a plugin install, PRG-FR-15)
    // are deliberately untouched.
    if let Some(root) = project.root() {
        progress::terminate_scope_and_publish(&app, &progress_registry, &root);
    }
    // BMI-FR-25: the nine indexes go with the rest of the content root's
    // in-memory state, so a query after the close returns an empty list rather
    // than the closed project's chunks.
    if let Some(indexer) = app.try_state::<crate::bm25_index::Bm25Indexer>() {
        indexer.clear();
    }
    // DCL-FR-XHSJ: the Documents collection stops its watch and discards its
    // snapshot, its documents instance, and its sources in memory. Its PDF text
    // cache stays for the application session.
    crate::documents::lifecycle::on_project_closed(&app);
    // PST-FR-30: closing a project schedules the draft-asset housekeeping pass
    // of `DAS-draft-assets.md` (DAS-FR-19), **before** the unmount below
    // releases the project's storage context — so it addresses drafts that
    // still resolve rather than drafts that have already gone. Each scheduled
    // pass owns the filesystem handle it was scheduled with, so the unmount
    // that follows cannot take the material out from under it. It is the one
    // write a close may perform, and it reaches draft-owned material
    // alone (PST-FR-14); the close itself completes at its ordinary speed
    // whatever the pass is doing.
    crate::draft_assets::sweep_project_draft_assets(&app);
    // SWN-FR-16 / PPK-FR-15: the Project settings entry is ABSENT rather than
    // greyed-out while no project is open, so the menu the picker is left with
    // offers nothing that names a project that does not exist.
    crate::menu::set_project_settings_present(&app, false);
    deactivate_project(
        project.inner(),
        watcher.inner(),
        tracker.inner(),
        drafts_watcher.inner(),
        candidates.inner(),
        &app.state::<crate::scanning::AttributionBaseline>(),
    );
}

// Project open/create. The pure `*_impl` functions derive the `ProjectHandle`
// and are unit-tested without a Tauri runtime; the `#[tauri::command]` wrappers
// thread the managed `GlobalSettingsStore` and, on success, append the project
// to the user-global recent-projects MRU (GSS-FR-10 / PST-FR-06) so it shows up
// in the picker on the next launch.

/// Append a just-opened/created project to the user-global recents MRU. This is
/// the shared, unit-testable seam every open/create command runs on success.
///
/// Best-effort: a recents-write failure (a poisoned store or a disk error) is
/// logged but must NOT fail the open/create that triggered it — the project was
/// already opened successfully.
fn record_open(store: &GlobalSettingsStore, handle: &ProjectHandle) {
    if let Err(e) = store.record_recent_project(&handle.name, &handle.path) {
        eprintln!("synthesis: failed to record recent project: {e}");
    }
}

/// Resolve an opened path into the project's two-level identity (PST-FR-01 /
/// `WTC-worktree-context.md` WTC-FR-02, WTC-FR-17, WTC-FR-18).
///
/// The **anchor** is the repository's primary worktree — the same value from
/// whichever worktree the user opened, which is what makes a repository with
/// several worktrees a single project and a single recents entry. The **content
/// root** is the worktree the open lands in: the one opened, or the remembered
/// one when the anchor itself was opened, falling back to the primary when that
/// memory names a directory that is gone.
///
/// A project outside a Git repository resolves to itself for both.
fn resolve_handle(store: &GlobalSettingsStore, opened: &str) -> ProjectHandle {
    let opened_path = Path::new(opened);
    let anchor = crate::worktree::project_anchor(opened_path);
    let (active, remembered_worktree_unavailable) =
        crate::worktree::resume_active_worktree(store, &anchor, opened_path);
    let anchor = anchor.to_string_lossy().to_string();
    ProjectHandle {
        // The project is named for its anchor, so opening `acme-main` still
        // presents the project as `acme`.
        name: basename(&anchor),
        path: anchor,
        active_worktree_path: active.to_string_lossy().to_string(),
        remembered_worktree_unavailable,
    }
}

fn open_project_at_path_impl(
    store: &GlobalSettingsStore,
    path: String,
) -> Result<ProjectHandle, String> {
    if path.trim().is_empty() {
        return Err("path is empty".into());
    }
    Ok(resolve_handle(store, &path))
}

#[tauri::command]
pub fn open_project_at_path(
    path: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
    watcher: State<'_, ProjectWatcher>,
) -> Result<ProjectHandle, String> {
    let handle = open_project_at_path_impl(&store, path)?;
    open_resolved(&app, &store, &project, &watcher, handle)
}

/// The last step of every route that opens or creates a project: refuse a
/// switch of worktree a dispatched direct run forbids, record the open, and
/// make the project the open one.
///
/// WTC-FR-TXKY: opening the open project at another of its worktrees is a
/// switch of worktree like any other, so each route that can do it passes
/// through this guard.
pub(crate) fn open_resolved<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &GlobalSettingsStore,
    project: &ProjectState,
    watcher: &ProjectWatcher,
    handle: ProjectHandle,
) -> Result<ProjectHandle, String> {
    if crate::worktree::opens_other_worktree(project, &handle.path, &handle.active_worktree_path) {
        crate::graduation::require_no_dispatched_direct_run(app)?;
    }
    record_open(store, &handle);
    activate_project(app, project, watcher, &handle);
    Ok(handle)
}

fn open_project_from_git_url_impl(
    store: &GlobalSettingsStore,
    url: String,
) -> Result<ProjectHandle, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("url is empty".into());
    }
    let tail = trimmed
        .rsplit(|c| c == '/' || c == ':')
        .next()
        .unwrap_or(trimmed);
    let name = tail.strip_suffix(".git").unwrap_or(tail).to_string();
    if name.is_empty() {
        return Err("could not derive project name from url".into());
    }
    Ok(resolve_handle(store, &format!("~/dev/{}", name)))
}

#[tauri::command]
pub fn open_project_from_git_url(
    url: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
    watcher: State<'_, ProjectWatcher>,
) -> Result<ProjectHandle, String> {
    let handle = open_project_from_git_url_impl(&store, url)?;
    open_resolved(&app, &store, &project, &watcher, handle)
}

fn create_project_impl(
    store: &GlobalSettingsStore,
    name: String,
    mode: CreateMode,
    target_path: String,
) -> Result<ProjectHandle, String> {
    let _ = mode;
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("name is empty".into());
    }
    if target_path.trim().is_empty() {
        return Err("target folder is required".into());
    }
    // A created project keeps the name the user chose; only its identity anchor
    // and content root come from the worktree resolution.
    Ok(ProjectHandle {
        name,
        ..resolve_handle(store, &target_path)
    })
}

#[tauri::command]
pub fn create_project(
    name: String,
    mode: CreateMode,
    target_path: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
    watcher: State<'_, ProjectWatcher>,
) -> Result<ProjectHandle, String> {
    create_project_routed(&app, &store, &project, &watcher, name, mode, target_path)
}

/// The whole of the create route, past the command's own arguments.
pub(crate) fn create_project_routed<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &GlobalSettingsStore,
    project: &ProjectState,
    watcher: &ProjectWatcher,
    name: String,
    mode: CreateMode,
    target_path: String,
) -> Result<ProjectHandle, String> {
    let handle = create_project_impl(store, name, mode, target_path)?;
    open_resolved(app, store, project, watcher, handle)
}

#[cfg(test)]
mod asset_housekeeping_tests;

#[cfg(test)]
mod tests;
