/**
 * Reviewing a proposal inside the document it changes
 * (`../../../specifications/ui/DCR-draft-change-review.md`, DCR-FR-GNRL;
 * `PCR-prompt-change-review.md` PCR-FR-QHZN).
 *
 * The one code path both proposal kinds take: the draft review
 * (`../NewArtifactWorkspace/useDraftReview.tsx`) and the review of a prompt or
 * DOC file in its Editor tab (`../Editor/useArtifactReview.tsx`). Each passes
 * what is its own — which commands decide a change, which store holds the hunk
 * edits, and what the surface calls the document — and this builds the one
 * `EditorReview` the shared Editor renders in place.
 *
 * Every route to a decision goes through the one `decide` below — the chip, the
 * accelerators, and Accept all — so what a decision does, what it reports, and
 * what it leaves standing is one behaviour rather than three.
 */
import { useCallback, useMemo, useState, useSyncExternalStore } from "react";

import { participantName } from "../../types";
import type { HunkCounts, HunkLedgerRow, Participant } from "../../types";
import { ReviewBar } from "./ReviewBar";
import { stepTo, useHunkNavigation } from "./useHunkNavigation";
import type { EditorReview, ReviewHunk } from "../Editor/props";
import { hunkKey, type CandidateBufferStore } from "../../state/candidateBuffers";
import { setFocusedHunk, useDraftReviewState } from "../../state/draftDiscussion";
import {
  hunkPosition,
  hunkStateOf,
  nextUndecidedAfter,
  undecidedHunks,
  visitedHunks,
} from "../../state/draftProposals";
import { rereadHunks, useProposalHunks, type HunkSource } from "../../state/proposalHunks";

/** DCR-FR-GNRL: what the review reads of a proposal, of either kind. */
export interface ReviewableProposal {
  id: string;
  agent: Participant;
  hunkCount: number;
  counts: HunkCounts;
  ledger: HunkLedgerRow[];
}

/** DCR-FR-GNRL: what one kind of proposal lends the review. */
export interface ProposalReviewKind<P extends ReviewableProposal> {
  /** Which command reads the proposal's changes. */
  source: HunkSource;
  /** The key of the review state: a draft id, or `artifactReviewKey`. */
  reviewKey: string;
  /** The store the author's hunk edits are held in. */
  buffers: CandidateBufferStore;
  /** What the surface calls the document: "draft" or "file". */
  noun: "draft" | "file";
  /**
   * DCR-FR-13 / PCR-FR-AXPW: decide one change. Resolves `false` when the kind
   * handled a refusal itself and nothing more is to be done.
   */
  decide: (proposal: P, hunkId: string, how: "accept" | "reject") => Promise<boolean>;
  /** DCR-FR-CXZG: reject every change still undecided, as one operation. */
  decline: (proposal: P) => Promise<void>;
  /** DCR-FR-16: a typed refusal in words. */
  message: (error: string) => string;
}

export function useProposalReview<P extends ReviewableProposal>({
  kind,
  proposal: pendingProposal,
  locked,
  viewingVersion,
  onDiscussHunk,
}: {
  kind: ProposalReviewKind<P>;
  /** DCP-FR-04 / PCP-FR-04: the one undecided proposal, if there is one. */
  proposal: P | undefined;
  /** A run holds the document, so nothing here may write it. */
  locked: boolean;
  /** NAW-FR-09: a past version is read-only, so it carries no review bar. */
  viewingVersion: boolean;
  /** DCR-FR-MTFD: open, or show, the discussion of one change. */
  onDiscussHunk: (hunkId: string) => void;
}): EditorReview | undefined {
  const { reviewKey, buffers, source, noun } = kind;
  const view = useDraftReviewState(reviewKey);

  /**
   * DCR-FR-08: the changes the standing proposal holds, read when the tab opens
   * on a document that has one and re-read when the backend reports one
   * changed. A document holding no proposal costs no read.
   */
  const hunkReading = useProposalHunks(pendingProposal?.id ?? null, source);
  /** DCR-FR-16: the typed error of the last refused decision. */
  const [decisionError, setDecisionError] = useState<string | null>(null);
  /**
   * DCR-FR-25: the author's own rewrites of the proposed changes, held in the
   * buffer store so an edit survives the tab being closed and reopened. This is
   * only the subscription that makes the surface follow it.
   */
  const hunkBufferVersion = useSyncExternalStore(
    buffers.subscribe,
    buffers.getVersion,
    buffers.getVersion,
  );
  /** A decision is in flight, so every decision control is off. */
  const [deciding, setDeciding] = useState(false);

  /** DCR-FR-05: the proposal's changes as the document column needs them. */
  const reviewHunks: ReviewHunk[] = useMemo(() => {
    if (pendingProposal === undefined) return [];
    if (hunkReading?.status !== "read") return [];
    const { hunks, resolutions, legacy } = hunkReading.hunks;
    const agent = participantName(pendingProposal.agent);
    return hunks.map((hunk, at) => ({
      id: hunk.id,
      kind: hunk.kind,
      state: hunkStateOf(pendingProposal, hunk.id) ?? "pending",
      before: hunk.before ?? "",
      after: hunk.after ?? "",
      lead: hunk.anchor.lead,
      hint: hunk.anchor.hint_start,
      lost: resolutions[at]?.kind === "lost",
      position: at + 1,
      total: hunks.length,
      agent,
      // DCR-FR-25: what the author has written into this change, if anything.
      draft: hunkBufferVersion >= 0
        ? buffers.get(hunkKey(pendingProposal.id, hunk.id))?.text
        : undefined,
      // DCR-FR-KDSV / PCR-FR-LGVY: a legacy proposal is one change covering
      // the whole document and accepts no edit to it.
      editable: !legacy,
    }));
    // `hunkBufferVersion` stands in for the mutable buffer store.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pendingProposal, hunkReading, hunkBufferVersion, buffers]);

  /** DCR-FR-11: the changes still to decide, as the record knows them. */
  const undecidedRows = useMemo(
    () => (pendingProposal ? undecidedHunks(pendingProposal) : []),
    [pendingProposal],
  );
  /** DCR-FR-QPRV: the changes a move visits, a rejected one included. */
  const visitedRows = useMemo(
    () => (pendingProposal ? visitedHunks(pendingProposal) : []),
    [pendingProposal],
  );

  /**
   * DCR-FR-05: the change the review is on — the first undecided one until the
   * author moves.
   */
  const focusedHunkId =
    view.focusedHunkId !== null &&
    visitedRows.some((row) => row.id === view.focusedHunkId)
      ? view.focusedHunkId
      : (undecidedRows[0]?.id ?? null);

  const proposalId = pendingProposal?.id ?? null;

  /** DCR-FR-13 / DCR-FR-14: one decision, about one change. */
  const decide = useCallback(
    async (hunkId: string, how: "accept" | "reject") => {
      if (pendingProposal === undefined) return false;
      const id = pendingProposal.id;
      setDeciding(true);
      setDecisionError(null);
      const key = hunkKey(id, hunkId);
      try {
        // DCR-FR-26 / DCR-FR-29: an edit still resting is brought forward by the
        // decision, so the text that is accepted is the text on screen. A write
        // that did not land stops the decision.
        const landed = await buffers.flush(key);
        if (!landed) {
          setDecisionError(
            buffers.get(key)?.conflict
              ? "This change has been rewritten elsewhere since you edited it. Choose which text to keep before deciding it."
              : "Your edit to this change could not be written, so it has not been decided.",
          );
          return false;
        }
        if (!(await kind.decide(pendingProposal, hunkId, how))) return false;
        // DCR-FR-25: the change is decided, so there is nothing left to edit.
        buffers.drop(key);
        // DCR-FR-12: the review moves to the **next** undecided change, chosen
        // from the ledger as it stood when the decision was made.
        setFocusedHunk(reviewKey, nextUndecidedAfter(pendingProposal, hunkId));
        rereadHunks(id, source);
        return true;
      } catch (e) {
        // DCR-FR-16: the document is byte-for-byte what it was and every change
        // is where it was. The decision is retried without retyping anything.
        setDecisionError(kind.message(String(e)));
        return false;
      } finally {
        setDeciding(false);
      }
    },
    [pendingProposal, buffers, kind, reviewKey, source],
  );

  const acceptHunk = useCallback((hunkId: string) => void decide(hunkId, "accept"), [decide]);
  const rejectHunk = useCallback(
    (hunkId: string) => {
      // DCR-FR-12: a rejected change offers no Reject, so its accelerator does
      // nothing either; the backend would refuse it.
      if (pendingProposal && hunkStateOf(pendingProposal, hunkId) === "rejected") return;
      void decide(hunkId, "reject");
    },
    [decide, pendingProposal],
  );

  /**
   * DCR-FR-CXZG: accept every undecided change, in order, stopping at the first
   * refusal and leaving the ones after it undecided.
   */
  const acceptAll = useCallback(async () => {
    for (const row of undecidedRows) {
      const landed = await decide(row.id, "accept");
      if (!landed) return;
    }
  }, [undecidedRows, decide]);

  /** DCR-FR-CXZG: reject every change still undecided, as one operation. */
  const rejectAll = useCallback(async () => {
    if (pendingProposal === undefined) return;
    setDeciding(true);
    setDecisionError(null);
    try {
      await kind.decline(pendingProposal);
      rereadHunks(pendingProposal.id, source);
    } catch (e) {
      setDecisionError(kind.message(String(e)));
    } finally {
      setDeciding(false);
    }
  }, [pendingProposal, kind, source]);

  /**
   * DCR-FR-24 / DCR-FR-26: the author rewrote a change's proposed text. It
   * rests briefly and then writes itself into proposal storage — and that write
   * is not a change to the document.
   */
  const editHunk = useCallback(
    (hunkId: string, after: string) => {
      if (proposalId === null) return;
      const key = hunkKey(proposalId, hunkId);
      const hunk = reviewHunks.find((h) => h.id === hunkId);
      // DCR-FR-25: the buffer is adopted from the agent's own text the first
      // time the author touches it, and never afterwards.
      if (!buffers.get(key)) {
        buffers.adopt(
          key,
          hunk?.after ?? "",
          hunkReading?.status === "read" ? hunkReading.hunks.checksum : "",
        );
      }
      buffers.edit(key, after);
    },
    [proposalId, reviewHunks, hunkReading, buffers],
  );

  /** DCR-FR-11 / DCR-FR-30: the changes a move steps through, as ids alone. */
  const movable = useMemo(() => visitedRows.map((row) => ({ id: row.id })), [visitedRows]);

  // DCR-FR-30: the move accelerators, live only while there is a proposal.
  useHunkNavigation({
    reviewKey,
    hunks: movable,
    focused: focusedHunkId,
    active: pendingProposal !== undefined && !locked,
  });

  /** DCR-FR-31: the changes the document could draw, as it reported them. */
  const [placedIds, setPlacedIds] = useState<string[]>([]);
  const onPlaced = useCallback((ids: string[]) => {
    setPlacedIds((prev) =>
      prev.length === ids.length && prev.every((id, i) => id === ids[i]) ? prev : ids,
    );
  }, []);
  const undrawnHunkId =
    reviewHunks.length > 0 && focusedHunkId !== null && !placedIds.includes(focusedHunkId)
      ? focusedHunkId
      : null;

  /** DCR-FR-11: the bar's controls move by exactly the accelerators' rule. */
  const moveTo = useCallback(
    (step: 1 | -1) => {
      const next = stepTo(movable, focusedHunkId, step);
      if (next !== null) setFocusedHunk(reviewKey, next);
    },
    [movable, focusedHunkId, reviewKey],
  );

  /** DCR-FR-29: the unresolved edit conflict on the change under review. */
  const conflict = useMemo(() => {
    if (proposalId === null || focusedHunkId === null) return null;
    const key = hunkKey(proposalId, focusedHunkId);
    if (hunkBufferVersion < 0) return null;
    if (!buffers.get(key)?.conflict) return null;
    const stored = reviewHunks.find((h) => h.id === focusedHunkId)?.after ?? "";
    const checksum = hunkReading?.status === "read" ? hunkReading.hunks.checksum : "";
    return {
      onKeep: () => buffers.resolveCandidate(key, "keep", { content: stored, checksum }),
      onTake: () => buffers.resolveCandidate(key, "take", { content: stored, checksum }),
    };
    // `hunkBufferVersion` stands in for the mutable buffer store.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [proposalId, focusedHunkId, hunkBufferVersion, reviewHunks, hunkReading, buffers]);

  /**
   * DCR-FR-27: what the surface says about the document, in words — nothing
   * has happened yet; the author has rewritten a change and still nothing has
   * happened; or a change has landed.
   */
  const standing = useMemo(() => {
    const accepted = pendingProposal?.counts?.accepted ?? 0;
    if (accepted > 0) {
      return accepted === 1
        ? `1 change accepted into this ${noun}`
        : `${accepted} changes accepted into this ${noun}`;
    }
    const subject = noun === "draft" ? "The draft" : "The file";
    if (reviewHunks.some((h) => h.draft != null && h.draft !== h.after)) {
      return `You have edited a proposed change. ${subject} has not changed.`;
    }
    return `${subject} has not changed.`;
  }, [pendingProposal, reviewHunks, noun]);

  if (pendingProposal === undefined) return undefined;
  /** DCR-FR-05: what the editing surface needs to render the review in place. */
  return {
    proposalId: pendingProposal.id,
    hunks: reviewHunks,
    focused: focusedHunkId,
    reveal: view.hunkReveal,
    // DCR-FR-29: a change with an unresolved edit is not decided until the
    // author has said which text survives.
    busy: deciding || locked || conflict !== null,
    onFocus: (hunkId) => setFocusedHunk(reviewKey, hunkId),
    onAccept: acceptHunk,
    onReject: rejectHunk,
    onDiscuss: onDiscussHunk,
    onEdit: editHunk,
    // DCR-FR-11: the review bar, at the foot of the document column.
    bar: !viewingVersion ? (
      <ReviewBar
        proposal={pendingProposal}
        position={focusedHunkId === null ? 0 : hunkPosition(pendingProposal, focusedHunkId)}
        busy={deciding || locked}
        error={decisionError}
        conflict={conflict}
        standing={standing}
        undrawn={undrawnHunkId !== null && undrawnHunkId === focusedHunkId}
        canPrevious={stepTo(movable, focusedHunkId, -1) !== null}
        canNext={stepTo(movable, focusedHunkId, 1) !== null}
        onPrevious={() => moveTo(-1)}
        onNext={() => moveTo(1)}
        onAcceptAll={() => void acceptAll()}
        onRejectAll={() => void rejectAll()}
      />
    ) : null,
    onPlaced,
  };
}
