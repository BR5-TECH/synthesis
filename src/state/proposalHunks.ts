/**
 * The changes one proposal holds, and where each lands in the prompt as it
 * stands (`DCR-draft-change-review.md` DCR-FR-05, DCR-FR-08).
 *
 * A module-level store rather than component state, for the reason DCR-FR-08
 * gives: the hunks are read when the tab opens on a draft that holds a proposal
 * and re-read when the backend reports one changed — never on a render. A store
 * is what lets both of those be one read that every reader shares.
 *
 * What is held here is the proposal's *text*, which is why it is separate from
 * `draftProposals.ts`: that store lists records and never reads a document, so a
 * draft carrying a long history of proposals costs a tab nothing on open.
 */
import { useSyncExternalStore } from "react";

import { loadDraftChangeProposalHunks } from "../api";
import { logDebug, logWarn } from "../logging";
import type { ProposalHunks } from "../types";

/** DCR-FR-05: one proposal's changes, or how far the reading of them has got. */
export type HunkReading =
  | { status: "reading" }
  | { status: "read"; hunks: ProposalHunks }
  | { status: "failed"; reason: string };

let readings: ReadonlyMap<string, HunkReading> = new Map();
const listeners = new Set<() => void>();
/**
 * DCR-FR-33: which content root the readings belong to.
 *
 * A promise cannot be cancelled, so this is what stops one issued against the
 * outgoing tree from answering for the incoming one.
 */
let generation = 0;

function emit(): void {
  listeners.forEach((l) => l());
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function put(proposalId: string, reading: HunkReading): void {
  const next = new Map(readings);
  next.set(proposalId, reading);
  readings = next;
  emit();
}

/** How far the reading of one proposal's changes has got. */
export function hunksOf(proposalId: string): HunkReading | undefined {
  return readings.get(proposalId);
}

/**
 * DCR-FR-08: read this proposal's changes, once.
 *
 * A reading already taken is not repeated — the way a proposal's changes move
 * on is the backend reporting it, which calls `rereadHunks`.
 */
export function ensureHunks(proposalId: string): void {
  if (readings.has(proposalId)) return;
  void read(proposalId);
}

/** DCR-FR-08 / DCR-FR-21: the proposal changed, so what it holds is read again. */
export function rereadHunks(proposalId: string): void {
  void read(proposalId);
}

async function read(proposalId: string): Promise<void> {
  const issued = generation;
  put(proposalId, { status: "reading" });
  try {
    const hunks = await loadDraftChangeProposalHunks(proposalId);
    if (issued !== generation) return;
    put(proposalId, { status: "read", hunks });
    logDebug(["frontend"], "read the changes a proposal holds", {
      proposalId,
      changes: hunks.hunks.length,
      // DCP-FR-BMLX: how many name text the prompt no longer holds, which is
      // what decides whether Accept stands for them.
      lost: hunks.resolutions.filter((r) => r.kind === "lost").length,
    });
  } catch (e) {
    if (issued !== generation) return;
    const reason = String(e);
    put(proposalId, { status: "failed", reason });
    logWarn(["frontend"], "a proposal's changes could not be read", {
      proposalId,
      reason,
    });
  }
}

/** The reading is no longer wanted — the proposal was decided or is gone. */
export function dropHunks(proposalId: string): void {
  if (!readings.has(proposalId)) return;
  const next = new Map(readings);
  next.delete(proposalId);
  readings = next;
  emit();
}

/** DCR-FR-33: discard every reading. The project closed or the worktree changed. */
export function resetProposalHunks(): void {
  generation += 1;
  readings = new Map();
  emit();
}

/**
 * One proposal's changes, reading them on first use.
 *
 * The read is kicked off from the subscribe callback rather than from an effect,
 * so a caller gets it by subscribing at all — every consumer of this store wants
 * the changes, and an effect in each of them would be a place to forget it.
 */
export function useProposalHunks(
  proposalId: string | null,
): HunkReading | undefined {
  return useSyncExternalStore(
    (listener) => {
      const unsubscribe = subscribe(listener);
      if (proposalId !== null) ensureHunks(proposalId);
      return unsubscribe;
    },
    () => (proposalId === null ? undefined : readings.get(proposalId)),
    () => (proposalId === null ? undefined : readings.get(proposalId)),
  );
}
