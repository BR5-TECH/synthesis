//! User-global settings storage — the backend half of
//! `specifications/ui/GLS-global-settings.md`, specified by
//! `specifications/core/GSS-global-settings-storage.md`.
//!
//! This module owns the user-global store: app preferences (theme), the
//! recent-projects list, the installed-plugin registry, and the
//! installed-adapter registry. It is **disk-backed**: state is persisted to
//! `app_data_dir()/synthesis.toml` (GSS-FR-01), written atomically through the
//! `FSA-filesystem-access.md` primitive (GSS-FR-03), and a missing or malformed
//! file is repaired to defaults rather than panicking (GSS-FR-13).
//!
//! The MRU append on a successful project open/create (GSS-FR-10 / PST-FR-06)
//! is performed by `record_recent_project`, called from the open/create
//! commands in `lib.rs`; this module owns persistence and the listing /
//! management surface.
//!
//! Tauri-free by design: the pure data types and the ordering/pruning/capping
//! logic live here so they are unit-testable without a Tauri runtime. Tests use
//! `GlobalSettingsStore::in_memory()` / `::with_path()` so they never touch the
//! real `app_data_dir()`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::fs;

mod preferences;
pub use preferences::*;

/// A recent-projects entry (GSS-FR-05).
///
/// Wire shape is camelCase. The timestamp field is `lastOpenedAt` — the
/// established wire name already consumed by the Project picker
/// (`PPK-project-picker.md`); `pinned` and `missing` are additive. Keeping the
/// existing name (rather than the spec's abstract `last_opened`) preserves the
/// picker contract; the two new flags are ignored by the picker, which remains
/// view-and-open only.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct RecentProjectEntry {
    pub name: String,
    pub path: String,
    /// ISO-8601 UTC instant of the last open. Lexicographically sortable, so
    /// string comparison yields chronological order.
    pub last_opened_at: String,
    pub pinned: bool,
    /// `true` when the project's `.synthesis/project.toml` no longer exists on
    /// disk. Only ever surfaced for pinned entries (GSS-FR-06): unpinned
    /// missing entries are pruned before listing.
    pub missing: bool,
}

/// An entry in the user-global installed-plugin registry (GSS-FR-11).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct InstalledPlugin {
    pub id: String,
    pub name: String,
    pub source: String,
}

/// An entry in the user-global installed-adapter registry (GSS-FR-12).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct InstalledAdapter {
    pub id: String,
    pub name: String,
    pub source: String,
}

/// The per-project slot (GSS-FR-16 / GSS-FR-17 / GSS-FR-18).
///
/// Two facts about a project that cannot live inside the project on disk.
/// `.synthesis/` is committed content, so a repository holds one copy per
/// worktree — and both of these have to be stable across worktrees:
///
/// - **layout** (GSS-FR-17): panel sizing/side/collapse/hide plus the main
///   window's geometry. Per-worktree storage would reshuffle the shell's shape
///   on every switch.
/// - **active worktree** (GSS-FR-16): which checkout roots the project. It has
///   to be readable *before* any worktree has been chosen, so it cannot live in
///   the worktree it names.
///
/// The slot is keyed by project (the repository's primary worktree, or the
/// project root when it is not in a repository), never by directory, so a
/// repository with several worktrees shares one slot (GSS-FR-18).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ProjectSlot {
    /// Absolute path of the worktree the project was last working in
    /// (GSS-FR-16). `None` until a worktree has been activated.
    ///
    /// Declared before `layout` deliberately: TOML requires a table's scalar
    /// values to be emitted before its sub-tables, and serde serialises fields
    /// in declaration order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_worktree: Option<String>,
    /// GSS-FR-23: the id of the GitHub token registry record this project's
    /// authenticated operations use. `None` until one is chosen.
    ///
    /// User-global rather than project-local because `.synthesis/project.toml`
    /// is committed content — a binding stored there would carry one author's
    /// choice to everyone who clones the repository. Like the two fields
    /// around it, it is a scalar and so must stay declared *before* `layout`:
    /// TOML requires a table's scalars to be emitted before its sub-tables.
    ///
    /// This module stores whatever id it is given and never checks that the
    /// record still exists; a binding pointing at a removed record is resolved
    /// by `crate::github_tokens` (GTS-FR-10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_token_id: Option<String>,
    /// GSS-FR-26: the vendor whose agentic integration this project uses in
    /// place of the user-global active one. `None` until one is chosen.
    ///
    /// User-global for the same reason as `github_token_id` above:
    /// `.synthesis/project.toml` is committed content, and an override stored
    /// there would carry one author's choice of local tooling to everyone who
    /// clones the repository. Also a scalar, so it must stay declared *before*
    /// `layout` — TOML requires a table's scalars to be emitted before its
    /// sub-tables.
    ///
    /// Stored as given and never checked for usability; an override naming a
    /// cleared or unverified integration is resolved by `crate::agentic`
    /// (AIC-FR-16).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agentic_vendor: Option<String>,
    /// GSS-FR-28: the provider whose AI API integration this project uses in
    /// place of the user-global active one. `None` until one is chosen.
    ///
    /// Independent of `agentic_vendor` above: a project may override one level,
    /// both, or neither, so the two are separate fields rather than one
    /// combined choice. Another scalar, so it too stays declared *before*
    /// `layout`.
    ///
    /// Stored as given and never checked for usability; an override naming a
    /// cleared or unverified provider is resolved by `crate::ai_api`
    /// (AAP-FR-16).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_api_provider: Option<String>,
    /// GSS-FR-31: the ids of the conversational agents this project may address,
    /// empty until one is enrolled.
    ///
    /// User-global rather than project-local for two reasons: it names records
    /// that exist only on this machine, and `.synthesis/project.toml` is
    /// committed content one clone would carry to everyone. Keyed by project
    /// (GSS-FR-18), so it is one enrolment for the repository however many
    /// worktrees it has.
    ///
    /// A TOML array of plain strings, which is a *value* rather than a
    /// sub-table, so it may sit among the scalars above — unlike `layout`
    /// below, which must stay last.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub enrolled_agents: Vec<String>,
    /// GSS-FR-CDYK: the reference documents this project selects — an ordered
    /// list of `{ kind, path }`, empty until a source is added. References only:
    /// never a document's content, its extracted text, or an index.
    ///
    /// User-global because a selected path may lie outside the project, and keyed
    /// by project (GSS-FR-18, GSS-FR-ZWJW) so every worktree of a repository reads
    /// the same list. An array of tables, so it stays after every scalar field
    /// and beside `layout`, which TOML also requires to come last.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub document_sources: Vec<crate::documents::StoredSource>,
    /// Layout preferences (GSS-FR-17). `None` until the shell first persists.
    /// Skipped when absent — the TOML serialiser rejects a bare `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<crate::layout::LayoutPreferences>,
}

/// The on-disk shape of `synthesis.toml` (GSS-FR-02). `#[serde(default)]` makes
/// every field optional, so a partial or forward-versioned file deserialises
/// with the missing pieces defaulted — the logic-level half of GSS-FR-13.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PersistedState {
    /// GSS-FR-14: which single vendor is the user-global active agentic
    /// integration, across both kinds, or `None` when none has been activated
    /// explicitly.
    ///
    /// Declared first deliberately: the two `active_*` fields are the only
    /// scalars at this level, and TOML requires a table's scalar values to be
    /// emitted before its sub-tables. Serde serialises fields in declaration
    /// order, so a scalar declared after `app_preferences` would fail to
    /// serialise at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_agentic: Option<String>,
    /// GSS-FR-27: which single provider is the user-global active AI API
    /// integration, or `None` when none has been activated explicitly. Kept
    /// beside `active_agentic` for the scalar-ordering reason above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_ai_api: Option<String>,
    pub app_preferences: AppPreferences,
    pub recent_projects: Vec<RecentProjectEntry>,
    pub installed_plugins: Vec<InstalledPlugin>,
    pub installed_adapters: Vec<InstalledAdapter>,
    /// GSS-FR-22: the GitHub token registry — one *descriptive* record per
    /// stored token. No secret is present here in any form; every record's
    /// secret lives in the OS keychain under the record's id
    /// (`crate::github_tokens`, GTS-FR-01), which is what keeps this file
    /// readable, copyable, and attachable to a bug report without disclosing a
    /// credential. This module persists the records and interprets none of them.
    pub github_tokens: Vec<crate::github_tokens::GithubTokenRecord>,
    /// GSS-FR-14: the agentic integration registry — one record per
    /// *configured* vendor, carrying the configuration fields of that vendor's
    /// kind (a binary path and how it was arrived at for a CLI, a base URL and
    /// the masked hint of its key for an API agent), the version and timestamp
    /// of its last successful verification, its model list and the origin of
    /// that list, and the model and effort chosen for it.
    ///
    /// No key is present here in any form: a CLI-kind backend authenticates
    /// itself, and an API-kind backend's key lives in the OS keychain under its
    /// vendor id (`crate::agentic`, AIC-FR-20) — which is what keeps this file
    /// readable, copyable, and attachable to a bug report. This module persists
    /// the records and interprets none of them: detection, verification, model
    /// probing, and the single-active rule are `crate::agentic`'s.
    pub agentic_integrations: Vec<crate::agentic::AgenticRecord>,
    /// GSS-FR-27: the AI API provider registry — one *descriptive* record per
    /// configured provider, carrying its base URL, the masked hint of its key,
    /// the timestamp of its last successful verification, its model list and
    /// the origin of that list, and the model chosen for it.
    ///
    /// No key is present here in any form; every configured provider's key
    /// lives in the OS keychain under the provider's id (`crate::ai_api`,
    /// AAP-FR-07), exactly as a GitHub token's secret does. This module
    /// persists the records and interprets none of them: verification, model
    /// probing, and the single-active rule are `crate::ai_api`'s.
    pub ai_api_integrations: Vec<crate::ai_api::AiApiRecord>,
    /// GSS-FR-30: the conversational agent registry — one record per persona the
    /// author has described, carrying its id, nickname, AI API provider, model,
    /// reasoning choice, and instructions (`crate::agents`, AGR-FR-01).
    ///
    /// User-global because a persona describes how its author wants to be argued
    /// with rather than anything about a repository. It holds no secret at all:
    /// an agent *names* a provider, and that provider's key lives in the OS
    /// keychain, so this file gains no credential by gaining an agent. Another
    /// array of tables, so it stays beside the registries above and before the
    /// two project tables below.
    pub agents: Vec<crate::agents::Agent>,
    /// GSS-FR-35: the **Docker backend** this machine reaches a container
    /// runtime through — the selected mode, the Docker Engine endpoint, the
    /// Docker CLI executable path, and the verification those three values
    /// earned (GSS-FR-39).
    ///
    /// User-global because how this machine reaches Docker is a fact about the
    /// machine rather than about any project: it is never written into a
    /// project's `.synthesis/project.toml` or into any other committed project
    /// file, whatever project is open. It holds no credential — a registry
    /// login is not something this application performs at all (PSS-FR-26) —
    /// so it costs `synthesis.toml` none of its readability.
    ///
    /// One table, so it is declared beside the registries above and before the
    /// two project tables below.
    pub docker_backend: crate::docker::DockerBackendRecord,
    /// The user-global relay endpoint and the validation state it last earned
    /// (GSS-FR-ZKQT). One table, declared beside `docker_backend` and before the
    /// two project tables below. It holds no token and no key (GSS-FR-VMRB).
    pub relay_endpoint: crate::relay_endpoint::RelayEndpointRecord,
    /// Per-project slots, keyed by project (GSS-FR-18). A `BTreeMap` so the
    /// serialised file has a stable key order rather than churning on save.
    pub projects: std::collections::BTreeMap<String, ProjectSlot>,
    /// The slot used when no project is open (`PSS-project-settings-storage.md`
    /// PSS-FR-06), so the first project opened inherits a sensible layout.
    pub default_project: ProjectSlot,
}

mod recents;
pub use recents::*;

/// Disk-backed user-global store (GSS-FR-01/02/03). State is persisted to
/// `path` (when set) on every mutation; when `path` is `None` the store is
/// purely in-memory (tests only).
pub struct GlobalSettingsStore {
    state: Mutex<PersistedState>,
    /// Skeleton stand-in for the currently-open project's plugin enablement,
    /// which is really owned by `PSS-project-settings-storage.md`. Project
    /// state, so it is NOT persisted into the user-global `synthesis.toml`;
    /// modelled here only so the `uninstall_plugin` cascade (GSS-FR-11) is
    /// exercisable end-to-end without the project-settings module.
    open_project_enabled_plugins: Mutex<HashSet<String>>,
    /// Persistence target. `None` => in-memory only.
    path: Option<PathBuf>,
    /// The access governing that target (FSA-FR-19). The user-global store
    /// lives under `app_data_dir()`, outside any project, so it carries its own
    /// instance rooted there rather than borrowing the project's — which is
    /// also what lets it answer on a fresh machine with no project open
    /// (GSS-FR-01, per FSA-FR-21's pre-project instance).
    access: Option<crate::fs::RootFs>,
}

impl Default for GlobalSettingsStore {
    /// Production constructor: persist to `app_data_dir()/synthesis.toml`,
    /// loading any existing state. If the data dir cannot be resolved the store
    /// degrades to in-memory rather than failing app startup.
    fn default() -> Self {
        match fs::app_data_dir() {
            Ok(dir) => Self::with_path(dir.join("synthesis.toml")),
            Err(e) => {
                eprintln!(
                    "synthesis: app_data_dir() unavailable, global settings will not persist: {e}"
                );
                Self::in_memory()
            }
        }
    }
}

impl GlobalSettingsStore {
    /// In-memory store, no persistence. Test/fallback constructor.
    pub fn in_memory() -> Self {
        Self {
            state: Mutex::new(PersistedState::default()),
            open_project_enabled_plugins: Mutex::new(HashSet::new()),
            path: None,
            access: None,
        }
    }

    /// Disk-backed store rooted at `path`, loading existing state (or defaults
    /// if the file is missing or malformed — GSS-FR-13).
    pub fn with_path(path: PathBuf) -> Self {
        // The store's own scope is the directory holding `synthesis.toml`.
        // Created first: `FsAccess` cannot be built over a directory that does
        // not exist (FSA-FR-18), and a store that failed to build one would
        // hold a path it could never write to — silently, since `persist`
        // returns `Ok(())` when there is no access. Previously the directory
        // came into being with the first write via FSA-FR-05; now it has to
        // exist before the instance that would perform that write.
        let access = path.parent().and_then(|dir| {
            // FSA-FR-19 exception, deliberate and narrow: this is the bootstrap.
            // An `FsAccess` cannot be built over a directory that does not
            // exist (FSA-FR-18), and this store's scope IS that directory, so
            // there is no instance available to create it with — the same
            // chicken-and-egg `app_data_dir()` resolves by creating its own
            // directory as a free function. One `create_dir_all` on the store's
            // own root, before any instance exists; nothing is read or written.
            let _ = std::fs::create_dir_all(dir);
            crate::fs::FsAccess::builder()
                .allow_root(dir)
                .build()
                .ok()
                .map(|a| crate::fs::RootFs::new(dir.to_path_buf(), std::sync::Arc::new(a)))
        });
        let state = access
            .as_ref()
            .map(|a| load_or_default(a, &path))
            .unwrap_or_default();
        Self {
            state: Mutex::new(state),
            open_project_enabled_plugins: Mutex::new(HashSet::new()),
            path: Some(path),
            access,
        }
    }

    /// Write the current state to disk atomically (GSS-FR-03). No-op for an
    /// in-memory store.
    fn persist(&self, state: &PersistedState) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let Some(access) = &self.access else {
            return Ok(());
        };
        access
            .write_toml_atomic(path, state)
            .map_err(|e| format!("failed to persist synthesis.toml: {e}"))
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, PersistedState>, String> {
        self.state
            .lock()
            .map_err(|e| format!("global settings store poisoned: {e}"))
    }

    // -- App preferences (GSS-FR-04) -------------------------------------

    pub fn load_app_preferences(&self) -> Result<AppPreferences, String> {
        Ok(self.lock()?.app_preferences.clone())
    }

    /// GSS-FR-20: a whole-record write. The stored record is *replaced* by
    /// `prefs`, so every caller supplies the fields it is not editing rather
    /// than relying on a per-field merge here. Keeping the write total is what
    /// makes the stored value unambiguous — a merge would leave "the caller
    /// omitted it" and "the caller cleared it" indistinguishable for a `bool`.
    pub fn save_app_preferences(&self, prefs: AppPreferences) -> Result<(), String> {
        let mut state = self.lock()?;
        state.app_preferences = prefs;
        self.persist(&state)
    }

    // -- Recent projects (GSS-FR-05..10) ---------------------------------

    pub fn list_recent_projects(&self) -> Result<Vec<RecentProjectEntry>, String> {
        Ok(ordered_visible_recents(&self.lock()?.recent_projects))
    }

    /// MRU append on a successful project open/create (GSS-FR-10 / PST-FR-06).
    /// Upserts by `path`: an existing entry is moved to the front with a fresh
    /// timestamp (its pinned flag preserved); a new project is inserted. The
    /// project just opened exists, so `missing` is cleared.
    ///
    /// Best-effort persistence: a failed disk write is logged but never fails
    /// the open/create that triggered it.
    pub fn record_recent_project(&self, name: &str, path: &str) -> Result<(), String> {
        let mut state = self.lock()?;
        let pinned = state
            .recent_projects
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.pinned)
            .unwrap_or(false);
        state.recent_projects.retain(|e| e.path != path);
        // Insert at the front so the vec stays most-recent-first. This also
        // makes ordering correct when two opens land in the same wall-clock
        // second (equal `last_opened_at`): the stable sort in
        // `ordered_visible_recents` then keeps the more-recently-recorded entry
        // ahead, because it sits earlier in the vec.
        state.recent_projects.insert(
            0,
            RecentProjectEntry {
                name: name.to_string(),
                path: path.to_string(),
                last_opened_at: now_iso8601(),
                pinned,
                missing: false,
            },
        );
        if let Err(e) = self.persist(&state) {
            eprintln!("synthesis: {e}");
        }
        Ok(())
    }

    pub fn remove_recent_project(&self, path: &str) -> Result<(), String> {
        let mut state = self.lock()?;
        state.recent_projects.retain(|e| e.path != path);
        self.persist(&state)
    }

    pub fn clear_recent_projects(&self) -> Result<(), String> {
        let mut state = self.lock()?;
        state.recent_projects.clear();
        self.persist(&state)
    }

    pub fn pin_recent_project(&self, path: &str) -> Result<(), String> {
        self.set_pinned(path, true)
    }

    pub fn unpin_recent_project(&self, path: &str) -> Result<(), String> {
        self.set_pinned(path, false)
    }

    fn set_pinned(&self, path: &str, pinned: bool) -> Result<(), String> {
        let mut state = self.lock()?;
        for entry in state.recent_projects.iter_mut() {
            if entry.path == path {
                entry.pinned = pinned;
            }
        }
        self.persist(&state)
    }

    // -- Plugin registry (GSS-FR-11) -------------------------------------

    pub fn list_installed_plugins(&self) -> Result<Vec<InstalledPlugin>, String> {
        Ok(self.lock()?.installed_plugins.clone())
    }

    pub fn install_plugin(&self, source: &str) -> Result<InstalledPlugin, String> {
        let source = source.trim();
        if source.is_empty() {
            return Err("plugin source is empty".into());
        }
        let id = derive_id_from_source(source);
        if id.is_empty() {
            return Err("could not derive plugin id from source".into());
        }
        let plugin = InstalledPlugin {
            id: id.clone(),
            name: id,
            source: source.to_string(),
        };
        let mut state = self.lock()?;
        if let Some(existing) = state
            .installed_plugins
            .iter_mut()
            .find(|p| p.id == plugin.id)
        {
            *existing = plugin.clone();
        } else {
            state.installed_plugins.push(plugin.clone());
        }
        self.persist(&state)?;
        Ok(plugin)
    }

    /// Remove a plugin from the user-global registry and cascade a disable into
    /// the currently-open project so the registry never references a missing
    /// plugin via stale enablement (GSS-FR-11).
    pub fn uninstall_plugin(&self, id: &str) -> Result<(), String> {
        {
            let mut state = self.lock()?;
            state.installed_plugins.retain(|p| p.id != id);
            self.persist(&state)?;
        }
        // Cascade: drop any open-project enablement for the uninstalled id.
        let mut enabled = self
            .open_project_enabled_plugins
            .lock()
            .map_err(|e| format!("open-project enablement poisoned: {e}"))?;
        enabled.remove(id);
        Ok(())
    }

    // -- Adapter registry (GSS-FR-12) ------------------------------------

    pub fn list_agent_adapters(&self) -> Result<Vec<InstalledAdapter>, String> {
        Ok(self.lock()?.installed_adapters.clone())
    }

    pub fn install_adapter(&self, source: &str) -> Result<InstalledAdapter, String> {
        let source = source.trim();
        if source.is_empty() {
            return Err("adapter source is empty".into());
        }
        let id = derive_id_from_source(source);
        if id.is_empty() {
            return Err("could not derive adapter id from source".into());
        }
        let adapter = InstalledAdapter {
            id: id.clone(),
            name: id,
            source: source.to_string(),
        };
        let mut state = self.lock()?;
        if let Some(existing) = state
            .installed_adapters
            .iter_mut()
            .find(|a| a.id == adapter.id)
        {
            *existing = adapter.clone();
        } else {
            state.installed_adapters.push(adapter.clone());
        }
        self.persist(&state)?;
        Ok(adapter)
    }

    // -- GitHub token registry (GSS-FR-22) -------------------------------

    /// The descriptive records of every stored GitHub token. Reads the
    /// user-global store only: no keychain access, no network.
    pub fn load_github_token_registry(
        &self,
    ) -> Result<Vec<crate::github_tokens::GithubTokenRecord>, String> {
        Ok(self.lock()?.github_tokens.clone())
    }

    /// Replace the registry wholesale and persist. A whole-list write for the
    /// same reason `save_app_preferences` is a whole-record write (GSS-FR-20):
    /// the caller (`crate::github_tokens`) owns the list's contents, so a merge
    /// here could not tell a removal from an omission.
    pub fn save_github_token_registry(
        &self,
        records: Vec<crate::github_tokens::GithubTokenRecord>,
    ) -> Result<(), String> {
        let mut state = self.lock()?;
        state.github_tokens = records;
        self.persist(&state)
    }

    // -- Agentic integration registry (GSS-FR-14) ------------------------

    /// The configured agentic integrations and which one is active. Reads the
    /// user-global store only: no filesystem probe, no child process, no
    /// keychain, no network — `crate::agentic` derives every live fact from
    /// these.
    pub fn load_agentic_registry(
        &self,
    ) -> Result<(Vec<crate::agentic::AgenticRecord>, Option<String>), String> {
        let state = self.lock()?;
        Ok((
            state.agentic_integrations.clone(),
            state.active_agentic.clone(),
        ))
    }

    /// Replace the registry and the active choice together, and persist. Both
    /// in one call because they are one fact: a save that wrote the records
    /// while another writer changed the active vendor could leave a vendor
    /// active that the records no longer carry.
    ///
    /// A whole-list write for the same reason `save_github_token_registry` is
    /// one — the caller owns the list's contents, so a merge here could not
    /// tell a removal from an omission.
    pub fn save_agentic_registry(
        &self,
        records: Vec<crate::agentic::AgenticRecord>,
        active_vendor: Option<String>,
    ) -> Result<(), String> {
        let mut state = self.lock()?;
        state.agentic_integrations = records;
        state.active_agentic = active_vendor;
        self.persist(&state)
    }

    // -- AI API provider registry (GSS-FR-27) ----------------------------

    /// The configured API providers and which one is active. Descriptions
    /// only: no key is part of this store, so this never touches the keychain
    /// (AAP-FR-07).
    pub fn load_ai_api_registry(
        &self,
    ) -> Result<(Vec<crate::ai_api::AiApiRecord>, Option<String>), String> {
        let state = self.lock()?;
        Ok((
            state.ai_api_integrations.clone(),
            state.active_ai_api.clone(),
        ))
    }

    /// Replace the API registry and its active choice together, and persist —
    /// one call for the same reason `save_agentic_registry` takes both.
    pub fn save_ai_api_registry(
        &self,
        records: Vec<crate::ai_api::AiApiRecord>,
        active_provider: Option<String>,
    ) -> Result<(), String> {
        let mut state = self.lock()?;
        state.ai_api_integrations = records;
        state.active_ai_api = active_provider;
        self.persist(&state)
    }

    // -- Conversational agents (GSS-FR-30 / GSS-FR-31) ---------------------

    /// GSS-FR-30: the described personas. Descriptions only — no key is part of
    /// this store, so this never touches the keychain.
    pub fn load_agent_registry(&self) -> Result<Vec<crate::agents::Agent>, String> {
        Ok(self.lock()?.agents.clone())
    }

    /// GSS-FR-30 / GSS-FR-31: replace the registry and persist, pruning from
    /// every project slot any enrolment naming an agent the new registry does
    /// not hold.
    ///
    /// The prune is part of this call rather than a separate one because
    /// GSS-FR-31 states the invariant as a property of the file, not of a
    /// sequence: an id naming a record the registry no longer holds is *not
    /// written*. `crate::agents::delete_agent` (AGR-FR-11) therefore removes a
    /// definition and every enrolment of it in one operation, and no
    /// interleaved reader can observe a slot pointing at a deleted agent.
    pub fn save_agent_registry(&self, agents: Vec<crate::agents::Agent>) -> Result<(), String> {
        let mut state = self.lock()?;
        let live: HashSet<&str> = agents.iter().map(|a| a.id.as_str()).collect();
        for slot in state.projects.values_mut() {
            slot.enrolled_agents.retain(|id| live.contains(id.as_str()));
        }
        state
            .default_project
            .enrolled_agents
            .retain(|id| live.contains(id.as_str()));
        state.agents = agents;
        self.persist(&state)
    }

    /// GSS-FR-31: the agents this project may address. An empty set for a
    /// project that has enrolled none, which is an ordinary state.
    // -- Docker backend (GSS-FR-35) --------------------------------------

    /// GSS-FR-35: the stored Docker backend record. Reads the store; runs no
    /// executable and reaches no daemon.
    pub fn load_docker_backend(&self) -> Result<crate::docker::DockerBackendRecord, String> {
        Ok(self.lock()?.docker_backend.clone())
    }

    /// GSS-FR-39: write the selection, keeping the value the unselected mode
    /// last carried and dropping a success the new selection no longer matches.
    ///
    /// The verification itself is never written here — only `record_docker_verification`
    /// does that, and only after a daemon has answered.
    pub fn save_docker_backend(
        &self,
        config: &crate::docker::DockerBackendConfig,
    ) -> Result<crate::docker::DockerBackendRecord, String> {
        let mut state = self.lock()?;
        state.docker_backend.apply(config);
        let record = state.docker_backend.clone();
        self.persist(&state)?;
        Ok(record)
    }

    /// GSS-FR-39: record a success **against the selection that earned it**.
    pub fn record_docker_verification(
        &self,
        config: &crate::docker::DockerBackendConfig,
        server_version: &str,
        at: &str,
    ) -> Result<crate::docker::DockerBackendRecord, String> {
        let mut state = self.lock()?;
        state
            .docker_backend
            .record_success(config, server_version, at);
        let record = state.docker_backend.clone();
        self.persist(&state)?;
        Ok(record)
    }

    // -- Relay endpoint (GSS-FR-ZKQT) ------------------------------------

    /// GSS-FR-ZKQT: the stored relay endpoint record. Reads the store and
    /// reaches no network.
    pub fn load_relay_endpoint(
        &self,
    ) -> Result<crate::relay_endpoint::RelayEndpointRecord, String> {
        Ok(self.lock()?.relay_endpoint.clone())
    }

    /// GSS-FR-NLDC: write the URL, dropping a success the new URL no longer
    /// matches. The verification itself is never written here — only
    /// `record_relay_verification` does that, and only after a relay answered.
    pub fn save_relay_endpoint(
        &self,
        url: &str,
    ) -> Result<crate::relay_endpoint::RelayEndpointRecord, String> {
        let mut state = self.lock()?;
        state.relay_endpoint.apply(url);
        let record = state.relay_endpoint.clone();
        self.persist(&state)?;
        Ok(record)
    }

    /// GSS-FR-NLDC: record a success **against the URL that earned it**.
    pub fn record_relay_verification(
        &self,
        url: &str,
        version: &str,
        capabilities: &[String],
        at: &str,
    ) -> Result<crate::relay_endpoint::RelayEndpointRecord, String> {
        let mut state = self.lock()?;
        state
            .relay_endpoint
            .record_success(url, version, capabilities, at);
        let record = state.relay_endpoint.clone();
        self.persist(&state)?;
        Ok(record)
    }

    pub fn load_project_agent_enrolment(&self, key: &str) -> Result<Vec<String>, String> {
        let state = self.lock()?;
        Ok(Self::slot(&state, key).enrolled_agents)
    }

    /// GSS-FR-31: remember which agents this project may address. Keyed by
    /// project (GSS-FR-18), so one enrolment serves every worktree of a
    /// repository and survives a change of active worktree.
    pub fn save_project_agent_enrolment(
        &self,
        key: &str,
        agent_ids: Vec<String>,
    ) -> Result<(), String> {
        self.update_slot(key, |slot| slot.enrolled_agents = agent_ids.clone())
    }

    /// GSS-FR-CDYK: the project's document sources in the order they were first
    /// added, empty when none was. Never checked against the disk here
    /// (GSS-FR-ZWJW).
    pub fn load_document_sources(
        &self,
        key: &str,
    ) -> Result<Vec<crate::documents::StoredSource>, String> {
        let state = self.lock()?;
        Ok(Self::slot(&state, key).document_sources)
    }

    /// GSS-FR-CDYK / GSS-FR-ZWJW: remember which documents this project selects.
    /// Keyed by project (GSS-FR-18), so one list serves every worktree of a
    /// repository and survives a change of active worktree. The paths are stored
    /// as given; this module does not check that they exist.
    pub fn save_document_sources(
        &self,
        key: &str,
        sources: Vec<crate::documents::StoredSource>,
    ) -> Result<(), String> {
        self.update_slot(key, |slot| slot.document_sources = sources.clone())
    }

    // -- Per-project slot (GSS-FR-16 / GSS-FR-17 / GSS-FR-18 / GSS-FR-23) --

    /// GSS-FR-23: the id of the GitHub token record this project uses, or
    /// `None` when none has been chosen. Never validated against the registry
    /// here — resolution is `crate::github_tokens`' concern (GTS-FR-10).
    pub fn load_github_token_binding(&self, key: &str) -> Result<Option<String>, String> {
        let state = self.lock()?;
        Ok(Self::slot(&state, key).github_token_id)
    }

    /// GSS-FR-23: remember which stored token this project authenticates with.
    /// Keyed by project (GSS-FR-18), so one binding serves every worktree of a
    /// repository and survives a change of active worktree.
    pub fn save_github_token_binding(&self, key: &str, token_id: &str) -> Result<(), String> {
        self.update_slot(key, |slot| {
            slot.github_token_id = Some(token_id.to_string())
        })
    }

    /// GSS-FR-26: the agentic integration this project overrides to, or `None`
    /// when it inherits the user-global choice. Never validated against the
    /// registry here — resolution is `crate::agentic`'s concern (AIC-FR-16).
    pub fn load_agentic_override(&self, key: &str) -> Result<Option<String>, String> {
        let state = self.lock()?;
        Ok(Self::slot(&state, key).agentic_vendor)
    }

    /// GSS-FR-26: remember which agentic integration this project uses. Keyed
    /// by project (GSS-FR-18), so one override serves every worktree of a
    /// repository and survives a change of active worktree.
    pub fn save_agentic_override(&self, key: &str, vendor: &str) -> Result<(), String> {
        self.update_slot(key, |slot| slot.agentic_vendor = Some(vendor.to_string()))
    }

    /// GSS-FR-26: return the project to inheriting the user-global agentic
    /// choice.
    pub fn clear_agentic_override(&self, key: &str) -> Result<(), String> {
        self.update_slot(key, |slot| slot.agentic_vendor = None)
    }

    /// GSS-FR-28: the AI API integration this project overrides to, or `None`
    /// when it inherits the user-global choice. Independent of the agentic
    /// override above, and likewise never validated here — resolution is
    /// `crate::ai_api`'s concern (AAP-FR-16).
    pub fn load_ai_api_override(&self, key: &str) -> Result<Option<String>, String> {
        let state = self.lock()?;
        Ok(Self::slot(&state, key).ai_api_provider)
    }

    /// GSS-FR-28: remember which API provider this project uses. Keyed by
    /// project (GSS-FR-18), like every other per-project fact.
    pub fn save_ai_api_override(&self, key: &str, provider: &str) -> Result<(), String> {
        self.update_slot(key, |slot| {
            slot.ai_api_provider = Some(provider.to_string())
        })
    }

    /// GSS-FR-28: return the project to inheriting the user-global API choice.
    pub fn clear_ai_api_override(&self, key: &str) -> Result<(), String> {
        self.update_slot(key, |slot| slot.ai_api_provider = None)
    }

    /// Read a project's slot, or the default slot when `key` is empty — which
    /// is what `PSS-project-settings-storage.md` PSS-FR-06 falls back to while
    /// no project is open.
    fn slot(state: &PersistedState, key: &str) -> ProjectSlot {
        if key.is_empty() {
            return state.default_project.clone();
        }
        state.projects.get(key).cloned().unwrap_or_default()
    }

    /// Mutate a project's slot in place and persist. An empty `key` targets the
    /// no-project-open default slot.
    fn update_slot<F>(&self, key: &str, apply: F) -> Result<(), String>
    where
        F: FnOnce(&mut ProjectSlot),
    {
        let mut state = self.lock()?;
        if key.is_empty() {
            apply(&mut state.default_project);
        } else {
            apply(state.projects.entry(key.to_string()).or_default());
        }
        self.persist(&state)
    }

    /// GSS-FR-17: the project's layout preferences, backing
    /// `"load layout preferences"` (PSS-FR-06).
    pub fn load_project_layout(
        &self,
        key: &str,
    ) -> Result<Option<crate::layout::LayoutPreferences>, String> {
        let state = self.lock()?;
        Ok(Self::slot(&state, key).layout)
    }

    /// GSS-FR-17: persist the project's layout preferences. Nothing here is
    /// ever written into a project's `.synthesis/`, which is what keeps the
    /// shell's shape stable as the active worktree changes.
    pub fn save_project_layout(
        &self,
        key: &str,
        prefs: crate::layout::LayoutPreferences,
    ) -> Result<(), String> {
        self.update_slot(key, |slot| slot.layout = Some(prefs))
    }

    /// GSS-FR-16: the worktree this project was last working in, or `None` when
    /// none has been recorded.
    pub fn load_active_worktree(&self, key: &str) -> Result<Option<String>, String> {
        let state = self.lock()?;
        Ok(Self::slot(&state, key).active_worktree)
    }

    /// GSS-FR-16: remember the project's active worktree. Written whenever it
    /// changes, and read back when the project is next opened.
    pub fn save_active_worktree(&self, key: &str, path: &str) -> Result<(), String> {
        self.update_slot(key, |slot| slot.active_worktree = Some(path.to_string()))
    }

    // -- Open-project enablement (skeleton stand-in for PSS) --------------

    /// Model the open project enabling a plugin. Real ownership is
    /// `PSS-project-settings-storage.md`; modelled here only to exercise the
    /// `uninstall_plugin` cascade (GSS-FR-11).
    pub fn set_open_project_plugin_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        let mut guard = self
            .open_project_enabled_plugins
            .lock()
            .map_err(|e| format!("open-project enablement poisoned: {e}"))?;
        if enabled {
            guard.insert(id.to_string());
        } else {
            guard.remove(id);
        }
        Ok(())
    }

    pub fn is_open_project_plugin_enabled(&self, id: &str) -> Result<bool, String> {
        let guard = self
            .open_project_enabled_plugins
            .lock()
            .map_err(|e| format!("open-project enablement poisoned: {e}"))?;
        Ok(guard.contains(id))
    }

    /// Seed the recents list directly. Test helper.
    #[cfg(test)]
    pub fn replace_recents(&self, entries: Vec<RecentProjectEntry>) {
        self.state.lock().unwrap().recent_projects = entries;
    }
}

/// Read `synthesis.toml` into a `PersistedState`, or return defaults when the
/// file is missing or malformed (GSS-FR-13: repair, never panic).
fn load_or_default(access: &crate::fs::RootFs, path: &Path) -> PersistedState {
    match access.read_toml::<PersistedState>(path) {
        Ok(state) => state,
        Err(fs::FsError::NotFound { .. }) => PersistedState::default(),
        Err(e) => {
            eprintln!("synthesis: malformed/unreadable {path:?}, repairing to defaults: {e}");
            PersistedState::default()
        }
    }
}

#[cfg(test)]
mod tests;
