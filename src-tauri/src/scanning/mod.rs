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
//! The Tauri-facing commands (`load_project_tree`, `rescan_project_tree`,
//! `assign_artifact_type`, `clear_artifact_type`) and the recursive filesystem
//! watcher live in `lib.rs` (they need the Tauri runtime / `AppHandle`); this
//! module keeps every classification and tree-shaping decision pure and
//! unit-tested. All reads/writes go through `crate::fs` (FSA, ASC-FR-12).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ignore::WalkBuilder;
use notify_debouncer_mini::DebounceEventResult;
use serde::{Deserialize, Serialize};

use crate::fs as fsa;

// ---------------------------------------------------------------------------
// Fixed built-in type set + node shape (ASC-FR-02, tree node shape)
// ---------------------------------------------------------------------------

/// The fixed built-in artifact-type set (ASC-FR-02). `Flow` is what the UI
/// opens in the Flow tab (`../ui/FLO-flow.md`). No other type is produced in v1.
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
    Instructions,
    Scenario,
    Scratchpad,
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
            ArtifactType::Instructions => "instructions",
            ArtifactType::Scenario => "scenario",
            ArtifactType::Scratchpad => "scratchpad",
        }
    }

    /// Parse a content-marker / TOML value into a built-in type. Accepts the
    /// canonical lowercase names, plus `harness` for `flow` on the same terms
    /// as the serde alias above.
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
    /// Folders only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<TreeNode>>,
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
    current.assignments.insert(
        rel,
        Assignment {
            artifact_type,
            scope,
        },
    );
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

/// Infer an artifact type from path/location convention across agent
/// ecosystems (ASC-FR-03). Returns `None` when no recognizer matches — the
/// caller then tries the content tiebreak and finally folder inheritance.
///
/// The representative recognizer table below spans Claude Code
/// (`.claude/skills`, `.claude/agents`, `.claude/commands`), Codex
/// (`AGENTS.md`), GitHub Copilot (`.github/**instructions**`,
/// `.github/prompts`), and the synthesis convention (`specifications/`). It is
/// intentionally extensible; the *type set* (ASC-FR-02) is what is fixed.
pub fn infer_from_path(rel: &str) -> Option<ArtifactType> {
    let rel = normalize_rel(rel);
    let segs: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
    let base = segs.last().copied().unwrap_or("");
    let base_lower = base.to_ascii_lowercase();

    // Directory-convention recognizers: a window of two consecutive path
    // segments anchors the ecosystem (e.g. `.claude/skills`).
    if has_dir_pair(&segs, ".claude", "skills") {
        return Some(ArtifactType::Skill);
    }
    if has_dir_pair(&segs, ".claude", "agents") {
        return Some(ArtifactType::Agent);
    }
    if has_dir_pair(&segs, ".claude", "commands") {
        return Some(ArtifactType::Prompt);
    }
    if has_dir_pair(&segs, ".github", "prompts") {
        return Some(ArtifactType::Prompt);
    }

    // Filename-convention recognizers (ecosystem-agnostic).
    if base == "AGENTS.md" {
        return Some(ArtifactType::Agent);
    }
    if base == "CLAUDE.md" || base == "AGENT.md" {
        return Some(ArtifactType::Instructions);
    }
    if base_lower.ends_with(".instructions.md") {
        return Some(ArtifactType::Instructions);
    }
    // GitHub Copilot keeps instruction files under `.github/` (e.g.
    // `.github/copilot-instructions.md`).
    if segs.first() == Some(&".github") && base_lower.contains("instructions") {
        return Some(ArtifactType::Instructions);
    }
    if base_lower.ends_with(".prompt.md") {
        return Some(ArtifactType::Prompt);
    }
    // The Flow document convention (ASC-FR-03): a `.flow` file, whose contents
    // are JSON. Because they are JSON, the content tiebreak
    // (`read_content_facts`, which reads Markdown frontmatter) can never reach a
    // Flow — the extension and an explicit assignment are the only two things
    // that classify one, and the assignment wins (ASC-FR-06).
    // `.js.flow` is a Facebook Flow type-declaration file that happens to share
    // the suffix; a JS project committing one is not describing a workflow, and
    // opening it on the Flow canvas would only fail to parse (FLO-FR-05). A bare
    // `.flow` with no stem is a tool's dotfile on the same reasoning.
    if base_lower.ends_with(".flow")
        && base_lower != ".flow"
        && !base_lower.ends_with(".js.flow")
    {
        return Some(ArtifactType::Flow);
    }
    if base_lower.ends_with(".scenario.md") {
        return Some(ArtifactType::Scenario);
    }
    if base_lower.ends_with(".scratchpad.md") {
        return Some(ArtifactType::Scratchpad);
    }
    if base_lower.ends_with(".spec.md") {
        return Some(ArtifactType::Spec);
    }
    // Synthesis spec convention: anything under a top-level `specifications/`.
    if segs.first() == Some(&"specifications") && segs.len() > 1 {
        return Some(ArtifactType::Spec);
    }

    None
}

/// True if `a` immediately followed by `b` appears as a consecutive segment
/// pair anywhere in `segs` (e.g. `.claude/skills`).
fn has_dir_pair(segs: &[&str], a: &str, b: &str) -> bool {
    segs.windows(2).any(|w| w[0] == a && w[1] == b)
}

/// What a file's leading frontmatter says about itself: the `type:` marker the
/// classification tiebreak reads (ASC-FR-04) and the `name:` a skill declares
/// (ASC-FR-19).
///
/// One shape for both because they come out of one read: the scan opens a file
/// at most once, and splitting them into two lookups would double that for
/// every file that has anything to say.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContentFacts {
    pub artifact_type: Option<ArtifactType>,
    pub declared_name: Option<String>,
}

/// Read a leading `---` frontmatter block for the keys the scan cares about.
///
/// Deliberately a line walk rather than a YAML parse: this runs over every file
/// the tiebreak is consulted for, the two keys it reads are scalars, and a file
/// whose frontmatter does not parse must still classify by the keys that do —
/// there is no error to report here, only facts that are present or absent.
/// Only top-level keys count; an indented `name:` belongs to something else.
pub fn read_content_facts(text: &str) -> ContentFacts {
    let mut facts = ContentFacts::default();
    let mut lines = text.lines();
    // Frontmatter must be the very first line.
    match lines.next() {
        Some(l) if l.trim() == "---" => {}
        _ => return facts,
    }
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break; // end of frontmatter
        }
        if line.starts_with([' ', '\t']) {
            continue; // nested under some other key, not the file's own
        }
        if let Some(rest) = trimmed
            .strip_prefix("type:")
            .or_else(|| trimmed.strip_prefix("kind:"))
        {
            // Strip surrounding quotes a YAML author might add.
            let value = rest.trim().trim_matches(|c| c == '"' || c == '\'');
            if facts.artifact_type.is_none() {
                facts.artifact_type = ArtifactType::from_marker(value);
            }
        } else if let Some(rest) = trimmed.strip_prefix("name:") {
            let value = rest.trim().trim_matches(|c| c == '"' || c == '\'').trim();
            // `name: |` / `name: >` open a block scalar whose value is on the
            // lines beneath. Taking the indicator itself would render a node as
            // "|" — worse than the filename this falls back to.
            if facts.declared_name.is_none() && !value.is_empty() && value != "|" && value != ">" {
                facts.declared_name = Some(value.to_string());
            }
        }
    }
    facts
}

/// Content-marker tiebreak (ASC-FR-04): the `type:` (or `kind:`) key of a
/// leading `---` frontmatter block. Only consulted when path inference is
/// ambiguous.
pub fn infer_from_content(text: &str) -> Option<ArtifactType> {
    read_content_facts(text).artifact_type
}

/// ASC-FR-19: whether `rel` is a file whose declared name identifies it, because
/// its filename does not. A skill lives in `SKILL.md`, so every skill in a
/// project shares one filename; nothing else in the built-in type set has that
/// problem.
fn names_itself(rel: &str, artifact_type: Option<ArtifactType>) -> bool {
    artifact_type == Some(ArtifactType::Skill)
        && rel
            .rsplit('/')
            .next()
            .is_some_and(|base| base.eq_ignore_ascii_case("SKILL.md"))
}

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
    // (1) Per-file assignment.
    if let Some(a) = assignments.assignments.get(rel) {
        if a.scope == Scope::File {
            return (Some(a.artifact_type), Some(TypeSource::Assigned));
        }
    }
    // (2) Path-convention inference, then the optional content tiebreak. Both
    // report `inferred` (the content read is part of inference, ASC-FR-06).
    if let Some(t) = infer_from_path(rel) {
        return (Some(t), Some(TypeSource::Inferred));
    }
    if let Some(t) = content(rel).artifact_type {
        return (Some(t), Some(TypeSource::Inferred));
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

/// Perform a full recursive scan of `root` and return the type-tagged tree.
///
/// Walks the whole project root but skips `.git/`, honours `.gitignore`, and
/// skips `.synthesis/cache/` + `.synthesis/drafts/` (ASC-FR-09). Never throws
/// on a transient filesystem race: an entry that errors mid-walk is skipped,
/// not fatal (ASC-FR-11).
pub fn scan(root: &crate::fs::RootFs) -> TreeNode {
    let assignments = load_assignments(root);
    let entries = walk_entries(root);
    let root_owned = root.clone();
    // Content tiebreak only reads small Markdown files, and only when path
    // inference has already failed (gated inside `classify_file`).
    let content = move |rel: &str| -> ContentFacts {
        if !rel.to_ascii_lowercase().ends_with(".md") {
            return ContentFacts::default();
        }
        match root_owned.read_text(rel) {
            Ok(text) => read_content_facts(&text),
            Err(_) => ContentFacts::default(),
        }
    };
    build_tree(&root_display_name(root), &entries, &assignments, &content)
}

/// Every file under `root`, as project-relative forward-slash paths, under the
/// same ASC-FR-09 scope rules `scan` applies.
///
/// The tree-shaped `scan` reads Markdown content to classify artifacts; callers
/// that only need the set of paths in the project should not pay for that. Used
/// by `comments::list_all_threads_in` to invert `comments::log_id` by
/// enumeration (CMS-FR-33).
pub fn file_rel_paths(root: &Path) -> Vec<String> {
    walk_entries(root)
        .into_iter()
        .filter(|e| !e.is_dir)
        .map(|e| e.rel)
        .collect()
}

/// Walk `root` into the flat entry list the pure builder consumes, applying the
/// ASC-FR-09 scope rules.
fn walk_entries(root: &Path) -> Vec<Entry> {
    let root_for_filter = root.to_path_buf();
    let walker = WalkBuilder::new(root)
        .hidden(false) // surface dotdirs like `.claude`
        .git_ignore(true) // honour `.gitignore`
        .git_global(false)
        .git_exclude(true)
        .require_git(false) // honour `.gitignore` even outside a git repo
        .parents(false)
        .filter_entry(move |entry| {
            // Prune `.git/`, `.synthesis/cache/`, `.synthesis/drafts/` so the
            // walker never descends into them (ASC-FR-09).
            let rel = match entry.path().strip_prefix(&root_for_filter) {
                Ok(r) => to_forward(r),
                Err(_) => return true,
            };
            // ASC-FR-23: never descend a symbolic link. `WalkBuilder` does not
            // follow links by default, but it still *yields* the link entry, and
            // pruning here keeps a linked directory from being opened at all.
            if entry.file_type().map(|t| t.is_symlink()).unwrap_or(false) {
                return false;
            }
            !is_pruned_dir(&rel)
        })
        .build();

    let mut out = Vec::new();
    for result in walker {
        // ASC-FR-11: skip entries that error mid-walk instead of failing.
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };
        let rel = match entry.path().strip_prefix(root) {
            Ok(r) => to_forward(r),
            Err(_) => continue,
        };
        if rel.is_empty() {
            continue; // the root itself is not a node
        }
        // ASC-FR-09: the attribution file is never surfaced as an artifact.
        // This governs the *tree* only; the watcher's raw path channel carries
        // the same path deliberately (ASC-FR-20, `changed_rel_paths`).
        if rel == LIBRARY_TOML_REL {
            continue;
        }
        // ASC-FR-23: a symbolic link yields no node — not a file node however
        // its path would classify, and not a folder node whose subtree could be
        // walked. The decision is made from the link alone; where it points is
        // never resolved, so a link into the project is skipped exactly as one
        // out of it is. `filter_entry` above already pruned linked directories;
        // this catches linked *files*, which `filter_entry` sees but does not
        // descend, and is the check that actually keeps them out of the tree.
        if entry.file_type().map(|t| t.is_symlink()).unwrap_or(false) {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        out.push(Entry { rel, is_dir });
    }
    out
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
/// the tree.
fn is_pruned_dir(rel: &str) -> bool {
    const PRUNED: &[&str] = &[
        ".git",
        ".synthesis/cache",
        ".synthesis/drafts",
        ".synthesis/notes",
        ".synthesis/comments",
        ".synthesis/statistics",
    ];
    PRUNED
        .iter()
        .any(|p| rel == *p || rel.starts_with(&format!("{p}/")))
}

/// Forward-slash, root-relative rendering of a path.
fn to_forward(path: &Path) -> String {
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

// ---------------------------------------------------------------------------
// The candidate list (ASC-FR-17) — the scan's file-node set, enumerable
// ---------------------------------------------------------------------------

/// One entry of the enumerable candidate list (ASC-FR-17): a `file` node of the
/// scan, carrying its stable id, project-relative path, and resolved
/// classification.
///
/// This is the set `../core/SCC-search.md` matches against (SCC-FR-03), which is
/// why search inherits the exclusions of ASC-FR-09 without restating any of
/// them: a path that never became a `file` node is never a candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// The node id (ASC-FR-13) — its project-relative path.
    pub id: String,
    /// Basename.
    pub name: String,
    /// Project-relative path (forward-slash separated).
    pub path: String,
    /// Position in the enumeration, from 0. The ordering key of SCC-FR-10.
    pub ordinal: u64,
    /// The scan's resolved type, or `None` for an unclassified file.
    pub artifact_type: Option<ArtifactType>,
}

/// ASC-FR-17: enumerate a scanned tree's `file` nodes in a deterministic order.
///
/// The order is the tree's own — folders before files, each alphabetical (the
/// `BTreeMap`/`BTreeSet` ordering `convert_dir` builds on) — so an unchanged
/// tree enumerates identically every time, which is what makes SCC-FR-10's
/// ordinals stable across two runs of the same query.
pub fn candidate_files(tree: &TreeNode) -> Vec<Candidate> {
    let mut out = Vec::new();
    collect_candidates(tree, &mut out);
    out
}

fn collect_candidates(node: &TreeNode, out: &mut Vec<Candidate>) {
    if node.node_kind == NodeKind::File {
        out.push(Candidate {
            id: node.id.clone(),
            name: node.name.clone(),
            path: node.path.clone(),
            ordinal: out.len() as u64,
            artifact_type: node.artifact_type,
        });
    }
    if let Some(children) = &node.children {
        for child in children {
            collect_candidates(child, out);
        }
    }
}

/// The mounted candidate list (ASC-FR-17 / SCC-FR-04).
///
/// It belongs to the project's content root and lives exactly as long as it: a
/// project close (ASC-FR-14) and a worktree change (ASC-FR-16) both [`clear`] it,
/// and the watcher [`invalidate`]s it on a structural change so the next
/// enumeration sees the current tree (ASC-FR-10 / ASC-FR-15). Between those, the
/// list is served from memory — which is what keeps a burst of searches from
/// re-walking the tree once per keystroke.
///
/// [`clear`]: CandidateStore::clear
/// [`invalidate`]: CandidateStore::invalidate
#[derive(Default)]
pub struct CandidateStore {
    inner: Mutex<Option<Mounted>>,
}

struct Mounted {
    root: PathBuf,
    /// Shared so a running search holds the list it started from even if the
    /// store is invalidated or torn down under it — a search never observes the
    /// list changing halfway through, and a mid-search project close cannot
    /// leave a worker reading freed state.
    files: Arc<Vec<Candidate>>,
}

impl CandidateStore {
    /// The candidate list for `root`, building it from a scan when nothing is
    /// mounted, when the mounted list belongs to a different root, or when the
    /// watcher has invalidated it. Never fails: an unreadable root scans to an
    /// empty tree (ASC-FR-11), which enumerates to no candidates.
    pub fn candidates(&self, root: &crate::fs::RootFs) -> Arc<Vec<Candidate>> {
        if let Ok(guard) = self.inner.lock() {
            if let Some(mounted) = guard.as_ref() {
                if mounted.root.as_path() == root.path() {
                    return Arc::clone(&mounted.files);
                }
            }
        }
        // Built outside the lock: a scan of a large project takes real time, and
        // holding the mutex across it would serialise every other consumer
        // behind it. A concurrent build of the same root produces an equal list,
        // so the last writer simply wins.
        let files = Arc::new(candidate_files(&scan(root)));
        if let Ok(mut guard) = self.inner.lock() {
            *guard = Some(Mounted {
                root: root.path().to_path_buf(),
                files: Arc::clone(&files),
            });
        }
        files
    }

    /// ASC-FR-10 / ASC-FR-15: the tree changed structurally, so the mounted list
    /// no longer describes it. The next enumeration rebuilds; searches already
    /// running keep the list they started from.
    pub fn invalidate(&self) {
        self.clear();
    }

    /// ASC-FR-14 / ASC-FR-16: tear the list down with the scan, on project close
    /// and on a change of active worktree.
    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
    }

    /// Whether a list is currently mounted. Test-facing: the teardown and
    /// invalidation claims of ASC-FR-17 have no other observable.
    pub fn is_mounted(&self) -> bool {
        self.inner
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }
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
    changed
        .iter()
        .filter(|rel| root.file_info(rel).is_err())
        .cloned()
        .collect()
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
/// the tree.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
