/**
 * The changes agents have proposed to **prompt and DOC files the project
 * holds** (`PCP-prompt-change-proposals.md`, `PCR-prompt-change-review.md`).
 *
 * PCR-FR-27: everything this surface says about a proposal it says from **one
 * reading per artifact** — the Editor tab's pending indication, the in-place
 * review, and the control on the agent's comment in the rail all
 * render from the same held reading, so several surfaces showing one file cost
 * one call and never disagree with one another.
 *
 * A module-level store with subscribers rather than state in a tab, for two
 * reasons the spec fixes:
 *
 * - **The raise is shell-level and the rendering is tab-level** (PCR-FR-17). A
 *   listener mounted in the Editor tab would hear nothing for a file whose tab
 *   is closed, which is exactly the case the author most needs telling about. So
 *   the shell subscribes and this store is what it writes into; the tab reads.
 * - **The control that shows the review is in the comment rail** (CMT-FR-48),
 *   several layers below the tab that renders it.
 *
 * What is *not* here is the proposal's text: its changes are read by
 * `./proposalHunks.ts` (PCP-FR-LDHK), so listing proposals reads no document.
 */
import { useCallback, useEffect, useMemo, useSyncExternalStore } from "react";
import { listPromptChangeProposals } from "../api";
import { logDebug, logInfo, logWarn } from "../logging";
import { promptHunkCandidateBuffers } from "./candidateBuffers";
import { artifactReviewKey, revealHunk } from "./draftDiscussion";
import { firstUndecidedHunk } from "./draftProposals";
import { hunksOf, rereadHunks } from "./proposalHunks";
import type {
  PromptChangeProposal,
  PromptProposalsChangedPayload,
} from "../types";

/** Every proposal of every artifact the session has loaded, keyed by artifact. */
let byArtifact: ReadonlyMap<string, readonly PromptChangeProposal[]> = new Map();

/**
 * PCR-FR-27: how far an artifact's one reading has got.
 *
 * The distinction this carries is the whole of CTA-FR-SPFS: a proposal missing
 * from a reading that **completed** is a proposal that is gone, and one missing
 * from a reading that was never taken, is still in flight, or failed is a
 * proposal nothing has looked for yet. Collapsing the two is what lets a surface
 * tell an author their standing proposal has vanished — and, the artifact's one
 * pending slot being held by it, leave the conversation with nothing able to
 * move it (PCP-FR-04).
 */
type ReadingStatus = "unread" | "asking" | "read" | "failed";

/** PCR-FR-27: one reading per artifact, and where each has got to. */
const status = new Map<string, ReadingStatus>();
/**
 * Consecutive failed asks per artifact, cleared by anything that proves the read
 * can succeed.
 *
 * CTA-FR-XVNC asks again whenever a previous ask failed, and this is what keeps
 * "again" from meaning "forever": a backend that refuses every read costs the
 * rail three calls per artifact rather than one per render.
 */
const failures = new Map<string, number>();
/** CTA-FR-LOOE: how many asks an artifact gets while every one of them fails. */
const MAX_FAILED_ASKS = 3;
/**
 * CTA-FR-LOOE: references whose **one further ask** has been made.
 *
 * Keyed by artifact and proposal together, because the further ask is per
 * reference: a conversation carrying a reference to a proposal the reading did
 * not return gets exactly one more reading out of it, and then settles on saying
 * the change is gone rather than asking after it for the rest of the session.
 */
const healed = new Set<string>();
/**
 * PCR-FR-28: which content root the readings in flight belong to.
 *
 * Bumped by the reset, and checked by a reading when it lands. A promise cannot
 * be cancelled, so this is what stops one issued against the outgoing tree from
 * answering for the incoming one.
 */
let readingGeneration = 0;

/** The key `healed` is kept under. ` ` cannot occur in either id. */
function referenceKey(artifactId: string, proposalId: string): string {
  return `${artifactId} ${proposalId}`;
}

const listeners = new Set<() => void>();

function emit(): void {
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * PCR-FR-28: reset every scrap of this store.
 *
 * Called when the project closes and when the active worktree changes: a reading
 * belongs to the content root it was taken from.
 */
export function resetPromptProposals(): void {
  byArtifact = new Map();
  // PCP-FR-EDTW: a hunk edit outlives every surface, but not the content root
  // it belongs to. One carried across would hold a baseline checksum naming a
  // file the application has left.
  promptHunkCandidateBuffers.clear();
  status.clear();
  failures.clear();
  healed.clear();
  readingGeneration += 1;
  emit();
}

/**
 * A stable empty array.
 *
 * `useSyncExternalStore` compares snapshots by identity and throws on a
 * getSnapshot that returns a fresh value every call, so an unloaded artifact
 * must answer with the *same* empty array each time rather than a new `[]`.
 */
const EMPTY: readonly PromptChangeProposal[] = Object.freeze([]);

/** The proposals of one artifact, empty until it has been loaded. */
export function proposalsOf(artifactId: string): readonly PromptChangeProposal[] {
  return byArtifact.get(artifactId) ?? EMPTY;
}

/** PCP-FR-04: the artifact's undecided proposal, if it has one (PCR-FR-16). */
export function pendingOf(artifactId: string): PromptChangeProposal | undefined {
  return proposalsOf(artifactId).find((p) => p.state === "pending");
}

/** PCR-FR-27: how far this artifact's reading has got. */
export function readingStatusOf(artifactId: string): ReadingStatus {
  return status.get(artifactId) ?? "unread";
}

/**
 * The reading as one value a subscriber can compare.
 *
 * It carries the failure count as well as the status because a second ask that
 * failed is a *change* a waiting reference has to hear about — without it, one
 * failure would look exactly like the next and the retry of CTA-FR-LOOE would stop
 * after a single attempt rather than at the bound.
 */
function readingSignal(artifactId: string | undefined): string {
  if (!artifactId) return "read#0";
  return `${readingStatusOf(artifactId)}#${failures.get(artifactId) ?? 0}`;
}

/**
 * PCR-FR-27 / CTA-FR-LOOE: take this artifact's reading unless one already
 * answers.
 *
 * Asks when nothing has been read for the artifact and again when a previous ask
 * failed — a reading that failed is not the session's answer, or an artifact
 * whose first read landed while the project was still opening would show no
 * indication, show no review in place (PCR-FR-QHZN), and hold a
 * standing proposal nothing in the window could decide. One ask is in flight per
 * artifact however many surfaces want it, and an artifact whose every ask fails
 * stops at `MAX_FAILED_ASKS` rather than being re-asked by every render that
 * follows.
 */
export function ensureLoaded(artifactId: string): void {
  const current = readingStatusOf(artifactId);
  if (current === "asking" || current === "read") return;
  if ((failures.get(artifactId) ?? 0) >= MAX_FAILED_ASKS) return;
  ask(artifactId);
}

/**
 * CTA-FR-XVNC: the reading one proposal reference asks for.
 *
 * Beyond what `ensureLoaded` does, this is the **one further ask** a reference
 * gets when a reading that completed did not return the proposal it names —
 * which is what heals a reading taken before that proposal existed and left
 * behind by an event that never arrived. It is made once per reference, so a
 * proposal that is genuinely gone settles rather than being asked after for the
 * rest of the session.
 */
export function requestProposalReading(
  artifactId: string,
  proposalId: string,
): void {
  const current = readingStatusOf(artifactId);
  if (current === "asking") return;
  if (current !== "read") {
    ensureLoaded(artifactId);
    return;
  }
  if (proposalsOf(artifactId).some((p) => p.id === proposalId)) return;
  const key = referenceKey(artifactId, proposalId);
  if (healed.has(key)) return;
  healed.add(key);
  logDebug(["frontend"], "re-reading an artifact's prompt proposals", {
    artifactId,
    proposalId,
  });
  ask(artifactId);
}

function ask(artifactId: string): void {
  status.set(artifactId, "asking");
  // PCR-FR-28: which content root this reading belongs to. A reading in flight
  // when the project closes or the worktree changes answers about a tree the
  // window has left, and folding it in would put the outgoing tree's proposals
  // into the incoming session.
  const generation = readingGeneration;
  void listPromptChangeProposals(artifactId)
    .then((proposals) => {
      if (generation !== readingGeneration) return;
      // A proposal recorded *while this read was in flight* is already in the
      // store and is newer than what the read returned. Anything this artifact
      // is already carrying therefore wins, and the read fills in the rest.
      const known = byArtifact.get(artifactId) ?? [];
      const merged = [
        ...known,
        ...proposals.filter((p) => !known.some((k) => k.id === p.id)),
      ];
      byArtifact = new Map(byArtifact).set(artifactId, merged);
      status.set(artifactId, "read");
      failures.delete(artifactId);
      emit();
    })
    .catch((error: unknown) => {
      if (generation !== readingGeneration) return;
      // PCR-FR-27: a failure is not the session's answer. The artifact is left
      // saying so rather than saying it holds nothing, so no surface reports a
      // proposal absent from a reading that never completed.
      status.set(artifactId, "failed");
      failures.set(artifactId, (failures.get(artifactId) ?? 0) + 1);
      logWarn(["frontend"], "could not load an artifact's prompt proposals", {
        artifactId,
        attempt: failures.get(artifactId) ?? 1,
        error: String(error),
      });
      emit();
    });
}

/**
 * PCP-FR-17: fold one `prompt-change-proposals-changed` payload into the store.
 *
 * The payload carries the whole proposal, so this replaces the record in place
 * rather than re-reading the artifact — which is what lets every indication
 * follow a decision made in another window without a round-trip (PCR-FR-26).
 *
 * PCR-FR-16: a proposal that arrives while the Editor tab is open enters review
 * in place with nothing to activate, because the tab renders from this store.
 */
export function notePromptProposalChanged(
  payload: PromptProposalsChangedPayload,
): void {
  const { artifactId, proposal } = payload;
  const current = byArtifact.get(artifactId) ?? [];
  const known = current.some((p) => p.id === proposal.id);
  const next = known
    ? current.map((p) => (p.id === proposal.id ? proposal : p))
    : [proposal, ...current];
  byArtifact = new Map(byArtifact).set(artifactId, next);
  // An event tells this store about **one** proposal, so it is not a reading of
  // the artifact and does not stand in for one. What it does settle is that the
  // backend is answering, so an artifact whose earlier reads failed is given its
  // asks back.
  failures.delete(artifactId);
  // PCP-FR-EDTW: a proposal decided — here or from another window — has nothing
  // left to edit, so any hunk edit still unwritten is dropped with the decision
  // controls rather than written into a proposal that would refuse it.
  if (proposal.state !== "pending") promptHunkCandidateBuffers.dropProposal(proposal.id);
  // PCP-FR-RVSN / DDS-FR-EHDK: a revision replaces a change in place, so the
  // changes a surface already holds are read again.
  if (hunksOf(proposal.id) !== undefined) rereadHunks(proposal.id, "prompt");
  emit();
}

/**
 * PCR-FR-18 / PCR-FR-RFJC: how a route that names a proposal reaches the review
 * — opening or focusing the artifact's Editor tab first.
 *
 * Registered by the shell rather than threaded as a prop, on exactly the terms
 * the notification facility is configured: the control sits in the comment
 * rail, several layers below the tab strip that knows how to open a tab.
 */
let openTab: ((artifactId: string) => void) | null = null;

/** Wire the route to the running shell. Called once, as the shell mounts. */
export function configurePromptReviewRoute(
  fn: ((artifactId: string) => void) | null,
): void {
  openTab = fn;
}

/**
 * PCR-FR-RFJC / PCR-FR-18 / CMT-FR-48: open or focus the artifact's Editor tab,
 * and scroll its document column to the first undecided change of the
 * proposal.
 */
export function showPromptProposal(artifactId: string, proposalId: string): void {
  openTab?.(artifactId);
  const proposal = proposalsOf(artifactId).find((p) => p.id === proposalId);
  const first = proposal ? firstUndecidedHunk(proposal) : undefined;
  logInfo(["frontend"], "a prompt proposal was asked to be shown", {
    artifactId,
    proposalId,
    found: first !== undefined,
  });
  if (first) revealHunk(artifactReviewKey(artifactId), first.id);
}

/**
 * PCR-FR-RFJC / CMT-FR-48: open or focus the artifact's Editor tab, and scroll
 * its document column to one change.
 */
export function showPromptHunk(
  artifactId: string,
  proposalId: string,
  hunkId: string,
): void {
  openTab?.(artifactId);
  logInfo(["frontend"], "a change of a prompt proposal was asked to be shown", {
    artifactId,
    proposalId,
    hunkId,
  });
  revealHunk(artifactReviewKey(artifactId), hunkId);
}

/**
 * One artifact's proposals, loading them on first use.
 *
 * The load is kicked off from the subscribe callback rather than from an effect
 * so a caller gets it by subscribing at all — every consumer of this store wants
 * the list, and an effect in each of them would be three places to forget it.
 */
export function usePromptProposals(
  artifactId: string | null,
): readonly PromptChangeProposal[] {
  const subscribeToArtifact = useCallback(
    (listener: () => void) => {
      const unsubscribe = subscribe(listener);
      if (artifactId === null) return unsubscribe;
      ensureLoaded(artifactId);
      return unsubscribe;
    },
    [artifactId],
  );
  const snapshot = useCallback(
    () => (artifactId === null ? EMPTY : proposalsOf(artifactId)),
    [artifactId],
  );
  return useSyncExternalStore(subscribeToArtifact, snapshot, snapshot);
}

/**
 * CTA-FR-UKIG: what this store can say about one proposal reference.
 *
 * Three answers rather than a proposal-or-nothing, because "I have not looked"
 * and "I looked and it is not there" are different things to tell an author, and
 * the second is the only one that may take the review away from them.
 */
export type PromptProposalReference =
  /** The reading holds it: render its state and open the review on it. */
  | { readonly kind: "known"; readonly proposal: PromptChangeProposal }
  /** No completed reading holds it, and one is in flight or still owed. */
  | { readonly kind: "unresolved" }
  /** A completed reading did not return it, and neither did the further one. */
  | { readonly kind: "gone" };

const UNRESOLVED: PromptProposalReference = Object.freeze({ kind: "unresolved" });
const GONE: PromptProposalReference = Object.freeze({ kind: "gone" });

/** CTA-FR-SACG: where one reference stands, read outside React. */
export function referenceTo(
  artifactId: string | undefined,
  proposalId: string,
): PromptProposalReference {
  // A reference naming no artifact is one nothing can look for: it is unresolved
  // rather than gone, and it asks for no reading.
  if (!artifactId) return UNRESOLVED;
  const proposal = proposalsOf(artifactId).find((p) => p.id === proposalId);
  if (proposal !== undefined) return { kind: "known", proposal };
  // Only a reading that completed can say a proposal is gone, and only after the
  // further ask of CTA-FR-LOOE has agreed with the first.
  const settled =
    readingStatusOf(artifactId) === "read" &&
    healed.has(referenceKey(artifactId, proposalId));
  return settled ? GONE : UNRESOLVED;
}

/**
 * One proposal reference, asking for the reading it needs (CMT-FR-67,
 * CTA-FR-UKIG, CTA-FR-XVNC).
 *
 * What the comment rail's control renders from: the attachment carries the
 * identity and the state lives here, so a control shows whatever the proposal
 * has since become without the log line having changed — and says nothing at all
 * about it until a reading has actually answered.
 */
export function usePromptProposalReference(
  artifactId: string | undefined,
  proposalId: string,
): PromptProposalReference {
  const proposals = usePromptProposals(artifactId ? artifactId : null);
  const snapshot = useCallback(() => readingSignal(artifactId), [artifactId]);
  const reading = useSyncExternalStore(subscribe, snapshot, snapshot);
  const proposal = proposals.find((p) => p.id === proposalId);

  useEffect(() => {
    if (!artifactId || proposal !== undefined) return;
    requestProposalReading(artifactId, proposalId);
  }, [artifactId, proposalId, proposal, reading]);

  // Stable across renders, so a caller may put the answer in a dependency list.
  // What is deliberately *not* memoised is the choice between the two answers
  // that carry nothing — whether a reference has settled turns on `healed`,
  // which is set outside a render.
  const known = useMemo(
    () => (proposal === undefined ? null : ({ kind: "known", proposal } as const)),
    [proposal],
  );
  return known ?? referenceTo(artifactId, proposalId);
}
