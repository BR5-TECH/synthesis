/**
 * The new-draft proposals of each draft's decomposition
 * (`DCR-draft-change-review.md` DCR-FR-RDLK, DCR-FR-KNDA).
 *
 * A module-level store, for the reason `draftProposals.ts` is one: the shell
 * hears `"new draft proposals changed"` for every draft (DCR-FR-QQLE), and the
 * cards in a discussion column read from here. One reading per draft, kept
 * current from the event.
 */
import { useEffect, useSyncExternalStore } from "react";

import { listNewDraftProposals } from "../api";
import { logWarn } from "../logging";
import type { NewDraftProposal, NewDraftProposalsChangedPayload } from "../types";

type Reading = "unread" | "asking" | "read" | "failed";

let byDraft: ReadonlyMap<string, readonly NewDraftProposal[]> = new Map();
const status = new Map<string, Reading>();
/** Drafts whose decomposition the author confirmed in this session. */
const confirmed = new Set<string>();
/** Proposals an event changed while a reading was in flight; the event wins. */
const touched = new Map<string, Set<string>>();
let generation = 0;
const listeners = new Set<() => void>();

let version = 0;

function notify(): void {
  version += 1;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Newest first, as the backend lists them (DCP-FR-ZDSP). */
function sorted(list: readonly NewDraftProposal[]): NewDraftProposal[] {
  return [...list].sort((a, b) =>
    b.createdAt === a.createdAt ? b.id.localeCompare(a.id) : b.createdAt.localeCompare(a.createdAt),
  );
}

/** DCR-FR-RDLK: take the draft's one reading, where none is taken or one failed. */
export function ensureNewDraftsLoaded(draftId: string): void {
  const now = status.get(draftId) ?? "unread";
  if (now === "asking" || now === "read") return;
  status.set(draftId, "asking");
  touched.set(draftId, new Set());
  const asked = generation;
  void listNewDraftProposals(draftId).then(
    (list) => {
      if (asked !== generation) return;
      // An event newer than the reading keeps its proposal.
      const newer = touched.get(draftId) ?? new Set<string>();
      const held = (byDraft.get(draftId) ?? []).filter((p) => newer.has(p.id));
      const merged = [...list.filter((p) => !newer.has(p.id)), ...held];
      touched.delete(draftId);
      const next = new Map(byDraft);
      next.set(draftId, sorted(merged));
      byDraft = next;
      status.set(draftId, "read");
      notify();
    },
    (error: unknown) => {
      if (asked !== generation) return;
      status.set(draftId, "failed");
      logWarn(["frontend"], "new draft proposals could not be read", {
        draftId,
        error: String(error),
      });
      notify();
    },
  );
}

/** DCR-FR-RDLK: fold one event into the reading. */
export function noteNewDraftChanged(payload: NewDraftProposalsChangedPayload): void {
  if (payload.cause === "confirmed") {
    confirmed.add(payload.draftId);
    notify();
    return;
  }
  const proposal = payload.proposal;
  if (!proposal) return;
  touched.get(payload.draftId)?.add(proposal.id);
  const current = byDraft.get(payload.draftId) ?? [];
  const next = new Map(byDraft);
  next.set(payload.draftId, sorted([...current.filter((p) => p.id !== proposal.id), proposal]));
  byDraft = next;
  notify();
}

/** One proposal as the store holds it, wherever its draft is. */
export function newDraftProposalById(proposalId: string): NewDraftProposal | undefined {
  for (const list of byDraft.values()) {
    const found = list.find((p) => p.id === proposalId);
    if (found) return found;
  }
  return undefined;
}

/** Whether the author confirmed this draft's decomposition in this session. */
export function decompositionConfirmed(draftId: string): boolean {
  return confirmed.has(draftId);
}

/** Tests and the content-root reset (DCR-FR-33 terms). */
export function resetNewDraftProposals(): void {
  byDraft = new Map();
  status.clear();
  confirmed.clear();
  touched.clear();
  generation += 1;
  notify();
}

/** The draft's proposals, newest first, and whether its reading answered. */
export function useNewDraftProposals(draftId: string | null): {
  proposals: readonly NewDraftProposal[];
  read: boolean;
} {
  useEffect(() => {
    if (draftId) ensureNewDraftsLoaded(draftId);
  }, [draftId]);
  const proposals = useSyncExternalStore(subscribe, () =>
    draftId ? (byDraft.get(draftId) ?? EMPTY) : EMPTY,
  );
  const read = useSyncExternalStore(subscribe, () =>
    draftId ? status.get(draftId) === "read" : false,
  );
  return { proposals, read };
}

/** Re-render on any change, for a component that reads by proposal id. */
export function useNewDraftStore(): number {
  return useSyncExternalStore(subscribe, () => version);
}

const EMPTY: readonly NewDraftProposal[] = [];
