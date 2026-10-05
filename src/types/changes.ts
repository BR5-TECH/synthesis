// Changes panel (CHG-changes.md / CHC-changes.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

import { ArtifactType, TypeSource } from "./core";

// ---------------------------------------------------------------------------
// Changes panel (CHG-changes.md / CHC-changes.md)
// ---------------------------------------------------------------------------

/** How Git sees a changed file (CHC-FR-06 / CHC-FR-09). */
export type ChangeStatus =
  | "added"
  | "modified"
  | "deleted"
  | "renamed"
  | "untracked";

/**
 * Which comparison produced a change set. `uncommitted` is the working tree
 * against HEAD (CHC-FR-03); `branch` is the working tree against the merge-base
 * of the current branch and `targetBranch` (CHC-FR-04).
 */
export type Comparison =
  | { kind: "uncommitted" }
  | { kind: "branch"; targetBranch: string; mergeBase: string };

/**
 * One changed file, as `list_uncommitted_changes` / `list_branch_changes`
 * report it. `addedLines` / `removedLines` are null for binary content — the
 * backend never fabricates counts (CHC-FR-08), and the row renders a marker in
 * their place (CHG-FR-11).
 */
export interface ChangeEntry {
  /** The same stable path-derived key the Library tree uses (CHC-FR-11). */
  id: string;
  path: string;
  name: string;
  changeStatus: ChangeStatus;
  /** Renamed entries only (CHC-FR-09 / CHG-FR-12). */
  previousPath?: string;
  addedLines: number | null;
  removedLines: number | null;
  isBinary: boolean;
  artifactType?: ArtifactType;
  typeSource?: TypeSource;
}

/** A comparison plus its flat entry list; the UI builds the tree (CHC-FR-12). */
export interface ChangeSet {
  comparison: Comparison;
  entries: ChangeEntry[];
}

/** One option in the target-branch picker (CHC-FR-14 / CHG-FR-05). */
export interface BranchOption {
  name: string;
  isCurrent: boolean;
  isDefault: boolean;
}

/** Payload of the backend `"changes updated"` event (CHC-FR-16). */
export interface ChangesUpdatedPayload {
  changeCount: number;
}

/**
 * The summed line counts of the uncommitted change set (CHC-FR-21), rendered as
 * the status bar's diff summary (STB-FR-25).
 *
 * `fileCount` counts every entry, binary included; the line totals count only
 * the entries that have counts at all, since none is fabricated for binary
 * content (CHC-FR-08 / STB-FR-28).
 */
export interface DiffTotals {
  addedLines: number;
  removedLines: number;
  fileCount: number;
}

/** The panel's two mutually-exclusive modes (CHG-FR-02). */
export type ChangesMode = "uncommitted" | "branch";

/**
 * The panel state persisted per machine in project-local scope (PSS-FR-15).
 * `targetBranch` is absent until one is configured; CHG-FR-06 then seeds it
 * from the repository's default branch.
 */
export interface ChangesPanelState {
  mode: ChangesMode;
  targetBranch?: string;
}

/**
 * The Library's artifact-type lens as it crosses the IPC boundary (PSS-FR-18).
 *
 * Deliberately distinct from the UI-internal `TypeLens` in `artifactTypes.ts`,
 * whose two sentinels are the shorter `"artifacts"` / `"files"`: this is the
 * persisted wire vocabulary and `artifactTypes.ts` owns the mapping between
 * them. The eight concrete values are the ASC-FR-02 type ids the tree already
 * tags nodes with.
 */
export type ArtifactTypeFilter = "all_artifacts" | "all_files" | ArtifactType;

/**
 * The Library panel state persisted per machine and per worktree in
 * project-local scope (PSS-FR-18 / LIB-FR-14).
 *
 * `expandedPaths` records **expansion**, not collapse: a folder renders expanded
 * iff its path is in the set, so an unknown folder — a newly-created one, or
 * every folder on a first-ever open — is collapsed (LIB-FR-15). A path naming a
 * folder that is not currently in the tree is retained rather than pruned, so a
 * folder a branch checkout removes and restores comes back expanded (LIB-FR-16).
 *
 * Field names must match `LibraryPanelState` in
 * `src-tauri/src/project_settings.rs` byte-for-byte: the Rust struct carries
 * `#[serde(default)]` and does not deny unknown fields, so a misspelled key here
 * is not an error — it is silently dropped on save and absent on load.
 */
export interface LibraryPanelState {
  expandedPaths: string[];
  artifactTypeFilter: ArtifactTypeFilter;
  textFilter: string;
}

/**
 * Which of the Notes panel's three scope positions is selected (NTS-FR-09),
 * persisted project-local (PSS-FR-19).
 */
export type NotesScopePosition = "entity" | "project" | "all";

/**
 * The Notes panel state persisted per machine and per worktree in project-local
 * scope (PSS-FR-19 / NTS-FR-09 / NTS-FR-13). The notes themselves are committed
 * (NTC-FR-01); this is the view of them, which is not.
 *
 * The store never validates that `scopePosition` is renderable — the entity
 * position has nothing to bind to while no entity-scoped tab is active, and
 * NTS-FR-09 decides what to render in that case without rewriting the record.
 */
export interface NotesPanelState {
  scopePosition: NotesScopePosition;
  textFilter: string;
}

/**
 * Which of the Drafts panel's four status positions is selected (DRP-FR-07),
 * persisted project-local (PSS-FR-20).
 */
export type DraftsStatusFilter = "active" | "archived" | "graduated" | "all";

/**
 * The Drafts panel state persisted per machine and per worktree in project-local
 * scope (PSS-FR-20 / DRP-FR-14).
 *
 * `expandedFolders` records **expansion**, not collapse: a folder renders
 * expanded iff its drafts-root-relative path is in the set, so a newly created
 * folder — and every folder on a first-ever open — is collapsed. The store never
 * validates that a path still names an existing folder; an absent one is
 * returned unchanged rather than pruned, so a folder that disappears and returns
 * comes back expanded.
 */
export interface DraftsPanelStateRecord {
  statusFilter: DraftsStatusFilter;
  textFilter: string;
  expandedFolders: string[];
}

/**
 * The entity the Notes panel's entity position binds to (NTS-FR-02): the
 * artifact or Flow the active tab owns, or the one a Library **Notes**
 * action named (LCM-FR-05).
 *
 * `id` is the entity's path-derived id (ASC-FR-13) — what the note record
 * stores — and `name` is what the selector and the group headers render.
 */
export interface NotesEntity {
  id: string;
  name: string;
}

/**
 * What a note is attached to (`NTC-notes-storage.md` contract surface).
 *
 * `entityId` is the entity's path-derived id (ASC-FR-13) and `entityPath` its
 * last-known project-relative path — what the panel renders for a note whose
 * entity no longer resolves (NTS-FR-23).
 */
export type NoteScope =
  | { kind: "entity"; entityId: string; entityPath: string }
  | { kind: "project" };

/** A persisted note (NTC-FR-03). */
export interface Note {
  id: string;
  scope: NoteScope;
  body: string;
  /** An optional scheduled reminder, RFC 3339 (NTC-FR-17). */
  reminder?: string;
  /** The revision the note was authored against; absent on a current-version note (NTC-FR-18). */
  revision?: string;
  createdAt: string;
  updatedAt: string;
}

/**
 * A note as the list commands return it: the record plus what the backend's
 * current view of the filesystem says about the entity it names (NTC-FR-10).
 */
export interface NoteListItem {
  note: Note;
  /** The entity's basename, present only while the entity resolves. */
  entityName?: string;
  /** True when the note names an entity that resolves to nothing on disk. */
  unresolved: boolean;
  /**
   * NTC-FR-19: the note's one discussion, absent when it carries none.
   *
   * What lets a note row know whether **Discuss** reopens a conversation or
   * begins one, without a call of its own (NTS-FR-28). Read from the comment
   * store's index rather than stored on the note, so nothing about the
   * association is written into a note file.
   */
  discussionThreadId?: string;
}

/**
 * The partial update of `"update note"` (NTC-FR-05). A field absent from the
 * payload is carried through unchanged; `reminder: null` clears the reminder,
 * which is why it is nullable rather than merely optional.
 */
export interface NoteFields {
  body?: string;
  reminder?: string | null;
  scope?: NoteScope;
}

/**
 * The three actions the Changes panel's footer split control offers
 * (CHG-FR-33). Persisted user-global (GSS-FR-25) as the author's habit, so an
 * action stays selected even while it is unavailable (CHG-FR-35).
 */
export type ChangesCommitAction = "commit" | "commit_and_push" | "push";

/**
 * GTC-FR-19: what a successful `commit_paths` reports.
 *
 * `committedPaths` names **every path the commit recorded**, which is not the
 * set that was submitted: a rename is recorded at both its locations, and a
 * named path the commit found nothing to record for is absent. The strip closes
 * Diff tabs from this (TAB-FR-22), so it acts on what landed rather than
 * re-deriving it from what was asked for.
 */
export interface CommitOutcome {
  commitId: string;
  committedPaths: string[];
}

/**
 * GTC-FR-30: what one side of the index holds for a path.
 *
 * `untracked` is an unstaged-side value alone: a path Git has never been told
 * about has nothing in the index to describe.
 */
export type PathStatus =
  | "added"
  | "modified"
  | "deleted"
  | "renamed"
  | "type_changed"
  | "untracked";

/**
 * GTC-FR-29 / GTC-FR-30: one changed path, with **what Git reports on each side
 * of the index kept apart**.
 *
 * Neither status stands in for the other and neither is collapsed into one
 * overall state: a path staged and then changed again carries both, which is
 * the combined state Git reports as two letters and which neither side alone
 * describes. At least one of the two is non-null.
 */
export interface WorkingTreeEntry {
  /** Project-relative, at the path's current location. */
  path: string;
  /** What the index holds against `HEAD`, or null where it holds nothing. */
  stagedStatus: PathStatus | null;
  /** What the working tree holds against the index, or null where they agree. */
  unstagedStatus: PathStatus | null;
  /** The pre-rename path; renamed entries only. */
  previousPath?: string;
}

/** GTC-FR-29: the active worktree's complete uncommitted state. */
export interface WorkingTreeStatus {
  /** Absolute path of the worktree the status describes. */
  worktreePath: string;
  entries: WorkingTreeEntry[];
}

/**
 * GSU-FR-MLEJ / GSU-FR-MLEJ: what the `source_worktree_dirty` refusal carries.
 *
 * The complete, unfiltered set of what stands in the worktree a graduation
 * would have captured, together with that worktree's identity — which the start
 * preflight holds for as long as it stands and carries back on every later
 * attempt (GSU-FR-MLEJ).
 */
export interface SourceWorktreeDirty {
  /** Absolute; the worktree the preflight read. */
  sourceWorktreePath: string;
  paths: WorkingTreeEntry[];
}

/**
 * GSU-FR-MLEJ / GSU-FR-MLEJ / GTC-FR-31: the checkout an operation was bound to is
 * no longer the active one, named beside the one that is.
 */
export interface WorktreeIdentityChanged {
  expected: string;
  active: string;
}

/**
 * GTC-FR-25: why one path of a rollback did not succeed.
 *
 * A typed kind rather than a message, because the panel renders it beside the
 * path (CHG-FR-65) and the decision it drives — whether that artifact's
 * in-memory buffer may be discarded — must not depend on prose.
 */
export interface RollbackPathFailure {
  path: string;
  kind: string;
}

/**
 * GTC-FR-25 / GTC-FR-26: what became of one selected entry.
 *
 * `restoredPaths` and `removedPaths` hold only what the backend **confirmed on
 * disk**. A path that failed is in `failures` and in neither list, and no path
 * is in both — which is the property CHG-FR-63 relies on: an in-memory buffer is
 * discarded only for a path named in one of the two success lists.
 *
 * A renamed entry carries two identities: `path` is its current location and
 * `previousPath` its pre-rename one, and each lands in the list its own result
 * earned — so half a rename failing leaves the other half's reset standing.
 */
export interface RollbackEntryOutcome {
  id: string;
  path: string;
  previousPath?: string | null;
  outcome: "restored" | "removed" | "failed";
  restoredPaths: string[];
  removedPaths: string[];
  failures: RollbackPathFailure[];
}

/** GTC-FR-23: what a `rollback_paths` call reports, one entry per named path. */
export interface RollbackOutcome {
  entries: RollbackEntryOutcome[];
}

/**
 * CHG-FR-61: a save that failed while a rollback was being prepared.
 *
 * Reported to the author alongside the backend's own per-path failures
 * (CHG-FR-65) rather than only logged, because a write that failed while they
 * were discarding is otherwise invisible to everyone but a developer reading
 * the Logs panel.
 */
export interface RollbackSaveFailure {
  artifactId: string;
  reason: string;
}

/**
 * CHG-FR-63 / CHG-FR-65: everything a completed rollback hands back to the
 * panel — what the backend confirmed per path, and what failed to save while
 * the rollback was being prepared.
 */
export interface RollbackResult {
  outcome: RollbackOutcome;
  saveFailures: RollbackSaveFailure[];
}

/**
 * The current branch's standing against its upstream, as
 * `get_upstream_sync_state` reports it (GTC-FR-21).
 *
 * `ahead` / `behind` are null rather than 0 when there is no upstream to count
 * against: "level with the remote" and "never published" are opposite answers
 * to whether Push is offered (CHG-FR-37). Resolved from local refs alone, so it
 * describes the remote as the last fetch left it and reaching for it never
 * touches the network.
 */
export interface UpstreamSyncState {
  hasRemote: boolean;
  hasUpstream: boolean;
  ahead: number | null;
  behind: number | null;
}

/**
 * One line of push output (GTC-FR-04). Carries no remote URL with userinfo, so
 * no token secret can ride out on it (GTC-FR-11).
 */
export interface GitOutputLinePayload {
  /** `"push"` — the transfer the line belongs to. */
  operation: string;
  line: string;
}

/** The single terminal event a transfer ends with (GTC-FR-04). */
export interface GitOperationFinishedPayload {
  operation: string;
  ok: boolean;
  /** The typed cause when `ok` is false. */
  error?: string;
}
