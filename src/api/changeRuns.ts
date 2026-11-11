// OpenSpec change runs (OCR-openspec-change-runs.md), the apply progress of a
// change (OSM-openspec-mode.md OSM-FR-PEAK), and change action requests
// (CAR-change-action-requests.md).

import { invoke } from "@tauri-apps/api/core";

import type {
  ApplyProgress,
  ChangeActionOutcome,
  ChangeActionRequest,
  ChangeReworkStart,
  GraduationRun,
} from "../types";

/**
 * OSM-FR-PEAK: the task counts, the next open task, and the missing artifacts
 * of one change on its stream. It rejects with the code of the read.
 */
export const readApplyProgress = (streamId: string, change: string) =>
  invoke<ApplyProgress>("read_apply_progress", { streamId, change });

/** OCR-FR-IDFC: start an apply run of one change on its stream. */
export const startChangeApply = (streamId: string, change: string) =>
  invoke<GraduationRun>("start_change_apply", { streamId, change });

/**
 * OCR-FR-IDFC / OCR-FR-UAWO: start a rework run of one change on the author's
 * instruction. It answers the run and its request thread.
 */
export const startChangeRework = (streamId: string, change: string, instruction: string) =>
  invoke<ChangeReworkStart>("start_change_rework", { streamId, change, instruction });

/** CAR-FR-ALHA: every request of one change, the most recently created first. */
export const listChangeActionRequests = (streamId: string, change: string) =>
  invoke<ChangeActionRequest[]>("list_change_action_requests", { streamId, change });

/** CAR-FR-MMPV: confirm a request, which starts its run. */
export const confirmChangeActionRequest = (requestId: string) =>
  invoke<ChangeActionOutcome>("confirm_change_action_request", { requestId });

/** CAR-FR-WXDA: decline a request. It starts nothing. */
export const declineChangeActionRequest = (requestId: string, feedback?: string | null) =>
  invoke<ChangeActionOutcome>("decline_change_action_request", {
    requestId,
    feedback: feedback ?? null,
  });
