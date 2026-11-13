/**
 * The split of each OpenSpec change into new changes
 * (`../../../specifications/core/NCP-new-change-proposals.md`,
 * `../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-BGDN,
 * OCW-FR-ANKT, OCW-FR-TZRT, OCW-FR-IJAI).
 *
 * A module-level store, on the terms of `../newDraftProposals.ts`: the change
 * workspace, the cards in its discussion column, and the reference controls on
 * comments read from here, so they never disagree. One reading per change,
 * kept current from `"new change proposals changed"` and from each decision.
 */
import { useEffect, useSyncExternalStore } from "react";

import { listNewChangeProposals } from "../../api";
import { logDebug, logWarn } from "../../logging";
import type {
  ChangeSplit,
  NewChangeProposal,
  NewChangeProposalsChangedPayload,
} from "../../types";
import { changeKey } from "./changeProposals";

type Reading = "unread" | "asking" | "read" | "failed";

interface ChangeSplitReading {
  /** Most recently created first. */
  proposals: readonly NewChangeProposal[];
  split: ChangeSplit | null;
}

const EMPTY: ChangeSplitReading = Object.freeze({ proposals: Object.freeze([]), split: null });

let byChange: ReadonlyMap<string, ChangeSplitReading> = new Map();
const status = new Map<string, Reading>();
/** Proposals an event changed while a reading was in flight; the event wins. */
const touched = new Map<string, Set<string>>();
/** The newest read of each change; an older answer that lands later is dropped. */
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

/** Most recently created first, as the backend lists them (NCP contract). */
function sorted(list: readonly NewChangeProposal[]): NewChangeProposal[] {
  return [...list].sort((a, b) =>
    b.createdAt === a.createdAt ? b.id.localeCompare(a.id) : b.createdAt.localeCompare(a.createdAt),
  );
}

function put(key: string, next: ChangeSplitReading): void {
  const map = new Map(byChange);
  map.set(key, next);
  byChange = map;
}

/** NCP contract: read the split of one change again. The answer replaces the store. */
export function rereadNewChanges(streamId: string, change: string): Promise<void> {
  const key = changeKey(streamId, change);
  status.set(key, "asking");
  if (!touched.has(key)) touched.set(key, new Set());
  const asked = generation;
  const mine = (asks.get(key) ?? 0) + 1;
  asks.set(key, mine);
  return listNewChangeProposals(streamId, change).then(
    (answer) => {
      if (asked !== generation || asks.get(key) !== mine) return;
      const list = Array.isArray(answer?.proposals) ? answer.proposals : [];
      // An event newer than the reading keeps its proposal.
      const newer = touched.get(key) ?? new Set<string>();
      const held = (byChange.get(key)?.proposals ?? []).filter((p) => newer.has(p.id));
      touched.delete(key);
      put(key, {
        proposals: sorted([...list.filter((p) => !newer.has(p.id)), ...held]),
        split: answer?.split ?? null,
      });
      status.set(key, "read");
      logDebug(["frontend"], "the split of a change was read", {
        streamId,
        proposals: list.length,
        confirmed: Boolean(answer?.split?.confirmedAt),
      });
      notify();
    },
    (error: unknown) => {
      if (asked !== generation || asks.get(key) !== mine) return;
      touched.delete(key);
      status.set(key, "failed");
      logWarn(["frontend"], "the new change proposals of a change could not be read", {
        streamId,
        error: String(error).slice(0, 80),
      });
      notify();
    },
  );
}

/** Take the change's one reading, where none is taken or the last one failed. */
export function ensureNewChangesLoaded(streamId: string, change: string): void {
  const now = status.get(changeKey(streamId, change)) ?? "unread";
  if (now === "asking" || now === "read") return;
  void rereadNewChanges(streamId, change);
}

/** NCP-FR-VLOO: fold one event, or one decision, into the reading. */
export function noteNewChangeChanged(payload: NewChangeProposalsChangedPayload): void {
  const proposal = payload.proposal;
  if (!proposal) return;
  const key = changeKey(payload.streamId, payload.change);
  touched.get(key)?.add(proposal.id);
  const current = byChange.get(key) ?? EMPTY;
  put(key, {
    ...current,
    proposals: sorted([...current.proposals.filter((p) => p.id !== proposal.id), proposal]),
  });
  notify();
}

/** NCP-FR-MQCS: the split a confirmation answered with. */
export function noteChangeSplit(streamId: string, change: string, split: ChangeSplit): void {
  const key = changeKey(streamId, change);
  put(key, { ...(byChange.get(key) ?? EMPTY), split });
  notify();
}

/** The split of one change as the store holds it. */
export function newChangesOf(streamId: string, change: string): ChangeSplitReading {
  return byChange.get(changeKey(streamId, change)) ?? EMPTY;
}

/** One proposal as the store holds it, wherever its change is. */
export function newChangeProposalById(proposalId: string): NewChangeProposal | undefined {
  for (const reading of byChange.values()) {
    const found = reading.proposals.find((p) => p.id === proposalId);
    if (found) return found;
  }
  return undefined;
}

/** The split of one change, and whether its reading answered. */
export function useNewChangeProposals(
  streamId: string,
  change: string,
): ChangeSplitReading & { read: boolean } {
  useEffect(() => {
    ensureNewChangesLoaded(streamId, change);
  }, [streamId, change]);
  const reading = useSyncExternalStore(subscribe, () => newChangesOf(streamId, change));
  const read = useSyncExternalStore(
    subscribe,
    () => status.get(changeKey(streamId, change)) === "read",
  );
  return { ...reading, read };
}

/** Re-render on any change, for a component that reads by proposal id. */
export function useNewChangeStore(): number {
  return useSyncExternalStore(subscribe, () => version);
}

/** OCW-FR-BGDN: the Discuss route of each open change workspace, by change. */
const discussRoutes = new Map<string, (proposal: NewChangeProposal) => void>();
/** A Discuss asked for before the change's tab was open, taken as the tab mounts. */
const pendingDiscuss = new Map<string, NewChangeProposal>();

/** A change workspace registers its Discuss route while it is mounted. */
export function registerNewChangeDiscussRoute(
  streamId: string,
  change: string,
  route: (proposal: NewChangeProposal) => void,
): () => void {
  const key = changeKey(streamId, change);
  discussRoutes.set(key, route);
  const waiting = pendingDiscuss.get(key);
  if (waiting) {
    pendingDiscuss.delete(key);
    route(waiting);
  }
  return () => {
    if (discussRoutes.get(key) === route) discussRoutes.delete(key);
  };
}

/**
 * OCW-FR-BGDN: Discuss on one proposed change. Where the change's tab is not
 * open, the request waits for it, and the result is false so the caller opens
 * the tab.
 */
export function discussNewChange(proposal: NewChangeProposal): boolean {
  const key = changeKey(proposal.streamId, proposal.change);
  const route = discussRoutes.get(key);
  if (!route) {
    pendingDiscuss.set(key, proposal);
    return false;
  }
  route(proposal);
  return true;
}

/** Tests and the content-root reset. */
export function resetNewChangeProposals(): void {
  byChange = new Map();
  status.clear();
  touched.clear();
  asks.clear();
  pendingDiscuss.clear();
  generation += 1;
  notify();
}
