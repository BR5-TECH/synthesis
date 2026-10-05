//! Project-local settings storage
//! (`specifications/core/PSS-project-settings-storage.md`).
//!
//! The project-local store — `<project_root>/.synthesis/local.toml`, gitignored
//! (PSS-FR-03) — holds the per-machine state for the open project: the
//! **Changes panel state**, the panel's selected mode and its configured target
//! branch (PSS-FR-15), and the **Library panel state**, which folders the tree
//! renders expanded plus its two local filters (PSS-FR-18). Both are kept out of
//! the committed `project.toml` so each machine — and each worktree — keeps its
//! own.
//!
//! The project-public store — `<project_root>/.synthesis/project.toml`,
//! committed (PSS-FR-02) — holds the conventions everyone who checks the
//! project out shares: the **line-ending convention** (PSS-FR-17), the optional
//! **draft template** (PSS-FR-21), and the **Docker image configuration** one
//! entry per agentic CLI vendor (PSS-FR-22), which `images` owns.
//!
//! Every write goes through the FSA atomic-write primitive (PSS-FR-04), and a
//! missing or malformed project-local file is repaired to defaults rather than
//! surfaced as an error (PSS-FR-10).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::project::ProjectState;

pub mod concurrency;
pub mod github_polling;
pub mod image_builder;
pub mod images;
pub mod loop_settings;
pub mod public_store;
pub mod publication_settings;
pub use concurrency::GraduationConcurrency;
pub use public_store::*;
pub use github_polling::*;
pub use publication_settings::*;
use public_store::{load_public, save_public};

// ---------------------------------------------------------------------------
// Wire + on-disk shapes
// ---------------------------------------------------------------------------

/// Which comparison the Changes panel is showing (CHG-FR-02).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangesMode {
    /// The working tree against `HEAD` — the default on a project with no
    /// persisted state (PSS-FR-15).
    #[default]
    Uncommitted,
    /// The current branch against a configured target branch.
    Branch,
}

/// The Changes panel's persisted state (PSS-FR-15).
///
/// This store never validates that `target_branch` still exists; a branch that
/// was deleted since it was persisted surfaces in the UI per CHG-FR-27 rather
/// than being silently rewritten here.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChangesPanelState {
    pub mode: ChangesMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_branch: Option<String>,
}

/// The Library's artifact-type lens (PSS-FR-18).
///
/// The two sentinels bracket the eight built-in artifact types of
/// `ASC-artifact-scanning.md` ASC-FR-02, matching the single-select the Library
/// renders (`../ui/LIB-library.md` LIB-FR-04). `AllArtifacts` is the default the
/// panel takes when nothing has been persisted (LIB-FR-12).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactTypeFilter {
    #[default]
    AllArtifacts,
    AllFiles,
    Skill,
    Agent,
    Prompt,
    Spec,
    /// `harness` is accepted on the way in for the same reason
    /// `scanning::ArtifactType::Flow` accepts it: a `local.toml` written before
    /// the type was named `flow` must still deserialise rather than silently
    /// resetting the panel to its default lens. Only `flow` is written back out.
    #[serde(alias = "harness")]
    Flow,
    Instructions,
    Scenario,
    Scratchpad,
}

/// The Library panel's persisted state (PSS-FR-18).
///
/// `expanded_paths` records **expansion**, not collapse: a folder renders
/// expanded iff its path is in the set, so an unknown folder is collapsed
/// (`../ui/LIB-library.md` LIB-FR-15). This store never validates that a path
/// still names an existing folder — an absent one is returned unchanged rather
/// than pruned, so a folder that a branch checkout removes and restores comes
/// back expanded (LIB-FR-16).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LibraryPanelState {
    pub expanded_paths: Vec<String>,
    pub artifact_type_filter: ArtifactTypeFilter,
    pub text_filter: String,
}

/// Which of the Notes panel's three scope positions is selected (PSS-FR-19).
///
/// The names match `../ui/NTS-notes.md` NTS-FR-09's three positions: the active
/// tab's entity, project-wide, and every note in the project.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotesScopePosition {
    /// The default when nothing has been persisted (PSS-FR-19).
    #[default]
    Entity,
    Project,
    All,
}

/// The Notes panel's persisted state (PSS-FR-19).
///
/// This store never validates that `scope_position` is renderable — the entity
/// position is unavailable while no entity-scoped tab is active, and it is
/// `../ui/NTS-notes.md` NTS-FR-09 that decides what to show in that case, taking
/// care not to rewrite the persisted value.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NotesPanelState {
    pub scope_position: NotesScopePosition,
    pub text_filter: String,
}

/// Which of the Drafts panel's four status positions is selected (PSS-FR-20).
///
/// The names match `../ui/DRP-drafts-panel.md` DRP-FR-07's four positions: the
/// drafts still being worked on, the retired ones, the ones a graduation has
/// already turned into specifications, and every draft at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DraftsStatusFilter {
    /// The default when nothing has been persisted (PSS-FR-20) — neither a
    /// retired draft nor a graduated one is what the author came to the panel
    /// for.
    #[default]
    Active,
    Archived,
    /// `DRS-draft-storage.md` DRS-FR-20's terminal status, a position of its
    /// own rather than a kind of archive.
    Graduated,
    All,
}

/// The Drafts panel's persisted state (PSS-FR-20).
///
/// `expanded_folders` records **expansion**, not collapse, exactly as the
/// Library's `expanded_paths` does: a folder renders expanded iff its
/// drafts-root-relative path is in the set, so an unknown folder — a newly
/// created one, or every folder on a first-ever open — is collapsed
/// (`../ui/DRP-drafts-panel.md` DRP-FR-14). This store never validates that a
/// path still names an existing drafts folder; an absent one is returned
/// unchanged rather than pruned, so a folder that disappears and returns comes
/// back expanded.
///
/// The paths here name the panel's own folders and never a path inside a
/// draft's file tree, which no store holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DraftsPanelState {
    pub status_filter: DraftsStatusFilter,
    pub text_filter: String,
    pub expanded_folders: Vec<String>,
}

/// The project's line-ending convention (PSS-FR-17). Project-public, because it
/// is a convention of the project that everyone who checks it out shares, and it
/// is what `PST-project-storage.md` PST-FR-23 normalises every artifact write to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineEndings {
    /// The default when the key is absent (PSS-FR-17).
    #[default]
    Lf,
    Crlf,
}

impl LineEndings {
    /// The bytes this convention separates lines with.
    pub fn terminator(self) -> &'static str {
        match self {
            LineEndings::Lf => "\n",
            LineEndings::Crlf => "\r\n",
        }
    }

    /// `PST-project-storage.md` PST-FR-23: rewrite every line ending in `body`
    /// to this convention, so the bytes on disk use it throughout regardless of
    /// what the submitted body or the previous on-disk content used.
    ///
    /// Every ending is recognised first — CRLF, bare LF, and the bare CR a
    /// classic-Mac file or a stray paste can leave behind — so a mixed body
    /// comes out uniform rather than half-converted. The pass is idempotent: a
    /// body already in the target convention is returned byte-identical, which
    /// is what keeps a save from inventing a change (`../ui/EDT-editor.md`
    /// EDT-FR-40).
    pub fn normalize(self, body: &str) -> String {
        let mut out = String::with_capacity(body.len());
        let mut chars = body.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\r' => {
                    // Consume the LF of a CRLF pair so it is not emitted twice.
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    out.push_str(self.terminator());
                }
                '\n' => out.push_str(self.terminator()),
                other => out.push(other),
            }
        }
        out
    }
}

/// The project-public config the Project settings tab and the status bar read
/// and write (PSS-FR-17).
///
/// Deliberately a *partial* view of `project.toml`: the store holds target repo
/// bindings, adapter configuration, plugin enablement and MCP connections too,
/// none of which this walking-skeleton payload declares. That is why
/// [`save_project_config_to`] merges rather than replaces — see its note.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProjectConfig {
    pub line_endings: LineEndings,
    /// PSS-FR-21: the project's optional Markdown **draft template** — the text
    /// a new draft's prompt is created holding (`DRS-draft-storage.md`
    /// DRS-FR-39).
    ///
    /// Three-valued across the two directions of the contract, which is what
    /// keeps PSS-FR-17's "persisting any other section carries the template
    /// through unchanged" true of a payload that is only a partial view of the
    /// store:
    ///
    /// * **read** — `None` is the *unset* state (the key is absent from
    ///   `project.toml`), and is what tells `create_draft` to create its prompt
    ///   empty (DRS-FR-06). `Some(text)` is a configured template.
    /// * **written** — `None` means this write says nothing about the template
    ///   and leaves whatever is stored exactly as it stands; `Some("")` clears
    ///   it back to unset (PSS-FR-21: an empty template is never persisted);
    ///   `Some(text)` records it.
    pub draft_template: Option<String>,
    /// PSS-FR-JRWC: how many graduation runs, stream runs and direct runs
    /// together, may hold a project slot at the same time (`GRD-graduation.md`
    /// GRD-FR-KKKN). A positive integer or `unlimited`; one by default.
    ///
    /// Two-valued across the two directions of the contract, like the
    /// template above:
    ///
    /// * **read** — always `Some`: an absent or malformed value is repaired to
    ///   one (PSS-FR-KMBT).
    /// * **written** — `None` says nothing about the limit and leaves what is
    ///   stored as it stands (PSS-FR-ZVSD); `Some` records it.
    #[serde(default)]
    pub graduation_concurrency_limit: Option<GraduationConcurrency>,
    /// PSS-FR-TQMV: how long one agent turn may run, in milliseconds.
    ///
    /// Three-valued across the two directions of the contract, exactly as
    /// `draft_template` above is, which is what keeps PSS-FR-17's "persisting
    /// any other section carries this one through unchanged" true of a payload
    /// that is only a partial view of the store:
    ///
    /// * **read** — `Some(None)` is the *unset* state of PSS-FR-ZLCF, in which
    ///   each loop uses its own default; `Some(Some(ms))` is a configured
    ///   bound.
    /// * **written** — an **absent** key says nothing about the bound and
    ///   leaves whatever is stored exactly as it stands; `null` clears it back
    ///   to unset; a value inside [`loop_settings::EXECUTION_TIMEOUT_BOUNDS`]
    ///   records it, and one outside them clears it, a bound nobody could have
    ///   meant never being persisted (PSS-FR-ZLCF).
    #[serde(default, deserialize_with = "explicit_option")]
    pub execution_timeout_ms: Option<Option<u64>>,
    /// PSS-FR-HDBN: how long one call to a model may run, in milliseconds. The
    /// three states are `execution_timeout_ms`'s.
    #[serde(default, deserialize_with = "explicit_option")]
    pub provider_call_deadline_ms: Option<Option<u64>>,
    /// PSS-FR-WPKS: how many attempts a bounded loop may spend. The graduation
    /// loop counts passes with it and the conversation loop counts physical
    /// attempts, each keeping its own meaning for the one number. The three
    /// states are `execution_timeout_ms`'s.
    #[serde(default, deserialize_with = "explicit_option")]
    pub retry_budget: Option<Option<u32>>,
}

/// Tell a key that was **absent** from a key that carried `null`.
///
/// Serde's own `Option` reads both as `None`, which would collapse "say nothing
/// about this bound" and "clear this bound" into one value — and PSS-FR-ZLCF
/// needs them apart.
fn explicit_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}


impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            line_endings: LineEndings::default(),
            draft_template: None,
            graduation_concurrency_limit: None,
            execution_timeout_ms: None,
            provider_call_deadline_ms: None,
            retry_budget: None,
        }
    }
}

/// PSS-FR-10: a project-public file that exists but cannot be parsed is a typed
/// error (it implies a damaged or wrong-versioned project), which
/// `../ui/SET-project-settings.md` surfaces inline.
pub const ERR_MALFORMED_PROJECT_CONFIG: &str = "malformed project config";

/// The key the Changes panel's state lives under in `.synthesis/local.toml`.
const CHANGES_PANEL_KEY: &str = "changes_panel";

/// The key an earlier layout of this store wrote a recently-edited MRU under
/// (PSS-FR-14).
///
/// No value is held under it any more and nothing reads one: the Dashboard's
/// Recently edited widget is served from the project's artifacts and their
/// source files instead (`PST-project-storage.md` PST-FR-31). It is named here
/// only so the ordinary write path can drop it.
const LEGACY_RECENTLY_EDITED_KEY: &str = "recently_edited";

/// The key the Library panel's state lives under in `.synthesis/local.toml`.
const LIBRARY_PANEL_KEY: &str = "library_panel";

/// The key the Notes panel's state lives under in `.synthesis/local.toml`.
const NOTES_PANEL_KEY: &str = "notes_panel";

/// The key the Drafts panel's state lives under in `.synthesis/local.toml`.
const DRAFTS_PANEL_KEY: &str = "drafts_panel";
/// PSS-FR-TQFB: the project-local GitHub remote a draft publication reuses.
const PUBLICATION_REMOTE_KEY: &str = "publication_remote";

/// The key the line-ending convention lives under in `.synthesis/project.toml`.
pub(crate) const LINE_ENDINGS_KEY: &str = "lineEndings";

/// The key the draft template lives under in `.synthesis/project.toml`
/// (PSS-FR-21). Committed with the rest of the project-public store, because a
/// template is a convention everyone who checks the project out shares.
pub(crate) const DRAFT_TEMPLATE_KEY: &str = "draftTemplate";
/// PSS-FR-JRWC: how many graduation runs may hold a project slot at once.
pub(crate) const GRADUATION_CONCURRENCY_KEY: &str = "graduationConcurrencyLimit";
/// PSS-FR-LGKY: the key the limit lived under before it became project-wide. It
/// is read where the current key is absent, and removed by a save that names a
/// limit.
pub(crate) const LEGACY_STREAM_CONCURRENCY_KEY: &str = "streamConcurrencyLimit";


/// `<project_root>/.synthesis`.
fn synthesis_dir(root: &Path) -> PathBuf {
    root.join(".synthesis")
}

/// `<project_root>/.synthesis/local.toml` (PSS-FR-03).
pub fn local_toml_path(root: &Path) -> PathBuf {
    synthesis_dir(root).join("local.toml")
}

/// `<project_root>/.synthesis/project.toml` (PSS-FR-02) — committed to the
/// project's Git repository, so a repository with several worktrees holds one
/// copy per worktree and the active one is authoritative.
pub fn project_toml_path(root: &Path) -> PathBuf {
    synthesis_dir(root).join("project.toml")
}

/// Read the project-local config, repairing a missing or malformed file to
/// defaults (PSS-FR-10). Never errors: project-local state is per-machine
/// convenience, and a damaged file must not block opening the project.
///
/// The file is read as a generic table rather than a struct because
/// `local.toml` is shared with every other project-local consumer. A typed
/// struct would silently drop the sections it does not declare, and the next
/// atomic write would delete them from disk.
///
/// PSS-FR-14: a `recently_edited` key written by an earlier layout is read back
/// with the rest and applied to nothing — no `load_*` command of this module
/// looks at it, so persisted data from that layout influences no command and no
/// widget. [`save_local`] is what takes it off disk.
fn load_local(root: &crate::fs::RootFs) -> toml::Table {
    root.read_toml::<toml::Table>(local_toml_path(root)).unwrap_or_default()
}

/// Write the project-local config atomically (PSS-FR-04) and make sure
/// `.synthesis/` carries the ignore rules that keep `local.toml` out of the
/// repository (PSS-FR-03).
fn save_local(root: &crate::fs::RootFs, value: &toml::Table) -> Result<(), String> {
    let _guard = local_store_lock();
    write_local(root, value)
}

/// One write of `local.toml` at a time in this process. A read-modify-write
/// that must not interleave with another holds it across its read too.
static LOCAL_STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn local_store_lock() -> std::sync::MutexGuard<'static, ()> {
    LOCAL_STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// [`save_local`] for a caller that already holds [`local_store_lock`].
fn write_local(root: &crate::fs::RootFs, value: &toml::Table) -> Result<(), String> {
    // PSS-FR-14: the legacy recently-edited key is dropped by the ordinary write
    // path rather than by a migration command. Every `save_*` of this module
    // rewrites the whole table, so the first one a project performs takes the
    // key off disk and every other persisted value survives the write; a project
    // never written to again keeps a key that changes nothing.
    let dropped;
    let value = if value.contains_key(LEGACY_RECENTLY_EDITED_KEY) {
        let mut without = value.clone();
        without.remove(LEGACY_RECENTLY_EDITED_KEY);
        dropped = without;
        &dropped
    } else {
        // The ordinary path copies nothing: almost every project has no such
        // key, and every panel state save comes through here.
        value
    };
    let dir = synthesis_dir(root);
    // No explicit mkdir: every write primitive creates its parents on demand
    // (FSA-FR-05), so `.synthesis/` comes into being with the first file
    // written into it.
    // Best-effort: an ignore file that cannot be written must not lose the
    // user's setting, but it is attempted before the write so a fresh project
    // never commits its local state.
    let _ = root.ensure_gitignored(&dir);
    root.write_toml_atomic(local_toml_path(root), value).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Docker image configuration (PSS-FR-22..PSS-FR-30)
// ---------------------------------------------------------------------------

/// PSS-FR-22: every vendor's status, in the stable order the section renders
/// them, configured or not — so a caller never has to reason about an absent
/// record.
pub fn load_project_docker_images_from(
    root: &crate::fs::RootFs,
) -> Result<Vec<images::ProjectVendorImageStatus>, String> {
    let table = load_public(root)?;
    let entries = images::read_entries(&table);
    Ok(images::VENDORS
        .iter()
        .map(|vendor| images::status_for(root, vendor, entries.get(*vendor)))
        .collect())
}

/// PSS-FR-22 / PSS-FR-24: validate, then write one vendor's entry on the
/// whole-store terms of PSS-FR-17.
pub fn save_project_vendor_image_to(
    root: &crate::fs::RootFs,
    vendor: &str,
    entry: &images::ProjectVendorImage,
) -> Result<images::ProjectVendorImageStatus, String> {
    if !images::VENDORS.contains(&vendor) {
        return Err(images::ERR_UNKNOWN_VENDOR.to_string());
    }
    // A refused save writes nothing and leaves the entry exactly as it was.
    let validated = images::validate_entry(root, entry)?;
    let mut table = load_public(root)?;
    images::write_entry(&mut table, vendor, &validated);
    save_public(root, &table)?;
    Ok(images::status_for(root, vendor, Some(&validated)))
}

/// PSS-FR-30: the image reference the open project commits for this vendor, or
/// the refusal.
///
/// The single read path an execution takes to the project's image. There is no
/// fallback to the shipped vendor image of `../infra/AVI-agent-vendor-images.md`
/// and no other source it can resolve from, so a project that configures
/// nothing runs nothing. Not a Tauri command.
pub fn resolve_project_vendor_image(
    root: &crate::fs::RootFs,
    vendor: &str,
) -> Result<String, String> {
    let table = load_public(root)?;
    images::read_entries(&table)
        .get(vendor)
        .and_then(|entry| entry.image_reference())
        .ok_or_else(|| images::ERR_VENDOR_IMAGE_UNCONFIGURED.to_string())
}

#[tauri::command]
pub fn load_project_docker_images(
    project: State<'_, ProjectState>,
) -> Result<Vec<images::ProjectVendorImageStatus>, String> {
    let root = project.require_root()?;
    load_project_docker_images_from(&root)
}

#[tauri::command]
pub fn save_project_vendor_image(
    vendor: String,
    entry: images::ProjectVendorImage,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<images::ProjectVendorImageStatus, String> {
    let root = project.require_root()?;
    let result = save_project_vendor_image_to(&root, &vendor, &entry);
    match &result {
        Ok(status) => crate::logging::log_info(
            &app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "project vendor image saved",
            crate::log_fields! {
                "vendor" => vendor.as_str(),
                "has_dockerfile" => status.dockerfile.is_some(),
                "graduation_state" => format!("{:?}", status.graduation_state)
            },
        ),
        Err(refusal) => crate::logging::log_warn(
            &app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "project vendor image refused",
            crate::log_fields! { "vendor" => vendor.as_str(), "reason" => refusal.as_str() },
        ),
    }
    result
}

/// The builder a build is performed through, held in state so a test swaps the
/// production one without touching a call site.
pub struct ImageBuilderSeam(pub std::sync::Arc<dyn images::ImageBuilder>);

impl Default for ImageBuilderSeam {
    fn default() -> Self {
        ImageBuilderSeam(std::sync::Arc::new(image_builder::RealImageBuilder))
    }
}

/// PSS-FR-27 / PSS-FR-28: the two events, emitted through the Tauri bus.
///
/// An emit that cannot be delivered is dropped rather than propagated, for the
/// same reason `ProgressSink` drops one: reporting must never block or fail the
/// build it is reporting on.
struct AppBuildEvents<R: tauri::Runtime>(tauri::AppHandle<R>);

impl<R: tauri::Runtime> images::BuildEvents for AppBuildEvents<R> {
    fn progress(&self, payload: images::ImageBuildProgress) {
        use tauri::Emitter;
        let _ = self.0.emit(images::PROJECT_IMAGE_BUILD_PROGRESS, payload);
    }

    fn finished(&self, payload: images::ImageBuildFinished) {
        use tauri::Emitter;
        let _ = self.0.emit(images::PROJECT_IMAGE_BUILD_FINISHED, payload);
    }
}

/// PSS-FR-26 / PSS-FR-29: start a local build of this vendor's Dockerfile.
///
/// Every refusal happens before anything is started, and the project's one build
/// slot is claimed last, so a refused build leaves the slot free.
#[tauri::command]
pub fn build_project_vendor_image(
    vendor: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
    store: State<'_, crate::global_settings::GlobalSettingsStore>,
    registry: State<'_, images::ImageBuildRegistry>,
) -> Result<images::StartedImageBuild, String> {
    let root = project.require_root()?;
    let table = load_public(&root)?;
    let entries = images::read_entries(&table);
    // Unique for the lifetime of the running application and never reused, so a
    // surface keying a build by its id never confuses one with a later one
    // (PSS-FR-27). A monotonic counter rather than a clock: two builds a
    // second apart and two in the same second are equally distinguishable.
    static NEXT_BUILD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = NEXT_BUILD.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let operation_id = format!("image-build-{vendor}-{sequence}");

    let prepared = images::prepare_build(
        entries.get(&vendor),
        crate::docker::resolve_docker_backend(&store),
        root.path().to_path_buf(),
        &vendor,
        &operation_id,
        &registry,
    )
    .inspect_err(|refusal| {
        crate::logging::log_warn(
            &app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "project image build refused",
            crate::log_fields! { "vendor" => vendor.as_str(), "reason" => refusal.as_str() },
        );
    })?;

    crate::logging::log_info(
        &app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Backend],
        "project image build started",
        crate::log_fields! {
            "vendor" => vendor.as_str(),
            "operation_id" => operation_id.as_str(),
            "backend" => format!("{:?}", prepared.backend.mode()),
            "dockerfile" => prepared.request.dockerfile.as_str()
        },
    );

    // The build runs on its own thread: it is minutes long, it streams, and the
    // command that started it must return the operation id at once so the
    // section can render against it.
    let started = images::StartedImageBuild {
        operation_id: operation_id.clone(),
    };
    let scope = root.path().to_path_buf();
    std::thread::spawn(move || {
        use tauri::Manager;
        // PSS-FR-27: the status bar shows a build in flight on
        // `PRG-progress-reporting.md`'s opt-in terms (PRG-FR-11), beside the
        // section's own inline report rather than instead of it.
        let progress = app.state::<crate::progress::ProgressRegistry>();
        let operation = crate::progress::register_and_publish(
            &app,
            &progress,
            "image-build",
            "Building the project image…",
            Some(scope),
            None,
        );

        let builder = app.state::<ImageBuilderSeam>().0.clone();
        let registry = app.state::<images::ImageBuildRegistry>();
        let events = AppBuildEvents(app.clone());
        let terminal = images::run_prepared_build(prepared, builder.as_ref(), &events, &registry);

        let state = match &terminal {
            images::BuildTerminal::Succeeded => crate::progress::OperationState::Finished,
            images::BuildTerminal::Failed(_) => crate::progress::OperationState::Failed,
            images::BuildTerminal::Cancelled => crate::progress::OperationState::Cancelled,
        };
        crate::progress::terminate_and_publish(&app, &progress, operation, state);

        // The diagnostic itself is not logged: it is the build's own output and
        // can run to thousands of lines, and the author reads it inline in the
        // section (SET-FR-26). What a log reader needs is which build ended and
        // how.
        crate::logging::log_info(
            &app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "project image build finished",
            crate::log_fields! {
                "operation_id" => operation_id.as_str(),
                "outcome" => format!("{:?}", state)
            },
        );
    });

    Ok(started)
}

/// PSS-FR-29: the build holding this project's one build slot right now, if
/// any.
///
/// A build outlives the surface that started it, so the Docker section reads
/// this when it mounts rather than forgetting a build is running: that is what
/// keeps a running build rendered against its own tab and every Build control
/// disabled on re-entry (`../ui/SET-project-settings.md` SET-FR-25, SET-FR-26).
#[tauri::command]
pub fn load_project_image_build_in_flight(
    registry: State<'_, images::ImageBuildRegistry>,
) -> Result<Option<images::InFlightImageBuild>, String> {
    Ok(registry.in_flight())
}

/// PSS-FR-28: ask the named build to stop. The terminal result arrives on
/// `"project image build finished"` as `cancelled`, and the project's image
/// configuration is left exactly as it was.
#[tauri::command]
pub fn cancel_project_vendor_image_build(
    operation_id: String,
    registry: State<'_, images::ImageBuildRegistry>,
) -> Result<(), String> {
    registry.cancel(&operation_id);
    Ok(())
}

// ---------------------------------------------------------------------------
// Changes panel state (PSS-FR-15)
// ---------------------------------------------------------------------------

/// PSS-FR-15: the persisted mode and target branch, or the `uncommitted`
/// default with no target branch when nothing has been persisted for the
/// project. `../ui/CHG-changes.md` CHG-FR-06 supplies the initial target from
/// the repository's default branch in that case.
pub fn load_changes_panel_state_from(root: &crate::fs::RootFs) -> ChangesPanelState {
    load_local(root)
        .get(CHANGES_PANEL_KEY)
        .cloned()
        .and_then(|value| value.try_into::<ChangesPanelState>().ok())
        .unwrap_or_default()
}

/// PSS-FR-15: persist the panel's mode and target branch to project-local scope,
/// leaving every other section of `local.toml` exactly as it was.
pub fn save_changes_panel_state_to(root: &crate::fs::RootFs, state: ChangesPanelState) -> Result<(), String> {
    let mut config = load_local(root);
    let value = toml::Value::try_from(state)
        .map_err(|e| format!("failed to encode changes panel state: {e}"))?;
    config.insert(CHANGES_PANEL_KEY.to_string(), value);
    save_local(root, &config)
}

#[tauri::command]
pub fn load_changes_panel_state(
    project: State<'_, ProjectState>,
) -> Result<ChangesPanelState, String> {
    // No project open -> the defaults, exactly as a project with no persisted
    // state (PSS-FR-15); the panel has nothing to restore either way.
    match project.require_root() {
        Ok(root) => Ok(load_changes_panel_state_from(&root)),
        Err(_) => Ok(ChangesPanelState::default()),
    }
}

#[tauri::command]
pub fn save_changes_panel_state(
    state: ChangesPanelState,
    project: State<'_, ProjectState>,
) -> Result<(), String> {
    let root = project.require_root()?;
    save_changes_panel_state_to(&root, state)
}

// ---------------------------------------------------------------------------
// Library panel state (PSS-FR-18)
// ---------------------------------------------------------------------------

/// PSS-FR-18: the persisted expanded set and the two local filters, or the
/// defaults — an empty expanded set, the `all_artifacts` lens and an empty text
/// filter — when nothing has been persisted for the active worktree.
///
/// Each field is read independently rather than by deserialising the section as
/// a whole, so one damaged value costs only itself. A record written by a newer
/// build whose lens this build does not know must not throw away the user's
/// expanded folders, which is exactly what a whole-section fallback would do.
pub fn load_library_panel_state_from(root: &crate::fs::RootFs) -> LibraryPanelState {
    let Some(section) = load_local(root).get(LIBRARY_PANEL_KEY).cloned() else {
        return LibraryPanelState::default();
    };
    let field = |key: &str| section.get(key).cloned();
    LibraryPanelState {
        expanded_paths: field("expandedPaths")
            .and_then(|v| v.try_into::<Vec<String>>().ok())
            .unwrap_or_default(),
        artifact_type_filter: field("artifactTypeFilter")
            .and_then(|v| v.try_into::<ArtifactTypeFilter>().ok())
            .unwrap_or_default(),
        text_filter: field("textFilter")
            .and_then(|v| v.try_into::<String>().ok())
            .unwrap_or_default(),
    }
}

/// PSS-FR-18: persist the panel state to project-local scope, leaving every
/// other section of `local.toml` exactly as it was.
///
/// The record is written whole — the caller supplies all three values — so a
/// change to the lens carries the expanded set and text filter through unchanged
/// (`../ui/LIB-library.md` LIB-FR-17).
pub fn save_library_panel_state_to(root: &crate::fs::RootFs, state: LibraryPanelState) -> Result<(), String> {
    let mut config = load_local(root);
    let value = toml::Value::try_from(state)
        .map_err(|e| format!("failed to encode library panel state: {e}"))?;
    config.insert(LIBRARY_PANEL_KEY.to_string(), value);
    save_local(root, &config)
}

#[tauri::command]
pub fn load_library_panel_state(
    project: State<'_, ProjectState>,
) -> Result<LibraryPanelState, String> {
    // No project open -> the defaults, exactly as a worktree with no persisted
    // state (PSS-FR-18); the panel has nothing to restore either way.
    match project.require_root() {
        Ok(root) => Ok(load_library_panel_state_from(&root)),
        Err(_) => Ok(LibraryPanelState::default()),
    }
}

#[tauri::command]
pub fn save_library_panel_state(
    state: LibraryPanelState,
    project: State<'_, ProjectState>,
) -> Result<(), String> {
    let root = project.require_root()?;
    save_library_panel_state_to(&root, state)
}

// ---------------------------------------------------------------------------
// Notes panel state (PSS-FR-19)
// ---------------------------------------------------------------------------

/// PSS-FR-19: the persisted scope position and filter text, or the defaults —
/// the entity position and an empty filter — when nothing has been persisted for
/// the active worktree.
///
/// Each field is read independently, for the same reason the Library's is: a
/// position written by a newer build must not throw away the user's filter text.
pub fn load_notes_panel_state_from(root: &crate::fs::RootFs) -> NotesPanelState {
    let Some(section) = load_local(root).get(NOTES_PANEL_KEY).cloned() else {
        return NotesPanelState::default();
    };
    let field = |key: &str| section.get(key).cloned();
    NotesPanelState {
        scope_position: field("scopePosition")
            .and_then(|v| v.try_into::<NotesScopePosition>().ok())
            .unwrap_or_default(),
        text_filter: field("textFilter")
            .and_then(|v| v.try_into::<String>().ok())
            .unwrap_or_default(),
    }
}

/// PSS-FR-19: persist the Notes panel state to project-local scope, leaving
/// every other section of `local.toml` exactly as it was. The record is written
/// whole, so persisting a new position carries the filter text through unchanged
/// (`../ui/NTS-notes.md` NTS-FR-09 / NTS-FR-13).
pub fn save_notes_panel_state_to(root: &crate::fs::RootFs, state: NotesPanelState) -> Result<(), String> {
    let mut config = load_local(root);
    let value = toml::Value::try_from(state)
        .map_err(|e| format!("failed to encode notes panel state: {e}"))?;
    config.insert(NOTES_PANEL_KEY.to_string(), value);
    save_local(root, &config)
}

#[tauri::command]
pub fn load_notes_panel_state(
    project: State<'_, ProjectState>,
) -> Result<NotesPanelState, String> {
    // No project open -> the defaults, exactly as a worktree with no persisted
    // state; the panel has nothing to restore either way.
    match project.require_root() {
        Ok(root) => Ok(load_notes_panel_state_from(&root)),
        Err(_) => Ok(NotesPanelState::default()),
    }
}

#[tauri::command]
pub fn save_notes_panel_state(
    state: NotesPanelState,
    project: State<'_, ProjectState>,
) -> Result<(), String> {
    let root = project.require_root()?;
    save_notes_panel_state_to(&root, state)
}

// ---------------------------------------------------------------------------
// Drafts panel state (PSS-FR-20)
// ---------------------------------------------------------------------------

/// PSS-FR-20: the persisted status position, filter text and expanded folder
/// set, or the defaults — the `active` position, an empty filter and an empty
/// expanded set — when nothing has been persisted for the active worktree.
///
/// Each field is read independently, for the reason the Library's is: a position
/// written by a newer build whose value this one does not know must not throw
/// away the author's expanded folders.
pub fn load_drafts_panel_state_from(root: &crate::fs::RootFs) -> DraftsPanelState {
    let Some(section) = load_local(root).get(DRAFTS_PANEL_KEY).cloned() else {
        return DraftsPanelState::default();
    };
    let field = |key: &str| section.get(key).cloned();
    DraftsPanelState {
        status_filter: field("statusFilter")
            .and_then(|v| v.try_into::<DraftsStatusFilter>().ok())
            .unwrap_or_default(),
        text_filter: field("textFilter")
            .and_then(|v| v.try_into::<String>().ok())
            .unwrap_or_default(),
        expanded_folders: field("expandedFolders")
            .and_then(|v| v.try_into::<Vec<String>>().ok())
            .unwrap_or_default(),
    }
}

/// PSS-FR-20: persist the Drafts panel state to project-local scope, leaving
/// every other section of `local.toml` exactly as it was. The record is written
/// whole, so persisting a new position carries the filter text and the expanded
/// set through unchanged (`../ui/DRP-drafts-panel.md` DRP-FR-14).
pub fn save_drafts_panel_state_to(
    root: &crate::fs::RootFs,
    state: DraftsPanelState,
) -> Result<(), String> {
    let mut config = load_local(root);
    let value = toml::Value::try_from(state)
        .map_err(|e| format!("failed to encode drafts panel state: {e}"))?;
    config.insert(DRAFTS_PANEL_KEY.to_string(), value);
    save_local(root, &config)
}

#[tauri::command]
pub fn load_drafts_panel_state(
    project: State<'_, ProjectState>,
) -> Result<DraftsPanelState, String> {
    // No project open -> the defaults, exactly as a worktree with no persisted
    // state; the panel has nothing to restore either way.
    match project.require_root() {
        Ok(root) => Ok(load_drafts_panel_state_from(&root)),
        Err(_) => Ok(DraftsPanelState::default()),
    }
}

#[tauri::command]
pub fn save_drafts_panel_state(
    state: DraftsPanelState,
    project: State<'_, ProjectState>,
) -> Result<(), String> {
    let root = project.require_root()?;
    save_drafts_panel_state_to(&root, state)
}

// ---------------------------------------------------------------------------
// Publication remote selection (PSS-FR-TQFB, PSS-FR-NKRE)
// ---------------------------------------------------------------------------

/// PSS-FR-TQFB: the remote a draft publication reuses — a configured Git
/// remote's name together with its canonicalized URL.
///
/// Both halves are stored because a name alone is not an identity: a remote
/// renamed or re-pointed since the choice was made is a different destination,
/// and the reader compares both before it trusts the choice
/// (`GHP-github-publication.md` GHP-FR-HVQG).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PublicationRemoteSelection {
    pub name: String,
    pub url: String,
}

/// PSS-FR-TQFB / PSS-FR-NKRE: the stored choice, or `None`.
///
/// A **hint and never an authority**: nothing here checks that the remote still
/// exists or that the token may reach it. A malformed or half-written value
/// reads as `None` rather than as an error, on the terms PSS-FR-10 repairs any
/// project-local value.
pub fn load_publication_remote_selection_from(
    root: &crate::fs::RootFs,
) -> Option<PublicationRemoteSelection> {
    let section = load_local(root).get(PUBLICATION_REMOTE_KEY).cloned()?;
    let selection = section.try_into::<PublicationRemoteSelection>().ok()?;
    if selection.name.is_empty() || selection.url.is_empty() {
        return None;
    }
    Some(selection)
}

/// PSS-FR-TQFB: persist the choice, or clear it with `None`, leaving every
/// other section of `local.toml` exactly as it was.
pub fn save_publication_remote_selection_to(
    root: &crate::fs::RootFs,
    selection: Option<PublicationRemoteSelection>,
) -> Result<(), String> {
    let mut config = load_local(root);
    match selection {
        Some(selection) => {
            let value = toml::Value::try_from(selection)
                .map_err(|e| format!("failed to encode publication remote: {e}"))?;
            config.insert(PUBLICATION_REMOTE_KEY.to_string(), value);
        }
        None => {
            config.remove(PUBLICATION_REMOTE_KEY);
        }
    }
    save_local(root, &config)
}

#[cfg(test)]
mod tests;
