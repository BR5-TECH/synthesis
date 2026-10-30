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
 *
 * PCR-FR-HDMC / DDS-FR-EHDK: the Editor tab of a prompt or DOC file takes the
 * same path. Its target is the artifact, its reference carries `artifactId`,
 * and the hold is `set_prompt_change_hunk_discussing`.
 */
import { useCallback, useState } from "react";

import * as api from "../../api";
import { logDebug, logInfo, logWarn } from "../../logging";
import {
  artifactReviewKey,
  setDraftDiscussionHidden,
  setFocusedHunk,
} from "../../state/draftDiscussion";
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
  type DiscussionTarget,
  type HunkLedgerRow,
  type Participant,
  type ProposalHunkReference,
} from "../../types";

/** DCR-FR-GNRL: what a hunk discussion reads of a proposal, of either kind. */
export interface DiscussableProposal {
  id: string;
  path: string;
  agent: Participant;
  hunkCount: number;
  ledger: HunkLedgerRow[];
}

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
  /** The draft whose tab this is; absent for a file's Editor tab. */
  draftId?: string;
  /** PCR-FR-HDMC: the prompt or DOC file whose Editor tab this is. */
  artifactId?: string;
  proposal: DiscussableProposal | undefined;
  discussions: readonly Discussion[];
  /** Select a discussion in the column. */
  selectDiscussion: (discussionId: string) => void;
  /** Put the caret in the column's composer. */
  focusComposer: () => void;
  /** Show the discussion column where it is hidden. Defaults to the draft's. */
  showColumn?: () => void;
}) {
  const { draftId, artifactId, proposal, discussions, selectDiscussion, focusComposer } = input;
  const showColumnInput = input.showColumn;
  const [opening, setOpening] = useState<HunkOpening | null>(null);
  /** The key of the review state, and the target a new discussion is of. */
  const reviewKey = artifactId ? artifactReviewKey(artifactId) : (draftId ?? "");
  const target: DiscussionTarget = artifactId
    ? { kind: "artifact", artifactId }
    : { kind: "draft", draftId: draftId ?? "" };
  const targetKey = discussionTargetKey(target);

  /** DCR-FR-MTFD / DCR-FR-VBQN: Discuss on one change. */
  const discuss = useCallback(
    (hunkId: string) => {
      if (proposal === undefined) return;
      setFocusedHunk(reviewKey, hunkId);
      // DDS-FR-RHKW: the column comes back where it is hidden, by the route the
      // action row's own control takes (DDS-FR-XQMF).
      if (showColumnInput) showColumnInput();
      else if (draftId) setDraftDiscussionHidden(draftId, false);
      const existing = hunkDiscussionOf(discussions, proposal.id, hunkId);
      if (existing) {
        logInfo(["frontend"], "a change's discussion was shown", {
          draftId,
          artifactId,
          proposalId: proposal.id,
          discussionId: existing.id,
        });
        setOpening(null);
        selectDiscussion(existing.id);
        focusComposer();
        return;
      }
      // CMS-FR-TQVB: exactly one of `draftId` and `artifactId`.
      const reference: ProposalHunkReference = artifactId
        ? { kind: "proposalHunk", proposalId: proposal.id, hunkId, artifactId, path: proposal.path }
        : { kind: "proposalHunk", proposalId: proposal.id, hunkId, draftId, path: proposal.path };
      // DCR-FR-MTFD: the opening composer is the target's own, so the tag goes
      // into the text it already holds rather than into a second field.
      const handle = proposal.agent.kind === "agent" ? proposal.agent.handle : "";
      setComposerBody(targetKey, seededBody(handle, getDiscussionSession(targetKey).body));
      setOpening({
        reference,
        position: hunkPosition(proposal, hunkId),
        total: proposal.hunkCount,
      });
      logInfo(["frontend"], "a discussion of a change is being opened", {
        draftId,
        artifactId,
        proposalId: proposal.id,
      });
      focusComposer();
    },
    [
      draftId,
      artifactId,
      reviewKey,
      targetKey,
      proposal,
      discussions,
      selectDiscussion,
      focusComposer,
      showColumnInput,
    ],
  );

  /** DDS-FR-RHKW / DDS-FR-YLRM: the author discarded or left the composer. */
  const cancel = useCallback(() => {
    setOpening(null);
    logDebug(["frontend"], "the composer of a change's discussion was left", {
      draftId,
      artifactId,
    });
  }, [draftId, artifactId]);

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
      // PCR-FR-HDMC: a change of a file's proposal is held by the prompt
      // module's own command.
      const hold = artifactId
        ? api.setPromptChangeHunkDiscussing(proposal.id, reference.hunkId, true)
        : api.setDraftChangeHunkDiscussing(proposal.id, reference.hunkId, true);
      void hold
        .then(() => rereadHunks(proposal.id, artifactId ? "prompt" : "draft"))
        .catch((e: unknown) => {
          logWarn(["frontend"], "a change could not be held for its discussion", {
            draftId,
            artifactId,
            proposalId: proposal.id,
            error: String(e),
          });
        });
    },
    [draftId, artifactId, proposal],
  );

  return { opening, discuss, cancel, opened };
}
