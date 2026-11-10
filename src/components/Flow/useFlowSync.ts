import * as api from "../../api";
import { logError, logInfo, logWarn } from "../../logging";
import { connectSync } from "../../state/flowDocument";
import type {
  FlowSessionStore,
  FlowSyncState,
  ReviewConflict,
} from "../../state/flowSessions";
import { showToast } from "../../state/toasts";
import type { ConflictResolution, SyncResult } from "../../types";
import { errorText } from "./geometry";

/** FLO-FR-WLHA: the conflicts and skipped issues the Sync control counts. */
export function reviewCount(state: FlowSyncState): number {
  return state.conflicts.length + state.skipped.length;
}

/** FLO-FR-WLHA: the short confirmation of a sync that left nothing to decide. */
function confirmSync(flowId: string, result: SyncResult): void {
  const pushed = result.created + result.updated;
  const pulled = result.added + result.changed;
  showToast({
    key: `flow-sync:${flowId}`,
    level: "Info",
    title: "Flow synced",
    body: `${pushed} pushed to GitHub, ${pulled} pulled from GitHub.`,
    address: null,
  });
}

/**
 * FLO-FR-AKMV / FLO-FR-KHBY / FLO-FR-WLHA: sync a Flow with its tracker.
 *
 * A Flow with no sync section gets one first. Every pending change is written
 * before the sync, and a refused write starts no sync. While the sync runs the
 * store keeps the tab inert. When it returns, the tab reloads its plan from
 * disk, and the result opens the review or shows a confirmation.
 *
 * It runs against the store and not against a mounted canvas, so a sync that
 * ends while the author looks at another tab still lands.
 */
export async function startFlowSync(flows: FlowSessionStore, flowId: string): Promise<void> {
  const session = flows.get(flowId);
  if (!session?.doc || flows.syncState(flowId).running) return;
  if (!session.doc.sync) flows.applyEdit(flowId, connectSync);
  // FLO-FR-KHBY: inert from the first step, so no edit and no second sync
  // gets in while the pending changes are written.
  flows.setSyncState(flowId, { running: "sync", error: null });
  const written = await flows.flush(flowId);
  if (!written.ok) {
    logWarn(["frontend"], "flow sync not started: the write was refused", { flow: flowId });
    if (flows.get(flowId)) flows.setSyncState(flowId, { running: null });
    return;
  }
  logInfo(["frontend", "remote"], "flow sync started", { flow: flowId });
  let patch: Partial<FlowSyncState> = {};
  try {
    const result = await api.syncFlowWithTracker(flowId);
    const conflicts = result?.conflicts ?? [];
    const skipped = result?.skipped ?? [];
    patch = {
      conflicts,
      skipped,
      applyError: null,
      settled: true,
      reviewOpen: conflicts.length + skipped.length > 0,
    };
    logInfo(["frontend", "remote"], "flow sync finished", {
      flow: flowId,
      conflicts: conflicts.length,
      skipped: skipped.length,
    });
    if (conflicts.length + skipped.length === 0 && result) confirmSync(flowId, result);
  } catch (e) {
    patch = { error: errorText(e) };
    logError(["frontend", "remote"], "flow sync failed", {
      flow: flowId,
      code: (e as { code?: string })?.code ?? "",
    });
  }
  // A tab closed during the sync dropped its session; nothing comes back.
  if (!flows.get(flowId)) return;
  // FLO-FR-KHBY: the tab reloads its plan, and only then do the affordances
  // return.
  await flows.load(flowId);
  flows.setSyncState(flowId, { ...patch, running: null });
}

/**
 * FSR-FR-IDRR / FSR-FR-AUWG / FSR-FR-UHAM / FSR-FR-BXJJ: apply the chosen
 * sides. Pending changes are written first. The result replaces the list; a
 * conflict the result reports as stale stays, marked. A refused Apply keeps
 * every row. Resolves true when the list was replaced.
 */
export async function applyFlowResolutions(
  flows: FlowSessionStore,
  flowId: string,
  resolutions: ConflictResolution[],
): Promise<boolean> {
  if (resolutions.length === 0 || flows.syncState(flowId).running) return false;
  flows.setSyncState(flowId, { running: "resolve", applyError: null });
  const written = await flows.flush(flowId);
  if (!written.ok) {
    if (flows.get(flowId)) {
      flows.setSyncState(flowId, {
        running: null,
        applyError: flows.get(flowId)?.error ?? "The Flow could not be written.",
      });
    }
    return false;
  }
  const before = flows.syncState(flowId).conflicts;
  let patch: Partial<FlowSyncState> = {};
  let replaced = false;
  try {
    const result = await api.resolveFlowSyncConflicts(flowId, resolutions);
    const stale = new Set(result?.stale ?? []);
    const standing = result?.conflicts ?? [];
    const kept: ReviewConflict[] = before
      .filter((c) => stale.has(c.id) && !standing.some((n) => n.id === c.id))
      .map((c) => ({ ...c, stale: true }));
    patch = {
      conflicts: [...standing.map((c) => ({ ...c, stale: stale.has(c.id) })), ...kept],
      skipped: result?.skipped ?? [],
      applyError: null,
      settled: true,
    };
    replaced = true;
    logInfo(["frontend", "remote"], "flow sync conflicts resolved", {
      flow: flowId,
      applied: resolutions.length,
      stale: stale.size,
    });
  } catch (e) {
    patch = { applyError: errorText(e) };
    logError(["frontend", "remote"], "flow sync resolve failed", {
      flow: flowId,
      code: (e as { code?: string })?.code ?? "",
    });
  }
  if (!flows.get(flowId)) return false;
  await flows.load(flowId);
  flows.setSyncState(flowId, { ...patch, running: null });
  return replaced;
}
