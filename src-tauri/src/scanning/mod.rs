//! Artifact scanning — the backend engine behind the Library tree.
//! See `specifications/core/ASC-artifact-scanning.md`.
//!
//! Responsibilities (all owned here per the ASC spec):
//! - recursive scan of the project root into a filesystem-mirroring tree
//!   (`scan` / `build_tree`, ASC-FR-01);
//! - classification into the fixed built-in artifact-type set via
//!   multi-ecosystem path inference + a content-marker tiebreak + user
//!   assignments, resolved by a fixed precedence (ASC-FR-02..ASC-FR-06);
//! - persistence of user assignments in committed `.synthesis/library.toml`
//!   (`assign` / `clear`, ASC-FR-05 / ASC-FR-07);
//! - per-folder `has_artifacts` for empty-folder hiding (ASC-FR-08);
//! - a debounced change summary the watcher in `lib.rs` emits (ASC-FR-10).
//!
//! - the project index, built from a parallel walk and kept current by the
//!   watcher, which every tree read answers from (`index`, ASC-FR-HPUY);
//! - repository boundaries inside the project (`repos`, ASC-FR-SFCR).
//!
//! The Tauri-facing commands live in `library.rs` and the recursive filesystem
//! watcher in `watcher.rs` (they need the Tauri runtime / `AppHandle`); this
//! module keeps every classification and tree-shaping decision pure and
//! unit-tested. All reads/writes go through `crate::fs` (FSA, ASC-FR-12).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Mutex;

use notify_debouncer_mini::DebounceEventResult;
use serde::{Deserialize, Serialize};

use crate::fs as fsa;

mod ignored;
mod index;
mod index_query;
mod inference;
pub mod repos;
mod store;
mod walk;
pub use ignored::drop_gitignored;
pub use index::{affected_folders, node_at, ProjectIndex};
pub use index_query::{FilterResult, FolderChildren, Lens, LensName, PathKind, PathSearch};
use inference::names_itself;
pub use inference::{infer_from_content, infer_from_path, read_content_facts, ContentFacts};
pub use repos::NestedRepo;
pub use store::{candidate_files, Candidate, CandidateStore, EVERY_TYPE};

// ---------------------------------------------------------------------------
// Fixed built-in type set + node shape (ASC-FR-02, tree node shape)
// ---------------------------------------------------------------------------

/// The fixed built-in artifact-type set (ASC-FR-02). `Flow` is what the UI
/// opens in the Flow tab (`../ui/FLO-flow.md`). `Architecture` is a FINOS CALM
/// architecture document, and only its `*.calm.json` name or an assignment
/// produces it (ASC-FR-QVCN). `Doc` makes a file or folder a
/// source of the Documents collection; only an assignment produces it
/// (ASC-FR-JWPH). No other type is produced in v1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactType {
    Skill,
    Agent,
    Prompt,
    Spec,
    /// `harness` is accepted on the way in so a `.synthesis/library.toml`
    /// written before the type was named `flow` still deserialises — dropping
    /// it would silently degrade every stored assignment to "no overrides"
    /// (`load_assignments` swallows a parse failure to keep the Library
    /// rendering). Only `flow` is ever written back out.
    #[serde(alias = "harness")]
    Flow,
    Architecture,
    Instructions,
    Scenario,
    Scratchpad,
    Doc,
}

impl ArtifactType {
    /// The type's canonical wire name — the same string `serde` writes, so a
    /// value named in prose and a value stored in `.synthesis/library.toml` are
    /// the one vocabulary rather than two that drift.
    ///
    /// Used where a type has to be *stated* rather than serialised: the artifact
    /// type an agent's input names for the material under discussion
    /// (`../core/AGC-agent-conversations.md` AGC-FR-07).
    pub fn as_str(self) -> &'static str {
        match self {
            ArtifactType::Skill => "skill",
            ArtifactType::Agent => "agent",
            ArtifactType::Prompt => "prompt",
            ArtifactType::Spec => "spec",
            ArtifactType::Flow => "flow",
            ArtifactType::Architecture => "architecture",
            ArtifactType::Instructions => "instructions",
            ArtifactType::Scenario => "scenario",
            ArtifactType::Scratchpad => "scratchpad",
            ArtifactType::Doc => "doc",
        }
    }

    /// AGC-FR-07: whether an agent is told this type. `doc` is not reported, so
    /// the agent-facing set stays at the other nine types.
    pub fn is_agent_facing(self) -> bool {
        self != ArtifactType::Doc
    }

    /// Parse a content-marker / TOML value into a built-in type. Accepts the
    /// canonical lowercase names, plus `harness` for `flow` on the same terms
    /// as the serde alias above. `doc` is not a marker: only an assignment
    /// gives that type (ASC-FR-JWPH). `architecture` is not a marker either:
    /// only the `*.calm.json` name or an assignment gives that type
    /// (ASC-FR-QVCN).
    fn from_marker(s: &str) -> Option<ArtifactType> {
        match s.trim().to_ascii_lowercase().as_str() {
            "skill" => Some(ArtifactType::Skill),
            "agent" => Some(ArtifactType::Agent),
            "prompt" => Some(ArtifactType::Prompt),
            "spec" => Some(ArtifactType::Spec),
            "flow" | "harness" => Some(ArtifactType::Flow),
            "instructions" => Some(ArtifactType::Instructions),
            "scenario" => Some(ArtifactType::Scenario),
            "scratchpad" => Some(ArtifactType::Scratchpad),
            _ => None,
        }
    }
}

/// Which precedence level produced a node's type (ASC-FR-06).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TypeSource {
    Inferred,
    Assigned,
    Inherited,
}

/// Scope of a user assignment (ASC-FR-05).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    File,
    Folder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Folder,
    File,
}

/// A node in the filesystem-mirroring tree. Serialised camelCase for the
/// frontend. Optional fields are omitted when absent so the wire shape matches
/// the `TreeNode` interface in `src/types.ts`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    /// Stable, project-relative-path-derived key (ASC-FR-13).
    pub id: String,
    /// Basename.
    pub name: String,
    /// Project-relative path (forward-slash separated).
    pub path: String,
    pub node_kind: NodeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_type: Option<ArtifactType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_source: Option<TypeSource>,
    /// ASC-FR-19: the name the file declares for itself, for the artifacts whose
    /// filename does not identify them. Absent for every other node, which is
    /// what tells a caller to fall back to the basename.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Folders only: subtree contains at least one artifact (ASC-FR-08).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_artifacts: Option<bool>,
    /// Folders only: the types present in the subtree (ASC-FR-OGRB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtree_types: Option<Vec<ArtifactType>>,
    /// Folders only: whether the folder holds a node (ASC-FR-YSWU).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_children: Option<bool>,
    /// Folders only: the repository boundary the folder is (ASC-FR-YLGN).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nested_repo: Option<NestedRepo>,
    /// Folders only, and only in a full tree or a filter result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<TreeNode>>,
}

impl TreeNode {
    /// A file node with no classification.
    pub fn plain_file(rel: &str) -> TreeNode {
        TreeNode {
            id: rel.to_string(),
            name: rel.rsplit('/').next().unwrap_or(rel).to_string(),
            path: rel.to_string(),
            node_kind: NodeKind::File,
            artifact_type: None,
            type_source: None,
            display_name: None,
            has_artifacts: None,
            subtree_types: None,
            has_children: None,
            nested_repo: None,
            children: None,
        }
    }

    /// An empty folder node with its own folder-scope type, if any.
    pub fn empty_folder(rel: &str, artifact_type: Option<ArtifactType>) -> TreeNode {
        TreeNode {
            node_kind: NodeKind::Folder,
            artifact_type,
            type_source: artifact_type.map(|_| TypeSource::Assigned),
            has_artifacts: Some(artifact_type.is_some()),
            subtree_types: Some(artifact_type.into_iter().collect()),
            has_children: Some(false),
            ..TreeNode::plain_file(rel)
        }
    }
}

// ---------------------------------------------------------------------------
// `.synthesis/library.toml` attribution model (ASC-FR-05 / ASC-FR-07)
// ---------------------------------------------------------------------------

/// One stored user override.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    #[serde(rename = "type")]
    pub artifact_type: ArtifactType,
    pub scope: Scope,
}

/// The committed, diffable `.synthesis/library.toml`: a map from
/// project-relative path to a `{ type, scope }` override. Plain TOML so type
/// curation is shareable across a team (ASC NFR).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LibraryAssignments {
    #[serde(default)]
    pub assignments: BTreeMap<String, Assignment>,
}

/// The attribution file's project-relative path, in the forward-slash form the
/// tree walk and the watcher's raw path channel both compare against.
///
/// One constant rather than two literals because the two sites disagreeing is
/// silent in both directions: the walk would surface the file as an artifact
/// node (ASC-FR-09), or the channel would stop carrying it and every stored
/// classification would go stale on an external write (BMI-FR-17).
pub const LIBRARY_TOML_REL: &str = ".synthesis/library.toml";

/// Absolute location of the attribution file under `root`.
fn library_toml_path(root: &Path) -> std::path::PathBuf {
    root.join(".synthesis").join("library.toml")
}

/// Load the stored assignments. A missing file is the empty set, not an error
/// (the file is created on demand — it is not part of the project scaffold).
pub fn load_assignments(root: &crate::fs::RootFs) -> LibraryAssignments {
    match root.read_toml::<LibraryAssignments>(library_toml_path(root)) {
        Ok(a) => a,
        Err(fsa::FsError::NotFound { .. }) => LibraryAssignments::default(),
        // A malformed/unreadable attribution file must not blow up the whole
        // Library — degrade to "no overrides" so the tree still renders.
        Err(_) => LibraryAssignments::default(),
    }
}

/// Persist assignments via the FSA atomic-write primitive (ASC-FR-12).
fn save_assignments(root: &crate::fs::RootFs, value: &LibraryAssignments) -> fsa::FsResult<()> {
    root.write_toml_atomic(library_toml_path(root), value)
}

/// Record a user override (ASC-FR-05). `scope = file` tags exactly `path`;
/// `scope = folder` tags `path`'s subtree as a default for the files it
/// contains.
pub fn assign(
    root: &crate::fs::RootFs,
    path: &str,
    artifact_type: ArtifactType,
    scope: Scope,
) -> fsa::FsResult<()> {
    let rel = normalize_rel(path);
    // ASC-FR-12 / FSA-FR-10: reject a `path` that would escape the project root
    // (e.g. `../../etc/x`) before it can be stored as an attribution key. The
    // write itself always targets `.synthesis/library.toml`, but the stored key
    // must stay within the project so a future resolver cannot be tricked.
    fsa::resolve_under(root, &rel)?;
    let mut current = load_assignments(root);
    current.assignments.insert(rel, Assignment { artifact_type, scope });
    save_assignments(root, &current)
}

/// Remove any stored override for `path` (ASC-FR-07). Clearing a path with no
/// stored assignment is a no-op, not an error.
pub fn clear(root: &crate::fs::RootFs, path: &str) -> fsa::FsResult<()> {
    let rel = normalize_rel(path);
    // ASC-FR-12 / FSA-FR-10: same path-escape gate as `assign`.
    fsa::resolve_under(root, &rel)?;
    let mut current = load_assignments(root);
    if current.assignments.remove(&rel).is_none() {
        // No-op: nothing stored. Do not rewrite the file.
        return Ok(());
    }
    save_assignments(root, &current)
}

/// Normalise a project-relative path to forward slashes with no trailing slash
/// so map lookups are stable across platforms and minor caller sloppiness.
fn normalize_rel(path: &str) -> String {
    let p = path.replace('\\', "/");
    p.trim_matches('/').to_string()
}

// ---------------------------------------------------------------------------
// Classification (pure) — ASC-FR-03 / ASC-FR-04 / ASC-FR-06
// ---------------------------------------------------------------------------

/// Resolve a file's `(artifact_type, type_source)` by the fixed precedence
/// (ASC-FR-06): per-file assignment > path inference (+ content tiebreak) >
/// nearest-ancestor folder assignment > unclassified.
///
/// `content` is consulted only when path inference fails; it is a closure so
/// `build_tree` stays pure (production wires it to an FSA read).
///
/// Public because `crate::changes` composes it too: a changed file must carry
/// exactly the type the Library shows for the same path (CHC-FR-10), which only
/// holds if both go through this one resolver rather than re-deriving it.
pub fn classify_file<F>(
    rel: &str,
    assignments: &LibraryAssignments,
    content: &F,
) -> (Option<ArtifactType>, Option<TypeSource>)
where
    F: Fn(&str) -> ContentFacts,
{
    classify_file_in_repo(rel, rel, assignments, content)
}

/// ASC-FR-06 with ASC-FR-LXOQ: the path conventions match `repo_rel`, the path
/// relative to the nearest repository root, and the assignments match `rel`,
/// the project-relative path.
pub fn classify_file_in_repo<F>(
    rel: &str,
    repo_rel: &str,
    assignments: &LibraryAssignments,
    content: &F,
) -> (Option<ArtifactType>, Option<TypeSource>)
where
    F: Fn(&str) -> ContentFacts,
{
    // (1) Per-file assignment.
    if let Some(a) = assignments.assignments.get(rel) {
        if a.scope == Scope::File {
            return (Some(a.artifact_type), Some(TypeSource::Assigned));
        }
    }
    // (2) Path-convention inference, then the optional content tiebreak. Both
    // report `inferred` (the content read is part of inference, ASC-FR-06).
    // ASC-FR-YODA: an archived OpenSpec change is typed by an assignment alone,
    // so neither inference step runs for it.
    if !inference::is_openspec_archive(repo_rel) {
        if let Some(t) = infer_from_path(repo_rel) {
            return (Some(t), Some(TypeSource::Inferred));
        }
        if let Some(t) = content(rel).artifact_type {
            return (Some(t), Some(TypeSource::Inferred));
        }
    }
    // (3) Nearest-ancestor folder-scope assignment.
    if let Some(t) = nearest_folder_assignment(rel, assignments) {
        return (Some(t), Some(TypeSource::Inherited));
    }
    // (4) Unclassified.
    (None, None)
}

/// Walk a file's ancestor directories nearest-first, returning the type of the
/// closest folder-scope assignment (ASC-FR-06 level 3).
fn nearest_folder_assignment(rel: &str, assignments: &LibraryAssignments) -> Option<ArtifactType> {
    let mut dir = parent_of(rel);
    loop {
        if let Some(a) = assignments.assignments.get(&dir) {
            if a.scope == Scope::Folder {
                return Some(a.artifact_type);
            }
        }
        if dir.is_empty() {
            return None;
        }
        dir = parent_of(&dir);
    }
}

/// ASC-FR-06, ASC-FR-LXOQ: the type of one project file, with its nearest
/// repository root read from the disk. For a caller that holds no index.
pub fn classify_path(
    root: &crate::fs::RootFs,
    rel: &str,
    assignments: &LibraryAssignments,
) -> (Option<ArtifactType>, Option<TypeSource>) {
    let repo = repos::nearest_root_on_disk(root, rel);
    let repo_rel = repos::relative_to(rel, &repo);
    classify_file_in_repo(rel, repo_rel, assignments, &markdown_facts(root))
}

/// The folder-scope type the entry at `rel` would inherit from its nearest
/// ancestor folder, if any (ASC-FR-06 level 3).
///
/// Exposed because folder creation materialises that inheritance onto a new
/// subfolder (PST-FR-25): a folder made inside a typed folder carries the same
/// type, so it is tagged and filtered exactly as its parent is rather than
/// vanishing under the **All Artifacts** lens.
pub fn inherited_folder_type(root: &crate::fs::RootFs, rel: &str) -> Option<ArtifactType> {
    nearest_folder_assignment(&normalize_rel(rel), &load_assignments(root))
}

/// The parent directory of a project-relative path (forward-slash). The parent
/// of a top-level entry — and of the root — is `""`.
fn parent_of(rel: &str) -> String {
    match rel.rsplit_once('/') {
        Some((parent, _)) => parent.to_string(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Tree building (pure) — ASC-FR-01 / ASC-FR-08 / ASC-FR-13
// ---------------------------------------------------------------------------

/// A single filesystem entry surfaced by the walk: a project-relative path and
/// whether it is a directory. The pure tree builder consumes these so it can be
/// exercised without touching the disk.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub rel: String,
    pub is_dir: bool,
}

/// Intermediate nested form used to assemble the tree from a flat entry list.
#[derive(Default)]
struct Dir {
    dirs: BTreeMap<String, Dir>,
    files: BTreeSet<String>,
}

fn insert_entry(dir: &mut Dir, segs: &[&str], is_dir: bool) {
    match segs {
        [] => {}
        [last] => {
            if is_dir {
                dir.dirs.entry((*last).to_string()).or_default();
            } else {
                dir.files.insert((*last).to_string());
            }
        }
        [head, tail @ ..] => {
            let child = dir.dirs.entry((*head).to_string()).or_default();
            insert_entry(child, tail, is_dir);
        }
    }
}

/// Build the filesystem-mirroring tree (ASC-FR-01) from a flat entry list,
/// classifying every file (ASC-FR-06) and computing `has_artifacts` for every
/// folder (ASC-FR-08). Returns the root folder node; the UI renders its
/// children. Pure over (`entries`, `assignments`, `content`).
pub fn build_tree<F>(
    root_name: &str,
    entries: &[Entry],
    assignments: &LibraryAssignments,
    content: &F,
) -> TreeNode
where
    F: Fn(&str) -> ContentFacts,
{
    let mut root = Dir::default();
    for e in entries {
        let segs: Vec<&str> = e.rel.split('/').filter(|s| !s.is_empty()).collect();
        if segs.is_empty() {
            continue; // the root itself
        }
        insert_entry(&mut root, &segs, e.is_dir);
    }
    convert_dir("", root_name, &root, assignments, content)
}

fn convert_dir<F>(
    rel: &str,
    name: &str,
    dir: &Dir,
    assignments: &LibraryAssignments,
    content: &F,
) -> TreeNode
where
    F: Fn(&str) -> ContentFacts,
{
    // Folders first, then files — each alphabetical (BTree ordering).
    let mut children: Vec<TreeNode> = Vec::with_capacity(dir.dirs.len() + dir.files.len());
    for (dname, dbuilder) in &dir.dirs {
        let child_rel = join_rel(rel, dname);
        children.push(convert_dir(&child_rel, dname, dbuilder, assignments, content));
    }
    for fname in &dir.files {
        let child_rel = join_rel(rel, fname);
        let (artifact_type, type_source) = classify_file(&child_rel, assignments, content);
        // ASC-FR-19: read only for the files whose filename does not identify
        // them, so a project of ordinary artifacts is not opened file by file
        // to build its tree.
        let display_name = if names_itself(&child_rel, artifact_type) {
            content(&child_rel).declared_name
        } else {
            None
        };
        children.push(TreeNode {
            id: child_rel.clone(),
            name: fname.clone(),
            path: child_rel,
            node_kind: NodeKind::File,
            artifact_type,
            type_source,
            display_name,
            has_artifacts: None,
            subtree_types: None,
            has_children: None,
            nested_repo: None,
            children: None,
        });
    }

    // ASC-FR-18: a folder-scope assignment is the only thing that types a folder
    // — no path convention infers a type onto a directory and no inheritance
    // travels upward into one — so this is the folder's own type, reported on the
    // node itself so it can be tagged and filtered by it (LIB-FR-08 / LIB-FR-09).
    let self_folder_type = assignments
        .assignments
        .get(rel)
        .filter(|a| a.scope == Scope::Folder)
        .map(|a| a.artifact_type);

    // ASC-FR-08: a folder has artifacts iff its subtree holds >= 1 artifact
    // node OR the folder itself carries a folder-scope assignment.
    let has_artifacts = self_folder_type.is_some()
        || children.iter().any(|c| match c.node_kind {
            NodeKind::File => c.artifact_type.is_some(),
            NodeKind::Folder => c.has_artifacts == Some(true),
        });
    // ASC-FR-OGRB: the types of the subtree, in the order of ASC-FR-02.
    let mut bits = self_folder_type.map(index::type_bit).unwrap_or(0);
    for c in &children {
        match c.node_kind {
            NodeKind::File => bits |= c.artifact_type.map(index::type_bit).unwrap_or(0),
            NodeKind::Folder => {
                for t in c.subtree_types.iter().flatten() {
                    bits |= index::type_bit(*t);
                }
            }
        }
    }
    let has_children = !children.is_empty();

    TreeNode {
        id: rel.to_string(),
        name: name.to_string(),
        path: rel.to_string(),
        node_kind: NodeKind::Folder,
        artifact_type: self_folder_type,
        // A folder's type is always a user assignment; there is no inferred or
        // inherited path to one (ASC-FR-18).
        type_source: self_folder_type.map(|_| TypeSource::Assigned),
        display_name: None,
        has_artifacts: Some(has_artifacts),
        subtree_types: Some(index::types_of(bits)),
        has_children: Some(has_children),
        nested_repo: None,
        children: Some(children),
    }
}

fn join_rel(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}/{child}")
    }
}

// ---------------------------------------------------------------------------
// I/O scan — composes the FSA walk + pure builder (ASC-FR-01 / ASC-FR-09 / ASC-FR-11)
// ---------------------------------------------------------------------------

/// ASC-FR-19: the name that one file shows: the name that it declares where its
/// filename does not identify it, else its filename.
pub fn file_display_name(
    root: &crate::fs::RootFs,
    rel: &str,
    artifact_type: Option<ArtifactType>,
) -> String {
    let declared = if names_itself(rel, artifact_type) {
        markdown_facts(root)(rel).declared_name
    } else {
        None
    };
    declared.unwrap_or_else(|| rel.rsplit('/').next().unwrap_or(rel).to_string())
}

/// Perform a full recursive scan of `root` and return the type-tagged tree.
///
/// Builds a fresh project index (ASC-FR-HPUY) and returns it whole. A command
/// reads the mounted index of [`CandidateStore`] instead; this is for a caller
/// that needs one full tree once. Never throws on a transient filesystem race:
/// an entry that errors mid-walk is skipped, not fatal (ASC-FR-11).
pub fn scan(root: &crate::fs::RootFs) -> TreeNode {
    ProjectIndex::build(root).full_tree(&root_display_name(root))
}

/// ASC-FR-HWQJ: a content read of the classifier reads at most this many
/// leading bytes of a file.
pub const CONTENT_READ_CAP: usize = 64 * 1024;

/// The content reader `classify_file` takes, over the files of `root`. The
/// tiebreak only reads Markdown files, and only when path inference has already
/// failed (gated inside `classify_file`). It reads at most
/// [`CONTENT_READ_CAP`] bytes (ASC-FR-HWQJ).
pub fn markdown_facts(root: &crate::fs::RootFs) -> impl Fn(&str) -> ContentFacts + use<> {
    let root_owned = root.clone();
    move |rel: &str| -> ContentFacts {
        if !rel.to_ascii_lowercase().ends_with(".md") {
            return ContentFacts::default();
        }
        match read_head(&root_owned, rel) {
            Some((text, complete)) => facts_of_head(&text, complete),
            None => ContentFacts::default(),
        }
    }
}

/// ASC-FR-HWQJ: the facts of the leading bytes of a file. When the read did
/// not reach the end of the file, frontmatter that does not close inside the
/// bytes is absent.
pub(crate) fn facts_of_head(text: &str, complete: bool) -> ContentFacts {
    if complete {
        return read_content_facts(text);
    }
    // The last line of a cut head can continue past the cap, so only whole
    // lines count: a `---` that the cap cut from `-----` closes nothing.
    let whole = match text.rfind('\n') {
        Some(at) => &text[..at + 1],
        None => "",
    };
    if !inference::frontmatter_closes(whole) {
        return ContentFacts::default();
    }
    read_content_facts(whole)
}

/// Read at most [`CONTENT_READ_CAP`] leading bytes of `rel`, and whether they
/// are the whole file.
fn read_head(root: &crate::fs::RootFs, rel: &str) -> Option<(String, bool)> {
    let mut handle = root.open_file(rel, crate::fs::OpenMode::ReadOnly).ok()?;
    let mut buf = vec![0u8; CONTENT_READ_CAP];
    let mut filled = 0;
    while filled < buf.len() {
        match handle.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(_) => return None,
        }
    }
    let complete = filled < buf.len() || matches!(handle.read(&mut [0u8; 1]), Ok(0));
    buf.truncate(filled);
    // A file that is not UTF-8 has no facts, as a whole-file text read gives
    // none. A cut head can end inside one character, which is dropped.
    match String::from_utf8(buf) {
        Ok(text) => Some((text, complete)),
        Err(e) => {
            let valid = e.utf8_error().valid_up_to();
            let cut_char = !complete && e.utf8_error().error_len().is_none();
            if !cut_char {
                return None;
            }
            let mut bytes = e.into_bytes();
            bytes.truncate(valid);
            String::from_utf8(bytes).ok().map(|text| (text, complete))
        }
    }
}

/// Every file under `root`, as project-relative forward-slash paths, under the
/// same ASC-FR-09 scope rules `scan` applies.
///
/// The tree-shaped `scan` reads Markdown content to classify artifacts; callers
/// that only need the set of paths in the project should not pay for that. Used
/// by `comments::list_all_threads_in` to invert `comments::log_id` by
/// enumeration (CMS-FR-33).
pub fn file_rel_paths(root: &crate::fs::RootFs) -> Vec<String> {
    let mut paths: Vec<String> =
        walk::walk_paths(root).into_iter().filter(|e| !e.is_dir).map(|e| e.rel).collect();
    paths.sort();
    paths
}

/// True for paths inside the prune set of ASC-FR-09.
///
/// `.synthesis/notes/`, `.synthesis/comments/`, `.synthesis/statistics/`, and
/// `.synthesis/drafts/` are pruned like the gitignored `.synthesis/cache/`
/// beside them even though all four are committed: their files are the
/// module-owned storage of `NTC-notes-storage.md` (NTC-FR-16),
/// `CMS-comments-storage.md` (CMS-FR-29), `DSS-draft-statistics-storage.md`
/// (DSS-FR-KQVN), and `DRS-draft-storage.md` (DRS-FR-04), and surfacing them
/// would put one Library node per note, per commented artifact, and per draft in
/// the tree. ASC-FR-SFCR: a `.git` entry at any depth is pruned too.
pub(crate) fn is_pruned_dir(rel: &str) -> bool {
    const PRUNED: &[&str] = &[
        ".git",
        ".synthesis/cache",
        ".synthesis/drafts",
        ".synthesis/notes",
        ".synthesis/comments",
        ".synthesis/statistics",
    ];
    PRUNED.iter().any(|p| rel == *p || rel.starts_with(&format!("{p}/")))
        || repos::has_git_segment(rel)
}

/// Forward-slash, root-relative rendering of a path.
pub(crate) fn to_forward(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Display name for the root node — the project folder's basename.
fn root_display_name(root: &Path) -> String {
    root.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned())
}

/// The SHA-256 of `.synthesis/library.toml` as this application last wrote it
/// (ASC-FR-05 / ASC-FR-07), so the watcher can tell an *external* assignment —
/// a `git pull` carrying a teammate's curation, an agentic CLI editing the file
/// — from the echo of the application's own write arriving a debounce window
/// later.
///
/// Deliberately **not** a [`crate::artifacts::ContentTracker`] entry. That map
/// is keyed by artifact id and is what `watcher::route_watch_change` consults,
/// so an entry there would make the per-path routing loop emit
/// `"artifact changed externally"` for a path that is not an artifact node
/// (ASC-FR-09) and that no editor tab can ever back. Separate state makes that
/// outcome impossible by construction rather than by loop ordering.
///
/// The baseline belongs to the content root and is cleared with it: the same
/// relative path names different bytes in a different worktree.
///
/// Absence is not an error. No baseline recorded, an unreadable file, and a
/// deleted file all read as *external*, which costs one redundant pass — the
/// opposite mistake would leave a classification stale for the life of the
/// project.
#[derive(Default)]
pub struct AttributionBaseline {
    inner: Mutex<Option<String>>,
}

impl AttributionBaseline {
    /// Adopt an observed checksum as the baseline.
    ///
    /// The baseline names **what consumers last saw**, not what this
    /// application last wrote, and the watcher adopts every checksum it
    /// observes for the same reason `ContentTracker` does (`watcher.rs`): a
    /// baseline that only ever moved on our own writes would suppress a revert
    /// back to bytes we once wrote. A `git checkout` to the previous branch is
    /// exactly that revert, and BMI-FR-18 names checkout explicitly — so
    /// without this the classification would stay stale for the life of the
    /// project, which is the staleness this whole mechanism exists to prevent.
    pub fn adopt(&self, checksum: Option<String>) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = checksum;
        }
    }

    /// Adopt what is on disk now, immediately after this application has
    /// written the attribution file.
    ///
    /// Reads back through [`crate::fs::RootFs::sha256_file`] — the same
    /// function the watcher hashes with — so the two sides cannot disagree
    /// about what "the checksum of this file" means. An unreadable file records
    /// `None`, leaving the next event to read as external.
    pub fn record_from_disk(&self, root: &crate::fs::RootFs) {
        self.adopt(root.sha256_file(LIBRARY_TOML_REL).ok());
    }

    /// The recorded baseline, if any.
    pub fn current(&self) -> Option<String> {
        self.inner.lock().ok().and_then(|guard| guard.clone())
    }

    /// ASC-FR-14 / ASC-FR-16: dropped with the content root it belongs to.
    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
    }
}

// ---------------------------------------------------------------------------
// Watcher change summary (ASC-FR-10) — the pure half of the debounced event
// ---------------------------------------------------------------------------

/// Payload of the `"project tree changed"` event (ASC-FR-10 / ASC-FR-22).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeChangedPayload {
    pub change_count: usize,
    /// ASC-FR-22: the project-relative paths that do not exist once this
    /// coalesced burst is applied. A deleted folder contributes its own path and
    /// need not enumerate what was under it — a consumer treats a removed folder
    /// as removing everything beneath it (`../ui/TAB-tabs.md` TAB-FR-19). The
    /// source side of a rename or a move is a removed path like any other,
    /// because the path itself is gone whatever took its place; nothing here
    /// correlates it with the destination that appeared in the same burst.
    ///
    /// Always present — a burst that removed nothing carries an empty list
    /// rather than omitting the field, so a consumer never has to distinguish
    /// "nothing removed" from "this build does not report removals".
    pub removed_paths: Vec<String>,
    /// ASC-FR-JYXO: the folders whose `list_tree_children` answer changed,
    /// `""` for the project root.
    pub changed_folders: Vec<String>,
}

/// ASC-FR-22: of `changed`, the project-relative paths that no longer exist
/// under `root`.
///
/// Existence is read with `symlink_metadata`, so a path is "still here" when a
/// link occupies it — the scan will not surface that link (ASC-FR-23), but the
/// path is not *gone*, and reporting it as removed would be a different claim
/// than the one this function makes.
///
/// Note the one case this cannot distinguish: a path created and removed inside
/// a single debounce window looks identical to a path that existed before the
/// window and was deleted in it, because both are simply absent now. Telling
/// them apart would require carrying the previous scan's path set through every
/// burst. The first case is reported as removed and is inert — nothing can hold
/// a tab or a tree node for a path that never outlived one debounce window — so
/// the state that would fix it buys no observable behaviour.
pub fn removed_rel_paths(changed: &[String], root: &crate::fs::RootFs) -> Vec<String> {
    changed.iter().filter(|rel| root.file_info(rel).is_err()).cloned().collect()
}

/// True for the atomic-write temp files of `FSA-filesystem-access.md`
/// (`sibling_tmp_path` formats them as `.{stem}.tmp.{pid}.{nanos}`). They appear
/// and vanish within a single save, so they must never be surfaced as a tree or
/// content change. The match is deliberately tight — a leading dot plus a
/// trailing `.tmp.<digits>.<digits>` — so a legitimately-named dotfile that
/// merely contains `.tmp.` (e.g. `.config.tmp.md`) is NOT skipped.
pub(crate) fn is_atomic_tmp_name(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    if !name.starts_with('.') {
        return false;
    }
    match name.rsplit_once(".tmp.") {
        Some((_, suffix)) => {
            let mut parts = suffix.split('.');
            match (parts.next(), parts.next(), parts.next()) {
                // exactly `<pid>.<nanos>`, both all-digits, nothing trailing.
                (Some(pid), Some(nanos), None) => {
                    !pid.is_empty()
                        && !nanos.is_empty()
                        && pid.bytes().all(|b| b.is_ascii_digit())
                        && nanos.bytes().all(|b| b.is_ascii_digit())
                }
                _ => false,
            }
        }
        None => false,
    }
}

/// ASC-FR-10 + ASC-FR-15 + ASC-FR-20: the deduped, project-relative paths a
/// debounced batch touched, with pruned dirs, atomic-write temp files, and
/// out-of-root paths excluded. An errored or empty batch yields an empty vec.
/// This is the pure half feeding the structural
/// `"project tree changed"` event, the content-modification channel that
/// `PST-project-storage.md` consumes for `"artifact changed externally"`, and
/// the raw path channel of ASC-FR-20.
///
/// **It is deliberately not filtered by `.gitignore`.** ASC-FR-09's ignore
/// rules govern the tree and everything derived from it, not this channel: a
/// consumer with its own inclusion rules — today
/// `DSL-dynamic-skills-loading.md`, whose four skill folders a project may
/// legitimately gitignore (DSL-FR-06) — would otherwise have no way to learn
/// that one of them changed, since a gitignored file's edit changes nothing in
/// the tree. The watcher then removes the ignored paths outside those folders
/// with [`drop_gitignored`] (ASC-FR-RSGA), which needs the disk and so is not
/// part of this pure half.
///
/// What *is* pruned is `.git/` — the watcher's own churn rather than the
/// project's — together with the `.synthesis/` subtrees that each have a
/// dedicated notification channel of their own or that no consumer observes
/// (`cache`, `drafts` via DRS-FR-28 and DRS-FR-42, `notes`, `comments`, and
/// `statistics`,
/// whose logs are read by folding them on demand rather than by watching them —
/// `DSS-draft-statistics-storage.md` DSS-FR-YOVS), which would otherwise be
/// reported twice.
/// A future consumer that needs one of those should read its owner's channel
/// rather than widening this one.
///
/// `.synthesis/library.toml` is deliberately **carried**, not filtered. It is
/// not one of those dedicated-channel subtrees — it has no channel of its own —
/// and every file's resolved type derives from it (ASC-FR-05 / ASC-FR-06), so a
/// write to it restates the whole classification. Dropping it would mean a
/// `git pull` carrying a teammate's folder-scope curation, or an agentic CLI
/// editing the file, reached no consumer at all and left every affected index
/// membership stale (BMI-FR-17, BMI-FR-18). It is still never a tree *node*
/// (ASC-FR-09, enforced in `walk_entries`), so a consumer that treats these
/// paths as nodes must exclude it itself — `watcher.rs` does, before its
/// per-path routing loop.
pub fn changed_rel_paths(result: &DebounceEventResult, root: &Path) -> Vec<String> {
    let events = match result {
        Ok(ev) => ev,
        Err(_) => return Vec::new(),
    };
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for ev in events {
        let rel = match ev.path.strip_prefix(root) {
            Ok(r) => to_forward(r),
            Err(_) => continue, // outside the watched root
        };
        if rel.is_empty() || is_pruned_dir(&rel) || is_atomic_tmp_name(&rel) {
            continue;
        }
        if seen.insert(rel.clone()) {
            out.push(rel);
        }
    }
    out
}

/// ASC-FR-UWMI, ASC-FR-SFCR: each folder whose own `.git` entry a debounced
/// batch touched — a submodule initialised, or a repository removed — so the
/// index can tell whether a repository boundary moved. The project root is
/// left out: its `.git` entry is not a boundary inside the project.
pub fn git_boundaries(result: &DebounceEventResult, root: &Path) -> Vec<String> {
    let Ok(events) = result else {
        return Vec::new();
    };
    let mut out = BTreeSet::new();
    for ev in events {
        let Ok(rel) = ev.path.strip_prefix(root).map(to_forward) else {
            continue;
        };
        if let Some(parent) = rel.strip_suffix("/.git") {
            if !repos::has_git_segment(parent) {
                out.insert(parent.to_string());
            }
        }
    }
    out.into_iter().collect()
}

/// ASC-FR-TWVB, ASC-FR-UWMI: whether a debounced batch wrote an `info/exclude`
/// file of a `.git` folder inside the project. Its rules change what the tree
/// admits, and the path itself is pruned from every other channel.
pub fn exclude_changed(result: &DebounceEventResult, root: &Path) -> bool {
    let Ok(events) = result else {
        return false;
    };
    events.iter().any(|ev| {
        ev.path
            .strip_prefix(root)
            .map(to_forward)
            .is_ok_and(|rel| rel == ".git/info/exclude" || rel.ends_with("/.git/info/exclude"))
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
