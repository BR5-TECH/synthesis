//! BM25 indexing (`BMI-bm25-indexing.md`).
//!
//! Twelve in-memory BM25 indexes — one per built-in artifact type (BMI-FR-02),
//! one for drafts, one for notes, one for skills, and one for the user's
//! reference documents — kept in agreement with
//! what is on disk, and a ranked multi-index [`search`] for the AI integrations
//! that retrieve against them. Nothing here is a `#[tauri::command]`: this
//! module's whole surface is the internal Rust API below, so no frontend
//! `invoke` can reach it (BMI-FR-01).
//!
//! ## Why two of the eleven are shaped differently
//!
//! Nine of the eleven hold the chunks of files. The `skills` index instead
//! holds one whole document per invocable skill, supplied by
//! `DSL-dynamic-skills-loading.md`, and the `notes` index one whole document
//! per persisted note — that note's `body` and nothing else — supplied by
//! `NTC-notes-storage.md`; each of those two modules is the only thing that
//! decides what belongs to its index (DSL-FR-13, NTC-FR-24, BMI-FR-28).
//! Neither is chunked, because a descriptor and a note are each already one
//! document with no sections to cut at, and neither takes its documents from
//! the scan's candidate list — a project may legitimately gitignore the skill
//! folders, and `.synthesis/notes/` is outside what the scan surfaces at all
//! (ASC-FR-09).
//!
//! ## Why a pass is a reconciliation rather than a delta
//!
//! Every trigger this module has — the watcher's structural events (BMI-FR-15),
//! its content-modification channel (BMI-FR-16), a type reassignment
//! (BMI-FR-17), a Git pull (BMI-FR-18), a draft mutation (BMI-FR-19), a
//! graduation (BMI-FR-20) — says only *that* something changed. A pass therefore
//! enumerates the set of files that ought to be indexed, fingerprints each, and
//! applies the difference against what is indexed now. One code path serves
//! every trigger, an interrupted pass costs staleness rather than divergence,
//! and BMI-FR-27's "incremental equals rebuilt" falls out rather than having to
//! be maintained.
//!
//! ## Why the shards are per language
//!
//! BMI-FR-08 detects a file's language once and stems every chunk of it that
//! way. A BM25 engine holds one tokenizer, so a query stemmed English can only
//! match documents stemmed English — mixing languages in one engine would make
//! a German document unfindable by a German query. Each index therefore holds
//! one engine per language actually present in it, and a query runs against
//! each with that shard's own stemming. A monolingual project — very nearly all
//! of them — has exactly one shard per index and pays nothing for this.

use std::collections::BTreeMap;
use std::path::PathBuf;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bm25::{Language, SearchEngine};
use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::logging::{log_error, log_info, log_warn, Domain, BUFFER};
use crate::progress::{self, OperationState, ProgressRegistry};
use crate::scanning::ArtifactType;

// ---------------------------------------------------------------------------
// Tuning
// ---------------------------------------------------------------------------

/// The chunk size ceiling of BMI-FR-06, in characters. No chunk exceeds it, so
/// no file — however large, however free of headings — is ever indexed as one
/// unbounded document.
pub const MAX_CHUNK_CHARS: usize = 4_000;

/// The per-file size ceiling of BMI-FR-09. A file above it contributes no
/// chunks and is skipped with a `WARN`, exactly as a file that does not decode
/// as UTF-8 is.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// BMI-FR-21: the window over which triggers collapse into one pass. A pull
/// that rewrites three hundred files, or a burst of keystroke-driven draft
/// saves, is one pass rather than one per file.
pub const COALESCE_WINDOW: Duration = Duration::from_millis(250);

// ---------------------------------------------------------------------------
// Contract surface types
// ---------------------------------------------------------------------------

/// The twelve indexes of BMI-FR-02: one per built-in artifact type of
/// ASC-FR-02, plus drafts, plus notes, plus skills, plus documents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndexId {
    Skill,
    Agent,
    Prompt,
    Spec,
    Flow,
    Instructions,
    Scenario,
    Scratchpad,
    Drafts,
    /// BMI-FR-28: one document per persisted note of the active worktree, and
    /// that document is the note's `body` and nothing else
    /// (`NTC-notes-storage.md` NTC-FR-03). A note's own TOML file reaches no
    /// artifact index — the scan does not surface `.synthesis/notes/`
    /// (ASC-FR-09, NTC-FR-16) — and this index holds no part of that file but
    /// the body.
    Notes,
    /// One descriptor per invocable skill (`DSL-dynamic-skills-loading.md`).
    /// Distinct from [`IndexId::Skill`], which holds the chunked full text of
    /// every file the scan classifies `skill` — including supporting files a
    /// skill keeps beside its `SKILL.md`, and including skills outside the four
    /// special folders (DSL-FR-14).
    Skills,
    /// BMI-FR-WBKZ: the text of every available document of the Documents
    /// collection (`DCL-documents-collection.md`). It holds no filesystem path:
    /// a document is identified by its id, which stands in the `path` slot of a
    /// key and comes back as `document_id` on a hit.
    Documents,
}

impl IndexId {
    /// Every index, in a stable order. The set a `search` with no explicit
    /// selection runs against (BMI-FR-10).
    pub const ALL: [IndexId; 12] = [
        IndexId::Skill,
        IndexId::Agent,
        IndexId::Prompt,
        IndexId::Spec,
        IndexId::Flow,
        IndexId::Instructions,
        IndexId::Scenario,
        IndexId::Scratchpad,
        IndexId::Drafts,
        IndexId::Notes,
        IndexId::Skills,
        IndexId::Documents,
    ];

    /// BMI-FR-03: the index a file's resolved artifact type names. A file with
    /// no resolved type belongs to no index, which is what leaves unclassified
    /// source files, playbooks, workstreams, and roles unindexed.
    pub fn for_artifact_type(artifact_type: ArtifactType) -> IndexId {
        match artifact_type {
            ArtifactType::Skill => IndexId::Skill,
            ArtifactType::Agent => IndexId::Agent,
            ArtifactType::Prompt => IndexId::Prompt,
            ArtifactType::Spec => IndexId::Spec,
            ArtifactType::Flow => IndexId::Flow,
            ArtifactType::Instructions => IndexId::Instructions,
            ArtifactType::Scenario => IndexId::Scenario,
            ArtifactType::Scratchpad => IndexId::Scratchpad,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            IndexId::Skill => "skill",
            IndexId::Agent => "agent",
            IndexId::Prompt => "prompt",
            IndexId::Spec => "spec",
            IndexId::Flow => "flow",
            IndexId::Instructions => "instructions",
            IndexId::Scenario => "scenario",
            IndexId::Scratchpad => "scratchpad",
            IndexId::Drafts => "drafts",
            IndexId::Notes => "notes",
            IndexId::Skills => "skills",
            IndexId::Documents => "documents",
        }
    }

    /// Whether this index is fed by the project scan (BMI-FR-03) rather than by
    /// the draft channel (BMI-FR-04), the note channel (BMI-FR-29), or the
    /// skill enumeration (DSL-FR-13).
    ///
    /// This is also what decides whether a hit carries a `node_id`: the scan is
    /// the only thing that mints one, so a draft chunk has none, a note has
    /// none — its file is not surfaced by the scan at all (ASC-FR-09) — and
    /// neither does a skill descriptor, a skill being eligible whether or not
    /// the scan surfaced its file (DSL-FR-06).
    fn is_artifact_index(self) -> bool {
        !matches!(
            self,
            IndexId::Drafts | IndexId::Notes | IndexId::Skills | IndexId::Documents
        )
    }

    /// BMI-FR-05: whether a document in this index is a chunk of a file.
    ///
    /// False for the `skills` and `notes` indexes, whose documents are already
    /// whole — a name and a description, and a note's body — with no sections
    /// to cut at. A note is bounded at 1 KiB by the store that writes it
    /// (NTC-FR-23), so nothing is gained by bounding it again here.
    fn splits_into_chunks(self) -> bool {
        !matches!(self, IndexId::Skills | IndexId::Notes)
    }
}

/// Identity of one indexed chunk: the file it came from and its position within
/// it (BMI-FR-07). Ordered so a file's chunks form one contiguous range, which
/// is what makes removing a file a range scan rather than a full walk.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkKey {
    /// The draft this chunk belongs to, or `None` for a project file.
    pub draft_id: Option<String>,
    /// Project-relative for an artifact index, draft-relative for the drafts
    /// index.
    pub path: String,
    /// 0-based position of this chunk within its own file, in file order.
    pub ordinal: u32,
}

/// The file a chunk belongs to — a [`ChunkKey`] without its ordinal.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileRef {
    pub draft_id: Option<String>,
    pub path: String,
}

impl FileRef {
    fn artifact(path: impl Into<String>) -> FileRef {
        FileRef {
            draft_id: None,
            path: path.into(),
        }
    }

    fn draft(draft_id: impl Into<String>, path: impl Into<String>) -> FileRef {
        FileRef {
            draft_id: Some(draft_id.into()),
            path: path.into(),
        }
    }

    /// BMI-FR-28: a note is identified by its id and by nothing else.
    ///
    /// It has no path in the sense the other indexes mean one: its file is
    /// `.synthesis/notes/<id>.toml`, which the scan does not surface (ASC-FR-09)
    /// and which this index holds no part of but the body. The id stands in the
    /// `path` slot because that slot is a document's identity **within its
    /// index**, and a note's identity is its id — which is why a hit from this
    /// index reports it as `note_id` rather than as a path (see [`ChunkHit`]).
    fn note(note_id: impl Into<String>) -> FileRef {
        FileRef {
            draft_id: None,
            path: note_id.into(),
        }
    }

    /// BMI-FR-FGGU: a document is identified by its id and by nothing else, for
    /// the reason a note is: no index holds a document's filesystem path.
    fn document(document_id: impl Into<String>) -> FileRef {
        FileRef {
            draft_id: None,
            path: document_id.into(),
        }
    }

    fn chunk(&self, ordinal: u32) -> ChunkKey {
        ChunkKey {
            draft_id: self.draft_id.clone(),
            path: self.path.clone(),
            ordinal,
        }
    }
}

/// One ranked chunk returned by [`search`].
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkHit {
    /// The index this chunk was found in.
    pub index: IndexId,
    /// Project-relative for an artifact index, draft-relative for drafts, and
    /// the note's own id for the `notes` index — which has no path to give
    /// (BMI-FR-28) — and the document id for the `documents` index, which holds
    /// no filesystem path either (BMI-FR-FGGU).
    pub path: String,
    /// Artifact indexes only: the ASC node id (ASC-FR-13), which for this scan
    /// is the project-relative path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    /// Drafts index only: the draft's id (DRS-FR-02).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<String>,
    /// Notes index only: the note's id (`NTC-notes-storage.md` NTC-FR-02),
    /// which is the whole of what a consumer needs to resolve the note's
    /// current record through NTC-FR-25.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_id: Option<String>,
    /// Documents index only: the document's id (`DCL-documents-collection.md`
    /// DCL-FR-QKJO), which is what a consumer resolves through the collection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    /// 0-based position of this chunk within its file (BMI-FR-07). Always 0 in
    /// the `skills` and `notes` indexes, whose documents are never split.
    pub chunk_ordinal: u32,
    /// BM25 score against this chunk's own index (BMI-FR-11).
    pub score: f32,
    /// The chunk's text.
    pub text: String,
}

// ---------------------------------------------------------------------------
// The index set
// ---------------------------------------------------------------------------

type Engine = SearchEngine<ChunkKey>;

/// What one file contributed to an index, so a later pass can tell whether it
/// still agrees with disk and can remove exactly what it added.
#[derive(Clone, Debug)]
struct FileEntry {
    /// Checksum of the text that produced the chunks below (BMI-FR-16: a
    /// content change replaces a file's chunks whole rather than amending
    /// them).
    checksum: String,
    language: Language,
    chunk_count: u32,
}

/// The eleven indexes (BMI-FR-02), as one immutable snapshot.
///
/// A pass builds the next snapshot from the current one and swaps it in whole,
/// so `search` never waits on indexing and never observes a half-applied pass
/// (BMI-FR-10). Shards a pass did not touch are shared with the previous
/// snapshot through their `Arc` rather than rebuilt.
#[derive(Default, Clone)]
pub struct IndexSet {
    files: BTreeMap<(IndexId, FileRef), FileEntry>,
    /// One engine per (index, language) actually present. The language is the
    /// key rather than a field: a shard is entirely defined by which index and
    /// which stemming it holds, and the [`FileEntry`] of each file records the
    /// language that placed it there.
    shards: BTreeMap<(IndexId, &'static str), Arc<Engine>>,
    /// DSL-FR-12: the skill registry, carried in the snapshot rather than
    /// beside it.
    ///
    /// This is what makes DSL-FR-18 structural instead of a discipline: the
    /// registry and the `skills` index reach readers through the same publish,
    /// so a skill `list_skills` returns is always one `search_skills` can rank.
    /// Held in the snapshot also means it is discarded by the same teardown
    /// (BMI-FR-25, DSL-FR-22) rather than needing one of its own.
    skills: Arc<Vec<crate::skills::SkillDescriptor>>,
    /// What was excluded from the skill registry on the pass that published
    /// this snapshot, keyed by path (DSL-FR-24).
    ///
    /// Carried so a pass can log only what *changed*. A pass runs on every
    /// filesystem change anywhere in the project, and an exclusion is usually a
    /// steady state rather than a transient — a skill deliberately carrying
    /// `disable-model-invocation: true` is excluded for the life of the
    /// session. Re-emitting its `WARN` on every unrelated edit would push real
    /// evidence out of a bounded ring buffer to say the same thing a thousand
    /// times.
    skill_exclusions: Arc<BTreeMap<String, &'static str>>,
}

impl IndexSet {
    /// BMI-FR-NEIW: a snapshot holding the `documents` index of this one and
    /// nothing else.
    fn only_documents(&self) -> IndexSet {
        IndexSet {
            files: self
                .files
                .iter()
                .filter(|((index, _), _)| *index == IndexId::Documents)
                .map(|(key, entry)| (key.clone(), entry.clone()))
                .collect(),
            shards: self
                .shards
                .iter()
                .filter(|((index, _), _)| *index == IndexId::Documents)
                .map(|(key, engine)| (*key, Arc::clone(engine)))
                .collect(),
            ..IndexSet::default()
        }
    }

    /// The skill registry this snapshot was published with (DSL-FR-15).
    pub fn skills(&self) -> &[crate::skills::SkillDescriptor] {
        &self.skills
    }

    /// Replace the skill registry. Called by a pass on the snapshot it is about
    /// to publish, so the registry and the `skills` index land together.
    pub fn set_skills(&mut self, skills: Vec<crate::skills::SkillDescriptor>) {
        self.skills = Arc::new(skills);
    }

    /// Why a path was excluded on the pass that published this snapshot, if it
    /// was.
    fn skill_exclusion(&self, path: &str) -> Option<&'static str> {
        self.skill_exclusions.get(path).copied()
    }

    fn set_skill_exclusions(&mut self, exclusions: BTreeMap<String, &'static str>) {
        self.skill_exclusions = Arc::new(exclusions);
    }

    /// How many files this index currently holds. Test- and log-facing.
    pub fn file_count(&self, index: IndexId) -> usize {
        self.files.keys().filter(|(i, _)| *i == index).count()
    }

    /// How many chunks this index currently holds.
    pub fn chunk_count(&self, index: IndexId) -> usize {
        self.files
            .iter()
            .filter(|((i, _), _)| *i == index)
            .map(|(_, e)| e.chunk_count as usize)
            .sum()
    }

    /// The paths an index holds, in order. Test-facing: index membership is
    /// what BMI-FR-03, BMI-FR-04, BMI-FR-17, and BMI-FR-27 are claims about.
    pub fn paths(&self, index: IndexId) -> Vec<String> {
        self.files
            .keys()
            .filter(|(i, _)| *i == index)
            .map(|(_, f)| f.path.clone())
            .collect()
    }

    /// The language a file was indexed under, if it is indexed at all.
    pub fn language_of(&self, index: IndexId, path: &str) -> Option<Language> {
        self.files
            .iter()
            .find(|((i, f), _)| *i == index && f.path == path)
            .map(|(_, e)| e.language.clone())
    }

    fn total_chunks(&self) -> usize {
        self.files.values().map(|e| e.chunk_count as usize).sum()
    }
}

// ---------------------------------------------------------------------------
// Reconciliation (BMI-FR-13 through BMI-FR-21, BMI-FR-24, BMI-FR-27)
// ---------------------------------------------------------------------------

/// What a pass did, for the log record that closes it (BMI-FR-23).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PassStats {
    pub files_seen: usize,
    pub files_indexed: usize,
    pub files_removed: usize,
    pub files_unchanged: usize,
    pub files_skipped: usize,
    pub chunks: usize,
}

/// One file a pass is asked to consider, already read.
pub struct SourceFile {
    pub index: IndexId,
    pub file: FileRef,
    /// `Ok(text)` to index it, `Err(reason)` to skip it with a `WARN`
    /// (BMI-FR-09).
    pub text: Result<String, String>,
    /// BMI-FR-FGGU: split at blank lines only, and never at a heading. A `text`
    /// or `pdf` document is plain text, so a `#` line in it is not a section.
    pub plain_text: bool,
}

// ---------------------------------------------------------------------------
// The module's state
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Schedule {
    /// The content root passes run against, or `None` when nothing is mounted.
    /// The content root the pass runs against, carrying the access that
    /// governs it — so a pass running on a background thread reads through the
    /// same gate the command that scheduled it would have (FSA-FR-19).
    root: Option<crate::fs::RootFs>,
    /// Whether a pass thread is alive and will run at least once more.
    scheduled: bool,
    /// Scope accumulated by triggers that arrived while a pass was pending or
    /// running (BMI-FR-21).
    pending: PassScope,
}

/// The published indexes and the generation they belong to, under one lock.
///
/// The two are inseparable: a pass checks the generation to decide whether its
/// result still describes the mounted root, and then publishes. If the check
/// and the swap were two critical sections, a `clear()` landing between them
/// would be overwritten by the very snapshot it was meant to discard, and a
/// closed project's chunks would answer queries for the rest of the session
/// (BMI-FR-25). Holding one lock across both is what makes the check binding.
struct Published {
    /// Bumped on every mount and teardown.
    generation: u64,
    set: Arc<IndexSet>,
}

impl Default for Published {
    fn default() -> Self {
        Published {
            generation: 0,
            set: Arc::new(IndexSet::default()),
        }
    }
}

/// The mounted BM25 indexes and the pass scheduler (BMI-FR-12, BMI-FR-14).
///
/// Held in memory alone: nothing here is written to the project, to
/// `.synthesis/cache/`, or to `app_data_dir()`, and nothing is read back at
/// startup, so an index is always derived from what is on disk now.
#[derive(Default)]
pub struct Bm25Indexer {
    /// The published snapshot and its generation. Cloned out under a very short
    /// lock so a query runs against an `Arc` nothing can mutate under it.
    published: Mutex<Published>,
    schedule: Mutex<Schedule>,
    /// Draft changes seen on the DRS-FR-28 channel, for the tests that assert
    /// what each command reports. Not compiled into the shipped binary.
    #[cfg(test)]
    draft_changes: Mutex<Vec<crate::drafts::DraftChange>>,
    /// Note changes seen on the NTC-FR-24 channel, on the same terms and for
    /// the same reason: the channel is internal by construction, so what a
    /// command surfaced on it has no other observable.
    #[cfg(test)]
    note_changes: Mutex<Vec<crate::notes::NoteChange>>,
}

impl Bm25Indexer {
    fn published(&self) -> std::sync::MutexGuard<'_, Published> {
        match self.published.lock() {
            Ok(guard) => guard,
            // A poisoned lock means a pass panicked mid-publish. A stale answer
            // beats no answer at all, so the inner value is taken regardless.
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// The snapshot a query runs against.
    pub fn snapshot(&self) -> Arc<IndexSet> {
        Arc::clone(&self.published().set)
    }

    /// The generation a pass starting now belongs to.
    pub(crate) fn generation(&self) -> u64 {
        self.published().generation
    }

    /// Publish `next` only if the mounted root has not moved since `generation`
    /// was taken. Returns whether it was published (BMI-FR-25).
    fn publish_if_current(&self, next: IndexSet, generation: u64) -> bool {
        let mut published = self.published();
        if published.generation != generation {
            return false;
        }
        published.set = Arc::new(next);
        true
    }

    /// Discard the indexes and move to a new generation, abandoning any pass
    /// still running against the previous one.
    fn reset(&self) {
        let mut published = self.published();
        published.generation += 1;
        published.set = Arc::new(IndexSet::default());
    }

    /// BMI-FR-NEIW: a new content root discards what the outgoing root's files
    /// produced and keeps the `documents` index, whose sources belong to the
    /// repository. The pass that follows reconciles it against the same sources.
    fn reset_keeping_documents(&self) {
        let mut published = self.published();
        published.generation += 1;
        published.set = Arc::new(published.set.only_documents());
    }

    #[cfg(test)]
    fn publish(&self, next: IndexSet) {
        self.published().set = Arc::new(next);
    }

    /// The scheduler, locked.
    ///
    /// Read what you need into a binding before calling anything else on this
    /// type: the guard lives to the end of the enclosing statement, so
    /// `run_pass(.., self.schedule().generation)` would still hold it when
    /// `run_pass` reaches for the same non-reentrant mutex.
    fn schedule(&self) -> std::sync::MutexGuard<'_, Schedule> {
        match self.schedule.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// BMI-FR-13: mount on `root` and discard whatever the previous root left.
    /// The full build itself is a pass like any other, requested by the caller.
    pub fn mount(&self, root: &crate::fs::RootFs) {
        // The generation moves first, so a pass finishing against the outgoing
        // root can no longer publish (BMI-FR-25). The two locks are taken in
        // sequence and never held together, here or anywhere else in this file.
        self.reset_keeping_documents();
        let mut schedule = self.schedule();
        schedule.root = Some(root.clone());
        schedule.pending = PassScope::ALL;
    }

    /// BMI-FR-25: discard all twelve indexes, the skill registry with them
    /// (DSL-FR-22). A pass already running abandons its
    /// result when it sees the generation has moved.
    pub fn clear(&self) {
        self.reset();
        let mut schedule = self.schedule();
        schedule.root = None;
        schedule.pending = PassScope::default();
    }

    /// The root passes currently run against, if any.
    pub fn root(&self) -> Option<PathBuf> {
        self.schedule().root.as_ref().map(|r| r.to_path_buf())
    }

    /// The same root carrying the access that governs it.
    ///
    /// Wanted by the one consumer that reads a file the registry named rather
    /// than answering from the snapshot alone (`../specifications/tools/LSK-load-skill-tool.md`
    /// LSK-FR-15): reading through this handle is what puts that read behind the
    /// same FSA gate the pass that enumerated the file used, rather than behind
    /// an instance the tool minted for itself.
    pub fn root_fs(&self) -> Option<crate::fs::RootFs> {
        self.schedule().root.clone()
    }

    /// Record a draft change for the tests that assert what each command
    /// reports on the DRS-FR-28 channel. A no-op in the shipped binary.
    fn record_draft_change(&self, change: &crate::drafts::DraftChange) {
        #[cfg(test)]
        match self.draft_changes.lock() {
            Ok(mut log) => log.push(change.clone()),
            Err(poisoned) => poisoned.into_inner().push(change.clone()),
        }
        #[cfg(not(test))]
        let _ = change;
    }

    /// Every draft change reported since the last drain. Test-facing: DRS-FR-28
    /// has no other observable, the channel being internal by construction.
    #[cfg(test)]
    pub fn take_draft_changes(&self) -> Vec<crate::drafts::DraftChange> {
        match self.draft_changes.lock() {
            Ok(mut log) => std::mem::take(&mut *log),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        }
    }

    /// Record a note change for the tests that assert what each command reports
    /// on the NTC-FR-24 channel. A no-op in the shipped binary.
    fn record_note_change(&self, change: &crate::notes::NoteChange) {
        #[cfg(test)]
        match self.note_changes.lock() {
            Ok(mut log) => log.push(change.clone()),
            Err(poisoned) => poisoned.into_inner().push(change.clone()),
        }
        #[cfg(not(test))]
        let _ = change;
    }

    /// Every note change reported since the last drain. Test-facing: NTC-FR-24
    /// has no other observable, the channel being internal by construction.
    #[cfg(test)]
    pub fn take_note_changes(&self) -> Vec<crate::notes::NoteChange> {
        match self.note_changes.lock() {
            Ok(mut log) => std::mem::take(&mut *log),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        }
    }

    /// Run one pass synchronously against `root`, for the tests and for the
    /// pass thread. Returns the stats of the pass, or `None` when it was
    /// abandoned because the mounted root moved under it.
    pub(crate) fn run_pass<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        root: &crate::fs::RootFs,
        scope: PassScope,
        generation: u64,
    ) -> Option<PassStats> {
        let registry = app.state::<ProgressRegistry>();

        // BMI-FR-22: one operation per pass, terminated whatever the outcome.
        let operation = progress::register_and_publish(
            app,
            &registry,
            "index",
            "Indexing project…",
            Some(root.to_path_buf()),
            None,
        );
        log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "index pass started",
            crate::log_fields! {
                "artifacts" => scope.artifacts,
                "drafts" => scope.drafts,
                "notes" => scope.notes,
                "documents" => scope.documents,
            },
        );

        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (sources, skills, exclusions) = collect_sources(app, root, scope);
            progress::update_and_publish(
                app,
                &registry,
                operation,
                Some(0),
                Some(sources.len() as u64),
            );
            let previous = self.snapshot();
            log_skill_exclusions(app, &previous, &exclusions);
            let (mut next, stats) = reconcile(
                &previous,
                sources,
                scope,
                |index, file, reason| {
                    // A note has no path to name (BMI-FR-28) — its identity is
                    // its id, and calling that a `path` would send a reader
                    // looking for a file that this index holds no part of.
                    // Neither field ever carries a line of what was skipped:
                    // the reason is a fixed phrase, and the contents are user
                    // content.
                    let mut fields = crate::log_fields! {
                        "index" => index.as_str(),
                        "reason" => reason,
                    };
                    if *index == IndexId::Notes {
                        fields.insert("note".to_string(), serde_json::json!(file.path));
                    } else if *index == IndexId::Documents {
                        // BMI-FR-FGGU: a document is named by its id, which is
                        // all this index holds of it.
                        fields.insert("document".to_string(), serde_json::json!(file.path));
                    } else {
                        fields.insert("path".to_string(), serde_json::json!(file.path));
                        fields.insert("draft".to_string(), serde_json::json!(file.draft_id));
                    }
                    log_warn(app, &BUFFER, &[Domain::Backend], "index skipped a file", fields);
                },
                |processed| {
                    progress::update_and_publish(
                        app,
                        &registry,
                        operation,
                        Some(processed),
                        None,
                    );
                },
            );
            // DSL-FR-18: the registry rides the same snapshot as the `skills`
            // index, so the one publish below makes both visible at once and
            // neither can be observed without the other.
            next.set_skills(skills);
            next.set_skill_exclusions(exclusions);
            (next, stats)
        }));

        let (next, stats) = match outcome {
            Ok(result) => result,
            Err(_) => {
                // BMI-FR-24: the indexes are left as they were rather than
                // partially updated, and the next change's pass runs normally.
                log_error(
                    app,
                    &BUFFER,
                    &[Domain::Backend],
                    "index pass failed",
                    crate::log_fields! {
                        "artifacts" => scope.artifacts,
                        "drafts" => scope.drafts,
                        "notes" => scope.notes,
                        "documents" => scope.documents,
                    },
                );
                progress::terminate_and_publish(
                    app,
                    &registry,
                    operation,
                    OperationState::Failed,
                );
                return None;
            }
        };

        // BMI-FR-25: the root moved while this pass ran, so its result
        // describes a checkout the application is no longer reading. The check
        // and the swap are one critical section, so a teardown cannot land
        // between them and be overwritten by the snapshot it was discarding.
        if !self.publish_if_current(next, generation) {
            progress::terminate_and_publish(app, &registry, operation, OperationState::Cancelled);
            return None;
        }
        progress::terminate_and_publish(app, &registry, operation, OperationState::Finished);
        log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "index pass finished",
            crate::log_fields! {
                "files" => stats.files_seen,
                "indexed" => stats.files_indexed,
                "unchanged" => stats.files_unchanged,
                "removed" => stats.files_removed,
                "skipped" => stats.files_skipped,
                "chunks" => stats.chunks,
            },
        );
        Some(stats)
    }
}

mod chunking;
mod documents;
mod reconcile;
mod retrieval;
mod scheduling;
mod sources;

pub use chunking::{chunk_text, detect_file_language};
pub use reconcile::reconcile;
pub use retrieval::{search, search_snapshot};
pub use scheduling::{
    note_draft_change, note_notes_change, request_pass, scope_for_draft_change,
    scope_for_note_change, PassScope,
};

use chunking::bound_to_blocks;
use sources::{collect_sources, log_skill_exclusions};
#[cfg(test)]
use crate::scanning::CandidateStore;
#[cfg(test)]
use sources::read_bounded;

#[cfg(test)]
mod tests;
