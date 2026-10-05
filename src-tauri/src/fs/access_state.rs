//! The application's shared `FsAccess`, as Tauri managed state (FSA-FR-21).

use std::path::{Path, PathBuf};

use super::access::FsAccess;
use super::error::FsAccessBuildError;

// ---------------------------------------------------------------------------
// FSA-FR-21: the instance the application actually runs on
// ---------------------------------------------------------------------------

/// The application's shared `FsAccess`, as Tauri managed state (FSA-FR-21).
///
/// Holds one instance at a time and *replaces* it rather than mutating it —
/// FSA-FR-18 makes an instance immutable, and that is the property this state
/// preserves: opening a project or changing the active worktree builds a new
/// instance and swaps it in, so the outgoing worktree stops being reachable at
/// the moment it stops being active.
///
/// Before any project is open the instance carries the two application-owned
/// roots — `app_data_dir()` and `short_data_dir()` — and a session temp
/// directory alone, which is what lets `GSS-global-settings-storage` serve the
/// recent-projects list on a fresh machine (GSS-FR-01) and `LGC-logging` answer
/// with no content root (LGC-FR-20).
#[derive(Debug, Default)]
pub struct FsAccessState {
    current: std::sync::RwLock<Option<std::sync::Arc<FsAccess>>>,
    /// FSA-FR-29: one instance per agent session, keyed by that session's id.
    ///
    /// Separate from `current` rather than a wider allowlist on it, because the
    /// difference between the two profiles is precisely what an agent may not
    /// reach: folding them together would give every agent session the IDE's
    /// `app_data_dir()` root.
    agents: std::sync::RwLock<std::collections::HashMap<String, std::sync::Arc<FsAccess>>>,
    /// The worktree the current IDE instance was built against, or `None` when
    /// no project is open. Held so an agent instance can be built against the
    /// same root without asking a caller to supply it — and so FSA-FR-29's
    /// "never against a closed project" is decidable here rather than trusted.
    worktree: std::sync::RwLock<Option<PathBuf>>,
    /// FSA-FR-DMKC: the documents instance — read-only reach to the selected
    /// Documents paths and to nothing else.
    ///
    /// A slot of its own, like `agents`, and for the same reason in the other
    /// direction: no worktree change replaces it (FSA-FR-WBKZ), and no other
    /// instance ever gains a selected path (FSA-FR-TXZU). Nothing in
    /// `install_pre_project`, `install_for_worktree`, or `open_agent_session`
    /// reads or writes it.
    documents: std::sync::RwLock<Option<std::sync::Arc<FsAccess>>>,
    /// FSA-FR-WBKZ: the grants the documents instance was built from, as
    /// `(is_folder, path)` and sorted. A refresh that finds the same grants keeps
    /// the instance.
    documents_grants: std::sync::Mutex<Vec<(bool, PathBuf)>>,
    /// Bumped every time the roots change, so a build that began against the
    /// outgoing worktree can be recognised and dropped instead of inserted.
    ///
    /// Building an instance does real filesystem work — canonicalising roots and
    /// creating a temp directory — and holding a lock across that would put I/O
    /// inside a critical section every reader of `agents` waits on. Reading the
    /// counter before the build and re-checking it under the insert's lock costs
    /// nothing and closes the same window.
    generation: std::sync::atomic::AtomicU64,
}

impl FsAccessState {
    /// Build the pre-project instance: `app_data_dir()`, `short_data_dir()`,
    /// and a session temp directory, links refused. Called once at startup.
    pub fn install_pre_project(&self) -> Result<(), FsAccessBuildError> {
        let app_data = super::app_data_dir()
            .map_err(|e| FsAccessBuildError::AppDataDir(e.to_string()))?;
        let short_data = super::short_data_dir()
            .map_err(|e| FsAccessBuildError::AppDataDir(e.to_string()))?;
        let built = FsAccess::builder()
            .owned_root(app_data)
            .owned_root(short_data)
            .session_temp(true)
            .build()?;
        self.set_worktree(None);
        self.discard_agent_sessions();
        self.replace(built);
        Ok(())
    }

    /// Build the IDE instance: the active worktree, `app_data_dir()`,
    /// `short_data_dir()`, and a session temp directory, links refused
    /// (FSA-FR-21).
    ///
    /// Every agent instance is discarded here. Opening a project and changing
    /// the active worktree both land in this call, and FSA-FR-29 gives an agent
    /// instance exactly the IDE instance's lifetime: no instance outlives the
    /// root it was built against, so no session keeps reading a checkout the
    /// application has stopped showing.
    pub fn install_for_worktree(&self, worktree: &Path) -> Result<(), FsAccessBuildError> {
        let app_data = super::app_data_dir()
            .map_err(|e| FsAccessBuildError::AppDataDir(e.to_string()))?;
        let short_data = super::short_data_dir()
            .map_err(|e| FsAccessBuildError::AppDataDir(e.to_string()))?;
        // FSA-FR-ZUCF: the two application roots are owned; the worktree is
        // the user's project and is never reclaimable by this application.
        let built = FsAccess::builder()
            .allow_root(worktree)
            .owned_root(app_data)
            .owned_root(short_data)
            .session_temp(true)
            .build()?;
        self.set_worktree(Some(worktree.to_path_buf()));
        self.discard_agent_sessions();
        self.replace(built);
        Ok(())
    }

    /// FSA-FR-29: build this agent session's instance — the active worktree and
    /// a session temp directory of its own, links refused, and no
    /// `app_data_dir()`.
    ///
    /// Returns [`FsAccessBuildError::NoRoots`] when no project is open, which is
    /// FSA-FR-29's "built only while a project is open" enforced rather than
    /// documented: with no worktree there is no root but `app_data_dir()`, and
    /// handing an agent that one is the single thing this profile exists to
    /// prevent.
    ///
    /// Calling twice for one id replaces the first instance, which drops its
    /// temp directory with it (FSA-FR-20).
    pub fn open_agent_session(
        &self,
        session_id: &str,
    ) -> Result<std::sync::Arc<FsAccess>, FsAccessBuildError> {
        use std::sync::atomic::Ordering;

        let began_at = self.generation.load(Ordering::Acquire);
        let worktree = self
            .worktree
            .read()
            .ok()
            .and_then(|guard| guard.clone())
            .ok_or(FsAccessBuildError::NoRoots)?;
        let built = std::sync::Arc::new(
            FsAccess::builder()
                .allow_root(worktree)
                .session_temp(true)
                .build()?,
        );
        let mut guard = self.agents.write().map_err(|_| FsAccessBuildError::NoRoots)?;
        // The roots moved while this was building, so `built` is rooted at a
        // worktree the application has stopped showing and the discard that
        // accompanied the move has already run. Inserting it now would resurrect
        // exactly the instance FSA-FR-29 says cannot outlive its root, so it is
        // dropped here instead — taking its temp directory with it.
        if self.generation.load(Ordering::Acquire) != began_at {
            return Err(FsAccessBuildError::NoRoots);
        }
        guard.insert(session_id.to_string(), std::sync::Arc::clone(&built));
        Ok(built)
    }

    /// The instance for `session_id`, or `None` when that session holds none.
    pub fn agent_session(&self, session_id: &str) -> Option<std::sync::Arc<FsAccess>> {
        self.agents
            .read()
            .ok()
            .and_then(|guard| guard.get(session_id).cloned())
    }

    /// End one agent session, dropping its instance and its temp directory
    /// (FSA-FR-20, FSA-FR-29).
    pub fn close_agent_session(&self, session_id: &str) {
        if let Ok(mut guard) = self.agents.write() {
            guard.remove(session_id);
        }
    }

    /// How many agent sessions hold an instance. Lets a caller — and a test —
    /// observe the discard that a worktree change performs.
    pub fn agent_session_count(&self) -> usize {
        self.agents.read().map(|guard| guard.len()).unwrap_or(0)
    }

    /// Drop every agent instance the state holds, and mark any build now in
    /// flight as belonging to the outgoing roots.
    ///
    /// "Drops" is true of the state's own reference. A caller still holding an
    /// `Arc` to one keeps that instance alive — and its temp directory with it —
    /// until it lets go, which is why a caller resolves its instance from here
    /// per call rather than holding one for its lifetime.
    fn discard_agent_sessions(&self) {
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        if let Ok(mut guard) = self.agents.write() {
            guard.clear();
        }
    }

    fn set_worktree(&self, worktree: Option<PathBuf>) {
        if let Ok(mut guard) = self.worktree.write() {
            *guard = worktree;
        }
    }

    /// The IDE profile widened to one further root, for a test whose module
    /// writes somewhere the two application-owned roots stand for in
    /// production.
    ///
    /// Test-only: production has exactly the three profiles above, and a fourth
    /// reachable from a command would be a way to widen the allowlist at
    /// runtime — the one thing FSA-FR-21 exists to prevent.
    #[cfg(test)]
    pub fn install_for_worktree_and(
        &self,
        worktree: &Path,
        also: &Path,
    ) -> Result<(), FsAccessBuildError> {
        let built = FsAccess::builder()
            .allow_root(worktree)
            .owned_root(also)
            .session_temp(true)
            .build()?;
        self.set_worktree(Some(worktree.to_path_buf()));
        self.discard_agent_sessions();
        self.replace(built);
        Ok(())
    }

    /// FSA-FR-WBKZ: bring the documents instance in line with the current
    /// Documents sources.
    ///
    /// Each source is probed on its own first, so one path that cannot be
    /// granted (missing, a link, the wrong kind) does not cost the others their
    /// reach. The refused paths come back with the reason, which is how the
    /// Documents collection reports a source unavailable. The instance is rebuilt
    /// only when the set of grantable sources differs from the one it was built
    /// from; with the same set the same instance stays, so a refresh that changed
    /// nothing builds nothing. While no source is grantable, no instance exists.
    /// A removed source's path is refused from the moment the replacement is
    /// installed.
    pub fn install_documents(
        &self,
        files: &[PathBuf],
        folders: &[PathBuf],
    ) -> Vec<(PathBuf, super::error::GrantRefusal)> {
        let mut refused = Vec::new();
        let mut granted: Vec<(bool, PathBuf)> = Vec::new();
        for path in files {
            match FsAccess::builder().read_only_file(path).build() {
                Ok(_) => granted.push((false, path.clone())),
                Err(e) => refused.push((path.clone(), refusal_of(&e))),
            }
        }
        for path in folders {
            match FsAccess::builder().read_only_folder(path).build() {
                Ok(_) => granted.push((true, path.clone())),
                Err(e) => refused.push((path.clone(), refusal_of(&e))),
            }
        }
        granted.sort();
        let mut current = self
            .documents_grants
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let standing = self.documents().is_some();
        if *current == granted && standing == !granted.is_empty() {
            return refused;
        }
        let mut builder = FsAccess::builder();
        for (is_folder, path) in &granted {
            builder = if *is_folder {
                builder.read_only_folder(path)
            } else {
                builder.read_only_file(path)
            };
        }
        let built = if granted.is_empty() {
            None
        } else {
            builder.build().ok()
        };
        if let Ok(mut guard) = self.documents.write() {
            *guard = built.map(std::sync::Arc::new);
        }
        *current = granted;
        refused
    }

    /// FSA-FR-DMKC: the documents instance, or `None` while no Documents source
    /// is available.
    pub fn documents(&self) -> Option<std::sync::Arc<FsAccess>> {
        self.documents.read().ok().and_then(|g| g.clone())
    }

    /// FSA-FR-WBKZ: discard the documents instance, on project close.
    pub fn discard_documents(&self) {
        if let Ok(mut guard) = self.documents.write() {
            *guard = None;
        }
        self.documents_grants
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    /// The instance in force, or `None` before one has been installed.
    pub fn get(&self) -> Option<std::sync::Arc<FsAccess>> {
        self.current.read().ok().and_then(|g| g.clone())
    }

    fn replace(&self, built: FsAccess) {
        if let Ok(mut guard) = self.current.write() {
            // The outgoing instance drops here, taking its session temp
            // directory with it (FSA-FR-20) — unless another thread still holds
            // an `Arc` to it, in which case it drops when that last reference
            // does. Either way no directory outlives the instance that owns it.
            *guard = Some(std::sync::Arc::new(built));
        }
    }
}

/// The reason a grant was refused, for a build error that carries one.
fn refusal_of(error: &FsAccessBuildError) -> super::error::GrantRefusal {
    match error {
        FsAccessBuildError::GrantRefused { reason, .. } => *reason,
        _ => super::error::GrantRefusal::Unreadable,
    }
}
