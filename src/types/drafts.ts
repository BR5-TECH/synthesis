// Drafts (NAW-new-artifact.md / DRP-drafts-panel.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

import { Participant } from "./comments";
import { DraftGraduation } from "./graduation";

// ---------------------------------------------------------------------------
// Drafts (NAW-new-artifact.md / DRP-drafts-panel.md, stored by `drafts.rs`)
// ---------------------------------------------------------------------------

/**
 * NAW-FR-16: a draft carries one of four positions.
 *
 * `active` and `archived` are the two a control sets, and moving between them
 * is reversible at any time and costs nothing.
 *
 * `archived` retires a draft from the Drafts panel's default view (DRP-FR-07)
 * and takes nothing off disk: the draft keeps its files, its threads, and its
 * conversation, opens and edits exactly as an active one does, and graduates
 * like any other. There is no readiness position — Graduate carries no
 * precondition on the status, because activating it is itself the author's
 * judgement that the specification is finished (NAW-FR-17).
 *
 * `published` is set by a successful GitHub publication over an `active` draft
 * and by nothing else (DRS-FR-VKQO). It is **not** read-only: a published draft
 * edits, renames, graduates, and archives exactly as an active one does
 * (DRS-FR-OGZC, NAW-FR-GKBP).
 */
export type DraftStatus =
  | "active"
  | "archived"
  | "graduated"
  | "published"
  /**
   * NAW-FR-16 / DRS-FR-WFLY: a draft a claim of a GitHub Task created. No
   * control sets it. The draft is read-only for its whole life and offers
   * Graduate alone (NAW-FR-UEWC, NAW-FR-AXFQ).
   */
  | "github_shadow";

/**
 * DRS-FR-XDWS: the GitHub issue a GitHub-shadow draft mirrors. A draft that
 * carries it is a shadow draft whatever its status, and the backend refuses
 * every edit of it with `draft_github_shadow`.
 */
export interface GithubIssueLink {
  repositoryOwner: string;
  repositoryName: string;
  issueNumber: number;
  issueUrl: string;
  projectNodeId: string;
  claimState: "claimed";
}

/** DRS-FR-XDWS: whether a draft mirrors a GitHub issue. */
export function isGithubShadow(draft: {
  status: DraftStatus;
  githubIssue?: GithubIssueLink | null;
}): boolean {
  return !!draft.githubIssue || draft.status === "github_shadow";
}

/**
 * The persisted draft record. Field names must match `DraftRecord` in
 * `src-tauri/src/drafts.rs` byte-for-byte.
 */
export interface DraftRecord {
  id: string;
  name: string;
  /**
   * NAW-FR-25 / DRS-FR-25: the draft-relative path of the draft's **one
   * prompt**, whose name and the draft's name are one thing.
   *
   * Optional on the wire only because a draft whose storage is not the single
   * prompt DRS-FR-11 requires is still *listed* (DRS-FR-15) — it simply does
   * not open. Every operation that reads or writes the prompt refuses such a
   * draft with `draft_not_single_file` rather than acting on a guess.
   */
  promptPath?: string | null;
  status: DraftStatus;
  createdAt: string;
  updatedAt: string;
  /** DRS-FR-XDWS: present on a GitHub-shadow draft alone. */
  githubIssue?: GithubIssueLink | null;
}

/**
 * DRS-FR-06: what `create_draft` hands back — the record, and the draft-relative
 * path of the one file it was created holding, which the tab opens on.
 */
export interface DraftCreated {
  draft: DraftRecord;
  file: string;
}

// -- Draft assets (draft_assets.rs / DAS-draft-assets.md) -------------------

/**
 * DAS-FR-02 / DAS-FR-05: one stored image, as `storeDraftImage` answers with it.
 *
 * The `reference` is what the surface inserts into the prompt, so no caller
 * derives a path of its own; the `path` is what `readDraftImage` and
 * `discardDraftImage` name the same file by. An image is **not a file of the
 * draft** (NAW-FR-56): it sits in the draft's own `assets/` folder beside the
 * one prompt, and no surface renders a file tree, an asset list, or a per-asset
 * selection for it.
 */
export interface DraftAsset {
  /** The draft-relative path of the stored file: `assets/<opaque-id>.<ext>`. */
  path: string;
  /** The Markdown destination that resolves to it: `../assets/<opaque-id>.<ext>`. */
  reference: string;
  mediaType: string;
  /** The name the image was supplied under, absent where none was. */
  filename?: string | null;
  bytes: number;
}

/** DAS-FR-08: what `readDraftImage` serves back, base64-encoded. */
export interface DraftImageContent {
  mediaType: string;
  filename?: string | null;
  data: string;
}

/** DAS-FR-09: what `discardDraftImage` answers with. */
export interface DraftAssetDiscarded {
  /** True where the asset is not on disk when this returns. */
  discarded: boolean;
  /** True where a valid reference in the saved prompt still names it. */
  retained: boolean;
}

/**
 * DAS-FR-15: what `sweepDraftAssets` answers with — an **acknowledgement and
 * not a result**.
 *
 * It says that a pass will run and whether this request joined one already
 * pending, and it carries no tally, because no tally exists yet. A caller that
 * wants the outcome reads the log; a surface wants neither and asks for neither
 * (NAW-FR-55).
 */
export interface DraftAssetSweepScheduled {
  scheduled: boolean;
  coalesced: boolean;
}

// -- Draft history (draft_history.rs / DHS-draft-history.md) ----------------

/**
 * DHS-FR-07 / DHS-FR-08: why a version exists — the draft's creation, or a
 * change the author accepted from an agent. Nothing else puts a row in the rail
 * (NAW-FR-36).
 */
export type DraftHistorySource =
  | { kind: "original" }
  | { kind: "proposal_accepted"; proposalId: string; agent: Participant };

/** One settled version of a draft's prompt (DHS-FR-05, DHS-FR-09). */
export interface DraftHistoryEntry {
  /** Opaque, stable, and derived from neither the draft, the path, nor the content. */
  id: string;
  draftId: string;
  /** `1` for `Original`, increasing by one per entry — the rail's sort key. */
  seq: number;
  /** The draft-relative path the prompt occupied when the snapshot was taken. */
  path: string;
  createdAt: string;
  byteLen: number;
  sha256: string;
  source: DraftHistorySource;
}

/**
 * DHS-FR-10: the live prompt's own digest, returned beside the entries so the
 * rail can say whether the author has typed since the last settled version
 * (NAW-FR-37) without reading a snapshot.
 */
export interface LivePrompt {
  path: string;
  byteLen: number;
  sha256: string;
  /** The live prompt's bytes are the newest entry's bytes. */
  matchesLatest: boolean;
}

/** What `list draft history` answers with (DHS-FR-10). */
export interface DraftHistoryList {
  /** Ascending `seq`, oldest first. */
  entries: DraftHistoryEntry[];
  live: LivePrompt;
}

/**
 * What `load draft history entry` answers with (DHS-FR-11) — the snapshot's
 * text together with the digest it was verified against.
 */
export interface DraftHistoryContent {
  content: string;
  sha256: string;
}

/**
 * DHS-FR-22: a version became visible, or a reconciliation withdrew one — in
 * which case `entry` is null.
 */
export interface DraftHistoryChanged {
  draftId: string;
  entry: DraftHistoryEntry | null;
}

/**
 * DRS-FR-17 / DRP-FR-17: why a draft survived the panel's text filter. A row
 * matched on its contents is annotated; one matched on its name is not, because
 * the reason is already on the row.
 */
export interface DraftMatch {
  draftId: string;
  matchedIn: "name" | "contents";
}

/** One row of the Drafts panel (DRP-FR-03). */
export interface DraftSummary {
  id: string;
  name: string;
  status: DraftStatus;
  /**
   * DRS-FR-29: the drafts-root-relative path of the folder this draft is filed
   * in, `""` being the implicit root. Not a fact held beside the draft — it is
   * where the draft's directory sits, so there is no second copy of it to
   * disagree with the disk.
   *
   * Optional on the wire so a summary from a build without the field still
   * folds; absent reads as the root.
   */
  folder?: string;
  /**
   * DRS-FR-15: this draft's storage is not the single prompt DRS-FR-11
   * requires, so it cannot be opened, edited, or graduated. The row still
   * renders — dropping it would read as data loss for material still on disk —
   * and offers Delete alone (DRP-FR-33).
   *
   * Optional on the wire so a summary from a build without the field still
   * folds; absent reads as consistent, which is the ordinary case.
   */
  inconsistent?: boolean;
  updatedAt: string;
  /**
   * DRP-FR-19: an agent has proposed a change to this draft that the author has
   * not yet decided (DCP-FR-04). Carried on the summary rather than fetched per
   * row, so the panel's marker costs it no call of its own — its cost claim is
   * one `list_drafts` however many drafts the worktree holds.
   *
   * Optional on the wire so a summary from a build without the field still
   * folds; absent reads as "none", which is the safe direction.
   */
  hasPendingProposal?: boolean;
  /**
   * DRS-FR-18: the graduation run the project's queue holds for this draft, or
   * absent where it holds none. Read from the queue and never cached, so the
   * row shows what the queue holds rather than what the draft remembers.
   */
  graduation?: DraftGraduation | null;
  /** DRS-FR-XDWS: present on a GitHub-shadow draft alone (DRP-FR-ZRJJ). */
  githubIssue?: GithubIssueLink | null;
}

/**
 * One of the author's organising directories under the drafts root
 * (DRS-FR-29). Deliberately *not* a `DraftNode`: that is a path inside one
 * draft's own file tree, and the two never name the same thing.
 *
 * It carries no id, because a drafts folder's identity is its path — the
 * directory structure is the whole of the organisation, and there is nothing
 * beside it to hold an id in.
 */
export interface DraftFolder {
  /** Drafts-root-relative, e.g. `UI/Components`. */
  path: string;
  /** Drafts-root-relative path of the containing folder; `""` for top-level. */
  parent: string;
}

/**
 * DRS-FR-08 / DRP-FR-20: the active worktree's whole drafts organisation, from
 * one call — every folder at every depth, empty ones included, and every draft
 * with the folder it is filed in.
 */
export interface DraftHierarchy {
  folders: DraftFolder[];
  drafts: DraftSummary[];
}

/** Body plus checksum, on the same contract `loadArtifactContentsById` serves. */
export interface DraftContents {
  body: string;
  checksum: string;
}

// -- Graduation (graduation.rs / GRD-graduation.md) -------------------------

/** GSU-FR-NCJN: decided once, at enqueue, and never changed under a run. */
