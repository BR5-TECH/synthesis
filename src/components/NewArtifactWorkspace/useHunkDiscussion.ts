/**
 * **Discuss** on one change of a proposal
 * (`../../../specifications/ui/DCR-draft-change-review.md` DCR-FR-MTFD,
 * DCR-FR-JTXE, DCR-FR-VBQN; `DDS-draft-discussion.md` DDS-FR-RHKW).
 *
 * Discuss opens a **new** discussion of the draft about that one change, rather
 * than a reply in the discussion on screen. The opening comment carries one
 * `proposal_hunk` attachment, which is the whole of the link between the change
 * and its discussion (DCP-FR-FHWN). A change that already has a discussion
 * shows that one instead of opening a second.
 *
 * A discussion decides nothing. After it opens, an undecided change is held for
 * discussion; a rejected change stays rejected (DCR-FR-JTXE).
 */
import { useCallback, useState } from "react";

import * as api from "../../api";
import { logDebug, logInfo, logWarn } from "../../logging";
import { setDraftDiscussionHidden, setFocusedHunk } from "../../state/draftDiscussion";
import {
  getDiscussionSession,
  setComposerBody,
} from "../../state/discussionSession";
import { hunkPosition, hunkStateOf } from "../../state/draftProposals";
import { rereadHunks } from "../../state/proposalHunks";
import { hunkDiscussionOf, openingHunkOf } from "../DraftDiscussion/hunkDiscussion";
import type { HunkOpening } from "../DraftDiscussion/OpenDiscussion";
import {
  discussionTargetKey,
  isUndecided,
  type Discussion,
  type DraftChangeProposal,
  type ProposalHunkReference,
} from "../../types";

/**
 * DCR-FR-MTFD: the opening text starts with the proposing agent's tag, so the
 * agent that made the change is the one who answers about it (CTA-FR-QUXJ).
 * Text the author already typed is kept after the tag.
 */
export function seededBody(handle: string, typed: string): string {
  const tag = `@${handle}`;
  if (handle === "" || typed.includes(tag)) return typed;
  return typed === "" ? `${tag} ` : `${tag} ${typed}`;
}

export function useHunkDiscussion(input: {
  draftId: string;
  proposal: DraftChangeProposal | undefined;
  discussions: readonly Discussion[];
  /** Select a discussion in the column. */
  selectDiscussion: (discussionId: string) => void;
  /** Put the caret in the column's composer. */
  focusComposer: () => void;
}) {
  const { draftId, proposal, discussions, selectDiscussion, focusComposer } = input;
  const [opening, setOpening] = useState<HunkOpening | null>(null);

  /** DCR-FR-MTFD / DCR-FR-VBQN: Discuss on one change. */
  const discuss = useCallback(
    (hunkId: string) => {
      if (proposal === undefined) return;
      setFocusedHunk(draftId, hunkId);
      // DDS-FR-RHKW: the column comes back where it is hidden, by the route the
      // action row's own control takes (DDS-FR-XQMF).
      setDraftDiscussionHidden(draftId, false);
      const existing = hunkDiscussionOf(discussions, proposal.id, hunkId);
      if (existing) {
        logInfo(["frontend"], "a change's discussion was shown", {
          draftId,
          proposalId: proposal.id,
          discussionId: existing.id,
        });
        setOpening(null);
        selectDiscussion(existing.id);
        focusComposer();
        return;
      }
      const reference: ProposalHunkReference = {
        kind: "proposalHunk",
        proposalId: proposal.id,
        hunkId,
        draftId,
        path: proposal.path,
      };
      // DCR-FR-MTFD: the opening composer is the draft's own, so the tag goes
      // into the text it already holds rather than into a second field.
      const key = discussionTargetKey({ kind: "draft", draftId });
      const handle = proposal.agent.kind === "agent" ? proposal.agent.handle : "";
      setComposerBody(key, seededBody(handle, getDiscussionSession(key).body));
      setOpening({
        reference,
        position: hunkPosition(proposal, hunkId),
        total: proposal.hunkCount,
      });
      logInfo(["frontend"], "a discussion of a change is being opened", {
        draftId,
        proposalId: proposal.id,
      });
      focusComposer();
    },
    [draftId, proposal, discussions, selectDiscussion, focusComposer],
  );

  /** DDS-FR-RHKW / DDS-FR-YLRM: the author discarded or left the composer. */
  const cancel = useCallback(() => {
    setOpening(null);
    logDebug(["frontend"], "the composer of a change's discussion was left", { draftId });
  }, [draftId]);

  /**
   * DCR-FR-JTXE: the discussion exists, so an undecided change is held for it.
   * A rejected change keeps its rejection, and the hold decides nothing.
   */
  const opened = useCallback(
    (created: Discussion) => {
      const reference = openingHunkOf(created);
      setOpening(null);
      if (reference === undefined || proposal === undefined) return;
      if (reference.proposalId !== proposal.id) return;
      const state = hunkStateOf(proposal, reference.hunkId);
      if (state === undefined || !isUndecided(state) || state === "discussing") return;
      void api
        .setDraftChangeHunkDiscussing(proposal.id, reference.hunkId, true)
        .then(() => rereadHunks(proposal.id))
        .catch((e: unknown) => {
          logWarn(["frontend"], "a change could not be held for its discussion", {
            draftId,
            proposalId: proposal.id,
            error: String(e),
          });
        });
    },
    [draftId, proposal],
  );

  return { opening, discuss, cancel, opened };
}
