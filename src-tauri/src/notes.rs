//! Notes storage (`specifications/core/NTC-notes-storage.md`).
//!
//! Every note is one committed TOML file at `.synthesis/notes/<note-id>.toml`
//! inside the active worktree (NTC-FR-01), so notes travel with the project and
//! diff cleanly. A note carries its own `created_at` / `updated_at` instants
//! (NTC-FR-03), an optional reminder, an optional authoring revision, and a
//! scope that moves between entities without changing the note's identity
//! (NTC-FR-02 / NTC-FR-07).
//!
//! Because an entity id is a project-relative path (`ASC-artifact-scanning.md`
//! ASC-FR-13), a rename would otherwise orphan every note on the renamed file.
//! [`follow_rename`] is the correlation seam: a rename the application performs
//! names exactly one old path and one new path, so the notes on it are rewritten
//! (NTC-FR-11). Anything this module cannot correlate leaves its notes bound to
//! an unresolvable id, which the list commands mark rather than drop
//! (NTC-FR-10 / NTC-FR-12) so the UI can offer them for reattachment.
//!
//! Every write goes through the FSA primitives (NTC-FR-13): `write_toml_atomic`
//! for a note file, `delete_path` for a removal, both behind the FSA-FR-10
//! path-escape gate.

use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::fs as fsa;
use crate::project::{basename, ProjectState};

/// NTC-FR-08: deleting (or updating) an id that names no note.
pub const ERR_NOTE_NOT_FOUND: &str = "note not found";

/// NTC-FR-21: the note's discussion could not be removed, so nothing was.
///
/// Typed and distinct from the note's own write failure because the two leave
/// different states behind and call for different corrections: this one leaves
/// the note and its conversation whole and is retried, where a failure of the
/// second step leaves a note whose conversation has gone.
pub const ERR_DISCUSSION_CLEANUP_FAILED: &str = "discussion_cleanup_failed";

/// NTC-FR-23: the ceiling on a note's `body`, in **bytes of UTF-8**.
///
/// Bytes rather than code points or displayed characters, so a body of emoji or
/// of Japanese reaches it sooner than one of English and the same text is
/// bounded identically wherever it was written.
///
/// It is what keeps a note small enough for the whole of it to be one document
/// of the `notes` index (`BMI-bm25-indexing.md` BMI-FR-28) and to be handed
/// whole to a model rather than as an excerpt of itself
/// (`../tools/NST-note-search-tool.md` NST-FR-11).
pub const MAX_NOTE_BODY_BYTES: usize = 1024;

/// NTC-FR-23: a `body` above [`MAX_NOTE_BODY_BYTES`], on a create or an update.
///
/// Typed and distinct from every other failure here because the correction is
/// the author's own — shorten the note — rather than a retry of the same write.
/// The message names the bound, so a caller that surfaces the error verbatim
/// tells the author what it was.
pub const ERR_NOTE_BODY_TOO_LARGE: &str = "note_body_too_large";

/// The notes folder, project-relative. Part of the project scaffold
/// (`PST-project-storage.md` PST-FR-02) and committed — it carries no gitignore
/// rule, unlike `.synthesis/cache/` and `.synthesis/drafts/`.
const NOTES_REL: &str = ".synthesis/notes";

// ---------------------------------------------------------------------------
// Wire shapes (the Contract surface of NTC-notes-storage.md)
// ---------------------------------------------------------------------------

/// What a note is attached to (NTC-FR-03).
///
/// `entity_path` is the entity's **last-known** project-relative path. It is
/// what the panel renders for a note whose entity no longer resolves
/// (`../ui/NTS-notes.md` NTS-FR-23), so it is recorded on every write that sets
/// an entity scope and rewritten when a rename is followed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum NoteScope {
    #[serde(rename_all = "camelCase")]
    Entity {
        entity_id: String,
        #[serde(default)]
        entity_path: String,
    },
    Project,
}

impl NoteScope {
    /// The entity id this scope names, or `None` for a project-wide note.
    pub fn entity_id(&self) -> Option<&str> {
        match self {
            NoteScope::Entity { entity_id, .. } => Some(entity_id),
            NoteScope::Project => None,
        }
    }
}

/// A persisted note (NTC-FR-03).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub scope: NoteScope,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reminder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A note as the list commands return it: the record plus what the current
/// filesystem says about the entity it names (NTC-FR-10).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteListItem {
    pub note: Note,
    /// The entity's basename, present only while the entity resolves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_name: Option<String>,
    /// True when the scope names an entity that resolves to nothing on disk.
    pub unresolved: bool,
    /// NTC-FR-19: the note's one discussion, absent when it carries none.
    ///
    /// Read from the comment store's index rather than stored here (CMS-FR-63):
    /// nothing about the association is written into a note file, so what this
    /// reports is what the comment store holds at the moment of the read. It is
    /// what lets a note row know whether **Discuss** reopens a conversation or
    /// begins one, without a call of its own (NTS-FR-28).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discussion_id: Option<String>,
}

/// The partial update of `update_note` (NTC-FR-05): a field absent from the
/// payload is carried through unchanged.
///
/// `reminder` is a double option so the three cases stay distinct on the wire:
/// absent (leave it alone), `null` (clear it), a string (set it). `id`,
/// `created_at` and `revision` are deliberately not writable.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NoteFields {
    pub body: Option<String>,
    #[serde(deserialize_with = "double_option")]
    pub reminder: Option<Option<String>>,
    pub scope: Option<NoteScope>,
}

fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

// ---------------------------------------------------------------------------
// The indexing channel (NTC-FR-24)
// ---------------------------------------------------------------------------

/// One change to the set of notes, or to a note's `body`, as it travels the
/// internal channel of NTC-FR-24.
///
/// The channel exists because `.synthesis/notes/` sits outside what the project
/// scan surfaces (`ASC-artifact-scanning.md` ASC-FR-09), so no watcher reports a
/// note file and there is nothing else an indexer could observe one through. It
/// is internal by construction: not a Tauri event, and reachable by no frontend
/// `invoke`.
///
/// It carries note ids alone and never a body — a note is what an author wrote
/// to themselves, and the consumer reads the text through [`note_documents`] if
/// it wants it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteChange {
    /// The note created, rewritten, or deleted.
    pub note_id: String,
}

impl NoteChange {
    pub(crate) fn to(note_id: &str) -> NoteChange {
        NoteChange {
            note_id: note_id.to_string(),
        }
    }
}

/// NTC-FR-24: surface a change on the internal channel.
///
/// Best-effort, and not a precondition of the operation that reported it: a
/// consumer's absence or slowness delays no note command.
pub(crate) fn announce_note_change<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    change: NoteChange,
) {
    crate::bm25_index::note_notes_change(app, &change);
}

/// One note as the `notes` index takes it (NTC-FR-24, BMI-FR-28): the note's id
/// and the whole of its `body`, and nothing else about it.
///
/// The scope, the last-known entity path, the reminder, the revision, and the
/// timestamps are deliberately absent — they are indexed nowhere, so a query
/// reaches what the author wrote and never where the note was filed or when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteDocument {
    pub note_id: String,
    pub body: String,
}

/// A note file [`note_documents`] would not serve (NTC-FR-14).
///
/// Carried out rather than swallowed so the consumer can say so: an author whose
/// note cannot be found has nowhere else to look, and BMI-FR-28 makes the
/// indexer answerable for a `WARN` naming each. The `reason` is a fixed phrase
/// this module chose and never a line of the file, which is user content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteSkip {
    /// The file's stem, which is the id a well-formed note there would carry.
    pub note_id: String,
    pub reason: &'static str,
}

const SKIP_UNREADABLE: &str = "the note file could not be read as TOML";
const SKIP_ID_MISMATCH: &str = "the file name does not match the note id inside it";
const SKIP_MALFORMED: &str = "the file does not describe a note";

// ---------------------------------------------------------------------------
// On-disk shape
// ---------------------------------------------------------------------------

/// The TOML a note file holds.
///
/// Deliberately **flat** rather than mirroring the wire shape's nested scope:
/// TOML requires every scalar of a table to precede its sub-tables, so a nested
/// `[scope]` between `id` and `body` would either fail to serialize or produce a
/// file that reads back wrong. Flat also keeps a note's diff to the line that
/// actually changed.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredNote {
    id: String,
    /// `"entity"` or `"project"`. Anything else is malformed (NTC-FR-14).
    scope: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    entity_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    entity_path: String,
    /// Always written, even when empty. A note may legitimately hold no text —
    /// the panel renders such a row as an empty note — so this field must NOT
    /// gain the `skip_serializing_if` its neighbours carry: an omitted `body`
    /// fails to deserialize, and the note would then be skipped as malformed
    /// (NTC-FR-14) and disappear.
    body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reminder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    revision: Option<String>,
    created_at: String,
    updated_at: String,
}

const SCOPE_ENTITY: &str = "entity";
const SCOPE_PROJECT: &str = "project";

impl StoredNote {
    /// `None` when the file parsed but does not describe a note this module can
    /// serve — an unknown scope, or an entity scope naming nothing. Treated
    /// exactly like a parse failure: skipped, never repaired (NTC-FR-14).
    fn into_note(self) -> Option<Note> {
        let scope = match self.scope.as_str() {
            SCOPE_ENTITY if !self.entity_id.is_empty() => NoteScope::Entity {
                entity_path: if self.entity_path.is_empty() {
                    self.entity_id.clone()
                } else {
                    self.entity_path
                },
                entity_id: self.entity_id,
            },
            SCOPE_PROJECT => NoteScope::Project,
            _ => return None,
        };
        if self.id.is_empty() {
            return None;
        }
        Some(Note {
            id: self.id,
            scope,
            body: self.body,
            reminder: self.reminder,
            revision: self.revision,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }

    fn from_note(note: &Note) -> Self {
        let (scope, entity_id, entity_path) = match &note.scope {
            NoteScope::Entity {
                entity_id,
                entity_path,
            } => (
                SCOPE_ENTITY.to_string(),
                entity_id.clone(),
                entity_path.clone(),
            ),
            NoteScope::Project => (SCOPE_PROJECT.to_string(), String::new(), String::new()),
        };
        StoredNote {
            id: note.id.clone(),
            scope,
            entity_id,
            entity_path,
            body: note.body.clone(),
            reminder: note.reminder.clone(),
            revision: note.revision.clone(),
            created_at: note.created_at.clone(),
            updated_at: note.updated_at.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Paths + I/O
// ---------------------------------------------------------------------------

fn notes_dir(root: &Path) -> PathBuf {
    root.join(NOTES_REL)
}

/// The project-relative path of a note file, or a typed error for an id this
/// module will not resolve.
fn note_rel(id: &str) -> Result<String, String> {
    if !is_valid_id(id) {
        return Err(format!("invalid note id: {id:?}"));
    }
    Ok(format!("{NOTES_REL}/{id}.toml"))
}

/// The absolute path of a note file, behind the FSA-FR-10 escape gate.
fn note_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    let rel = note_rel(id)?;
    fsa::resolve_under(root, &rel).map_err(|e| e.to_string())
}

/// Every well-formed note in the project, in no particular order.
///
/// NTC-FR-14: a file that is missing, unreadable, malformed, or does not
/// describe a note is skipped rather than failing the load, and is left on disk
/// untouched — one damaged note never costs the user the rest of them.
fn load_all(root: &crate::fs::RootFs) -> Vec<Note> {
    load_all_reporting(root).0
}

/// The same load, saying which files it would not serve and why (NTC-FR-24).
///
/// The skip list exists for the one consumer that is answerable for reporting
/// it — the `notes` index, which owes a `WARN` per note it could not take up
/// (`BMI-bm25-indexing.md` BMI-FR-28). Every other caller here reads through
/// [`load_all`] and drops it, a damaged note costing them nothing.
fn load_all_reporting(root: &crate::fs::RootFs) -> (Vec<Note>, Vec<NoteSkip>) {
    let dir = notes_dir(root);
    let entries = match root.list_dir(&dir) {
        Ok(entries) => entries,
        // No notes folder yet is simply no notes; it is created by the first
        // write (FSA-FR-05).
        Err(_) => return (Vec::new(), Vec::new()),
    };
    let mut notes = Vec::new();
    let mut skipped = Vec::new();
    for entry in entries {
        let path = dir.join(&entry.name);
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        // NTC-FR-01: a note lives at `<id>.toml`, and every other operation
        // addresses it by that name. A file whose stem disagrees with the id
        // inside it — a merge resolution, a hand edit, a `git mv` — is skipped
        // like any other malformed note: serving it would put a row in the
        // panel that `update_note` and `delete_note` can never reach, leaving
        // the user an error they cannot clear.
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let mut skip = |reason: &'static str| {
            skipped.push(NoteSkip {
                note_id: stem.clone(),
                reason,
            });
        };
        match root.read_toml::<StoredNote>(&path) {
            Ok(stored) => {
                if stored.id != stem {
                    skip(SKIP_ID_MISMATCH);
                    continue;
                }
                match stored.into_note() {
                    Some(note) => notes.push(note),
                    None => skip(SKIP_MALFORMED),
                }
            }
            Err(_) => skip(SKIP_UNREADABLE),
        }
    }
    (notes, skipped)
}

fn load_one(root: &crate::fs::RootFs, id: &str) -> Result<Note, String> {
    let path = note_path(root, id)?;
    let stored =
        root.read_toml::<StoredNote>(&path).map_err(|_| ERR_NOTE_NOT_FOUND.to_string())?;
    stored.into_note().ok_or_else(|| ERR_NOTE_NOT_FOUND.to_string())
}

/// NTC-FR-13: the single write path. Nothing in this module writes a note file
/// any other way.
fn store(root: &crate::fs::RootFs, note: &Note) -> Result<(), String> {
    let path = note_path(root, &note.id)?;
    root.write_toml_atomic(path, &StoredNote::from_note(note)).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Listing (NTC-FR-09 / NTC-FR-10)
// ---------------------------------------------------------------------------

/// Whether `entity_id` names a file that currently exists under the content
/// root. Entity ids are project-relative paths (ASC-FR-13), so this answers the
/// same question the scan does without walking the tree on every list call —
/// which is what keeps a list to the single round-trip its non-functional
/// requirement asks for.
///
/// `is_file`, not `exists`: a note's entity is an artifact or a Harness, so an
/// id that has come to name a directory resolves to no node and the note is
/// unresolved rather than being served with a folder's basename.
pub(crate) fn entity_resolves(root: &crate::fs::RootFs, entity_id: &str) -> bool {
    // `file_info` rather than `Path::is_file`, which follows a symlink and so
    // would call a link a resolved entity (FSA-FR-17 / FSA-FR-23).
    fsa::resolve_under(root.path(), entity_id)
        .ok()
        .and_then(|p| root.file_info(p).ok())
        .is_some_and(|i| i.kind == fsa::EntryKind::File)
}

/// NTC-FR-10: attach what the filesystem currently says about the note's entity.
/// A project-wide note is never unresolved and carries no entity name.
/// `discussions` is the index read once for the whole list (NTC-FR-19), so a
/// panel of two hundred notes costs one directory read rather than two hundred.
fn resolve_item(
    root: &crate::fs::RootFs,
    note: Note,
    discussions: &HashMap<String, String>,
) -> NoteListItem {
    let discussion_id = discussions.get(&note.id).cloned();
    match note.scope.entity_id() {
        Some(entity_id) if entity_resolves(root, entity_id) => {
            let entity_name = basename(entity_id);
            NoteListItem {
                note,
                entity_name: Some(entity_name),
                unresolved: false,
                discussion_id,
            }
        }
        Some(_) => NoteListItem {
            note,
            entity_name: None,
            unresolved: true,
            discussion_id,
        },
        None => NoteListItem {
            note,
            entity_name: None,
            unresolved: false,
            discussion_id,
        },
    }
}

/// NTC-FR-09: most-recently-edited first. `updated_at` is fixed-width UTC, so a
/// string comparison is a chronological one. The id breaks a tie so a list is
/// stable across loads rather than following directory order.
fn sort_most_recent_first(notes: &mut [Note]) {
    notes.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| b.created_at.cmp(&a.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
}

fn list_impl<F>(root: &crate::fs::RootFs, store: &crate::fs::RootFs, keep: F) -> Vec<NoteListItem>
where
    F: Fn(&Note) -> bool,
{
    let mut notes: Vec<Note> = load_all(root).into_iter().filter(|n| keep(n)).collect();
    sort_most_recent_first(&mut notes);
    // NTC-FR-19: one pass over the comment store's notes root for the whole
    // list, whichever of the three list commands is being served.
    let discussions = crate::comments::note_discussion_index(store);
    notes
        .into_iter()
        .map(|note| resolve_item(root, note, &discussions))
        .collect()
}

/// Whether the project holds a note under this id.
///
/// The existence check `comments::get_or_create_note_discussion_in` performs
/// before it writes anything (CMS-FR-62), so a `note_id` naming nothing leaves
/// no folder behind. It resolves the note the same way every other operation
/// here does — by its own filename — so a note this returns `true` for is one
/// `update_note` and `delete_note` can both reach.
pub fn note_exists(root: &crate::fs::RootFs, id: &str) -> bool {
    load_note(root, id).is_some()
}

/// The ids of every note the project holds, read in one pass.
pub fn note_ids(root: &crate::fs::RootFs) -> std::collections::HashSet<String> {
    load_all(root).into_iter().map(|n| n.id).collect()
}

/// One note by id, or `None` for an id that names nothing this module will
/// serve — malformed, missing, or a file whose stem disagrees with the id
/// inside it (NTC-FR-14).
pub fn load_note(root: &crate::fs::RootFs, id: &str) -> Option<Note> {
    if note_rel(id).is_err() {
        return None;
    }
    load_all(root).into_iter().find(|n| n.id == id)
}

pub fn list_notes_for_entity_in(
    root: &crate::fs::RootFs,
    store: &crate::fs::RootFs,
    entity_id: &str,
) -> Vec<NoteListItem> {
    list_impl(root, store, |n| n.scope.entity_id() == Some(entity_id))
}

pub fn list_project_notes_in(
    root: &crate::fs::RootFs,
    store: &crate::fs::RootFs,
) -> Vec<NoteListItem> {
    list_impl(root, store, |n| matches!(n.scope, NoteScope::Project))
}

pub fn list_all_notes_in(root: &crate::fs::RootFs, store: &crate::fs::RootFs) -> Vec<NoteListItem> {
    list_impl(root, store, |_| true)
}

/// NTC-FR-24: one `{ note_id, body }` entry per persisted note of the active
/// worktree, for the module that keeps the `notes` index current.
///
/// Every note the store holds, in the order NTC-FR-09 lists them, and skipping
/// the missing or malformed ones NTC-FR-14 skips — those come back in the second
/// half of the pair so the consumer can report them (BMI-FR-28) rather than
/// leaving an author's note silently unfindable.
///
/// It carries the body and nothing else about the note: the scope, the
/// last-known entity path, the reminder, the revision, and the timestamps are
/// indexed nowhere (BMI-FR-28), so handing them over would only invite a query
/// to match on them.
///
/// Read-only. No record is rewritten, no `updated_at` is refreshed, and nothing
/// is surfaced on the channel of NTC-FR-24 by a read.
pub fn note_documents(root: &crate::fs::RootFs) -> (Vec<NoteDocument>, Vec<NoteSkip>) {
    let (mut notes, skipped) = load_all_reporting(root);
    sort_most_recent_first(&mut notes);
    let documents = notes
        .into_iter()
        .map(|note| NoteDocument {
            note_id: note.id,
            body: note.body,
        })
        .collect();
    (documents, skipped)
}

/// NTC-FR-25: the note a caller holding an id and nothing else is asking about.
///
/// Read from `.synthesis/notes/<id>.toml` in the active worktree and never from
/// a cache, so a caller sees the note as it now stands rather than as some
/// earlier index pass found it. That freshness is the whole point of it:
/// `../tools/NST-note-search-tool.md` (NST-FR-12) addresses a note by id alone
/// and must report the scope and the body this module holds now, a note moved or
/// edited since the last pass included.
///
/// [`ERR_NOTE_NOT_FOUND`] for an id no note in the active worktree carries and
/// for one whose file is missing or malformed (NTC-FR-14). It reports nothing of
/// the note's discussion, which is `CMS-comments-storage.md`'s.
///
/// Read-only — no record is rewritten, no `updated_at` is refreshed, and nothing
/// is surfaced on the channel of NTC-FR-24 — because reading a note is not
/// activity on it. An internal Rust call, registered as no Tauri command, so no
/// frontend surface reaches a note through it.
pub fn note_record(root: &crate::fs::RootFs, id: &str) -> Result<Note, String> {
    let note = load_one(root, id)?;
    // NTC-FR-01 / NTC-FR-14: a note lives at `<id>.toml` and is addressed by
    // that name everywhere. A file whose stem disagrees with the id inside it is
    // what the listing path skips as malformed, and answering for it here would
    // hand a caller an id that no later call could resolve — which is exactly
    // the row `../tools/NST-note-search-tool.md` NST-FR-13 drops.
    if note.id != id {
        return Err(ERR_NOTE_NOT_FOUND.to_string());
    }
    Ok(note)
}

// ---------------------------------------------------------------------------
// Mutation (NTC-FR-04 … NTC-FR-08)
// ---------------------------------------------------------------------------

/// Record `entity_path` from the entity's current project-relative path
/// (NTC-FR-04 / NTC-FR-07). An entity id *is* that path (ASC-FR-13), so a scope
/// arriving from the UI with no path — or a stale one — is normalised here
/// rather than trusted.
fn normalize_scope(scope: NoteScope) -> NoteScope {
    match scope {
        NoteScope::Entity { entity_id, .. } => NoteScope::Entity {
            entity_path: entity_id.clone(),
            entity_id,
        },
        NoteScope::Project => NoteScope::Project,
    }
}

/// NTC-FR-04: a new note whose `created_at` and `updated_at` are the same
/// instant.
pub fn create_note_in(
    root: &crate::fs::RootFs,
    scope: NoteScope,
    body: String,
    reminder: Option<String>,
    revision: Option<String>,
    now: &str,
) -> Result<Note, String> {
    // NTC-FR-23: refused whole rather than trimmed to fit. Checked before
    // anything is written, so a refusal leaves no file under `.synthesis/notes/`
    // and no note to list.
    check_body_bound(&body)?;
    if let NoteScope::Entity { entity_id, .. } = &scope {
        if entity_id.trim().is_empty() {
            return Err("entity scope requires an entity id".into());
        }
    }
    let note = Note {
        id: new_note_id(),
        scope: normalize_scope(scope),
        body,
        reminder,
        revision,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    };
    store(root, &note)?;
    Ok(note)
}

/// NTC-FR-05 / NTC-FR-06: a partial update. A field absent from `fields` is
/// carried through unchanged; any change to `body`, `reminder` or `scope` stamps
/// `updated_at` with the instant of the write, and `created_at` is never
/// rewritten.
pub fn update_note_in(
    root: &crate::fs::RootFs,
    id: &str,
    fields: NoteFields,
    now: &str,
) -> Result<Note, String> {
    update_note_at(root, id, fields, now).map(|(note, _)| note)
}

/// The same update, saying whether the note's **body** actually changed.
///
/// That is the one thing the channel of NTC-FR-24 turns on: a rewrite surfaces,
/// and an update that touched only the reminder or only the scope does not,
/// neither of them changing what is indexed. The command layer announces, for
/// the reason `crate::drafts` announces there — a write and the report of it are
/// one step from the caller's side, and neither is a precondition of the other.
pub fn update_note_at(
    root: &crate::fs::RootFs,
    id: &str,
    fields: NoteFields,
    now: &str,
) -> Result<(Note, bool), String> {
    // NTC-FR-23: refused before the note is even loaded, so nothing is written,
    // no `updated_at` is refreshed, and the note stands exactly as it did. An
    // update carrying no `body` is unaffected however long the stored body is.
    if let Some(body) = &fields.body {
        check_body_bound(body)?;
    }
    let mut note = load_one(root, id)?;
    let mut changed = false;
    let mut body_changed = false;
    // NTC-FR-06 turns on an actual *change*, not on a submission: re-saving a
    // note without editing it must not reorder the panel (NTS-FR-15) or claim
    // the user touched it.
    if let Some(body) = fields.body {
        if body != note.body {
            note.body = body;
            changed = true;
            body_changed = true;
        }
    }
    if let Some(reminder) = fields.reminder {
        if reminder != note.reminder {
            note.reminder = reminder;
            changed = true;
        }
    }
    if let Some(scope) = fields.scope {
        if let NoteScope::Entity { entity_id, .. } = &scope {
            if entity_id.trim().is_empty() {
                return Err("entity scope requires an entity id".into());
            }
        }
        let scope = normalize_scope(scope);
        if scope != note.scope {
            note.scope = scope;
            changed = true;
        }
    }
    if changed {
        note.updated_at = now.to_string();
    }
    store(root, &note)?;
    Ok((note, body_changed))
}

/// NTC-FR-23: the bound, applied to a body about to be written.
///
/// `len()` is the UTF-8 byte count, which is the unit the requirement names —
/// a body of 400 emoji is over the bound while its character count is nowhere
/// near it.
fn check_body_bound(body: &str) -> Result<(), String> {
    if body.len() > MAX_NOTE_BODY_BYTES {
        return Err(format!(
            "{ERR_NOTE_BODY_TOO_LARGE}: a note is at most {MAX_NOTE_BODY_BYTES} bytes of UTF-8"
        ));
    }
    Ok(())
}

/// NTC-FR-08 / NTC-FR-21: remove the note **and whatever discussion it carries**,
/// as one transaction owned here.
///
/// The order is the whole of what makes this recoverable, and it is deliberate:
/// the discussion goes first, then the note. A failure of the first leaves both
/// exactly as they were; a failure of the second leaves a note whose
/// conversation is gone, which a retry completes and which **Discuss** begins
/// afresh on. Neither failure can leave a discussion no note names, because the
/// only step that could orphan one is the step that runs last.
///
/// Both steps are idempotent (CMS-FR-64, and the not-found below), so a retry of
/// a partially applied deletion completes rather than failing on work already
/// done. A note carrying no discussion deletes exactly as it did before, the
/// cleanup step finding nothing to remove.
pub fn delete_note_in(
    root: &crate::fs::RootFs,
    store: &crate::fs::RootFs,
    id: &str,
) -> Result<(), String> {
    let rel = note_rel(id)?;
    // NTC-FR-22: **both** steps under one critical section, so an opening cannot
    // slip between them and leave a conversation for a note this call is about
    // to remove. Held for the whole transaction rather than for the cleanup
    // alone, which is the difference between the two steps being individually
    // atomic and the transaction being so.
    let _guard = crate::comments::lock_note_discussions();
    // NTC-FR-21: first, so a failure here leaves the note and its whole
    // conversation intact and nothing has been reported as deleted.
    crate::comments::delete_note_discussion_locked(store, id)
        .map_err(|_| ERR_DISCUSSION_CLEANUP_FAILED.to_string())?;
    // NTC-FR-13: a note is a single file, so the narrow (non-recursive) delete
    // is the right one — this module never removes a directory.
    match root.delete_under(root.path(), &rel, false) {
        Ok(()) => Ok(()),
        Err(fsa::FsError::NotFound { .. }) => Err(ERR_NOTE_NOT_FOUND.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Rename following (NTC-FR-11) and reminders (NTC-FR-17)
// ---------------------------------------------------------------------------

/// NTC-FR-11: rewrite every note bound to `old_rel` — or to something inside it,
/// when a folder moved — onto `new_rel`, leaving `updated_at` untouched because
/// the note's content did not change.
///
/// This is the correlation seam of NTC-FR-11: it is called where the application
/// knows a single old path became a single new path. A filesystem change it
/// cannot correlate that way never reaches here, and its notes stay bound to an
/// id that no longer resolves — surfaced per NTC-FR-10 and reattached by the
/// user through `update_note` (NTC-FR-12).
///
/// Best-effort: a note that cannot be rewritten is left as it was rather than
/// failing the rename that triggered this. Returns how many notes moved.
pub fn follow_rename(root: &crate::fs::RootFs, old_rel: &str, new_rel: &str) -> usize {
    if old_rel.is_empty() || new_rel.is_empty() || old_rel == new_rel {
        return 0;
    }
    let prefix = format!("{old_rel}/");
    let mut moved = 0;
    for mut note in load_all(root) {
        let Some(entity_id) = note.scope.entity_id() else {
            continue;
        };
        let rewritten = if entity_id == old_rel {
            new_rel.to_string()
        } else if let Some(tail) = entity_id.strip_prefix(&prefix) {
            format!("{new_rel}/{tail}")
        } else {
            continue;
        };
        note.scope = NoteScope::Entity {
            entity_path: rewritten.clone(),
            entity_id: rewritten,
        };
        if store(root, &note).is_ok() {
            moved += 1;
        }
    }
    moved
}

/// NTC-FR-17: the notes carrying a reminder, with their scope and instant. This
/// is what `PST-project-storage.md`'s `list_due_reminders` loader (PST-FR-12)
/// reads to populate the Dashboard's Reminders widget; this module schedules no
/// delivery of its own.
pub fn reminder_notes(root: &crate::fs::RootFs) -> Vec<Note> {
    let mut notes: Vec<Note> = load_all(root)
        .into_iter()
        .filter(|n| n.reminder.is_some())
        .collect();
    // Soonest first: a due-reminder widget reads from the front of this list.
    notes.sort_by(|a, b| a.reminder.cmp(&b.reminder).then_with(|| a.id.cmp(&b.id)));
    notes
}

mod commands;
mod timestamps;

pub use commands::*;
pub use timestamps::*;
pub(crate) use timestamps::is_valid_id;

#[cfg(test)]
mod tests;
