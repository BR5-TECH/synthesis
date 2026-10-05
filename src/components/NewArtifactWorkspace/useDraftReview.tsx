/**
 * Reviewing a proposal inside the draft's document
 * (`../../../specifications/ui/DCR-draft-change-review.md`).
 *
 * A hook rather than a component, because what the review produces is not a
 * surface of its own: it is what the **editing surface** renders — decorations
 * in the prose, a bar between the toolbar and the page, and a chip over the
 * space each change reserves for it. The tab holds the draft; this holds what
 * has been proposed for it.
 *
 * Every route to a decision goes through the one `decide` below — the chip, the
 * accelerators, and Accept all — so what a decision does, what it reports, and
 * what it leaves standing is one behaviour rather than three.
 */
import { useCallback, useMemo, useState, useSyncExternalStore } from "react";

import * as api from "../../api";
import { participantName } from "../../types";
import type { DraftChangeProposal, ProjectAgent } from "../../types";
import { ReviewBar, decisionMessage } from "../DraftDiscussion";
import { stepTo, useHunkNavigation } from "../DraftDiscussion/useHunkNavigation";
import type { EditorReview, ReviewHunk } from "../Editor/props";
import { hunkCandidateBuffers, hunkKey } from "../../state/candidateBuffers";
import {
  releaseHunkFocus,
  setFocusedHunk,
  useDraftDiscussion,
} from "../../state/draftDiscussion";
import {
  hunkPosition,
  hunkStateOf,
  undecidedHunks,
} from "../../state/draftProposals";
import { rereadHunks, useProposalHunks } from "../../state/proposalHunks";
import { dispatchDecision } from "../DraftDiscussion/decisionDispatch";

export function useDraftReview({
  draftId,
  proposal: pendingProposal,
  locked,
  viewingVersion,
  path,
  flushBeforeApply,
  reloadAfterApply,
  reloadHistory: loadHistory,
  onDiscussFocus,
  agents,
}: {
  draftId: string;
  /** DCP-FR-04: the draft's one undecided proposal, if it has one. */
  proposal: DraftChangeProposal | undefined;
  /** NAW-FR-44: a run holds the prompt, so nothing here may write it. */
  locked: boolean;
  /** NAW-FR-09: a past version is read-only, so it carries no review bar. */
  viewingVersion: boolean;
  /** The draft file an acceptance rewrites, which is the prompt. */
  path: string | null;
  /** NAW-FR-13: write the pending buffer before an acceptance lands on it. */
  flushBeforeApply: (path: string) => Promise<void>;
  /** NAW-FR-42: read the prompt again once an acceptance has rewritten it. */
  reloadAfterApply: (path: string) => void;
  reloadHistory: () => void;
  /** DCR-FR-MTFD: take the author to the discussion column's composer. */
  onDiscussFocus: () => void;
  /** CTA-FR-VQFJ: the project's agents, for the decision's own dispatch. */
  agents: readonly ProjectAgent[];
}): EditorReview | undefined {
  const view = useDraftDiscussion(draftId);

  /**
   * DCR-FR-08: the changes the standing proposal holds, read when the tab opens
   * on a draft that has one and re-read when the backend reports one changed. A
   * draft holding no proposal costs no read.
   */
  const hunkReading = useProposalHunks(pendingProposal?.id ?? null);
  /** DCR-FR-16: the typed error of the last refused decision. */
  const [decisionError, setDecisionError] = useState<string | null>(null);
  /**
   * DCR-FR-25: the author's own rewrites of the proposed changes.
   *
   * Held in the buffer store rather than here, keyed by change, so an edit
   * survives the tab being closed and the draft reopened. This is only the
   * subscription that makes the surface follow it.
   */
  const hunkBufferVersion = useSyncExternalStore(
    hunkCandidateBuffers.subscribe,
    hunkCandidateBuffers.getVersion,
    hunkCandidateBuffers.getVersion,
  );
  /** A decision is in flight, so every decision control is off. */
  const [deciding, setDeciding] = useState(false);

  /**
   * DCR-FR-05: the proposal's changes as the document column needs them —
   * each with the text it alters, where it was recorded, and whether the prompt
   * still holds that text.
   */
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
      // Read from the buffer rather than from the record, so it is what is on
      // screen and survives the tab being closed and the draft reopened.
      draft: hunkBufferVersion >= 0
        ? hunkCandidateBuffers.get(hunkKey(pendingProposal.id, hunk.id))?.text
        : undefined,
      // DCR-FR-KDSV: a legacy proposal is one change covering the whole prompt
      // and accepts no edit to it.
      editable: !legacy,
    }));
    // `hunkBufferVersion` stands in for the mutable buffer store.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pendingProposal, hunkReading, hunkBufferVersion]);

  /**
   * DCR-FR-11: the changes still to decide as the **record** knows them.
   *
   * The record carries the ledger and arrives with the proposal, so the review
   * bar says which change is under review from the moment the tab knows there
   * is one — without waiting for the document the changes themselves live in
   * (DCP-FR-01, DCP-FR-09).
   */
  const undecidedRows = useMemo(
    () => (pendingProposal ? undecidedHunks(pendingProposal) : []),
    [pendingProposal],
  );

  /**
   * DCR-FR-05: the change the review is on.
   *
   * The first undecided one until the author moves, and it moves on by itself
   * as changes are decided — an author who accepts one is looking at the next
   * without having to go and find it.
   */
  const focusedHunkId =
    view.focusedHunkId !== null &&
    undecidedRows.some((row) => row.id === view.focusedHunkId)
      ? view.focusedHunkId
      : (undecidedRows[0]?.id ?? null);

  const proposalId = pendingProposal?.id ?? null;

  /**
   * DCR-FR-13 / DCR-FR-14: one decision, about one change.
   *
   * Every route to a decision goes through here — the chip, the accelerators,
   * and Accept all — so what a decision does, what it reports, and what it
   * leaves standing is one behaviour rather than three.
   */
  const decide = useCallback(
    async (hunkId: string, how: "accept" | "reject") => {
      if (proposalId === null) return false;
      setDeciding(true);
      setDecisionError(null);
      const key = hunkKey(proposalId, hunkId);
      try {
        // DCR-FR-26 / DCR-FR-29: an edit still resting is brought forward by the
        // decision, so the text that is accepted is the text on screen.
        //
        // A write that did not land is what stops the decision. The one case
        // that matters is an unresolved conflict: the author has a rewrite on
        // screen and storage holds another, and accepting now would apply
        // **storage's** text and then drop theirs with the buffer — which is
        // the exact failure DCR-FR-29 exists to prevent. Neither side is chosen
        // for them.
        const landed = await hunkCandidateBuffers.flush(key);
        if (!landed) {
          setDecisionError(
            hunkCandidateBuffers.get(key)?.conflict
              ? "This change has been rewritten elsewhere since you edited it. Choose which text to keep before deciding it."
              : "Your edit to this change could not be written, so it has not been decided.",
          );
          return false;
        }
        let outcome;
        if (how === "accept") {
          // The autosave timer can still be counting down when the author
          // accepts. Written first, the acceptance overwrites it and the timer
          // has nothing left to put back over the accepted text.
          if (path !== null) await flushBeforeApply(path);
          outcome = await api.acceptDraftChangeHunk(proposalId, hunkId);
          if (path !== null) reloadAfterApply(path);
        } else {
          outcome = await api.rejectDraftChangeHunk(proposalId, hunkId);
        }
        // The record and the changes both moved on. The record arrives through
        // the backend's own event; the changes are re-read here, because their
        // placements are derived against a prompt an acceptance has rewritten.
        // DCR-FR-25: the change is decided, so there is nothing left to edit.
        hunkCandidateBuffers.drop(key);
        // DCR-FR-12: the review moves to the **next** undecided change, not
        // back to the first. An author who accepts the third of seven and is
        // sent to the first has the document scroll away from what they were
        // reading, to a change they have already looked at — and the
        // acceptance they just made reads as though nothing happened.
        //
        // Chosen here, from the order as it stood when the decision was made,
        // rather than left to the fallback: once the ledger has moved on, the
        // decided change is no longer in it to be counted from.
        const order = undecidedRows.map((row) => row.id);
        const at = order.indexOf(hunkId);
        const next =
          at === -1
            ? null
            : (order.slice(at + 1)[0] ?? order.slice(0, at)[0] ?? null);
        if (next !== null) setFocusedHunk(draftId, next);
        rereadHunks(proposalId);
        loadHistory();
        // DCR-FR-15: the decision that leaves nothing undecided is the one that
        // appended a comment, so it is the one with something for a turn to
        // answer. An unresolving decision carries no `commentId` and dispatches
        // nothing, which `dispatchDecision` reads off the outcome itself.
        if (pendingProposal !== undefined) {
          void dispatchDecision(pendingProposal, outcome, undefined, agents, "");
        }
        return true;
      } catch (e) {
        // DCR-FR-16: the prompt is byte-for-byte what it was and every change
        // is where it was. The decision is retried without retyping anything.
        setDecisionError(decisionMessage(String(e)));
        return false;
      } finally {
        setDeciding(false);
      }
    },
    [
      proposalId,
      pendingProposal,
      agents,
      path,
      draftId,
      undecidedRows,
      flushBeforeApply,
      reloadAfterApply,
      loadHistory,
    ],
  );

  const acceptHunk = useCallback(
    (hunkId: string) => void decide(hunkId, "accept"),
    [decide],
  );
  const rejectHunk = useCallback(
    (hunkId: string) => void decide(hunkId, "reject"),
    [decide],
  );

  /**
   * DCR-FR-CXZG: accept every undecided change, in order, stopping at the first
   * refusal and leaving the ones after it undecided.
   *
   * Walked here rather than asked for as one operation, because each acceptance
   * is its own transaction: what did and did not land is then exactly what the
   * author is looking at.
   */
  const acceptAll = useCallback(async () => {
    // Walked over the record's ledger rather than over the placed changes: the
    // ledger is what says which changes are undecided, and a change the
    // document could not place is still one the backend can apply.
    for (const row of undecidedRows) {
      const landed = await decide(row.id, "accept");
      if (!landed) return;
    }
  }, [undecidedRows, decide]);

  /**
   * DCR-FR-CXZG: reject every change still undecided, as one operation.
   *
   * The escape hatch the draft needs: a change held for discussion still holds
   * the draft's one pending slot, and this is what releases it (DCP-FR-04).
   */
  const rejectAll = useCallback(async () => {
    if (proposalId === null) return;
    setDeciding(true);
    setDecisionError(null);
    try {
      await api.declineDraftChangeProposal(proposalId);
      rereadHunks(proposalId);
    } catch (e) {
      setDecisionError(decisionMessage(String(e)));
    } finally {
      setDeciding(false);
    }
  }, [proposalId]);

  /**
   * DCR-FR-24 / DCR-FR-26: the author rewrote a change's proposed text.
   *
   * It rests briefly and then writes itself into proposal storage, exactly as
   * an artifact's own buffer does — and **that write is not a change to the
   * draft**: it touches no file under the draft's `files/` and leaves the
   * draft's `updated_at` where it was (DCP-FR-25).
   */
  const editHunk = useCallback(
    (hunkId: string, after: string) => {
      if (proposalId === null) return;
      const key = hunkKey(proposalId, hunkId);
      const hunk = reviewHunks.find((h) => h.id === hunkId);
      // DCR-FR-25: the buffer is adopted from the agent's own text the first
      // time the author touches it, and never afterwards — a reopened review
      // shows what the author last wrote rather than what the agent composed.
      if (!hunkCandidateBuffers.get(key)) {
        hunkCandidateBuffers.adopt(
          key,
          hunk?.after ?? "",
          hunkReading?.status === "read" ? hunkReading.hunks.checksum : "",
        );
      }
      hunkCandidateBuffers.edit(key, after);
    },
    [proposalId, reviewHunks, hunkReading],
  );

  /**
   * DCR-FR-MTFD: hold the change for discussion and take the author to the
   * composer with the reply addressed to it.
   */
  const discussHunk = useCallback(
    (hunkId: string) => {
      if (proposalId === null) return;
      setFocusedHunk(draftId, hunkId);
      void api
        .setDraftChangeHunkDiscussing(proposalId, hunkId, true)
        .then(() => rereadHunks(proposalId))
        .catch((e) => setDecisionError(decisionMessage(String(e))));
      onDiscussFocus();
    },
    [proposalId, draftId],
  );

  /**
   * DCR-FR-11 / DCR-FR-30: the changes a move steps through.
   *
   * The record's **ledger**, for the same reason Accept all walks it: it is
   * what says which changes are undecided, and it is there before the document
   * that holds them has been read. Built once so the bar's controls and the
   * accelerators step through one list rather than two.
   */
  const movable = useMemo(
    // Ids alone. A move needs to know which changes there are and in what
    // order, and nothing else — and taking more would tie the accelerators'
    // own effect to `reviewHunks`, whose identity changes on every keystroke
    // of a hunk edit, rebinding the window listener as the author types.
    () => undecidedRows.map((row) => ({ id: row.id })),
    [undecidedRows],
  );

  // DCR-FR-30: the accelerators, live only while there is a proposal to review.
  useHunkNavigation({
    draftId,
    hunks: movable,
    focused: focusedHunkId,
    active: pendingProposal !== undefined && !locked,
    onAccept: acceptHunk,
    onReject: rejectHunk,
  });

  /**
   * DCR-FR-31: the changes the document could draw, as it reported them.
   *
   * Empty until the surface has placed anything, which is why the bar reads it
   * only once there are changes to place — before that, "not drawn" would be
   * true of every change and would say nothing.
   */
  const [placedIds, setPlacedIds] = useState<string[]>([]);
  const onPlaced = useCallback((ids: string[]) => {
    setPlacedIds((prev) =>
      prev.length === ids.length && prev.every((id, i) => id === ids[i]) ? prev : ids,
    );
  }, []);
  const undrawnHunkId =
    reviewHunks.length > 0 &&
    focusedHunkId !== null &&
    !placedIds.includes(focusedHunkId)
      ? focusedHunkId
      : null;

  /** DCR-FR-11: the bar's controls move by exactly the accelerators' rule. */
  const moveTo = useCallback(
    (step: 1 | -1) => {
      const next = stepTo(movable, focusedHunkId, step);
      if (next !== null) setFocusedHunk(draftId, next);
    },
    [movable, focusedHunkId, draftId],
  );

  /**
   * DCR-FR-29: the unresolved edit conflict on the change under review, and the
   * two ways out of it.
   *
   * `keep` writes what is on screen over whatever storage holds; `take` adopts
   * storage's text, discarding the author's edit with the conflict. Neither is
   * chosen for them, and until one is the change's decisions stay off.
   */
  const conflict = useMemo(() => {
    if (proposalId === null || focusedHunkId === null) return null;
    const key = hunkKey(proposalId, focusedHunkId);
    if (hunkBufferVersion < 0) return null;
    if (!hunkCandidateBuffers.get(key)?.conflict) return null;
    const stored = reviewHunks.find((h) => h.id === focusedHunkId)?.after ?? "";
    const checksum =
      hunkReading?.status === "read" ? hunkReading.hunks.checksum : "";
    return {
      onKeep: () =>
        hunkCandidateBuffers.resolveCandidate(key, "keep", {
          content: stored,
          checksum,
        }),
      onTake: () =>
        hunkCandidateBuffers.resolveCandidate(key, "take", {
          content: stored,
          checksum,
        }),
    };
    // `hunkBufferVersion` stands in for the mutable buffer store.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [proposalId, focusedHunkId, hunkBufferVersion, reviewHunks, hunkReading]);

  /**
   * DCR-FR-27: what the surface says about the draft, in words.
   *
   * Three states, and the difference between them is the whole point: nothing
   * has happened to the draft yet; the author has rewritten what an agent
   * proposed and *still* nothing has happened to the draft; and a change has
   * landed, which is the only one of the three in which the draft has moved.
   */
  const standing = useMemo(() => {
    const accepted = pendingProposal?.counts?.accepted ?? 0;
    if (accepted > 0) {
      return accepted === 1
        ? "1 change accepted into this draft"
        : `${accepted} changes accepted into this draft`;
    }
    if (reviewHunks.some((h) => h.draft != null && h.draft !== h.after)) {
      return "You have edited a proposed change. The draft has not changed.";
    }
    return "The draft has not changed.";
  }, [pendingProposal, reviewHunks]);

  /** DCR-FR-05: what the editing surface needs to render the review in place. */
  const review: EditorReview | undefined =
    // Whenever a proposal stands, whether or not the changes it holds have been
    // read yet: the review bar is said from the record's own ledger and must be
    // there the moment the tab knows there is something to decide (DCR-FR-11).
    pendingProposal !== undefined
      ? {
          proposalId: pendingProposal.id,
          hunks: reviewHunks,
          focused: focusedHunkId,
          // DCR-FR-29: a change with an unresolved edit is not decided until
          // the author has said which text survives.
          busy: deciding || locked || conflict !== null,
          onFocus: (hunkId) => setFocusedHunk(draftId, hunkId),
          onAccept: acceptHunk,
          onReject: rejectHunk,
          onDiscuss: discussHunk,
          onEdit: editHunk,
          // DCR-FR-11: the review bar belongs between the formatting toolbar
          // and the page, which is inside the editing surface — so it is built
          // here and placed there rather than being drawn above the surface,
          // where it would read as chrome of the tab.
          bar:
            !viewingVersion ? (
              <ReviewBar
                proposal={pendingProposal}
                position={
                  focusedHunkId === null
                    ? 0
                    : hunkPosition(pendingProposal, focusedHunkId)
                }
                busy={deciding || locked}
                error={decisionError}
                conflict={conflict}
                standing={standing}
                // DCR-FR-11: unavailable where there is nowhere to move — the
                // one change left is the one the review is on. Both controls
                // stay live at the ends, because a move rounds them
                // (DCR-FR-30) rather than stopping there.
                // DCR-FR-31: whether the document could draw the change the
                // review is on. The editing surface is what places a change,
                // so it is what reports one it could not place.
                undrawn={undrawnHunkId !== null && undrawnHunkId === focusedHunkId}
                canPrevious={stepTo(movable, focusedHunkId, -1) !== null}
                canNext={stepTo(movable, focusedHunkId, 1) !== null}
                onPrevious={() => moveTo(-1)}
                onNext={() => moveTo(1)}
                onAcceptAll={() => void acceptAll()}
                onRejectAll={() => void rejectAll()}
              />
            ) : null,
          onCaretInProse: () => releaseHunkFocus(draftId),
          onPlaced,
        }
      : undefined;
  return review;
}
