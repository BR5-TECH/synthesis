// OpenSpec change runs and change action requests
// (`../../specifications/core/OCR-openspec-change-runs.md`,
// `../../specifications/core/CAR-change-action-requests.md`, and the
// `ApplyProgress` of `../../specifications/core/OSM-openspec-mode.md`
// OSM-FR-PEAK).

import type { Participant } from "./comments";
import type { GraduationRun } from "./graduation";

/** OSM contract: the state the apply instructions of a change are in. */
export type ApplyState = "blocked" | "ready" | "all_done";

/** OSM-FR-PEAK: what `"read apply progress"` answers. */
export interface ApplyProgress {
  state: ApplyState;
  total: number;
  complete: number;
  /** The description of the first task that is not done, or null where none is. */
  nextTask: string | null;
  missingArtifacts: string[];
}

/** OCR-FR-VHQW: the phase of the `openspec` data of a run. */
export type OpenSpecRunPhase = "propose" | "apply" | "rework";

/** OCR-FR-UAWO: what `"start change rework"` answers. */
export interface ChangeReworkStart {
  run: GraduationRun;
  /** The request thread of the run. */
  threadId: string;
}

/** OCR contract: the typed refusals of a start, by their codes. */
export const CHANGE_RUN_ERRORS = {
  noProjectOpen: "no_project_open",
  unknownStream: "unknown_stream",
  modeInactive: "openspec_mode_inactive",
  cliUnavailable: "cli_unavailable",
  invalidName: "invalid_change_name",
  changeNotFound: "change_not_found",
  runActive: "openspec_run_active",
  notReady: "change_not_ready",
  progressUnavailable: "apply_progress_unavailable",
  blankInstruction: "blank_instruction",
  proposalPending: "proposal_pending",
  discussionResolved: "discussion_resolved",
  imageUnconfigured: "vendor_image_unconfigured",
  imageInvalid: "vendor_image_invalid",
  executionUnsupported: "vendor_execution_unsupported",
  dockerUnverified: "docker_backend_unverified",
  socketUnavailable: "docker_socket_unavailable",
  writeUnconfirmed: "main_branch_write_unconfirmed",
} as const;

/** CAR contract: the action a request asks for. */
export type ChangeAction = "apply" | "rework";

/** CAR-FR-KYJU: the one state a request stands in. */
export type ChangeActionRequestState = "pending" | "started" | "declined" | "failed";

/** CAR-FR-TJAD: one change action request, as `"list change action requests"` serves it. */
export interface ChangeActionRequest {
  id: string;
  streamId: string;
  change: string;
  action: ChangeAction;
  /** A rework alone. */
  instruction?: string | null;
  rationale: string;
  threadId: string;
  commentId: string;
  agent: Participant;
  state: ChangeActionRequestState;
  /** A started request alone. */
  runId?: string | null;
  /** The code of the last refused confirmation, or of the failure. */
  lastRefusal?: string | null;
  createdAt: string;
  decidedAt?: string | null;
}

/** CAR-FR-UPOV / CAR-FR-WXDA: what a decision answers. */
export interface ChangeActionOutcome {
  request: ChangeActionRequest;
  /** A started request alone. */
  run?: GraduationRun | null;
  /** The decision comment. */
  commentId?: string | null;
  originKind: "change_discussion" | "change_comment";
}

/** CAR-FR-ZSIQ: what `"change action requests changed"` carries. */
export interface ChangeActionRequestsChangedPayload {
  streamId: string;
  change: string;
  requestId: string;
  cause: "recorded" | "decided" | "refused";
}

/** CAR contract: the refusals of a decision that are this module's own. */
export const CHANGE_ACTION_ERRORS = {
  requestNotFound: "request_not_found",
  alreadyDecided: "already_decided",
  actionRequestPending: "action_request_pending",
  writeFailed: "write_failed",
} as const;

/** CMS-FR-QVZP: the `change_action_request` attachment of an agent's comment. */
export interface ChangeActionRequestReference {
  kind: "changeActionRequest";
  requestId: string;
  streamId: string;
  change: string;
  action: ChangeAction;
}
