//! Library tree commands + file operations
//! (PST-project-storage.md / ASC-artifact-scanning.md / LIB-library.md).
//!
//! - Tree: `load_project_tree` (PST-FR-07) plus the mutation/rescan commands
//!   (ASC-FR-05/07/11), all backed by the pure scanner in `scanning`.
//! - File ops: `delete_path` / `rename_path` / `copy_path_into_folder`
//!   (PST-FR-18/19/20) resolve a project-relative path against the open project
//!   root and route through the `fs` structural-mutation primitives
//!   (FSA-FR-11/12/13) — no raw `fs::remove`/`rename`/`copy` here (PST-FR-10).
//!   The resulting structural change reaches the UI through the ASC watcher's
//!   `"project tree changed"` event, not a return payload.
//!
//! Operation names match the spec surfaces byte-for-byte (modulo Tauri's
//! snake/camel auto-conversion); the pure `*_impl` halves are unit-tested
//! without a Tauri runtime.

use std::path::Path;

use tauri::{Manager, State};

use crate::fs;
use crate::progress::{self, ProgressRegistry};
use crate::project::{basename, ProjectState};
use crate::scanning;

/// PST-FR-07: return the filesystem-mirroring, type-tagged project tree.
///
/// The scan is one of the operations that attribute progress
/// (`../core/PRG-progress-reporting.md` PRG-FR-11): it walks the whole project,
/// so on a large one it is exactly the "am I wedged or working?" case the status
/// bar exists for. It is scoped to the content root, so a worktree change
/// terminates a scan still running against the outgoing one (PRG-FR-13).
#[tauri::command]
pub fn load_project_tree(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
    progress: State<'_, ProgressRegistry>,
) -> Result<scanning::TreeNode, String> {
    let root = project.require_root()?;
    progress::attribute(
        &app,
        &progress,
        "scan",
        "Indexing project…",
        Some(root.to_path_buf()),
        || Ok(scanning::scan(&root)),
    )
}

/// ASC-FR-11: force a fresh scan and return the current tree. Never throws on a
/// transient filesystem race (the scan skips errored entries).
#[tauri::command]
pub fn rescan_project_tree(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
    progress: State<'_, ProgressRegistry>,
) -> Result<scanning::TreeNode, String> {
    let root = project.require_root()?;
    progress::attribute(
        &app,
        &progress,
        "scan",
        "Indexing project…",
        Some(root.to_path_buf()),
        || Ok(scanning::scan(&root)),
    )
}

/// ASC-FR-05: persist a user type assignment, then return the freshly-scanned
/// tree so the UI can re-tag without a separate reload.
#[tauri::command]
pub fn assign_artifact_type(
    path: String,
    artifact_type: scanning::ArtifactType,
    scope: scanning::Scope,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
    candidates: State<'_, scanning::CandidateStore>,
) -> Result<scanning::TreeNode, String> {
    let root = project.require_root()?;
    scanning::assign(&root, &path, artifact_type, scope).map_err(|e| e.to_string())?;
    republish_classification(&app, &root, &candidates);
    Ok(scanning::scan(&root))
}

/// ASC-FR-07: remove a stored assignment (no-op if absent), then return the
/// freshly-scanned tree.
#[tauri::command]
pub fn clear_artifact_type(
    path: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
    candidates: State<'_, scanning::CandidateStore>,
) -> Result<scanning::TreeNode, String> {
    let root = project.require_root()?;
    scanning::clear(&root, &path).map_err(|e| e.to_string())?;
    republish_classification(&app, &root, &candidates);
    Ok(scanning::scan(&root))
}

/// A stored assignment changed, so every consumer holding a resolved
/// classification is now holding a stale one.
///
/// The watcher carries `.synthesis/library.toml` too (ASC-FR-20), but it does so
/// a debounce window later. This call is what makes an assignment take effect
/// *synchronously* with the tree the command returns, so the UI never renders a
/// type the indexes have not moved yet.
///
/// - The mounted candidate list carries each file's resolved type (ASC-FR-17),
///   which is what `../core/SCC-search.md` groups a hit by (SCC-FR-08), so it
///   is invalidated and the next enumeration rebuilds it.
/// - The BM25 indexes hold a file under the index its type named
///   (`../core/BMI-bm25-indexing.md` BMI-FR-03), so a pass is requested to move
///   it to the index its new type names (BMI-FR-17).
/// - The attribution baseline adopts what we just wrote, so the watcher
///   recognises the echo as our own and does not reload the tree a second time
///   for a change this call already published.
///
/// The ordering is what makes the baseline safe. It is only ever refreshed at a
/// moment when this same call also invalidates the candidate list and requests a
/// pass — so even if an external write lands between our write and our read-back
/// and we adopt *its* bytes, suppressing its watcher event, nothing goes stale:
/// the pass requested here picks that content up regardless.
pub(crate) fn republish_classification<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    candidates: &scanning::CandidateStore,
) {
    if let Some(baseline) = app.try_state::<scanning::AttributionBaseline>() {
        baseline.record_from_disk(root);
    }
    candidates.invalidate();
    crate::bm25_index::request_pass(app, crate::bm25_index::PassScope::ARTIFACTS);
}

/// The typed error string a caller sees when a path leaves the project root —
/// through a `..` segment, an absolute value, or a symbolic-link component
/// (PST-FR-26 / PST-FR-27). One constant so the UI can match on it and the log
/// record and the returned error never drift apart.
pub(crate) const ERR_PATH_ESCAPES_ROOT: &str = "path escapes project root";

/// PST-FR-18: the typed "not empty" a non-recursive folder delete returns. The
/// Library matches on this to raise its recursive-delete confirmation
/// (`../ui/LCM-library-context-menu.md` LCM-FR-11), so it is a contract string
/// rather than a message.
pub(crate) const ERR_NOT_EMPTY: &str = "directory not empty";

/// PST-FR-19: the typed error for a rename whose `new_name` is what the entry is
/// already called. Distinct from the sibling-collision error, which is what a
/// self-collision would otherwise be reported as — a misleading diagnosis, since
/// nothing is in the way except the file itself.
pub(crate) const ERR_UNCHANGED_NAME: &str = "unchanged name";

/// PST-FR-26: verify `path` resolves inside `root` **before** any FSA primitive
/// is invoked with it, and emit exactly one `ERROR` record when it does not.
///
/// This is deliberately the module's own check rather than an inherited
/// consequence of FSA-FR-10. Two reasons it earns its place despite the
/// primitives rejecting the same paths underneath it: it runs before any
/// primitive is called at all, so a multi-step operation cannot perform half of
/// itself and then be stopped; and it is the layer that knows the *operation*
/// name, which is what makes the log record worth reading.
///
/// A path any component of which is a symbolic link resolves outside the root
/// for this purpose (PST-FR-27) — the refusal is decided from the link, never
/// from where it points.
///
/// Logs the operation and the offending path. A path is not user content and
/// carries no credential, so it is safe in a record; nothing of the file's
/// *contents* is read, let alone logged.
pub(crate) fn ensure_inside_root<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    operation: &str,
    path: &str,
) -> Result<(), String> {
    match fs::resolve_inside(root, path) {
        Ok(()) => Ok(()),
        Err(reason) => {
            crate::logging::log_error(
                app,
                &crate::logging::BUFFER,
                &[crate::logging::Domain::Backend],
                "file operation refused: path escapes project root",
                crate::log_fields! {
                    "operation" => operation,
                    "path" => path,
                    "reason" => reason,
                },
            );
            Err(ERR_PATH_ESCAPES_ROOT.to_string())
        }
    }
}

/// Map an `FsError` from a structural mutation onto the typed contract strings
/// the UI matches against, so "not empty" stays distinguishable from every other
/// failure once it has crossed the wire as a string.
fn mutation_error(e: fs::FsError) -> String {
    match e {
        fs::FsError::NotEmpty { .. } => ERR_NOT_EMPTY.to_string(),
        // All three are "this path is not one the project may act on", and the
        // UI matches on the one string. `EscapesAllowedRoots` is the sandbox's
        // refusal and `PathEscape` the module's own; without them a refusal
        // would cross the wire as a raw message carrying an absolute path, and
        // the UI's typed match would fall through.
        fs::FsError::SymlinkRefused { .. }
        | fs::FsError::EscapesAllowedRoots { .. }
        | fs::FsError::PathEscape { .. } => ERR_PATH_ESCAPES_ROOT.to_string(),
        other => other.to_string(),
    }
}

/// PST-FR-18 core: delete the file or folder at the project-relative `path`.
///
/// `recursive = false` removes a file, or a folder that is already empty, and
/// returns the typed `ERR_NOT_EMPTY` for a folder holding anything — which is
/// what lets the Library ask before removing a subtree without ever having to
/// decide emptiness from the tree it renders (LCM-FR-11). Emptiness is read from
/// the filesystem, so a folder holding only gitignored content still reports as
/// non-empty even though the scan shows it as bare.
fn delete_path_impl(root: &fs::RootFs, path: &str, recursive: bool) -> Result<(), String> {
    root.delete_under(root.path(), path, recursive).map_err(mutation_error)
}

/// The project-relative path an entry lands at when `path` is renamed to the
/// bare basename `new_name`: its existing parent folder, with the new basename.
/// Pure so the join rule is unit-testable beside `paste_destination`.
fn renamed_path(path: &str, new_name: &str) -> String {
    match path.trim_end_matches('/').rsplit_once('/') {
        Some((parent, _)) => format!("{parent}/{new_name}"),
        None => new_name.to_string(),
    }
}

/// PST-FR-19 core: rename the node at `path` to the bare basename `new_name`.
///
/// `NTC-notes-storage.md` NTC-FR-11: an entity id is a project-relative path
/// (ASC-FR-13), so this rename would otherwise orphan every note attached to the
/// renamed file — or to anything inside a renamed folder. This is the one place
/// the application knows a single old path became a single new path, which is
/// exactly the correlation NTC-FR-11 asks for, so the notes are carried across
/// here. Best-effort by design: a note that cannot be rewritten surfaces as
/// unresolved (NTC-FR-10) rather than failing a rename that already happened.
///
/// `CMS-comments-storage.md` CMS-FR-24 rides on the same correlation for the same
/// reason: a comment log is named from the artifact's path, so a rename that did
/// not carry the log across would strand every thread on the renamed file.
fn rename_path_impl(
    root: &fs::RootFs,
    comment_store: &fs::RootFs,
    path: &str,
    new_name: &str,
) -> Result<(), String> {
    // PST-FR-19: a rename that would change nothing is an error rather than a
    // successful no-op, so a caller can never mistake it for work performed.
    // Checked here rather than left to the primitive because the primitive would
    // report it as a *collision* — technically true (the destination exists) but
    // a misleading account of what happened, since the only thing in the way is
    // the file itself. No filesystem call is made.
    if basename(path) == new_name {
        return Err(ERR_UNCHANGED_NAME.to_string());
    }
    root.rename_under(root.path(), path, new_name).map_err(mutation_error)?;
    let renamed = renamed_path(path, new_name);
    crate::notes::follow_rename(root, path, &renamed);
    // CMS-FR-24: the log that follows the rename lives in the repository machine
    // store, and what moved lives in the worktree, so the correlation takes
    // both.
    crate::comments::follow_rename(comment_store, root, path, &renamed);
    Ok(())
}

/// Compute the project-relative destination for pasting `source_path` into
/// `dest_folder`: `dest_folder/<basename(source_path)>`, or just the basename
/// when `dest_folder` is the project root (empty path). Pure so the join rule is
/// unit-testable.
fn paste_destination(dest_folder: &str, source_path: &str) -> String {
    let name = basename(source_path);
    let folder = dest_folder.trim_end_matches('/');
    if folder.is_empty() {
        name
    } else {
        format!("{folder}/{name}")
    }
}

/// PST-FR-20 core: copy `source_path` into `dest_folder` as
/// `dest_folder/<basename>` via the FSA copy primitive (which rejects a
/// pre-existing destination — the collision case — without overwriting).
fn copy_path_into_folder_impl(
    root: &fs::RootFs,
    source_path: &str,
    dest_folder: &str,
) -> Result<(), String> {
    let dest = paste_destination(dest_folder, source_path);
    root.copy_under(root.path(), source_path, &dest).map_err(mutation_error)
}

/// Validate a new artifact's basename (PST-FR-21 / NAW-FR-08): non-empty after
/// trimming, not `.`/`..`, and free of path separators. Mirrors the rename
/// constraint (PST-FR-19) so the modal's client-side check and the backend agree.
/// Pure so the rule is unit-testable on its own.
fn is_valid_artifact_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty() && n != "." && n != ".." && !n.contains('/') && !n.contains('\\')
}

/// Join an optional location folder with a (trimmed) basename into a project-
/// relative path. An absent / empty / whitespace location means the project root,
/// so the artifact is created at `<name>` (PST-FR-21). Pure.
fn artifact_rel_path(location: Option<&str>, name: &str) -> String {
    let name = name.trim();
    match location {
        Some(loc) if !loc.trim().is_empty() => {
            let folder = loc.trim().trim_matches('/');
            if folder.is_empty() {
                name.to_string()
            } else {
                format!("{folder}/{name}")
            }
        }
        _ => name.to_string(),
    }
}

/// Find the node at project-relative `rel` within a scanned tree (used to return
/// the freshly-created artifact's node). Pure over the tree.
fn find_node<'a>(node: &'a scanning::TreeNode, rel: &str) -> Option<&'a scanning::TreeNode> {
    if node.path == rel {
        return Some(node);
    }
    node.children
        .as_ref()
        .and_then(|cs| cs.iter().find_map(|c| find_node(c, rel)))
}


/// The node a just-created plain file has in the project tree.
///
/// The counterpart of `created_folder_node`, and for the same reason: the scan
/// honours the project's ignore rules (ASC-FR-09), so a file whose path a
/// `.gitignore` rule covers — `.env`, anything under `dist/` or `target/` — is
/// legitimately absent from it. Creating a plain file is exactly when that
/// happens, far more often than it does for an artifact, and the creation still
/// succeeded: PST-FR-26's "return the new file node" is answered with the node
/// the file would have had rather than with a spurious error about a file that is
/// sitting on disk. The Editor can still open it, because `open_artifact_by_id`
/// resolves a path rather than a scan entry.
///
/// Carries no `artifact_type`: `create_file` records no assignment, and a node
/// this fallback produces is one the scan does not classify at all.
fn created_file_node(rel: &str) -> scanning::TreeNode {
    scanning::TreeNode {
        id: rel.to_string(),
        name: basename(rel),
        path: rel.to_string(),
        node_kind: scanning::NodeKind::File,
        artifact_type: None,
        type_source: None,
        display_name: None,
        has_artifacts: None,
        children: None,
    }
}

/// PST-FR-26 core: create an empty file named `name` inside `location` (the
/// project root when `location` is absent/empty) and return the created node.
///
/// `name` is the file's complete basename, extension included, and nothing here
/// requires, derives, appends, or validates an extension: `Makefile`,
/// `.gitignore`, and `helpers.ts` are all accepted as given (NFI-FR-05). The one
/// rule is that it be a bare basename, so one invocation creates one file rather
/// than a file plus the folders leading to it.
///
/// Records no assignment at any scope: the file's type is whatever ASC-FR-06
/// resolves for its path — an inference, an inherited folder-scope assignment, or
/// none — and there is no body to normalise, since the file is created empty.
fn create_file_impl(root: &fs::RootFs, location: Option<&str>, name: &str) -> Result<scanning::TreeNode, String> {
    if !is_valid_artifact_name(name) {
        return Err(format!("invalid name: {name:?}"));
    }
    // The join rule is the one artifact and folder creation already use: an
    // absent/empty location means the project root.
    let rel = artifact_rel_path(location, name);
    // FSA-FR-10 escape gate + the absolute on-disk target.
    let target = fs::resolve_under(root, &rel).map_err(|e| e.to_string())?;
    // PST-FR-26: never overwrite or truncate — a pre-existing entry of that name,
    // file or folder alike, is a collision and nothing is written. The atomic
    // write primitive would happily replace a file, so the guard has to be here.
    // `symlink_metadata` does not follow a final symlink, so a dangling link
    // still counts as occupying the name.
    if root.file_info(&target).is_ok() {
        return Err(format!("already exists: {rel}"));
    }
    // An empty body: there is nothing to apply the project's line-ending
    // convention to (PST-FR-23), so no normalisation step is involved.
    root.write_text_atomic(&target, "").map_err(|e| e.to_string())?;
    let tree = scanning::scan(root);
    Ok(find_node(&tree, &rel)
        .cloned()
        .unwrap_or_else(|| created_file_node(&rel)))
}

/// PST-FR-29: the typed validation error a `create_typed_file` carrying no
/// artifact type is refused with. Named rather than formatted so the UI can
/// recognise it and the requirement is testable.
pub const ERR_TYPED_FILE_NEEDS_A_TYPE: &str =
    "an artifact type is required to create a typed file";

/// PST-FR-29 core: create the empty file **and** record its file-scope type
/// assignment as **one transaction**, for the New Artifact window
/// (`../ui/NTA-new-typed-artifact.md`).
///
/// The file half is exactly [`create_file_impl`]'s — the same complete-basename
/// rule, the same refusal of an empty name, a name holding a path separator, or
/// a destination escaping the root, and the same typed collision against any
/// existing entry of that name — because the two operations differ in the
/// assignment and in nothing else.
///
/// The assignment half goes through the one attribution write of
/// `ASC-artifact-scanning.md` (ASC-FR-05, ASC-FR-24) at `scope = file`, so a
/// file created with a type and a file typed afterwards from the Project
/// context menu end in a byte-identical stored assignment and resolve
/// identically under ASC-FR-06.
///
/// **Either both land or neither does.** A file write that fails records no
/// assignment, because it never reaches this far. An assignment write that
/// fails after the file was written takes the file back off disk before
/// returning, and clears whatever the failed write may have left in
/// `.synthesis/library.toml`, so no partially-created file and no assignment
/// naming a path the project does not have survives the call (ASC-FR-24). The
/// caller is told the creation failed and is never asked to repair a
/// half-completed one (`../ui/NTA-new-typed-artifact.md` NTA-FR-16).
///
/// Creates no draft, reaches nothing under `.synthesis/drafts/`, and reads no
/// draft template: this operation writes one project file and records one type
/// (PST-FR-29, per `DRS-draft-storage.md` DRS-FR-39).
fn create_typed_file_impl(
    root: &fs::RootFs,
    location: Option<&str>,
    name: &str,
    artifact_type: Option<scanning::ArtifactType>,
) -> Result<scanning::TreeNode, String> {
    // PST-FR-29: the type is required, and a call carrying none creates
    // nothing. Checked before the name so a caller missing both is told about
    // the one that separates this operation from `create_file`.
    let Some(artifact_type) = artifact_type else {
        return Err(ERR_TYPED_FILE_NEEDS_A_TYPE.to_string());
    };
    if !is_valid_artifact_name(name) {
        return Err(format!("invalid name: {name:?}"));
    }
    let rel = artifact_rel_path(location, name);
    let target = fs::resolve_under(root, &rel).map_err(|e| e.to_string())?;
    // PST-FR-29 / PST-FR-26: never overwrite or truncate. A pre-existing entry
    // of that name, file or folder alike, is a collision, and the existing
    // file's bytes are left untouched.
    if root.file_info(&target).is_ok() {
        return Err(format!("already exists: {rel}"));
    }
    // NTA-FR-11: zero bytes. The window captures no content, so a Flow created
    // here opens on the empty graph its empty body deserializes to
    // (`../core/FGV-flow-graph-validation.md` FGV-FR-05) rather than in an
    // error state. Nothing to normalise for PST-FR-23 either.
    root.write_text_atomic(&target, "").map_err(|e| e.to_string())?;
    if let Err(e) = scanning::assign(root, &rel, artifact_type, scanning::Scope::File) {
        // ASC-FR-24 / PST-FR-29: put the project back exactly as this call
        // found it. Best-effort, in that a rollback which cannot proceed must
        // not replace the real error with its own — but attempted in both
        // directions, because an assignment write can fail after having
        // rewritten `library.toml` and a stored assignment naming a path the
        // project does not have is precisely what ASC-FR-24 forbids.
        let _ = root.delete_under(root.path(), &rel, false);
        let _ = scanning::clear(root, &rel);
        return Err(e.to_string());
    }
    let tree = scanning::scan(root);
    Ok(find_node(&tree, &rel).cloned().unwrap_or_else(|| {
        // The scan honours the project's ignore rules (ASC-FR-09), so a file
        // whose path a `.gitignore` rule covers is legitimately absent from it.
        // The creation still succeeded, and the assignment still stands, so the
        // node the file would have had is returned rather than a spurious error
        // — carrying the type the caller chose, at the precedence ASC-FR-06
        // gives a file-scope assignment.
        let mut node = created_file_node(&rel);
        node.artifact_type = Some(artifact_type);
        node.type_source = Some(scanning::TypeSource::Assigned);
        node
    }))
}

/// Undo a `create_folder` that failed after the directory was already made, so a
/// failure leaves nothing behind (PST-FR-25).
///
/// Removes only the folder this call just created, and only while it is still
/// empty, so a file some other writer dropped into it in the meantime is never
/// destroyed — that folder is left in place instead, which is the safe way to
/// lose the race. Best-effort: a rollback that cannot proceed must not replace
/// the real error with its own.
fn rollback_created_folder(root: &fs::RootFs, rel: &str) {
    let Ok(abs) = fs::resolve_under(root, rel) else {
        return;
    };
    let empty = root
        .list_dir(&abs)
        .map(|entries| entries.is_empty())
        .unwrap_or(false);
    if empty {
        // PST-FR-10: through the FSA primitive, not a raw `fs::remove`.
        let _ = root.delete_under(root.path(), rel, true);
    }
}

/// The node a just-created folder has in the project tree.
///
/// Normally read back from the scan, but the scan honours the project's ignore
/// rules (ASC-FR-09), so a folder whose name a `.gitignore` rule covers — `dist`,
/// `build`, `target`, `node_modules` — is legitimately absent from it. The
/// creation still succeeded, so PST-FR-25's "return the new folder node" is
/// answered with the node the folder would have had rather than with a spurious
/// error about a folder that is sitting on disk.
fn created_folder_node(
    rel: &str,
    artifact_type: Option<scanning::ArtifactType>,
) -> scanning::TreeNode {
    scanning::TreeNode {
        id: rel.to_string(),
        name: basename(rel),
        path: rel.to_string(),
        node_kind: scanning::NodeKind::Folder,
        artifact_type,
        // A folder's type is always an assignment; there is no other route to one
        // (ASC-FR-18).
        type_source: artifact_type.map(|_| scanning::TypeSource::Assigned),
        display_name: None,
        // ASC-FR-08: a folder-scope assignment is itself enough to count as
        // artifact-bearing; without one a brand-new folder holds nothing.
        has_artifacts: Some(artifact_type.is_some()),
        children: Some(Vec::new()),
    }
}

/// PST-FR-25 core: create an empty folder named `name` inside `location` (the
/// project root when `location` is absent/empty) and return the created node.
///
/// `name` must be a bare basename, so one invocation creates one folder rather
/// than a chain of nested ones. An invalid name, a destination escaping the
/// project root, or a collision with any existing entry — file or folder alike —
/// is an error and nothing is created: `fs::create_dir` refuses to adopt or
/// clear what is already there (FSA-FR-14), which is why the raw
/// `fs::create_dir_all` PST-FR-10 forbids would be wrong here even ignoring the
/// primitive rule.
///
/// The new folder carries a **folder**-scope assignment (ASC-FR-05, scope =
/// folder) for the type it ends up with, so the files it later holds resolve to
/// that type by inheritance (ASC-FR-06). That type is the caller's `artifact_type`
/// when one was supplied and otherwise the type the new folder inherits from the
/// nearest ancestor folder that carries one — a folder created inside a typed
/// folder is of that type, and is tagged and filtered as its parent is instead of
/// disappearing under the **All Artifacts** lens. Only a folder with no typed
/// ancestor is left untyped, its future contents falling to inference.
fn create_folder_impl(
    root: &fs::RootFs,
    location: Option<&str>,
    name: &str,
    artifact_type: Option<scanning::ArtifactType>,
) -> Result<scanning::TreeNode, String> {
    if !is_valid_artifact_name(name) {
        return Err(format!("invalid name: {name:?}"));
    }
    // The join rule is the same one artifact creation uses: an absent/empty
    // location means the project root (PST-FR-21 / PST-FR-25).
    let rel = artifact_rel_path(location, name);
    // Resolved BEFORE the folder exists. Afterwards the folder is its own nearest
    // ancestor candidate only if it already carried an assignment, which it cannot
    // — but reading first also keeps the lookup independent of the creation.
    let inherited = scanning::inherited_folder_type(root, &rel);
    // PST-FR-10: through the FSA primitive, which owns both the FSA-FR-10 escape
    // gate and the typed "exists" collision.
    root.create_dir_under(root.path(), &rel).map_err(|e| e.to_string())?;
    let effective_type = artifact_type.or(inherited);
    if let Some(t) = effective_type {
        if let Err(e) = scanning::assign(root, &rel, t, scanning::Scope::Folder) {
            rollback_created_folder(root, &rel);
            return Err(e.to_string());
        }
    }
    let tree = scanning::scan(root);
    Ok(find_node(&tree, &rel)
        .cloned()
        .unwrap_or_else(|| created_folder_node(&rel, effective_type)))
}

/// PST-FR-18: delete a project file or folder
/// (`"delete path (path, recursive)"`).
///
/// `recursive = false` is the narrow delete *and* the emptiness probe: a folder
/// holding anything comes back as `ERR_NOT_EMPTY` with nothing removed, which is
/// how the Library learns it must ask before taking a subtree (LCM-FR-11).
#[tauri::command]
pub fn delete_path(
    app: tauri::AppHandle,
    path: String,
    recursive: bool,
    project: State<'_, ProjectState>,
) -> Result<(), String> {
    let root = project.require_root()?;
    ensure_inside_root(&app, &root, "delete_path", &path)?;
    delete_path_impl(&root, &path, recursive)
}

/// PST-FR-19: rename a project file or folder within its parent (`"rename path"`).
#[tauri::command]
pub fn rename_path(
    app: tauri::AppHandle,
    path: String,
    new_name: String,
    project: State<'_, ProjectState>,
) -> Result<(), String> {
    let root = project.require_root()?;
    ensure_inside_root(&app, &root, "rename_path", &path)?;
    // RMS-FR-HAJC / CMS-FR-24: resolved **before** the rename, so a store that
    // cannot be reached refuses the operation with nothing written. Resolving it
    // afterwards would leave the rename landed and every thread on the renamed
    // file stranded under its old log id, which no later call repairs.
    let store = project.require_store()?;
    rename_path_impl(&root, &store, &path, &new_name)
}

/// PST-FR-20: copy a project file or folder into a destination folder
/// (`"copy path into folder"`). Copy, not move — the source is left in place.
#[tauri::command]
pub fn copy_path_into_folder(
    app: tauri::AppHandle,
    source_path: String,
    dest_folder: String,
    project: State<'_, ProjectState>,
) -> Result<(), String> {
    let root = project.require_root()?;
    // PST-FR-26 names both sides: a copy composes two paths, and either can be
    // the one that leaves the root.
    ensure_inside_root(&app, &root, "copy_path_into_folder", &source_path)?;
    ensure_inside_root(&app, &root, "copy_path_into_folder", &dest_folder)?;
    copy_path_into_folder_impl(&root, &source_path, &dest_folder)
}


/// PST-FR-26: create a plain file in the open project (`"create file (location,
/// name)"`) and return the created node so the UI can open and reveal it. The
/// structural change also reaches the Library through the ASC watcher's
/// `"project tree changed"` event.
#[tauri::command]
pub fn create_file(
    app: tauri::AppHandle,
    location: Option<String>,
    name: String,
    project: State<'_, ProjectState>,
) -> Result<scanning::TreeNode, String> {
    let root = project.require_root()?;
    // PST-FR-26: the location is the caller-composed part of the destination, so
    // it is what has to be proven inside the root before anything is written.
    // The name is validated separately as a bare basename.
    if let Some(loc) = location.as_deref() {
        ensure_inside_root(&app, &root, "create_file", loc)?;
    }
    create_file_impl(&root, location.as_deref(), &name)
}

/// PST-FR-25: create a new folder in the open project
/// (`"create folder (location, name, artifact type)"`) and return the created
/// node so the UI can reveal it. The structural change also reaches the Library
/// through the ASC watcher's `"project tree changed"` event.
#[tauri::command]
pub fn create_folder(
    app: tauri::AppHandle,
    location: Option<String>,
    name: String,
    artifact_type: Option<scanning::ArtifactType>,
    project: State<'_, ProjectState>,
) -> Result<scanning::TreeNode, String> {
    let root = project.require_root()?;
    // PST-FR-26, as in `create_file`: the location is the caller-composed part.
    if let Some(loc) = location.as_deref() {
        ensure_inside_root(&app, &root, "create_folder", loc)?;
    }
    let node = create_folder_impl(&root, location.as_deref(), &name, artifact_type)?;
    // PST-FR-25 records a folder-scope assignment when the new folder carries or
    // inherits a type, which is an attribution write like any other: it changes
    // what the files placed under it will resolve to (ASC-FR-06), and it changes
    // the `has_artifacts` the panel renders from (ASC-FR-18). Published here for
    // the same reason `assign_artifact_type` publishes — and so the baseline
    // adopts what we wrote, keeping the watcher from reloading a second time for
    // the write the `mkdir` above is already going to refresh the tree for.
    //
    // Not threaded into `create_folder_impl`: that signature is shared with the
    // test call sites, which have no `AppHandle` to give it.
    // `state`, not `try_state`: this is the command path, where the store is
    // always managed (`lib.rs`), and a missing one is a wiring bug worth a
    // panic rather than a silently skipped republish. The `try_state` one
    // function up guards a different caller — the watcher's notify thread.
    republish_classification(&app, &root, &app.state::<scanning::CandidateStore>());
    Ok(node)
}

/// PST-FR-29: create a typed artifact file in the open project
/// (`"create typed file (location, name, artifact type)"`) and return the
/// created node so the UI can open, reveal, and select it
/// (`../ui/NTA-new-typed-artifact.md` NTA-FR-13). The structural change also
/// reaches the Project panel through the ASC watcher's `"project tree changed"`
/// event (ASC-FR-10).
#[tauri::command]
pub fn create_typed_file(
    app: tauri::AppHandle,
    location: Option<String>,
    name: String,
    artifact_type: Option<scanning::ArtifactType>,
    project: State<'_, ProjectState>,
) -> Result<scanning::TreeNode, String> {
    let root = project.require_root()?;
    // PST-FR-26, as in `create_file`: the location is the caller-composed part
    // of the destination, so it is what has to be proven inside the root before
    // anything is written. The name is validated separately as a bare basename.
    if let Some(loc) = location.as_deref() {
        ensure_inside_root(&app, &root, "create_typed_file", loc)?;
    }
    let node = create_typed_file_impl(&root, location.as_deref(), &name, artifact_type);
    match &node {
        Ok(created) => crate::logging::log_info(
            &app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "typed artifact file created",
            // The path and the type, which is what a reader needs to follow the
            // creation through the tree and the attribution store. The file is
            // empty, so there is no content here to keep out of the record.
            crate::log_fields! {
                "path" => created.path.clone(),
                "artifact_type" => format!("{:?}", created.artifact_type),
            },
        ),
        Err(reason) => crate::logging::log_warn(
            &app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "typed artifact file creation refused",
            // The refusal is shown inline in the window (NTA-FR-16), which is
            // the one place it is otherwise visible — and a refused creation
            // rolled back is exactly the handled path that leaves no other
            // trace. `name` is a filename the author typed, not their content.
            crate::log_fields! {
                "location" => location.clone().unwrap_or_default(),
                "name" => name.clone(),
                "reason" => reason.clone(),
            },
        ),
    }
    let node = node?;
    // PST-FR-29 records a file-scope assignment, which is an attribution write
    // like any other: it changes what the file resolves to (ASC-FR-06) and what
    // the panel renders from. Published here for the same reason
    // `assign_artifact_type` and `create_folder` publish — and so the baseline
    // adopts what we wrote, keeping the watcher from reloading a second time.
    republish_classification(&app, &root, &app.state::<scanning::CandidateStore>());
    Ok(node)
}

#[cfg(test)]
mod tests;
