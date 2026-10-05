/**
 * The changes agents have proposed to drafts, and which one the author is
 * reviewing (`DCP-draft-change-proposals.md`, `DCR-draft-change-review.md`).
 *
 * A module-level store with subscribers rather than state in a tab, for two
 * reasons the spec fixes:
 *
 * - **The raise is shell-level and the rendering is tab-level** (DCR-FR-18). A
 *   listener mounted in the New Artifact tab would hear nothing for a draft
 *   whose tab is closed, which is exactly the case the author most needs telling
 *   about. So the shell subscribes and this store is what it writes into; the
 *   tab reads.
 * - **The control that opens the modal is in the comment rail** (CMT-FR-48),
 *   several layers below the tab that renders the modal. Threading a callback
 *   from the tab through the rail, the card, and the attachment list would make
 *   four components know about a fifth's modal.
 *
 * What is *not* here is the proposal's text: that is read when a diff is
 * actually rendered (DCR-FR-08), so a draft carrying a pending proposal costs
 * its tab a marker and no document.
 */
import { useCallback, useEffect, useMemo, useSyncExternalStore } from "react";
import { listDraftChangeProposals } from "../api";
import { logDebug, logWarn } from "../logging";
import { hunkCandidateBuffers } from "./candidateBuffers";
import { hunksOf, rereadHunks } from "./proposalHunks";
import { PROPOSAL_ERRORS } from "../types";
import { isUndecided, undecidedCount } from "../types";
import type {
  DraftChangeProposal,
  HunkLedgerRow,
  HunkState,
  ProposalsChangedPayload,
} from "../types";

/** Every proposal of every draft the session has loaded, keyed by draft. */
let byDraft: ReadonlyMap<string, readonly DraftChangeProposal[]> = new Map();
/** DCR-FR-02: the proposal the review modal is open on, if any. */
let reviewing: string | null = null;

/**
 * DCR-FR-32: how far a draft's one reading has got.
 *
 * The distinction this carries is the whole of CTA-FR-SPFS: a proposal missing
 * from a reading that **completed** is a proposal that is gone, and one missing
 * from a reading that was never taken, is still in flight, or failed is a
 * proposal nothing has looked for yet. Collapsing the two is what lets a
 * surface tell an author their standing proposal has vanished — and, the draft's
 * one pending slot being held by it, leave the conversation with nothing able to
 * move it (DCP-FR-04).
 */
type ReadingStatus =
  | "unread"
  | "asking"
  | "read"
  /**
   * The draft itself is not there (DCP-FR-22), which is a **completed answer**
   * and not a failed read: a graduated or deleted draft took its proposals with
   * it (DCP-FR-02), so every reference into it is answered at once and there is
   * nothing a further ask could heal. This is the state CTA-FR-SACG's "no longer
   * available" is actually said from, its two routes — the draft gone, and a
   * committed log read from outside the draft it belonged to — both being reads
   * the backend refuses rather than empty lists it returns.
   */
  | "absent"
  | "failed";

/** DCR-FR-32: one reading per draft, and where each has got to. */
const status = new Map<string, ReadingStatus>();
/**
 * Consecutive failed asks per draft, cleared by anything that proves the read
 * can succeed.
 *
 * CTA-FR-XVNC asks again whenever a previous ask failed, and this is what keeps
 * "again" from meaning "forever": a backend that refuses every read costs the
 * rail three calls per draft rather than one per render.
 */
const failures = new Map<string, number>();
/** CTA-FR-LOOE: how many asks a draft gets while every one of them fails. */
const MAX_FAILED_ASKS = 3;
/**
 * CTA-FR-LOOE: references whose **one further ask** has been made.
 *
 * Keyed by draft and proposal together, because the further ask is per
 * reference: a conversation carrying a reference to a proposal the reading did
 * not return gets exactly one more reading out of it, and then settles on
 * saying the change is gone rather than asking after it for the rest of the
 * session.
 */
const healed = new Set<string>();
/**
 * DCR-FR-33: which content root the readings in flight belong to.
 *
 * Bumped by the reset, and checked by a reading when it lands. A promise cannot
 * be cancelled, so this is what stops one issued against the outgoing tree from
 * answering for the incoming one.
 */
let readingGeneration = 0;

/** The key `healed` is kept under. `\u0000` cannot occur in either id. */
function referenceKey(draftId: string, proposalId: string): string {
  return `${draftId}\u0000${proposalId}`;
}
/**
 * DCR-FR-03: drafts with a consumer mounted right now, ref-counted.
 *
 * What makes an arriving proposal *arrive somewhere*. The subscriber that hears
 * the event is the shell's (DCR-FR-18), so it hears one for every draft in the
 * worktree including drafts whose tab is closed — and a modal cannot open in a
 * tab that is not there. Without this, such a proposal would sit in the open
 * slot until the author switched to that draft for some reason of their own and
 * was interrupted by a modal they never asked for, which is exactly what
 * DCR-FR-03 says a standing proposal must not do; and while it sat there, an
 * arrival for the draft the author *is* looking at would find the slot taken.
 */
const watching = new Map<string, number>();

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
 * DCR-FR-33: reset every scrap of this store.
 *
 * Called when the project closes and when the active worktree changes: a
 * proposal names a draft of the outgoing content root and does not survive that
 * tree (DCP-FR-02), and a modal left open over one would be reviewing a change
 * to a file the application is no longer reading (DCR-FR-22). The readings go
 * with the proposals rather than being kept as an optimisation — a reading of
 * the outgoing tree would otherwise answer for a draft of the incoming one, and
 * answer wrongly.
 */
export function resetDraftProposals(): void {
  byDraft = new Map();
  reviewing = null;
  status.clear();
  failures.clear();
  healed.clear();
  readingGeneration += 1;
  // Not `watching`: it is ref-counted against components that are still mounted,
  // and clearing it here would leave those counts unbalanced when they unmount.
  emit();
}

/** The proposals of one draft, empty until it has been loaded. */
export function proposalsOf(draftId: string): readonly DraftChangeProposal[] {
  return byDraft.get(draftId) ?? EMPTY;
}

/**
 * A stable empty array.
 *
 * `useSyncExternalStore` compares snapshots by identity and throws on a
 * getSnapshot that returns a fresh value every call, so an unloaded draft must
 * answer with the *same* empty array each time rather than a new `[]`.
 */
const EMPTY: readonly DraftChangeProposal[] = Object.freeze([]);

/** DCP-FR-04: the draft's undecided proposal, if it has one (NAW-FR-35). */
export function pendingOf(draftId: string): DraftChangeProposal | undefined {
  return proposalsOf(draftId).find((p) => p.state === "pending");
}

/** DCR-FR-32: how far this draft's reading has got. */
export function readingStatusOf(draftId: string): ReadingStatus {
  return status.get(draftId) ?? "unread";
}

/**
 * The reading as one value a subscriber can compare.
 *
 * It carries the failure count as well as the status because a second ask that
 * failed is a *change* a waiting reference has to hear about — without it, one
 * failure would look exactly like the next and the retry of CTA-FR-LOOE would
 * stop after a single attempt rather than at the bound.
 */
function readingSignal(draftId: string | undefined): string {
  // An attachment naming no draft names nothing this store can read.
  if (!draftId) return "read#0";
  return `${readingStatusOf(draftId)}#${failures.get(draftId) ?? 0}`;
}

/**
 * DCR-FR-32 / CTA-FR-LOOE: take this draft's reading unless one already answers.
 *
 * Asks when nothing has been read for the draft and again when a previous ask
 * failed — a reading that failed is not the session's answer, or a draft whose
 * first read landed while the project was still opening would show no
 * indication, open no review, and hold a standing proposal nothing in the
 * window could decide. One ask is in flight per draft however many surfaces
 * want it, and a draft whose every ask fails stops at `MAX_FAILED_ASKS` rather
 * than being re-asked by every render that follows.
 *
 * Failure is deliberately quiet: a marker that cannot be drawn is not worth a
 * blocking error over a tab the author opened to write in. It is logged,
 * because a marker that silently never appears is otherwise invisible to anyone
 * debugging it.
 */
export function ensureLoaded(draftId: string): void {
  const current = readingStatusOf(draftId);
  // "absent" is as settled as "read": the draft is gone, and asking after it
  // again would only be refused again.
  if (current === "asking" || current === "read" || current === "absent") return;
  if ((failures.get(draftId) ?? 0) >= MAX_FAILED_ASKS) return;
  ask(draftId);
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
  draftId: string,
  proposalId: string,
): void {
  const current = readingStatusOf(draftId);
  if (current === "asking" || current === "absent") return;
  if (current !== "read") {
    ensureLoaded(draftId);
    return;
  }
  if (proposalsOf(draftId).some((p) => p.id === proposalId)) return;
  const key = referenceKey(draftId, proposalId);
  if (healed.has(key)) return;
  healed.add(key);
  logDebug(["frontend"], "re-reading a draft's proposals for a reference", {
    draftId,
    proposalId,
  });
  ask(draftId);
}

function ask(draftId: string): void {
  status.set(draftId, "asking");
  // DCR-FR-33: which content root this reading belongs to. A reading in flight
  // when the project closes or the worktree changes answers about a tree the
  // window has left, and folding it in would put the outgoing tree's proposals
  // into the incoming session — and, the draft then counting as read, suppress
  // the fresh reading that was supposed to replace them.
  const generation = readingGeneration;
  void listDraftChangeProposals(draftId)
    .then((proposals) => {
      if (generation !== readingGeneration) return;
      // A proposal recorded *while this read was in flight* is already in the
      // store and is newer than what the read returned — the backend appended it
      // after the list was taken. Overwriting would drop it, leaving the tab
      // with no marker for a proposal the author was just notified about and the
      // review slot pointing at a record nothing holds. Anything this draft is
      // already carrying therefore wins, and the read fills in the rest.
      const known = byDraft.get(draftId) ?? [];
      const merged = [
        ...known,
        ...proposals.filter((p) => !known.some((k) => k.id === p.id)),
      ];
      byDraft = new Map(byDraft).set(draftId, merged);
      status.set(draftId, "read");
      failures.delete(draftId);
      emit();
    })
    .catch((error: unknown) => {
      if (generation !== readingGeneration) return;
      // CTA-FR-SACG: a draft that is not there answers every reference into it —
      // it was graduated or deleted and took its proposals with it (DCP-FR-02),
      // and no number of further asks will bring one back. This is a reading
      // that completed, so a control may say the change is gone from it.
      if (String(error).includes(PROPOSAL_ERRORS.draftNotFound)) {
        status.set(draftId, "absent");
        failures.delete(draftId);
        logDebug(["frontend"], "a draft carrying proposals is no longer there", {
          draftId,
        });
        emit();
        return;
      }
      // DCR-FR-32: any other failure is not the session's answer. The draft is
      // left saying so rather than saying it holds nothing, so no surface
      // reports a proposal absent from a reading that never completed, and the
      // next reference that renders asks again.
      status.set(draftId, "failed");
      failures.set(draftId, (failures.get(draftId) ?? 0) + 1);
      logWarn(["frontend"], "could not load a draft's change proposals", {
        draftId,
        attempt: failures.get(draftId) ?? 1,
        error: String(error),
      });
      emit();
    });
}

/**
 * DCP-FR-16: fold one `draft-change-proposals-changed` payload into the store.
 *
 * The payload carries the whole proposal, so this replaces the record in place
 * rather than re-reading the draft — which is what lets the marker follow a
 * decision made in another window without a round-trip.
 */
export function noteProposalChanged(payload: ProposalsChangedPayload): void {
  const { draftId, proposal } = payload;
  const current = byDraft.get(draftId) ?? [];
  const known = current.some((p) => p.id === proposal.id);
  const next = known
    ? current.map((p) => (p.id === proposal.id ? proposal : p))
    : [proposal, ...current];
  byDraft = new Map(byDraft).set(draftId, next);
  // DCR-FR-32: an event tells this store about **one** proposal, so it is not a
  // reading of the draft and does not stand in for one — a draft known only
  // through events holds whatever it was told about and nothing else, and a
  // reference to any other proposal of it would be judged against a list nobody
  // ever read. What the event does settle is that the backend is answering, so
  // a draft whose earlier reads failed is given its asks back.
  failures.delete(draftId);
  // DCR-FR-03: a proposal that *arrives somewhere* shows itself. The author
  // asked an agent a question and is waiting for the answer; making them find
  // and click an indication to read what came back is a step with nothing in it.
  //
  // Four conditions, each closing a way this could interrupt someone who did not
  // ask to be interrupted:
  //
  // - it must be **new** — a decision, or a second event about a proposal
  //   already seen, is not an arrival, and reopening on one would make the modal
  //   impossible to get rid of;
  // - it must be **pending** — there is nothing to decide about a decided one;
  // - nothing may already be **open** (DCR-FR-02) — what is showing is the
  //   proposal being decided, and swapping it out would decide it by accident;
  // - the draft must have a **tab mounted** to arrive in. The listener is the
  //   shell's (DCR-FR-18) and hears about every draft in the worktree, so
  //   without this a proposal for a closed tab would take the slot and surface
  //   nowhere — then ambush the author the next time they opened that draft for
  //   a reason of their own, which is the interruption DCR-FR-03 rules out.
  //   Such a proposal is left to the tab's indication and to the notification.
  if (
    !known &&
    proposal.state === "pending" &&
    reviewing === null &&
    watching.has(draftId)
  ) {
    reviewing = proposal.id;
  }
  // DCR-FR-08 / DCR-FR-21 / DCR-FR-WRJP: what the proposal *holds* moved too,
  // so the changes are read again — an agent's revision replaces that change's
  // decoration in place, and a change decided in another window clears its own.
  // The record this payload carries says only where the proposal stands.
  //
  // Here rather than in the shell's listener, because this is the one place
  // every route to "the proposal moved" passes through: a decision made in this
  // window comes through here too.
  if (hunksOf(proposal.id) !== undefined) rereadHunks(proposal.id);
  // DCR-FR-21 / DCR-FR-25: a proposal decided — here or from another window —
  // has nothing left to edit, so any candidate edit still unwritten is dropped
  // with the decision controls rather than left to be written into a proposal
  // that would refuse it (DCP-FR-26).
  if (proposal.state !== "pending") {
    hunkCandidateBuffers.dropProposal(proposal.id);
  }
  emit();
}

// --- Per-change selectors (DCP-FR-PWSF) -----------------------------------
//
// Everything below reads the record's ledger, which is identity and decision and
// no text (DCP-FR-01). None of it reads a document, so a surface can say how a
// proposal stands without loading one.

/**
 * The ledger a proposal carries, or an empty one.
 *
 * A record written before proposals held changes carries no ledger at all, and
 * such a record still arrives — `proposals/` is committed and travels through
 * Git (DCP-FR-02), so an older build's proposal reaches this one on a pull. It
 * reads as a proposal with nothing yet known about its changes rather than as a
 * shape this build refuses.
 */
function ledgerOf(proposal: DraftChangeProposal): readonly HunkLedgerRow[] {
  return proposal.ledger ?? EMPTY_LEDGER;
}

const EMPTY_LEDGER: readonly HunkLedgerRow[] = [];

/** The ledger rows of one proposal that the author has not settled. */
export function undecidedHunks(
  proposal: DraftChangeProposal,
): readonly HunkLedgerRow[] {
  return ledgerOf(proposal).filter((row) => isUndecided(row.state));
}

/**
 * DCR-FR-05: the change a review opens on — the first undecided one in proposal
 * order, which is the order the author reads the document in.
 */
export function firstUndecidedHunk(
  proposal: DraftChangeProposal,
): HunkLedgerRow | undefined {
  return ledgerOf(proposal).find((row) => isUndecided(row.state));
}

/** Where one change of one proposal stands, or `undefined` if it has no row. */
export function hunkStateOf(
  proposal: DraftChangeProposal,
  hunkId: string,
): HunkState | undefined {
  return ledgerOf(proposal).find((row) => row.id === hunkId)?.state;
}

/**
 * DCR-FR-05: the change's position in the proposal, one-based, for the review
 * bar's `change N of M`.
 *
 * Counted over every change rather than over the undecided ones, so a change
 * keeps the number the author has been calling it by after its neighbours are
 * decided.
 */
export function hunkPosition(
  proposal: DraftChangeProposal,
  hunkId: string,
): number {
  return ledgerOf(proposal).findIndex((row) => row.id === hunkId) + 1;
}

/**
 * DCP-FR-04: how many changes across a draft's proposals are still undecided.
 *
 * What the Drafts panel's marker (DRP-FR-19) and the tab's badge are said from:
 * the draft holds one pending proposal at a time, so this is the count the
 * author still owes an answer to.
 */
export function undecidedCountOf(draftId: string): number {
  let total = 0;
  for (const proposal of proposalsOf(draftId)) {
    if (proposal.state !== "pending") continue;
    total += undecidedCount(proposal.counts);
  }
  return total;
}

/** DCR-FR-02: open the review modal on this proposal. */
export function openReview(proposalId: string): void {
  reviewing = proposalId;
  emit();
}

/** DCR-FR-23: dismiss it, deciding nothing. */
export function closeReview(): void {
  if (reviewing === null) return;
  reviewing = null;
  emit();
}

/**
 * DCR-FR-02: the proposal the review is open on, read outside React.
 *
 * Exported beside the hook because the shell decides whether an arriving
 * proposal opens itself (DCR-FR-03), and that decision is made in an event
 * listener rather than in a render.
 */
export function reviewingProposal(): string | null {
  return reviewing;
}

/** The proposal the review modal is open on (DCR-FR-02). */
export function useReviewing(): string | null {
  return useSyncExternalStore(subscribe, reviewingProposal, reviewingProposal);
}

/**
 * One draft's proposals, loading them on first use.
 *
 * The load is kicked off from the subscribe callback rather than from an effect
 * so a caller gets it by subscribing at all — every consumer of this store wants
 * the list, and an effect in each of them would be three places to forget it.
 */
export function useDraftProposals(
  draftId: string | null,
): readonly DraftChangeProposal[] {
  const subscribeToDraft = useCallback(
    (listener: () => void) => {
      const unsubscribe = subscribe(listener);
      if (draftId === null) return unsubscribe;
      ensureLoaded(draftId);
      // DCR-FR-03: this draft now has somewhere for an arriving proposal to
      // arrive. Ref-counted rather than a flag, because a tab has more than one
      // consumer of this store — the workspace, the review itself, and every
      // proposal reference in the comment rail — and any one of them unmounting
      // must not take the draft out from under the others.
      const count = watching.get(draftId) ?? 0;
      watching.set(draftId, count + 1);
      return () => {
        const left = (watching.get(draftId) ?? 1) - 1;
        if (left > 0) watching.set(draftId, left);
        else watching.delete(draftId);
        unsubscribe();
      };
    },
    [draftId],
  );
  const snapshot = useCallback(
    () => (draftId === null ? EMPTY : proposalsOf(draftId)),
    [draftId],
  );
  return useSyncExternalStore(subscribeToDraft, snapshot, snapshot);
}

/**
 * CTA-FR-UKIG: what this store can say about one proposal reference.
 *
 * Three answers rather than a proposal-or-nothing, because "I have not looked"
 * and "I looked and it is not there" are different things to tell an author, and
 * the second is the only one that may take the review away from them.
 */
export type ProposalReference =
  /** The reading holds it: render its state and open the review on it. */
  | { readonly kind: "known"; readonly proposal: DraftChangeProposal }
  /** No completed reading holds it, and one is in flight or still owed. */
  | { readonly kind: "unresolved" }
  /** A completed reading did not return it, and neither did the further one. */
  | { readonly kind: "gone" };

/** CTA-FR-SACG: where one reference stands, read outside React. */
export function referenceTo(
  draftId: string | undefined,
  proposalId: string,
): ProposalReference {
  // A reference naming no draft is one nothing can look for: it is unresolved
  // rather than gone, and it asks for no reading.
  if (!draftId) return UNRESOLVED;
  const proposal = proposalsOf(draftId).find((p) => p.id === proposalId);
  if (proposal !== undefined) return { kind: "known", proposal };
  const reading = readingStatusOf(draftId);
  // Only a reading that completed can say a proposal is gone. A draft that is
  // not there says it of every reference at once; a draft that is there says it
  // only after the further ask of CTA-FR-LOOE has agreed with the first.
  if (reading === "absent") return GONE;
  const settled =
    reading === "read" && healed.has(referenceKey(draftId, proposalId));
  return settled ? GONE : UNRESOLVED;
}

const UNRESOLVED: ProposalReference = Object.freeze({ kind: "unresolved" });
const GONE: ProposalReference = Object.freeze({ kind: "gone" });

/**
 * One proposal reference, asking for the reading it needs (CMT-FR-67,
 * CTA-FR-UKIG, CTA-FR-XVNC).
 *
 * What the comment rail's control renders from: the attachment carries the
 * identity and the state lives here, so a control shows whatever the proposal
 * has since become without the log line having changed — and says nothing at all
 * about it until a reading has actually answered.
 *
 * The ask is an effect rather than part of the subscribe of `useDraftProposals`
 * because it is driven by *this reference's* answer rather than by the draft
 * being wanted at all: it re-runs when the reading moves on, which is what makes
 * the further ask happen once a first reading has completed without the
 * proposal, and what makes a failed reading be tried again — and, deps being
 * what they are, what stops either from turning into a loop.
 */
export function useProposalReference(
  draftId: string | undefined,
  proposalId: string,
): ProposalReference {
  const proposals = useDraftProposals(draftId ? draftId : null);
  const snapshot = useCallback(() => readingSignal(draftId), [draftId]);
  const reading = useSyncExternalStore(subscribe, snapshot, snapshot);
  const proposal = proposals.find((p) => p.id === proposalId);

  useEffect(() => {
    if (!draftId || proposal !== undefined) return;
    requestProposalReading(draftId, proposalId);
  }, [draftId, proposalId, proposal, reading]);

  // Stable across renders, so a caller may put the answer in a dependency list:
  // the two answers that carry nothing are frozen singletons, and the one that
  // carries a proposal is held against it. What is deliberately *not* memoised
  // is the choice between them — whether a reference has settled turns on
  // `healed`, which is set outside a render, so a memo over it would keep
  // rendering a settled control as unresolved.
  const known = useMemo(
    () => (proposal === undefined ? null : ({ kind: "known", proposal } as const)),
    [proposal],
  );
  return known ?? referenceTo(draftId, proposalId);
}
