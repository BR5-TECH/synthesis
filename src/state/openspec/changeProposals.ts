/**
 * The edits agents have proposed to OpenSpec changes on work streams
 * (`../../../specifications/core/CEP-change-edit-proposals.md`,
 * `../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-COLR,
 * OCW-FR-TAZJ).
 *
 * One reading per change, shared by the change workspace and by every control
 * on a comment that names a proposal of that change, so they never disagree.
 * The hunk text is not here: `../proposalHunks.ts` reads it with the `change`
 * source.
 */
import { useCallback, useEffect, useMemo, useSyncExternalStore } from "react";

import { listChangeEditProposals } from "../../api";
import { logDebug, logInfo, logWarn } from "../../logging";
import { changeHunkCandidateBuffers } from "../candidateBuffers";
import { revealHunk } from "../draftDiscussion";
import { dropRevisedEdits, firstUndecidedHunk } from "../draftProposals";
import { hunksOf, rereadHunks } from "../proposalHunks";
import { openOpenSpecChange } from "./changeRoutes";
import type {
  ChangeEditProposal,
  ChangeProposalsChangedPayload,
  HunkLedgerRow,
} from "../../types";

type ReadingStatus = "unread" | "asking" | "read" | "failed";

/** CVP-FR-FWOB: the key of one change, as its discussion target key. */
export function changeKey(streamId: string, change: string): string {
  return `change:${streamId}:${change}`;
}

/** DCR-FR-GNRL: the key of the review state of one change. */
export function changeReviewKey(streamId: string, change: string): string {
  return changeKey(streamId, change);
}

/**
 * DCR-FR-GNRL: a change proposal as the shared review reads a proposal. The
 * ledger of a change proposal is its `hunks`, each row with a path.
 */
export type ReviewableChangeProposal = ChangeEditProposal & {
  ledger: HunkLedgerRow[];
};

export function asReviewable(proposal: ChangeEditProposal): ReviewableChangeProposal {
  return { ...proposal, ledger: proposal.hunks };
}

let byChange: ReadonlyMap<string, readonly ChangeEditProposal[]> = new Map();
const status = new Map<string, ReadingStatus>();
const failures = new Map<string, number>();
/**
 * How many readings of each change completed. A reading that answers what the
 * last one did is still news to a reference that waits for it.
 */
const completed = new Map<string, number>();
/** How many asks a change gets while every one of them fails. */
const MAX_FAILED_ASKS = 3;
/** References whose one further ask has been made (CTA-FR-LOOE). */
const healed = new Set<string>();
let generation = 0;
const listeners = new Set<() => void>();

const EMPTY: readonly ChangeEditProposal[] = Object.freeze([]);

function emit(): void {
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Discard every reading. The project closed, or a test starts. */
export function resetChangeProposals(): void {
  byChange = new Map();
  status.clear();
  failures.clear();
  completed.clear();
  healed.clear();
  changeHunkCandidateBuffers.clear();
  generation += 1;
  emit();
}

/** The proposals of one change, empty until it has been read. */
export function changeProposalsOf(streamId: string, change: string): readonly ChangeEditProposal[] {
  return byChange.get(changeKey(streamId, change)) ?? EMPTY;
}

/** CEP-FR-ZOIG: the one pending proposal of a change, if it holds one. */
export function pendingChangeProposalOf(
  streamId: string,
  change: string,
): ChangeEditProposal | undefined {
  return changeProposalsOf(streamId, change).find((p) => p.state === "pending");
}

function statusOf(key: string): ReadingStatus {
  return status.get(key) ?? "unread";
}

/**
 * OCW-FR-TAZJ / OCW-FR-DTRX: read the proposals of one change again. A list
 * replaces what the store held for the change, because a read that completed
 * is the backend's answer.
 */
export function rereadChangeProposals(streamId: string, change: string): Promise<void> {
  const key = changeKey(streamId, change);
  status.set(key, "asking");
  const issued = generation;
  return listChangeEditProposals(streamId, change)
    .then((proposals) => {
      if (issued !== generation) return;
      const list = Array.isArray(proposals) ? proposals : [];
      byChange = new Map(byChange).set(key, list);
      status.set(key, "read");
      failures.delete(key);
      completed.set(key, (completed.get(key) ?? 0) + 1);
      logDebug(["frontend"], "read the edit proposals of a change", {
        streamId,
        proposals: list.length,
        pending: list.filter((p) => p.state === "pending").length,
      });
      // A pending proposal whose hunks are already held is read again, so a
      // decision or an update shows in the text.
      for (const p of list) {
        if (p.state === "pending" && hunksOf(p.id) !== undefined) rereadHunks(p.id, "change");
      }
      emit();
    })
    .catch((error: unknown) => {
      if (issued !== generation) return;
      status.set(key, "failed");
      failures.set(key, (failures.get(key) ?? 0) + 1);
      logWarn(["frontend"], "could not read the edit proposals of a change", {
        streamId,
        attempt: failures.get(key) ?? 1,
        error: String(error),
      });
      emit();
    });
}

/** Read one change's proposals unless a reading already answers. */
export function ensureChangeProposals(streamId: string, change: string): void {
  const key = changeKey(streamId, change);
  const current = statusOf(key);
  if (current === "asking" || current === "read") return;
  if ((failures.get(key) ?? 0) >= MAX_FAILED_ASKS) return;
  void rereadChangeProposals(streamId, change);
}

/**
 * CEP-FR-SQCG: fold one event payload into the store. The payload holds the
 * whole proposal, so the record is replaced in place.
 */
export function noteChangeProposalChanged(payload: ChangeProposalsChangedPayload): void {
  const { streamId, change, proposal } = payload;
  const key = changeKey(streamId, change);
  const current = byChange.get(key) ?? [];
  const previous = current.find((p) => p.id === proposal.id);
  const next = previous
    ? current.map((p) => (p.id === proposal.id ? proposal : p))
    : [proposal, ...current];
  byChange = new Map(byChange).set(key, next);
  failures.delete(key);
  if (proposal.state !== "pending") changeHunkCandidateBuffers.dropProposal(proposal.id);
  else
    dropRevisedEdits(
      changeHunkCandidateBuffers,
      previous ? asReviewable(previous) : undefined,
      asReviewable(proposal),
    );
  if (hunksOf(proposal.id) !== undefined) rereadHunks(proposal.id, "change");
  emit();
}

/**
 * CMT-FR-48 / OCW-FR-COLR: open or focus the change workspace tab of the
 * change on its stream, at the first undecided hunk of the proposal.
 */
export function showChangeProposal(streamId: string, change: string, proposalId: string): void {
  const proposal = changeProposalsOf(streamId, change).find((p) => p.id === proposalId);
  const first = proposal ? firstUndecidedHunk(asReviewable(proposal)) : undefined;
  const path = first ? proposal?.hunks.find((h) => h.id === first.id)?.path : undefined;
  openOpenSpecChange({ kind: "stream", streamId }, change, { file: path ?? null });
  logInfo(["frontend"], "a change edit proposal was asked to be shown", {
    streamId,
    proposalId,
    found: first !== undefined,
  });
  if (first) revealHunk(changeReviewKey(streamId, change), first.id);
}

/** CMT-FR-48: open or focus the change workspace tab, at one hunk. */
export function showChangeHunk(
  streamId: string,
  change: string,
  proposalId: string,
  hunkId: string,
  path: string,
): void {
  openOpenSpecChange({ kind: "stream", streamId }, change, { file: path });
  logInfo(["frontend"], "a hunk of a change edit proposal was asked to be shown", {
    streamId,
    proposalId,
    hunkId,
  });
  revealHunk(changeReviewKey(streamId, change), hunkId);
}

/** The proposals of one change, read on first use. */
export function useChangeProposals(
  streamId: string | null,
  change: string | null,
): readonly ChangeEditProposal[] {
  const subscribeToChange = useCallback(
    (listener: () => void) => {
      const unsubscribe = subscribe(listener);
      if (streamId !== null && change !== null) ensureChangeProposals(streamId, change);
      return unsubscribe;
    },
    [streamId, change],
  );
  const snapshot = useCallback(
    () => (streamId === null || change === null ? EMPTY : changeProposalsOf(streamId, change)),
    [streamId, change],
  );
  return useSyncExternalStore(subscribeToChange, snapshot, snapshot);
}

/** CTA-FR-UKIG: what the store can say about one proposal reference. */
export type ChangeProposalReference =
  | { readonly kind: "known"; readonly proposal: ChangeEditProposal }
  | { readonly kind: "unresolved" }
  | { readonly kind: "gone" };

const UNRESOLVED: ChangeProposalReference = Object.freeze({ kind: "unresolved" });
const GONE: ChangeProposalReference = Object.freeze({ kind: "gone" });

function referenceKey(streamId: string, change: string, proposalId: string): string {
  return `${changeKey(streamId, change)} ${proposalId}`;
}

/**
 * CMT-FR-67 / CTA-FR-XVNC: one reference to a change proposal. A reading that
 * completed without the proposal gets one further ask, and only then is the
 * proposal gone.
 */
export function useChangeProposalReference(
  streamId: string | undefined,
  change: string | undefined,
  proposalId: string,
): ChangeProposalReference {
  const known = Boolean(streamId) && Boolean(change);
  const proposals = useChangeProposals(known ? streamId! : null, known ? change! : null);
  const key = known ? changeKey(streamId!, change!) : "";
  const signal = useCallback(
    () =>
      key === ""
        ? "read#0"
        : `${statusOf(key)}#${failures.get(key) ?? 0}#${completed.get(key) ?? 0}`,
    [key],
  );
  const reading = useSyncExternalStore(subscribe, signal, signal);
  const proposal = proposals.find((p) => p.id === proposalId);

  useEffect(() => {
    if (!known || proposal !== undefined) return;
    if (statusOf(key) !== "read") return;
    const ref = referenceKey(streamId!, change!, proposalId);
    if (healed.has(ref)) return;
    healed.add(ref);
    void rereadChangeProposals(streamId!, change!);
  }, [known, key, streamId, change, proposalId, proposal, reading]);

  const found = useMemo(
    () => (proposal === undefined ? null : ({ kind: "known", proposal } as const)),
    [proposal],
  );
  if (found) return found;
  if (!known) return UNRESOLVED;
  const settled =
    statusOf(key) === "read" && healed.has(referenceKey(streamId!, change!, proposalId));
  return settled ? GONE : UNRESOLVED;
}
