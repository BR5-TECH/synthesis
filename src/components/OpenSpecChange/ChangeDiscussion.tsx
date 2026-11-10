/**
 * The discussion column of a change workspace on a stream
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-TRLG,
 * OCW-FR-JBFZ; `DDS-draft-discussion.md` DDS-FR-POJC; `CMT-comments.md`
 * CMT-FR-AYSL).
 *
 * The column is the one every draft discussion has: the shared
 * `DiscussionColumn` beside the reading pane, with the splitter. Its owner is
 * the change on its stream, and it holds the whole-change discussion and the
 * fragment discussions of the files of the change (CMS-FR-UEXD).
 */
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";

import * as api from "../../api";
import { useDiscussionControl } from "../../hooks/useDiscussionControl";
import { identityBlockFor } from "../../hooks/useComments";
import { logDebug, logInfo, logWarn } from "../../logging";
import { changeColumn } from "../../state/discussionColumn";
import { requestPendingFocus } from "../../state/discussionFocus";
import {
  getDiscussionSession,
  setComposerBody,
} from "../../state/discussionSession";
import { setFocusedHunk } from "../../state/draftDiscussion";
import { hunkPosition, hunkStateOf, reviewedHunkTotal } from "../../state/draftProposals";
import {
  asReviewable,
  changeReviewKey,
  rereadChangeProposals,
} from "../../state/openspec/changeProposals";
import { rereadHunks } from "../../state/proposalHunks";
import {
  changeDiscussionVersion,
  subscribeChangeDiscussion,
  takeChangeDiscussion,
} from "../../state/openspec/changeRoutes";
import {
  DiscussionColumn,
  Splitter,
  gridTemplate,
  useDiscussionSplit,
} from "../DraftDiscussion";
import { hunkDiscussionOf, openingHunkOf } from "../DraftDiscussion/hunkDiscussion";
import type { HunkOpening } from "../DraftDiscussion/OpenDiscussion";
import { seededBody } from "../NewArtifactWorkspace/useHunkDiscussion";
import {
  discussionTargetKey,
  isUndecided,
  type ChangeEditProposal,
  type Discussion,
  type DiscussionTarget,
  type FragmentTarget,
} from "../../types";

/** CMT-FR-AYSL: a range in the source of one file of the change. */
export interface ChangeSelection {
  path: string;
  start: number;
  end: number;
  quote: string;
}

export function useChangeDiscussion(streamId: string, change: string) {
  const target = useMemo<DiscussionTarget>(
    () => ({ kind: "change", streamId, change }),
    [streamId, change],
  );
  const targetKey = discussionTargetKey(target);
  const control = useDiscussionControl(target, true);
  const discussions = control.discussions;
  const [focusedThreadId, setFocusedThreadId] = useState<string | null>(null);
  const [focusSignal, setFocusSignal] = useState(0);
  const [openingFragment, setOpeningFragment] = useState<FragmentTarget | null>(null);
  const [hunkOpening, setHunkOpening] = useState<HunkOpening | null>(null);
  const column = changeColumn(streamId, change);
  const split = useDiscussionSplit(column);

  /**
   * CVP-FR-TFRD: `revealDiscussion` of a change owner selects the discussion
   * in this column, on mount and while the tab is open.
   */
  const requests = useSyncExternalStore(
    subscribeChangeDiscussion,
    changeDiscussionVersion,
    changeDiscussionVersion,
  );
  useEffect(() => {
    const wanted = takeChangeDiscussion(targetKey);
    if (wanted === null) return;
    logDebug(["frontend"], "a change workspace selects a revealed discussion", {
      streamId,
      discussionId: wanted,
    });
    setOpeningFragment(null);
    setHunkOpening(null);
    setFocusedThreadId(wanted);
  }, [requests, targetKey, streamId]);

  const columnDiscussions = useMemo(
    () => discussions.discussions.map((d) => d.thread),
    [discussions.discussions],
  );

  /**
   * OCW-FR-JBFZ / CMT-FR-AYSL: a selection in the Source of a file opens the
   * composer of a fragment discussion, with the change-relative path.
   */
  const commentOn = useCallback(
    (selection: ChangeSelection) => {
      setHunkOpening(null);
      setOpeningFragment({
        owner: target,
        path: selection.path,
        start: selection.start,
        end: selection.end,
        quote: selection.quote,
      });
      setFocusSignal((n) => n + 1);
      logDebug(["frontend"], "a fragment discussion of a change is being opened", {
        streamId,
        path: selection.path,
      });
    },
    [target, streamId],
  );

  /**
   * DCR-FR-MTFD / PCR-FR-HDMC: Discuss on one hunk. A hunk that has a
   * discussion shows it. Any other hunk opens the composer of a new one, whose
   * opening comment names the hunk (CMS-FR-YQAX).
   */
  const discussHunk = useCallback(
    (proposal: ChangeEditProposal | undefined, hunkId: string) => {
      if (proposal === undefined) return;
      const reviewable = asReviewable(proposal);
      setFocusedHunk(changeReviewKey(streamId, change), hunkId);
      const existing = hunkDiscussionOf(columnDiscussions, proposal.id, hunkId);
      if (existing) {
        setHunkOpening(null);
        setFocusedThreadId(existing.id);
        setFocusSignal((n) => n + 1);
        return;
      }
      const path = proposal.hunks.find((h) => h.id === hunkId)?.path ?? "";
      const handle = proposal.agent.kind === "agent" ? proposal.agent.handle : "";
      setComposerBody(targetKey, seededBody(handle, getDiscussionSession(targetKey).body));
      setOpeningFragment(null);
      setHunkOpening({
        reference: {
          kind: "proposalHunk",
          proposalId: proposal.id,
          hunkId,
          change: { streamId, change },
          path,
        },
        position: hunkPosition(reviewable, hunkId),
        total: reviewedHunkTotal(reviewable),
      });
      setFocusSignal((n) => n + 1);
      logInfo(["frontend"], "a discussion of a hunk of a change is being opened", {
        streamId,
        proposalId: proposal.id,
      });
    },
    [streamId, change, targetKey, columnDiscussions],
  );

  /**
   * DCR-FR-JTXE: the discussion of a hunk exists, so an undecided hunk is held
   * for it. A rejected hunk keeps its rejection.
   */
  const holdOpenedHunk = useCallback(
    (created: Discussion, proposal: ChangeEditProposal | undefined) => {
      const reference = openingHunkOf(created);
      if (reference === undefined || proposal === undefined) return;
      if (reference.proposalId !== proposal.id) return;
      const state = hunkStateOf(asReviewable(proposal), reference.hunkId);
      if (state === undefined || !isUndecided(state) || state === "discussing") return;
      void api
        .setChangeEditHunkDiscussing(proposal.id, reference.hunkId, true)
        .then(() => {
          rereadHunks(proposal.id, "change");
          void rereadChangeProposals(streamId, change);
        })
        .catch((e: unknown) => {
          logWarn(["frontend"], "a hunk of a change could not be held for its discussion", {
            streamId,
            proposalId: proposal.id,
            error: String(e),
          });
        });
    },
    [streamId, change],
  );

  return {
    target,
    control,
    discussions,
    columnDiscussions,
    focusedThreadId,
    setFocusedThreadId,
    focusSignal,
    openingFragment,
    setOpeningFragment,
    hunkOpening,
    setHunkOpening,
    column,
    split,
    commentOn,
    discussHunk,
    holdOpenedHunk,
  };
}

export type ChangeDiscussion = ReturnType<typeof useChangeDiscussion>;

/**
 * DDS-FR-POJC / OCW-FR-TRLG: the reading pane as the document column, the
 * splitter, and the discussion column, as one grid.
 */
export function ChangeDiscussionSplit({
  discussion,
  change,
  pending,
  onFocusFragment,
  children,
}: {
  discussion: ChangeDiscussion;
  change: string;
  pending: ChangeEditProposal | undefined;
  /** CMT-FR-28: the quote of a fragment discussion was activated. */
  onFocusFragment: (fragment: FragmentTarget) => void;
  children: ReactNode;
}) {
  const { split, discussions } = discussion;
  const identityBlock = identityBlockFor(discussions.identityError);
  return (
    <div
      className="dds-split openspec-change__split"
      ref={split.splitRef}
      style={{
        gridTemplateColumns: split.view.hidden ? "1fr" : gridTemplate(split.view.ratio),
      }}
      data-discussion={split.view.hidden ? "hidden" : "shown"}
      data-testid="openspec-change-split"
    >
      <div className="openspec-change__doc">{children}</div>
      <Splitter
        ratio={split.view.ratio}
        onChange={split.setRatio}
        boundsRef={split.splitRef}
        hidden={split.view.hidden}
      />
      <DiscussionColumn
        hidden={split.view.hidden}
        column={discussion.column}
        target={discussion.target}
        noun="change"
        material="change"
        testId="openspec-change-discussion"
        discussions={discussion.columnDiscussions}
        selectedThreadId={discussion.focusedThreadId}
        onSelectThread={discussion.setFocusedThreadId}
        identity={discussions.identity}
        identityBlock={
          identityBlock ? { message: identityBlock.message, route: identityBlock.route } : null
        }
        agents={discussions.agents}
        onReply={discussions.reply}
        onSetResolved={discussions.setResolved}
        errors={discussions.errors}
        blocked={false}
        anchor={null}
        onOpenDiscussion={async (request) => {
          const created = await discussions.open(
            request.body,
            request.attachments,
            request.fragmentTarget,
          );
          return created;
        }}
        onOpened={(created) => {
          discussion.setOpeningFragment(null);
          discussion.setHunkOpening(null);
          requestPendingFocus(created.id, "composer");
          discussion.setFocusedThreadId(created.id);
          discussion.holdOpenedHunk(created, pending);
        }}
        openingHunk={discussion.hunkOpening}
        onCancelHunk={() => discussion.setHunkOpening(null)}
        openingFragment={discussion.openingFragment}
        onCancelFragment={() => discussion.setOpeningFragment(null)}
        onFocusFragment={(fragment) => onFocusFragment(fragment)}
        ownerLabel={change}
        focusSignal={discussion.focusSignal}
      />
    </div>
  );
}
