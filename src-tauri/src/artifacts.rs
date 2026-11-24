//! Artifact contents (PST-project-storage.md / EDT-editor.md).
//!
//! `load_artifact_contents_by_id` / `save_artifact_contents` carry a SHA-256
//! checksum (PST-FR-15) the Editor uses as its external-change baseline
//! (EXC-FR-WCOM), and both record it in the [`ContentTracker`] so the watcher can
//! tell an external modification from the Editor's own save.
//!
//! **A save keeps no record of what it saved** (PST-FR-17): no
//! most-recently-edited list is written here or anywhere else, and no command of
//! `PSS-project-settings-storage.md` is invoked for one. What puts an artifact
//! at the head of the Dashboard's Recently edited widget is the modification
//! time the write itself gives the artifact's source file, which is a fact about
//! the project rather than about this application — so an artifact written by an
//! external editor and one written here reach that widget on identical terms
//! (`crate::dashboard`, PST-FR-31).
//!
//! The pure cores (`*_impl`) are unit-tested here without a Tauri runtime.

use crate::project_scope::ProjectStores;
use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;

use crate::fs;
use crate::project::ProjectState;
use crate::project_settings;
use crate::scanning;

/// Per-artifact content checksum baselines (PST-FR-15 / PST-FR-16). Maps an
/// artifact id (its project-relative path, ASC-FR-13) to the SHA-256 this
/// module last *served* (`load_artifact_contents_by_id`) or *wrote*
/// (`save_artifact_contents`). The watcher consults it to distinguish an
/// EXTERNAL content modification (-> emit `"artifact changed externally"`) from
/// the Editor's own save (checksum already recorded -> suppressed). Only files
/// the Editor has actually opened are tracked, so this map stays small.
#[derive(Default)]
pub struct ContentTracker {
    inner: Mutex<HashMap<String, String>>,
}

impl ContentTracker {
    /// Record the checksum `load`/`save` last observed for `id`.
    pub fn record(&self, id: &str, checksum: &str) {
        if let Ok(mut g) = self.inner.lock() {
            g.insert(id.to_string(), checksum.to_string());
        }
    }

    /// The last checksum recorded for `id`, if this module has served it.
    pub fn current(&self, id: &str) -> Option<String> {
        self.inner.lock().ok().and_then(|g| g.get(id).cloned())
    }

    /// Forget the baseline for `id` alone.
    ///
    /// PST-FR-16: a save records its checksum before the write becomes visible,
    /// and a write that then fails has to take that record back — an entry
    /// naming bytes no file holds would make the watcher read the file's own,
    /// unchanged content as an external modification.
    pub fn forget(&self, id: &str) {
        if let Ok(mut g) = self.inner.lock() {
            g.remove(id);
        }
    }

    /// Drop all baselines (project switch / close, PST-FR-14).
    pub fn clear(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.clear();
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.len()).unwrap_or(0)
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Wire shape of `load_artifact_contents_by_id` (PST-FR-15): body + baseline.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactContents {
    pub body: String,
    pub checksum: String,
}

/// Wire shape of `save_artifact_contents` (PST-FR-15): the written checksum.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    pub checksum: String,
}

/// Payload of the `"artifact changed externally"` event (PST-FR-16).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactChangedPayload {
    pub artifact_id: String,
    pub checksum: String,
}

/// PST-FR-15 core (testable without a Tauri runtime): read `id`'s body and
/// checksum and record the baseline. `id` is the project-relative path key
/// (ASC-FR-13); FSA path-escape rejection (FSA-FR-10) guards it. The checksum is
/// taken over the bytes just read (byte-identical to `sha256_file`) so it
/// matches what the watcher computes for the same content.
fn load_artifact_contents_impl(
    root: &fs::RootFs,
    id: &str,
    tracker: &ContentTracker,
) -> Result<ArtifactContents, String> {
    let path = fs::resolve_under(root, id).map_err(|e| e.to_string())?;
    let body = root.read_text(&path).map_err(|e| e.to_string())?;
    let checksum = fs::sha256_bytes(body.as_bytes());
    tracker.record(id, &checksum);
    Ok(ArtifactContents { body, checksum })
}

/// PST-FR-15 / PST-FR-09 / PST-FR-23 core: normalise `body`'s line endings to
/// the project's convention, atomically write it to `id`'s file via the FSA
/// atomic-write primitive, and return the checksum of the bytes written.
///
/// The checksum is taken over the **normalised** bytes — the ones that actually
/// reach disk — which is what makes it a valid baseline for the Editor
/// (`../ui/EDT-editor.md` EDT-FR-40). Hashing the submitted body instead would
/// leave the baseline disagreeing with the file on every save that converted
/// anything, and the watcher's echo of our own write would raise the
/// external-change modal (PST-FR-16).
///
/// The baseline is recorded BEFORE the write becomes visible: the on-disk file
/// is unchanged until `write_text_atomic`'s rename, so by the time the watcher
/// can observe the change the tracker already holds the new checksum. This
/// closes the self-write race (PST-FR-16) — there is no window where the watcher
/// sees the new content while the tracker still holds the old checksum.
fn save_artifact_contents_impl(
    root: &fs::RootFs,
    id: &str,
    body: &str,
    tracker: &ContentTracker,
) -> Result<SaveResult, String> {
    let path = fs::resolve_under(root, id).map_err(|e| e.to_string())?;
    let body = project_settings::line_endings_for(root).normalize(body);
    let checksum = fs::sha256_bytes(body.as_bytes());
    // Held so a failed write can put it back. Recording ahead of the write is
    // what closes the self-write race, but a write that never lands would
    // otherwise leave the tracker naming bytes that are not on disk — and the
    // watcher's next pass would read the file's real, unchanged content as an
    // EXTERNAL modification and raise the divergence modal over an edit the
    // author never lost (PST-FR-16). That matters most to the one caller whose
    // failure path promises the session is untouched: an acceptance refused
    // before its commit point (`crate::prompt_proposals`, PCP-FR-14).
    let previous = tracker.current(id);
    tracker.record(id, &checksum);
    if let Err(e) = root.write_text_atomic(&path, &body) {
        match previous {
            Some(previous) => tracker.record(id, &previous),
            // Nothing was tracked before this call, so the entry this call added
            // is the one thing to take back.
            None => tracker.forget(id),
        }
        return Err(e.to_string());
    }
    Ok(SaveResult { checksum })
}

/// PST-FR-15: load a markdown artifact's body + baseline checksum (EXC-FR-WCOM).
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn load_artifact_contents_by_id(
    app: tauri::AppHandle,
    id: String,
    project: crate::project_scope::Scoped<'_, ProjectState>,
    tracker: crate::project_scope::Scoped<'_, ContentTracker>,
) -> Result<ArtifactContents, String> {
    let root = project.require_root()?;
    // PST-FR-26: an `id` is a project-relative path (ASC-FR-13) and is checked
    // explicitly before any primitive runs, exactly as the mutation commands'
    // path arguments are. PST-FR-27: a symlinked id is refused on the same
    // terms, which is what keeps a save from replacing a link with a file.
    crate::library::ensure_inside_root(&app, &root, "load_artifact_contents_by_id", &id)?;
    load_artifact_contents_impl(&root, &id, &tracker)
}

/// PST-FR-15 / PST-FR-09: write a markdown artifact's body, return the checksum
/// of the bytes written (the Editor adopts it as its new baseline, EXC-FR-HKKG).
///
/// PST-FR-17: the save keeps **no record of what it saved**. It writes no
/// most-recently-edited list, invokes no command of
/// `PSS-project-settings-storage.md` for one, and leaves no application save
/// history anywhere. The artifact reaches the head of the Dashboard's Recently
/// edited widget through the modification time this write gives its source file
/// (PST-FR-31), which is also why a save that fails moves nothing: it changed no
/// file.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub async fn save_artifact_contents<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    body: String,
) -> Result<SaveResult, String>
where
    tauri::AppHandle<R>: crate::logging::LogSink + Clone + Send + 'static,
{
    // The check can read GitHub, so the command runs off the main thread.
    crate::git::write_guard::off_main::run_write(app, "save_artifact_contents", move |app| {
        save_artifact_contents_blocking(&app, &id, &body)
    })
    .await
}

/// The save of `save_artifact_contents`, on the calling thread.
///
/// `pub(crate)` so that a backend module that writes a Flow file can use the
/// same checks and the same validated write as a save from the Flow tab.
pub(crate) fn save_artifact_contents_blocking<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    id: &str,
    body: &str,
) -> Result<SaveResult, String>
where
    tauri::AppHandle<R>: crate::logging::LogSink + Clone + Send + 'static,
{
    let root = crate::git::write_guard::off_main::project_root(app)?;
    let tracker = app
        .try_project_store::<ContentTracker>()
        .ok_or_else(|| "content tracker unavailable".to_string())?;
    // PST-FR-26 / PST-FR-27, as in the load above. Checked before the write so
    // a refusal leaves the file and any link at that path untouched.
    crate::library::ensure_inside_root(app, &root, "save_artifact_contents", id)?;
    // PST-FR-ZHVE: the main branch write check, before the write.
    // The check logs its refusal at WARN, as an expected outcome.
    crate::git::write_guard::commands::require_active_branch_write(
        app,
        &root,
        "save_artifact_contents",
    )?;
    let _sequence = crate::git::write_guard::off_main::in_sequence();
    let outcome = save_validated_artifact(&root, id, body, &tracker);
    if let Err(reason) = &outcome {
        // A refused write is the one thing about a save a reader will come to
        // the Logs panel for, and the PST-FR-28 refusal is invisible otherwise:
        // it is handled, so nothing crashes and no file changes. The artifact's
        // path and the refusal's summary line are structural — neither carries
        // the body, which is user content.
        crate::logging::log_error(
            app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "artifact save refused",
            crate::log_fields! {
                "artifact" => id.to_string(),
                "reason" => reason.lines().next().unwrap_or("").to_string(),
            },
        );
    }
    outcome
}

/// PST-FR-15 + PST-FR-28 core: validate what needs validating, then write.
///
/// PST-FR-28: a `flow`-typed artifact is checked against the Flow schema before
/// any write is attempted, and a body carrying violations is refused with them.
/// The gate lives INSIDE the write rather than beside it, which is what makes it
/// the authority: the Flow tab's own save, Save All, and the flushes on tab
/// close, project close, worktree switch, and quit all reach disk through this
/// one function, so none of them can route around it. No other artifact type is
/// inspected — a Markdown artifact and a plain text file are written as the bytes
/// they are.
///
/// PST-FR-17: nothing is recorded about the write anywhere. What the write
/// leaves behind is the file's new modification time, which is what orders the
/// Dashboard's Recently edited widget (PST-FR-31).
///
/// `pub(crate)` because `crate::prompt_proposals` composes it too: accepting a
/// change an agent proposed to a prompt artifact writes that file, and PST-FR-09
/// keeps this the only write path for one — so the acceptance normalises line
/// endings and suppresses its own external-change event exactly as the author's
/// own typing does (PCP-FR-12).
pub(crate) fn save_validated_artifact(
    root: &fs::RootFs,
    id: &str,
    body: &str,
    tracker: &ContentTracker,
) -> Result<SaveResult, String> {
    if resolved_type(root, id) == Some(scanning::ArtifactType::Flow) {
        let report = crate::flow_validation::validate(body);
        if !report.valid {
            // Refused before the write: the file on disk keeps the bytes it had,
            // no atomic-write primitive runs, no `"artifact changed externally"`
            // follows, and the file's modification time is untouched — so the
            // artifact's position in Recently edited is exactly what it was.
            return Err(crate::flow_validation::describe(&report));
        }
    }
    save_artifact_contents_impl(root, id, body, tracker)
}

/// The kind hint the UI routes on (PST-FR-08 / PST-FR-12 / DSH-FR-06): a
/// `flow` opens in the Flow tab, every other artifact in the Editor (an
/// `architecture` too: its `.calm.json` name makes it a source file there, per
/// ESH-FR-SSDV, so the kind does not have to tell it apart),
/// and a `text` — a file the scan surfaces with no resolved artifact type — in
/// the Editor as a plain text file (PST-FR-24 / ESH-FR-ATDS).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    Markdown,
    Flow,
    Text,
}

impl From<scanning::ArtifactType> for ArtifactKind {
    fn from(t: scanning::ArtifactType) -> Self {
        match t {
            scanning::ArtifactType::Flow => ArtifactKind::Flow,
            _ => ArtifactKind::Markdown,
        }
    }
}

impl ArtifactKind {
    /// PST-FR-08 / PST-FR-24: the kind hint for a resolved classification. An
    /// unclassified file is `text`, which is what routes it to the Editor as a
    /// plain text file rather than leaving it with nowhere to open.
    fn from_resolved(artifact_type: Option<scanning::ArtifactType>) -> ArtifactKind {
        match artifact_type {
            Some(t) => ArtifactKind::from(t),
            None => ArtifactKind::Text,
        }
    }
}

/// Wire shape of `open_artifact_by_id` (PST-FR-08): the stable key plus the kind
/// hint the UI routes to Editor or Flow on.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedArtifact {
    /// The stable, path-derived key (ASC-FR-13) — the same id that was asked for.
    pub key: String,
    pub kind: ArtifactKind,
    /// PST-FR-QQAE: the resolved artifact type of the file. A file with no
    /// type (PST-FR-23) has none, and the field is then absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_type: Option<scanning::ArtifactType>,
}

/// PST-FR-08 / PST-FR-24 core: resolve `id`'s classification into the routing
/// hint. `None` when nothing exists at that path.
///
/// Classification goes through `scanning::classify_file` rather than being
/// re-derived, which is what keeps the hint in step with the type the Library
/// shows for the same path (ASC-FR-06).
fn open_artifact_by_id_impl(root: &fs::RootFs, id: &str) -> Result<OpenedArtifact, String> {
    let path = fs::resolve_under(root.path(), id).map_err(|e| e.to_string())?;
    // `file_info`, not `Path::is_file`: the latter follows a symlink, so a link
    // at the id's path would open as an artifact whose every read is refused.
    if !root.file_info(&path).is_ok_and(|i| i.kind == fs::EntryKind::File) {
        return Err(format!("not a file: {id}"));
    }
    let artifact_type = resolved_type(root, id);
    Ok(OpenedArtifact {
        key: id.to_string(),
        kind: ArtifactKind::from_resolved(artifact_type),
        artifact_type,
    })
}

/// FDL-FR-BDOD: every `flow`-typed artifact of the active worktree, as its id
/// and its body, in path order. A file that cannot be read is left out.
pub(crate) fn flow_bodies<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
) -> Vec<(String, String)> {
    let mut paths: Vec<String> = crate::library::project_artifacts(app, root)
        .into_iter()
        .filter(|n| {
            n.node_kind == scanning::NodeKind::File
                && n.artifact_type == Some(scanning::ArtifactType::Flow)
        })
        .map(|n| n.path)
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| root.read_text(root.join(&path)).ok().map(|body| (path, body)))
        .collect()
}

/// The artifact type the scan resolves for the project-relative `id`, or `None`
/// for a file carrying none.
///
/// Composes the one resolver (ASC-FR-06) rather than re-deriving classification,
/// and reads the file only when path inference has already failed — the content
/// tiebreak is gated inside `classify_file`.
///
/// `pub(crate)` because `crate::agent_conversations` composes it too: the type an
/// agent's input names for the material under discussion has to be the type the
/// Library shows for the same path (AGC-FR-07), which only holds if both go
/// through this one resolver.
pub(crate) fn resolved_type(root: &fs::RootFs, id: &str) -> Option<scanning::ArtifactType> {
    let assignments = scanning::load_assignments(root);
    scanning::classify_path(root, id, &assignments).0
}

/// PST-FR-08 / PST-FR-24: resolve an id to its stable key and the kind hint the
/// UI routes on. An id naming a file the scan surfaces without a resolved
/// artifact type answers `kind = "text"`, so a plain file opens in the Editor as
/// plain text (`../ui/EDT-editor.md` ESH-FR-ATDS) rather than having nowhere to go.
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn open_artifact_by_id(
    id: String,
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<OpenedArtifact, String> {
    let root = project.require_root()?;
    open_artifact_by_id_impl(&root, &id)
}

#[cfg(test)]
mod tests;
