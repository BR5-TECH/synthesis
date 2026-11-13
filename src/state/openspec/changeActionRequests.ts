/**
 * The change action requests of each OpenSpec change
 * (`../../../specifications/core/CAR-change-action-requests.md`,
 * `../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-QCPB,
 * OCW-FR-CTWP, OCW-FR-GDZT).
 *
 * A module-level store, on the terms of `./newChangeProposals.ts`: the request
 * cards in a discussion column read from here, so two cards of one request
 * never disagree. One reading per change, kept current from
 * `"change action requests changed"` and from each decision. An older answer
 * that lands after a newer read is dropped.
 */
import { useEffect, useSyncExternalStore } from "react";

import { listChangeActionRequests } from "../../api";
import { logDebug, logWarn } from "../../logging";
import type { ChangeActionRequest } from "../../types";
import { changeKey } from "./changeProposals";

type Reading = "unread" | "asking" | "read" | "failed";

const EMPTY: readonly ChangeActionRequest[] = Object.freeze([]);

let byChange: ReadonlyMap<string, readonly ChangeActionRequest[]> = new Map();
const status = new Map<string, Reading>();
const asks = new Map<string, number>();
let generation = 0;
let version = 0;
const listeners = new Set<() => void>();

function notify(): void {
  version += 1;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function put(key: string, list: readonly ChangeActionRequest[]): void {
  const map = new Map(byChange);
  map.set(key, list);
  byChange = map;
}

/** CAR-FR-ALHA: read the requests of one change again. The answer replaces the store. */
export function rereadChangeActionRequests(streamId: string, change: string): Promise<void> {
  const key = changeKey(streamId, change);
  status.set(key, "asking");
  const asked = generation;
  const mine = (asks.get(key) ?? 0) + 1;
  asks.set(key, mine);
  return listChangeActionRequests(streamId, change).then(
    (answer) => {
      if (asked !== generation || asks.get(key) !== mine) return;
      const list = Array.isArray(answer) ? answer : [];
      put(key, list);
      status.set(key, "read");
      logDebug(["frontend"], "the action requests of a change were read", {
        streamId,
        requests: list.length,
        pending: list.filter((r) => r.state === "pending").length,
      });
      notify();
    },
    (error: unknown) => {
      if (asked !== generation || asks.get(key) !== mine) return;
      status.set(key, "failed");
      logWarn(["frontend"], "the action requests of a change could not be read", {
        streamId,
        error: String(error).slice(0, 80),
      });
      notify();
    },
  );
}

/** Take the change's one reading, where none is taken or the last one failed. */
export function ensureChangeActionRequests(streamId: string, change: string): void {
  const now = status.get(changeKey(streamId, change)) ?? "unread";
  if (now === "asking" || now === "read") return;
  void rereadChangeActionRequests(streamId, change);
}

/** OCW-FR-QCPB: fold the request a decision answered into the reading. */
export function noteChangeActionRequest(request: ChangeActionRequest): void {
  const key = changeKey(request.streamId, request.change);
  const current = byChange.get(key) ?? EMPTY;
  const at = current.findIndex((r) => r.id === request.id);
  put(key, at === -1 ? [request, ...current] : current.map((r, i) => (i === at ? request : r)));
  notify();
  // A read asked before the decision can land after it with the old state.
  // A new read takes its place: the backend wrote the decision before it
  // answered, so that read holds it.
  if (status.get(key) === "asking") void rereadChangeActionRequests(request.streamId, request.change);
}

/** One request as the store holds it, or undefined. */
export function changeActionRequestOf(
  streamId: string,
  change: string,
  requestId: string,
): ChangeActionRequest | undefined {
  return (byChange.get(changeKey(streamId, change)) ?? EMPTY).find((r) => r.id === requestId);
}

/** Whether the change's reading has answered. */
export function changeActionRequestsRead(streamId: string, change: string): boolean {
  return status.get(changeKey(streamId, change)) === "read";
}

/** Re-render on any change of the store, and take the change's reading. */
export function useChangeActionRequests(streamId: string, change: string): number {
  useEffect(() => ensureChangeActionRequests(streamId, change), [streamId, change]);
  return useSyncExternalStore(subscribe, () => version);
}

/** Tests and the content-root reset. */
export function resetChangeActionRequests(): void {
  byChange = new Map();
  status.clear();
  asks.clear();
  generation += 1;
  notify();
}
