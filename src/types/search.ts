// Search (SCC-search.md / SCH-search.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

import { ArtifactType } from "./core";
import { ArtifactKind } from "./diff";
import { DraftStatus } from "./drafts";
import { GraduationRunState } from "./graduation";
import {
  GraduationStageCondition,
  GraduationVisualStage,
} from "./graduationObservability";

// ---------------------------------------------------------------------------
// Search (SCC-search.md / SCH-search.md)
// ---------------------------------------------------------------------------

/**
 * SCC-FR-05: what the query text means. Persisted user-global as the search
 * bar's active toggle (GSS-FR-21 / SCH-FR-13), and sent verbatim as the
 * dispatched search's `mode`.
 */
export type SearchMode = "literal_insensitive" | "smart_case" | "regex";

/**
 * SCC-FR-11 / SCH-FR-18: how far the sweep runs. The overlay dispatches
 * `capped` so typing stays cheap; the Search results tab dispatches `full`.
 */
export type SearchScope = "capped" | "full";

/** SCC-FR-07: whether the file matched on its content or only on its path. */
export type SearchMatchKind = "content" | "name";

/**
 * SCC-FR-08 / SCC-FR-09: which group a hit renders under. `run` and `history`
 * are part of the contract and are never populated in v1, so the UI renders
 * them from the same shape without either ever arriving.
 */
export type SearchGroup =
  | "artifact"
  | "playbook"
  | "workstream"
  | "role"
  | "run"
  | "history"
  | "file";

/** Where an artifact is edited — what SCH-FR-09 routes a click by. */
export type SearchEditContext = "standalone" | "flow";

/** One matching file (SCC contract surface). At most one hit per file. */
export interface SearchHit {
  /** The ASC node id (ASC-FR-13) — the identity TAB-FR-04 compares. */
  id: string;
  name: string;
  path: string;
  /** Enumeration position; the ordering key SCH-FR-16 renders by (SCC-FR-10). */
  ordinal: number;
  group: SearchGroup;
  matchKind: SearchMatchKind;
  /** Artifact group only. */
  subtype?: ArtifactType;
  /** Artifact group only. */
  editContext?: SearchEditContext;
  /** Content matches only: 1-based line number of the first match. */
  line?: number;
  /** Content matches only: the text of that line (SCC-FR-07). */
  snippet?: string;
}

/** Payload of the backend `"search results"` event (SCC-FR-19). */
export interface SearchResultsPayload {
  searchId: string;
  hits: SearchHit[];
}

/** Why a search ended (SCC contract surface). */
export type SearchEndReason =
  | "completed"
  | "capped"
  | "cancelled"
  | "superseded"
  | "failed";

/** Payload of the backend `"search ended"` event (SCC-FR-13). */
export interface SearchEndedPayload {
  searchId: string;
  reason: SearchEndReason;
}

/**
 * DSH-FR-09 / PST-FR-31: one row of the Recently edited widget.
 *
 * Served from the project's current artifact records and the current state of
 * its filesystem, and from nothing this application remembers about what it has
 * saved — so an edit made by an external editor moves an artifact exactly as an
 * edit made in the Editor does.
 */
export interface RecentlyEditedArtifact {
  id: string;
  name: string;
  /** Never `text`: a file carrying no artifact type is not an artifact. */
  kind: ArtifactKind;
  /** RFC 3339 UTC — the primary source file's modification time, read at load. */
  modifiedAt: string;
}

/**
 * DSH-FR-11 / PST-FR-32: one row of the Active workstreams widget — a draft.
 *
 * `name` and `status` are the draft's **stored** record (DRS-FR-03); only
 * `activityAt` comes from the filesystem, and it is the modification time of
 * the draft's one prompt file (DRS-FR-41).
 */
export interface ActiveDraftItem {
  draftId: string;
  name: string;
  /** Always `active`: the loader keeps no other status. */
  status: DraftStatus;
  activityAt: string;
}

/**
 * DSH-FR-13 / PST-FR-33: one row of the Last/current agent activity widget.
 *
 * A run the queue still holds in `queued` is listed like any other, so work
 * waiting to start is visible rather than absent.
 */
export interface AgentRunItem {
  runId: string;
  draftId: string;
  /** Absent for a run whose source draft no longer resolves. */
  draftName?: string;
  /** PST-FR-33: the work stream the run belongs to. */
  streamId: string;
  /** That stream's own name. */
  streamName: string;
  state: GraduationRunState;
  stage: GraduationVisualStage;
  stageCondition: GraduationStageCondition;
  /**
   * PST-FR-33: the run's index in **its own stream's** queue while it is
   * `queued`; absent otherwise.
   */
  queuePosition?: number;
  updatedAt: string;
}

/**
 * DSH-FR-14 / PST-FR-34: the four counts of the Pending Git activity widget.
 *
 * `null` means **unavailable**, not zero: the two commit counts on a branch
 * with no upstream, and all four for a project outside a Git repository. The
 * widget renders unavailable rather than `0`, because "level with the remote"
 * and "there is no remote to be level with" are different answers.
 */
export interface PendingGitActivity {
  modifiedArtifacts: number | null;
  modifiedSourceFiles: number | null;
  unpushedCommits: number | null;
  fetchableCommits: number | null;
}

/**
 * DRS-FR-42: payload of `"draft prompt changed"` — the content of one draft's
 * prompt file changed on disk, whoever wrote it.
 */
export interface DraftPromptChangedPayload {
  draftId: string;
  /** RFC 3339 UTC — the prompt file's new modification time. */
  promptActivityAt: string;
}

/** The widget a Dashboard refresh event names (PST-FR-37). */
export type DashboardWidgetName =
  | "recently_edited"
  | "active_drafts"
  | "agent_activity"
  | "pending_git";

/**
 * PST-FR-37: payload of `"dashboard refresh failed"` — a background refresh of
 * one widget could not be completed. It names one widget and touches no other.
 */
export interface DashboardRefreshFailedPayload {
  widget: DashboardWidgetName;
  error: string;
}
