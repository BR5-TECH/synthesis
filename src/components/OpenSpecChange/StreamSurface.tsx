/**
 * What a stream location adds to the change workspace
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-TRLG,
 * OCW-FR-COLR, OCW-FR-EJGH, OCW-FR-RKCC, OCW-FR-DRQD, OCW-FR-BGDN,
 * OCW-FR-ANKT, OCW-FR-TZRT, OCW-FR-JHAB, OCW-FR-NWRS, OCW-FR-XJMR).
 *
 * The discussion column beside the reading pane, the review of the pending
 * edit proposal with its bar, the History rail and the version it shows, and
 * the split of the change with its Confirm split.
 */
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";

import { pendingChangeProposalOf } from "../../state/openspec/changeProposals";
import { useNewChangeProposals } from "../../state/openspec/newChangeProposals";
import type { ChangeDetail, FragmentTarget } from "../../types";
import { railStacksAt, useWidthRule } from "./ArtifactNav";
import { ChangeDiscussionSplit, useChangeDiscussion } from "./ChangeDiscussion";
import { ChangeHistoryRail } from "./ChangeHistoryRail";
import { ChangeReviewBar } from "./ChangeReviewBar";
import { ConfirmSplitBar, SplitMarker, offersConfirmSplit, useChangeSplit } from "./ChangeSplit";
import { useChangeHistory } from "./useChangeHistory";
import { PLAN_QUESTIONS_FIRST, useChangeReview } from "./useChangeReview";
import { VersionView } from "./VersionView";

/** What a stream location lends its reading pane. */
export interface StreamReview {
  review: ReturnType<typeof useChangeReview>;
  onComment: ReturnType<typeof useChangeDiscussion>["commentOn"];
}

/**
 * Move the focus to the reading pane of the stage when a part of the stage
 * went away and took the focus with it to the document body: the version
 * view, the review bar, or the split bar. A focus on any other control stays
 * where it is.
 */
function useFocusAfterParts(
  stage: HTMLElement | null,
  version: boolean,
  review: boolean,
  split: boolean,
) {
  const before = useRef({ version, review, split });
  useEffect(() => {
    const was = before.current;
    before.current = { version, review, split };
    // A bar that a shown version hides has not gone away.
    const gone =
      (was.version && !version) ||
      (!version && !was.version && ((was.review && !review) || (was.split && !split)));
    if (!gone || stage === null) return;
    const active = document.activeElement;
    if (active !== null && active !== document.body) return;
    stage.querySelector<HTMLElement>(".openspec-change__reading[tabindex]")?.focus();
  }, [stage, version, review, split]);
}

export function StreamSurface({
  streamId,
  change,
  detail,
  historyOpen,
  strip,
  onDecided,
  onFocusFragment,
  render,
}: {
  streamId: string;
  change: string;
  /** The last read of the current change, or null before the first. */
  detail: ChangeDetail | null;
  /** OCW-FR-RKCC: the History rail is open. */
  historyOpen: boolean;
  /**
   * OCW-FR-JHAB / OCW-FR-NWRS: the run strip, over the full width of the
   * document column, or null.
   */
  strip: ReactNode;
  onDecided: () => void;
  onFocusFragment: (fragment: FragmentTarget) => void;
  render: (stream: StreamReview) => ReactNode;
}) {
  const discussion = useChangeDiscussion(streamId, change);
  const { discussHunk, discussNewChange } = discussion;
  const onDiscussHunk = useCallback(
    (hunkId: string) => discussHunk(pendingChangeProposalOf(streamId, change), hunkId),
    [discussHunk, streamId, change],
  );
  const review = useChangeReview({
    streamId,
    change,
    onDecided,
    onDiscussHunk,
    agents: discussion.discussions.agents,
  });
  const history = useChangeHistory(streamId, change, historyOpen);
  const split = useChangeSplit(streamId, change, discussNewChange);
  const confirmed = Boolean(split.split?.confirmedAt);
  // OCW-FR-DRQD: while a version is shown, nothing of the current change is
  // decided.
  const version = detail && history.shown ? history.shown : null;
  const reviewShown = Boolean(detail && review.pending) && version === null;
  const splitShown = Boolean(detail && offersConfirmSplit(split.proposals, confirmed)) && version === null;

  // OCW-FR-GCME: the History rail moves above the document where the reading
  // pane beside it would be narrower than 360 px.
  const { applies: railAbove, ref: measureStage } = useWidthRule(railStacksAt);
  const [stage, setStage] = useState<HTMLElement | null>(null);
  const stageRef = useCallback(
    (element: HTMLElement | null) => {
      measureStage(element);
      setStage(element);
    },
    [measureStage],
  );
  useFocusAfterParts(stage, version !== null, reviewShown, splitShown);

  return (
    <ChangeDiscussionSplit
      discussion={discussion}
      change={change}
      pending={review.pending}
      historyOpen={historyOpen}
      onFocusFragment={onFocusFragment}
    >
      {strip}
      <div
        className="openspec-change__stage"
        ref={stageRef}
        data-rail={historyOpen ? (railAbove ? "above" : "beside") : undefined}
        data-testid="openspec-change-stage"
      >
        {historyOpen && <ChangeHistoryRail history={history} />}
        <div className="openspec-change__main">
          {version && detail ? (
            <VersionView shown={version} detail={detail} onBack={history.back} />
          ) : (
            <>
              {/* OCW-FR-EJGH: the review bar above the reading pane. */}
              {reviewShown && (
                <ChangeReviewBar
                  undecided={review.undecided}
                  onRejectAll={review.rejectAll}
                  // OCW-FR-XJMR: the plan questions are answered first.
                  held={review.planHeld ? PLAN_QUESTIONS_FIRST : null}
                />
              )}
              {/* OCW-FR-ANKT: Confirm split, while the split waits for it. */}
              {splitShown && (
                <ConfirmSplitBar streamId={streamId} change={change} proposals={split.proposals} />
              )}
              {render({ review, onComment: discussion.commentOn })}
            </>
          )}
        </div>
      </div>
    </ChangeDiscussionSplit>
  );
}

/** OCW-FR-TZRT: the split marker of the header, where the split is confirmed. */
export function StreamSplitMarker({ streamId, change }: { streamId: string; change: string }) {
  const { split } = useNewChangeProposals(streamId, change);
  if (!split?.confirmedAt || split.accepted.length === 0) return null;
  return <SplitMarker streamId={streamId} names={split.accepted} />;
}
