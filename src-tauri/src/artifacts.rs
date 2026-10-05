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

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

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
#[tauri::command]
pub fn load_artifact_contents_by_id(
    app: tauri::AppHandle,
    id: String,
    project: State<'_, ProjectState>,
    tracker: State<'_, ContentTracker>,
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
#[tauri::command]
pub fn save_artifact_contents(
    app: tauri::AppHandle,
    id: String,
    body: String,
    project: State<'_, ProjectState>,
    tracker: State<'_, ContentTracker>,
) -> Result<SaveResult, String> {
    let root = project.require_root()?;
    // PST-FR-26 / PST-FR-27, as in the load above. Checked before the write so
    // a refusal leaves the file and any link at that path untouched.
    crate::library::ensure_inside_root(&app, &root, "save_artifact_contents", &id)?;
    let outcome = save_validated_artifact(&root, &id, &body, &tracker);
    if let Err(reason) = &outcome {
        // A refused write is the one thing about a save a reader will come to
        // the Logs panel for, and the PST-FR-28 refusal is invisible otherwise:
        // it is handled, so nothing crashes and no file changes. The artifact's
        // path and the refusal's summary line are structural — neither carries
        // the body, which is user content.
        crate::logging::log_error(
            &app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "artifact save refused",
            crate::log_fields! {
                "artifact" => id.clone(),
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
/// `flow` opens in the Flow tab, every other Markdown artifact in the Editor,
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
    Ok(OpenedArtifact {
        key: id.to_string(),
        kind: ArtifactKind::from_resolved(resolved_type(root, id)),
    })
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
    let root_owned = root.clone();
    let content = move |rel: &str| -> scanning::ContentFacts {
        if !rel.to_ascii_lowercase().ends_with(".md") {
            return scanning::ContentFacts::default();
        }
        match root_owned.read_text(rel) {
            Ok(text) => scanning::read_content_facts(&text),
            Err(_) => scanning::ContentFacts::default(),
        }
    };
    scanning::classify_file(id, &assignments, &content).0
}

/// PST-FR-08 / PST-FR-24: resolve an id to its stable key and the kind hint the
/// UI routes on. An id naming a file the scan surfaces without a resolved
/// artifact type answers `kind = "text"`, so a plain file opens in the Editor as
/// plain text (`../ui/EDT-editor.md` ESH-FR-ATDS) rather than having nowhere to go.
#[tauri::command]
pub fn open_artifact_by_id(
    id: String,
    project: State<'_, ProjectState>,
) -> Result<OpenedArtifact, String> {
    let root = project.require_root()?;
    open_artifact_by_id_impl(&root, &id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::external_divergence;

    // ------------------------------------------------------------------
    // Artifact contents + external-change detection
    // (PST-FR-15 / PST-FR-16, backing `EXC-editor-external-change.md` EXC-FR-WCOM..VNLZ). The command fns need a
    // Tauri runtime (State), but their pure cores (`*_impl`, `external_divergence`)
    // and the `ContentTracker` are exercised here without one.
    // ------------------------------------------------------------------

    #[test]
    fn load_artifact_contents_impl_returns_body_and_checksum_and_records_baseline() {
        // PST-FR-16: load returns body + checksum (the Editor's baseline).
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        std::fs::write(root.join("a.md"), b"# hello\n").unwrap();
        let tracker = ContentTracker::default();
        let got = load_artifact_contents_impl(root, "a.md", &tracker).unwrap();
        assert_eq!(got.body, "# hello\n");
        let expected = root.sha256_file("a.md").unwrap();
        assert_eq!(got.checksum, expected);
        assert_eq!(tracker.current("a.md").as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn save_artifact_contents_impl_writes_atomically_and_returns_checksum() {
        // PST-FR-09, PST-FR-15 (checksum half) / PST-FR-16: save writes the body and
        // returns the checksum of the bytes written; parent dirs are created.
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        let tracker = ContentTracker::default();
        let res = save_artifact_contents_impl(root, "sub/b.md", "BODY", &tracker).unwrap();
        let on_disk = std::fs::read_to_string(root.join("sub/b.md")).unwrap();
        assert_eq!(on_disk, "BODY");
        let expected = root.sha256_file("sub/b.md").unwrap();
        assert_eq!(res.checksum, expected);
        assert_eq!(tracker.current("sub/b.md").as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn save_is_self_write_then_external_edit_is_a_divergence() {
        // PST-FR-17, PST-FR-31: a save records the new checksum, so the watcher's recompute
        // matches -> suppressed; a later EXTERNAL edit changes the checksum ->
        // divergence emitted.
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        let tracker = ContentTracker::default();
        save_artifact_contents_impl(root, "a.md", "v1", &tracker).unwrap();

        // Self-write: the on-disk checksum equals the recorded baseline.
        let after_save = root.sha256_file("a.md").ok();
        assert_eq!(
            external_divergence(tracker.current("a.md").as_deref(), after_save.as_deref()),
            None,
            "the Editor's own save must not raise a divergence"
        );

        // External edit: a different checksum -> divergence carrying the new one.
        std::fs::write(root.join("a.md"), b"v2-external").unwrap();
        let after_ext = root.sha256_file("a.md").ok();
        let div = external_divergence(tracker.current("a.md").as_deref(), after_ext.as_deref());
        assert!(div.is_some(), "an external edit must raise a divergence");
        assert_eq!(div.as_deref(), after_ext.as_deref());
    }

    #[test]
    fn content_tracker_clear_drops_all_baselines() {
        // PST-FR-14: closing/switching a project unmounts the baselines.
        let t = ContentTracker::default();
        t.record("a.md", "x");
        t.record("b.md", "y");
        assert_eq!(t.len(), 2);
        t.clear();
        assert_eq!(t.len(), 0);
        assert!(t.current("a.md").is_none());
    }

    #[test]
    fn artifact_contents_wire_shapes_are_camelcase() {
        let c = serde_json::to_value(ArtifactContents {
            body: "b".into(),
            checksum: "c".into(),
        })
        .unwrap();
        assert!(c.get("body").is_some() && c.get("checksum").is_some());
        let s = serde_json::to_value(SaveResult { checksum: "c".into() }).unwrap();
        assert!(s.get("checksum").is_some());
        // The event payload speaks camelCase (`artifactId`) — pin it.
        let e = serde_json::to_value(ArtifactChangedPayload {
            artifact_id: "id".into(),
            checksum: "c".into(),
        })
        .unwrap();
        assert_eq!(e.get("artifactId").and_then(|v| v.as_str()), Some("id"));
        assert_eq!(e.get("checksum").and_then(|v| v.as_str()), Some("c"));
    }

    #[test]
    fn artifact_content_command_functions_are_in_scope() {
        // Compile-time guard: renaming/removing either command without updating
        // `generate_handler!` / `COMMAND_NAMES` fails to compile here.
        let _ = load_artifact_contents_by_id;
        let _ = save_artifact_contents;
    }

    /// PST-FR-17 / PSS-FR-12: several saves leave the project-local store
    /// **byte-for-byte** what it was.
    ///
    /// The store is seeded with a real section first, and the comparison is over
    /// its bytes rather than over the absence of the file. Asserting that
    /// `local.toml` does not exist would pass in a temp directory where nothing
    /// ever created one — and it would have passed against the implementation
    /// this change removed, which kept its MRU in memory and never wrote that
    /// file either. What PSS-FR-12 claims is that a save records nothing
    /// anywhere, and only a store that exists can prove it.
    #[test]
    fn saves_leave_the_project_local_store_byte_for_byte_what_it_was() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
        let local = dir.path().join(".synthesis/local.toml");
        std::fs::write(&local, "[notes_panel]\nscopePosition = \"all\"\n").unwrap();
        let before = std::fs::read(&local).unwrap();

        let tracker = ContentTracker::default();
        for (id, body) in [
            ("a.spec.md", "ONE"),
            ("b.spec.md", "TWO"),
            ("a.spec.md", "ONE AGAIN"),
        ] {
            save_validated_artifact(root, id, body, &tracker).unwrap();
        }

        assert_eq!(root.read_text(&root.join("a.spec.md")).unwrap(), "ONE AGAIN");
        assert_eq!(root.read_text(&root.join("b.spec.md")).unwrap(), "TWO");
        assert_eq!(
            std::fs::read(&local).unwrap(),
            before,
            "no save records what it saved, here or in the project-local store"
        );
    }

    /// PST-FR-17: a save that fails changes no file, and so moves nothing in the
    /// Dashboard's Recently edited widget — which follows modification times
    /// rather than an account this application keeps of its own writes.
    #[test]
    fn a_failed_save_writes_nothing_and_leaves_no_baseline() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        let tracker = ContentTracker::default();
        let result = save_validated_artifact(root, "../escape.md", "BODY", &tracker);
        assert!(result.is_err(), "a root-escaping id must fail the save");
        assert!(
            tracker.current("../escape.md").is_none(),
            "a refused write leaves no phantom checksum baseline"
        );
    }

    /// PST-FR-31, FGV-FR-12 / PST-FR-28: a valid Flow is written on the ordinary terms.
    #[test]
    fn a_valid_flow_is_written() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        let tracker = ContentTracker::default();
        let body = r#"{"version":1,"name":"ok","nodes":[
            {"id":"n1","name":"a","position":{"x":0,"y":0}}],"edges":[]}"#;
        assert_eq!(
            resolved_type(root, "flows/a.flow"),
            Some(scanning::ArtifactType::Flow),
            "the fixture must actually classify as a Flow or the gate is untested"
        );
        save_validated_artifact(root, "flows/a.flow", body, &tracker).unwrap();
        assert_eq!(root.read_text(&root.join("flows/a.flow")).unwrap(), body);
    }

    /// PST-FR-31, FGV-FR-12 / PST-FR-28: an invalid Flow is refused before the write. The
    /// file keeps the bytes it had, no baseline is recorded (so no self-write
    /// suppression entry is left behind for a write that never happened), and
    /// its modification time is untouched — so the artifact's position in
    /// Recently edited is exactly what it was (PST-FR-31).
    #[test]
    fn an_invalid_flow_is_refused_and_nothing_is_written() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        std::fs::create_dir_all(root.join("flows")).unwrap();
        std::fs::write(root.join("flows/a.flow"), b"ORIGINAL").unwrap();
        let tracker = ContentTracker::default();
        // An edge joining a node inside a loop to one outside it (FGV-FR-12).
        let body = r#"{"version":1,"name":"x","loops":[
            {"id":"l1","name":"l","position":{"x":0,"y":0},"size":{"width":10,"height":10}}
        ],"nodes":[
            {"id":"n1","name":"a","position":{"x":0,"y":0}},
            {"id":"n2","name":"b","parentId":"l1","position":{"x":0,"y":0}}
        ],"edges":[{"id":"e1","from":"n1","to":"n2"}]}"#;
        let err = save_validated_artifact(root, "flows/a.flow", body, &tracker)
            .expect_err("an invalid Flow must be refused");
        assert!(err.contains("could not be written"), "{err}");
        assert!(err.contains("e1"), "the refusal names the offending edge: {err}");
        assert_eq!(
            std::fs::read(root.join("flows/a.flow")).unwrap(),
            b"ORIGINAL",
            "the file on disk keeps the bytes it had"
        );
        assert!(tracker.current("flows/a.flow").is_none());
    }

    /// PST-FR-28: no other artifact type is inspected — the same invalid text
    /// saved as Markdown is written unexamined.
    #[test]
    fn a_markdown_artifact_is_not_validated() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        let tracker = ContentTracker::default();
        save_validated_artifact(root, "a.spec.md", "{ not json", &tracker).unwrap();
        assert_eq!(root.read_text(&root.join("a.spec.md")).unwrap(), "{ not json");
    }

    #[test]
    fn artifact_kind_from_artifact_type_only_flow_is_flow() {
        use scanning::ArtifactType;
        assert_eq!(ArtifactKind::from(ArtifactType::Flow), ArtifactKind::Flow);
        for t in [
            ArtifactType::Skill,
            ArtifactType::Agent,
            ArtifactType::Prompt,
            ArtifactType::Spec,
            ArtifactType::Instructions,
            ArtifactType::Scenario,
            ArtifactType::Scratchpad,
        ] {
            assert_eq!(ArtifactKind::from(t), ArtifactKind::Markdown);
        }
    }

    #[test]
    fn load_artifact_contents_impl_missing_file_is_error_and_records_no_baseline() {
        // Negative path: a non-existent id surfaces a typed error and must not
        // leave a phantom baseline behind.
        let dir = tempfile::TempDir::new().unwrap();
        let tracker = ContentTracker::default();
        let err = load_artifact_contents_impl(&fs::RootFs::for_root(dir.path()), "nope.md", &tracker).unwrap_err();
        assert!(!err.is_empty());
        assert!(tracker.current("nope.md").is_none());
    }

    // ------------------------------------------------------------------
    // PST-FR-31 — line-ending normalisation on write (PST-FR-23)
    // ------------------------------------------------------------------

    /// Configure `root`'s project-public line-ending convention.
    fn set_convention(root: &crate::fs::RootFs, endings: project_settings::LineEndings) {
        project_settings::save_project_config_to(
            root,
            project_settings::ProjectConfig {
                line_endings: endings,
                draft_template: None,
                            ..Default::default()
            },
        )
        .unwrap();
    }

    #[test]
    fn a_crlf_project_writes_crlf_and_checksums_the_written_bytes() {
        // PST-FR-23, PST-FR-31: an LF body submitted to a CRLF project is written entirely
        // with CRLF, and the returned checksum is of those bytes — not of the
        // body that was submitted. That distinction is the whole contract: a
        // checksum over the submitted body would leave the Editor's baseline
        // disagreeing with the file, and the watcher's echo of this very write
        // would raise the external-change modal (EDT-FR-40 / PST-FR-16).
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        set_convention(root, project_settings::LineEndings::Crlf);
        let tracker = ContentTracker::default();

        let submitted = "one\ntwo\nthree\n";
        let res = save_artifact_contents_impl(root, "a.md", submitted, &tracker).unwrap();

        let on_disk = std::fs::read_to_string(root.join("a.md")).unwrap();
        assert_eq!(on_disk, "one\r\ntwo\r\nthree\r\n");
        assert_eq!(res.checksum, root.sha256_file("a.md").unwrap());
        assert_ne!(
            res.checksum,
            fs::sha256_bytes(submitted.as_bytes()),
            "the checksum is of the bytes written, not of the body submitted"
        );
        // And the tracker's baseline is the same one, so the watcher suppresses
        // this write rather than reporting it as an external change.
        assert_eq!(tracker.current("a.md").as_deref(), Some(res.checksum.as_str()));
    }

    #[test]
    fn an_lf_project_flattens_a_mixed_body() {
        // PST-FR-23, PST-FR-31, second half: mixed LF and CRLF in, LF throughout out.
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        set_convention(root, project_settings::LineEndings::Lf);
        let tracker = ContentTracker::default();

        save_artifact_contents_impl(root, "a.md", "one\r\ntwo\nthree\r\n", &tracker).unwrap();
        let on_disk = std::fs::read_to_string(root.join("a.md")).unwrap();
        assert_eq!(on_disk, "one\ntwo\nthree\n");
        assert!(!on_disk.contains('\r'));
    }

    #[test]
    fn an_unconfigured_project_normalises_to_lf() {
        // PSS-FR-17's default reaching the write path: no `project.toml` at all
        // still normalises, it just normalises to LF.
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        let tracker = ContentTracker::default();
        save_artifact_contents_impl(root, "a.md", "one\r\ntwo\r\n", &tracker).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("a.md")).unwrap(),
            "one\ntwo\n"
        );
    }

    #[test]
    fn normalisation_is_idempotent_so_a_resave_changes_no_byte() {
        // A body already in the project's convention must come out
        // byte-identical, or every save would "convert" the file again and the
        // checksum would churn (EDT-FR-40).
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        set_convention(root, project_settings::LineEndings::Crlf);
        let tracker = ContentTracker::default();

        let first = save_artifact_contents_impl(root, "a.md", "one\ntwo\n", &tracker).unwrap();
        let written = std::fs::read_to_string(root.join("a.md")).unwrap();
        let second = save_artifact_contents_impl(root, "a.md", &written, &tracker).unwrap();
        assert_eq!(first.checksum, second.checksum);
        assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), written);
    }

    #[test]
    fn normalisation_leaves_a_body_with_no_line_endings_untouched() {
        // The degenerate case: a single line with no terminator at all is not a
        // line ending to convert, in either convention.
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        set_convention(root, project_settings::LineEndings::Crlf);
        let tracker = ContentTracker::default();
        save_artifact_contents_impl(root, "a.md", "just one line", &tracker).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("a.md")).unwrap(),
            "just one line"
        );
    }

    #[test]
    fn a_bare_cr_is_converted_too() {
        // A stray classic-Mac ending would otherwise survive a "normalisation"
        // and leave the file mixed — the one thing PST-FR-23 rules out.
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        set_convention(root, project_settings::LineEndings::Lf);
        let tracker = ContentTracker::default();
        save_artifact_contents_impl(root, "a.md", "one\rtwo\r\nthree\n", &tracker).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("a.md")).unwrap(),
            "one\ntwo\nthree\n"
        );
    }

    #[test]
    fn the_synthesis_toml_stores_are_unaffected_by_the_convention() {
        // PST-FR-23: only artifact content is normalised. `.synthesis/` metadata
        // goes through the FSA TOML primitive and keeps whatever that writes.
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        set_convention(root, project_settings::LineEndings::Crlf);
        let toml = std::fs::read_to_string(root.join(".synthesis/project.toml")).unwrap();
        assert!(
            !toml.contains('\r'),
            "the TOML writer is not routed through the artifact normalisation: {toml:?}"
        );
    }

    #[test]
    fn load_then_save_same_body_yields_a_stable_checksum() {
        // The whole self-write suppression scheme rests on sha256 being
        // content-stable across load/save/watcher-recompute.
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        let tracker = ContentTracker::default();
        std::fs::write(root.join("a.md"), b"same").unwrap();
        let loaded = load_artifact_contents_impl(root, "a.md", &tracker).unwrap();
        let saved = save_artifact_contents_impl(root, "a.md", "same", &tracker).unwrap();
        assert_eq!(loaded.checksum, saved.checksum);
        assert_eq!(saved.checksum, root.sha256_file("a.md").unwrap());
    }

    // -----------------------------------------------------------------------
    // PST-FR-24 / PST-FR-25 — a plain text file resolves through the same
    // commands, and never joins the recently-edited MRU
    // -----------------------------------------------------------------------

    #[test]
    fn pst_ts26_an_unclassified_file_resolves_as_text_and_round_trips() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), b"fn main() {}\n").unwrap();

        // The routing hint says `text`, with the stable key it was asked for.
        let opened = open_artifact_by_id_impl(root, "src/main.rs").unwrap();
        assert_eq!(opened.key, "src/main.rs");
        assert_eq!(opened.kind, ArtifactKind::Text);
        assert_eq!(
            serde_json::to_value(&opened).unwrap().get("kind").unwrap(),
            "text"
        );

        // And it loads and saves with the same checksum semantics as an artifact.
        let tracker = ContentTracker::default();
        let loaded = load_artifact_contents_impl(root, "src/main.rs", &tracker).unwrap();
        assert_eq!(loaded.body, "fn main() {}\n");
        let saved = save_validated_artifact(root, "src/main.rs", "fn main() { /* edited */ }\n", &tracker)
            .unwrap();
        assert_eq!(
            saved.checksum,
            root.sha256_file("src/main.rs").unwrap(),
            "the checksum is over the bytes that reached disk (PST-FR-15)"
        );
    }

    #[test]
    fn a_classified_artifact_saves_on_the_same_terms() {
        // The other half of PST-FR-23, PST-FR-31: the type a file carries changes how it
        // ROUTES and nothing about how it is written. Both reach disk through
        // the one write path, and neither leaves a record of the save behind
        // (PST-FR-17).
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        std::fs::create_dir_all(root.join(".claude/skills")).unwrap();
        std::fs::write(root.join(".claude/skills/foo.md"), b"body\n").unwrap();

        let tracker = ContentTracker::default();
        save_validated_artifact(root, ".claude/skills/foo.md", "edited\n", &tracker).unwrap();
        assert_eq!(
            root.read_text(&root.join(".claude/skills/foo.md")).unwrap(),
            "edited\n"
        );

        assert_eq!(
            open_artifact_by_id_impl(root, ".claude/skills/foo.md")
                .unwrap()
                .kind,
            ArtifactKind::Markdown
        );
    }

    #[test]
    fn a_flow_resolves_to_the_flow_routing_hint() {
        // PST-FR-08: the hint is what sends a Flow to the Flow tab rather than
        // the Editor, and adding `text` must not disturb it.
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        // ASC-FR-03: the `.flow` path convention, with no frontmatter to
        // fall back on — the only route by which a Flow document classifies.
        std::fs::write(root.join("review.flow"), b"{\"version\":1}").unwrap();
        assert_eq!(
            open_artifact_by_id_impl(root, "review.flow").unwrap().kind,
            ArtifactKind::Flow
        );
    }

    // The ordinary product path: New Artifact records a per-file assignment for
    // whatever type was chosen (PST-FR-21), so a Flow the author named without
    // the extension reaches the Flow canvas through the assignment alone.
    #[test]
    fn a_flow_named_without_the_extension_still_routes_to_the_flow_canvas() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        std::fs::create_dir_all(root.join(".synthesis")).unwrap();
        // What New Artifact leaves behind for `Type: Flow, name: pipeline`: the
        // file, plus the per-file assignment `create_artifact` records (PST-FR-21).
        std::fs::write(root.join("pipeline"), b"{}").unwrap();
        scanning::assign(
            root,
            "pipeline",
            scanning::ArtifactType::Flow,
            scanning::Scope::File,
        )
        .unwrap();

        assert_eq!(
            open_artifact_by_id_impl(root, "pipeline").unwrap().kind,
            ArtifactKind::Flow
        );

        // …and the converse: a `.flow` file assigned another type is not a Flow.
        std::fs::write(root.join("review.flow"), b"{}").unwrap();
        scanning::assign(
            root,
            "review.flow",
            scanning::ArtifactType::Spec,
            scanning::Scope::File,
        )
        .unwrap();
        assert_eq!(
            open_artifact_by_id_impl(root, "review.flow").unwrap().kind,
            ArtifactKind::Markdown
        );
    }

    #[test]
    fn open_artifact_by_id_rejects_a_missing_path_and_one_that_escapes_the_root() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = &crate::fs::RootFs::for_root(dir.path());
        assert!(open_artifact_by_id_impl(root, "nope.md").is_err());
        // FSA-FR-10: a crafted `..` path is rejected rather than resolved.
        assert!(open_artifact_by_id_impl(root, "../../etc/passwd").is_err());
        // A folder is not openable content.
        std::fs::create_dir_all(root.join("dir")).unwrap();
        assert!(open_artifact_by_id_impl(root, "dir").is_err());
    }

    #[test]
    fn artifact_kind_covers_every_routing_destination() {
        for (kind, wire) in [
            (ArtifactKind::Markdown, "markdown"),
            (ArtifactKind::Flow, "flow"),
            (ArtifactKind::Text, "text"),
        ] {
            assert_eq!(serde_json::to_value(kind).unwrap(), wire);
        }
        assert_eq!(ArtifactKind::from_resolved(None), ArtifactKind::Text);
        assert_eq!(
            ArtifactKind::from_resolved(Some(scanning::ArtifactType::Spec)),
            ArtifactKind::Markdown
        );
    }

    #[test]
    fn open_artifact_by_id_command_is_in_scope() {
        // Compile-time guard: renaming/removing it without updating
        // `generate_handler!` / `COMMAND_NAMES` fails to compile here.
        let _ = open_artifact_by_id;
    }
}
