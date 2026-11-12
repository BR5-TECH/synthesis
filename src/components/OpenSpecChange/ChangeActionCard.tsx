/**
 * The request card of a change action request
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-QCPB,
 * OCW-FR-CTWP, OCW-FR-GDZT), on the terms of the new-change card
 * (`./NewChangeCard.tsx`).
 *
 * It reads its request from the one reading per change that
 * `../../state/openspec/changeActionRequests.ts` keeps, and the run a started
 * request names from `../../state/openspec/changeRunQueue.ts`. It decides
 * nothing but on Confirm and Decline.
 */
import { useMemo, useRef, useState } from "react";

import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import { requestDiscussionFoot } from "../../state/discussionSession";
import { runTitle, stateLabelOf } from "../../state/graduation";
import { openGraduationRunRoute } from "../../state/graduation/runRoute";
import {
  changeActionRequestOf,
  ensureChangeActionRequests,
  noteChangeActionRequest,
  useChangeActionRequests,
} from "../../state/openspec/changeActionRequests";
import { openOpenSpecChange } from "../../state/openspec/changeRoutes";
import { noticeFailureOf, shortForm, toOpenSpecFailure } from "../../state/openspec/failureNotice";
import { useShortFormLog } from "../../state/openspec/useShortFormLog";
import {
  changeRunById,
  noteStartedRun,
  rereadChangeRunQueue,
  useChangeRunQueue,
} from "../../state/openspec/changeRunQueue";
import {
  CHANGE_ACTION_ERRORS,
  participantName,
  type ChangeAction,
  type ChangeActionOutcome,
  type ChangeActionRequest,
  type ChangeActionRequestReference,
  type ChangeActionRequestState,
  type Comment,
} from "../../types";
import { dispatchDecision } from "../DraftDiscussion/decisionDispatch";
import { TypeChip } from "../DraftDiscussion/stream/TypeChip";
import { formatRelative } from "../ProjectPicker";
import { startRefusal, type StartRefusal } from "./changeRunStart";
import { RefusalStatement } from "./ChangeRunActions";
import { refusalCode } from "./decisions";
import { knownRunTitle } from "./useChangeRuns";

/** OCW-FR-QCPB: each state in words, with its own marker. */
const STATE: Record<ChangeActionRequestState, { word: string; mark: string }> = {
  pending: { word: "Waiting for you", mark: "●" },
  started: { word: "Started", mark: "✓" },
  declined: { word: "Declined", mark: "✕" },
  failed: { word: "Failed", mark: "!" },
};

/** OCW-FR-QCPB: the action of a request in words. */
function actionWords(request: Pick<ChangeActionRequest, "action" | "change">): string {
  return request.action === "apply"
    ? `Apply the change ${request.change}`
    : `Rework the change ${request.change}`;
}

/**
 * OCW-FR-CTWP: why a request failed, in words. An OpenSpec failure takes the
 * short form of its notice (`OFN-openspec-failure-notices.md` OFN-FR-QZEC).
 */
export function failureWords(code: string | null | undefined): string {
  const failure = code ? toOpenSpecFailure(code) : null;
  const sentence = failure ? shortForm(failure) : null;
  if (sentence) return `${sentence} No run started.`;
  switch (code) {
    case "unknown_stream":
      return "The work stream is gone, so no run started.";
    case "change_not_found":
      return "The change is no longer on its stream, so no run started.";
    default:
      return `No run started (${code || "unknown"}).`;
  }
}

/** A refused decision, as the card states it. */
function decisionRefusal(error: unknown, action: ChangeAction): StartRefusal {
  const code = refusalCode(error);
  const words = (sentence: string): StartRefusal => ({
    code,
    sentence,
    runId: null,
    threadId: null,
    failure: null,
  });
  if (code.includes(CHANGE_ACTION_ERRORS.alreadyDecided)) {
    return words("This request has already been decided.");
  }
  if (code.includes(CHANGE_ACTION_ERRORS.requestNotFound)) {
    return words("This request is no longer available.");
  }
  // OCW-FR-CTWP: a refused confirmation on the terms of OCW-FR-EKYF.
  return startRefusal(error, knownRunTitle, action);
}

/**
 * OCW-FR-GDZT: a decision landed. The store takes the request at once, the
 * discussion follows the decision comment, and the agents of the conversation
 * are told on the terms of PCR-FR-14.
 */
function settleDecision(outcome: ChangeActionOutcome, feedback: string): void {
  const { request } = outcome;
  noteChangeActionRequest(request);
  if (outcome.run) noteStartedRun(outcome.run);
  void rereadChangeRunQueue();
  logInfo(["frontend"], "a change action request was decided", {
    streamId: request.streamId,
    requestId: request.id,
    action: request.action,
    state: request.state,
    originKind: outcome.originKind,
  });
  requestDiscussionFoot(request.threadId);
  if (!outcome.commentId) return;
  void dispatchDecision(
    {
      id: request.id,
      threadId: request.threadId,
      target: { kind: "change", streamId: request.streamId, change: request.change },
    },
    { commentId: outcome.commentId },
    undefined,
    [],
    feedback,
  ).catch((e: unknown) => {
    logWarn(["ai", "frontend"], "could not tell the agents what was decided", {
      requestId: request.id,
      error: refusalCode(e).slice(0, 80),
    });
  });
}

export function ChangeActionCard({
  reference,
  comment,
}: {
  reference: ChangeActionRequestReference;
  /** The comment that carries this reference. */
  comment: Comment;
}) {
  useChangeActionRequests(reference.streamId, reference.change);
  useChangeRunQueue();
  const request = changeActionRequestOf(reference.streamId, reference.change, reference.requestId);
  const [feedback, setFeedback] = useState("");
  const [busy, setBusy] = useState(false);
  /** The guard of the decision in flight, so a second press before the next render does nothing. */
  const inFlight = useRef(false);
  const [refusal, setRefusal] = useState<StartRefusal | null>(null);
  const cardRef = useRef<HTMLElement | null>(null);
  // OCW-FR-CTWP: the last refused confirmation stays stated after a re-read.
  // It is read once for each recorded code, so its notice appears once.
  const recorded =
    request?.state === "pending" && request.lastRefusal ? request.lastRefusal : null;
  const action = request?.action ?? reference.action;
  const recordedRefusal = useMemo(
    () => (recorded ? startRefusal(recorded, knownRunTitle, action) : null),
    [recorded, action],
  );
  // OFN-FR-LKAT: a failed request states the short form of its OpenSpec
  // failure (OCW-FR-CTWP), so the card logs that notice once.
  const failedCode = request?.state === "failed" ? (request.lastRefusal ?? null) : null;
  useShortFormLog("change_action_card", failedCode ? noticeFailureOf(failedCode) : null);

  if (!request) {
    return (
      <div className="dds-newdraft-ref" data-testid="change-action-card-unread">
        <TypeChip kind="run" /> {actionWords(reference)}
      </div>
    );
  }

  const decide = (confirm: boolean) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    setRefusal(null);
    const sent = feedback.trim();
    const call = confirm
      ? api.confirmChangeActionRequest(request.id)
      : api.declineChangeActionRequest(request.id, sent === "" ? null : sent);
    call.then(
      (outcome) => {
        inFlight.current = false;
        setBusy(false);
        setFeedback("");
        settleDecision(outcome, confirm ? "" : sent);
        requestAnimationFrame(() => cardRef.current?.focus());
      },
      (e: unknown) => {
        const refused = decisionRefusal(e, request.action);
        logWarn(["frontend"], "a change action request could not be decided", {
          streamId: request.streamId,
          requestId: request.id,
          code: refused.code.slice(0, 80),
        });
        inFlight.current = false;
        setBusy(false);
        setRefusal(refused);
        // CAR-FR-ARMK: a refused confirmation is recorded on the request.
        ensureChangeActionRequests(request.streamId, request.change);
      },
    );
  };

  const state = STATE[request.state];
  const run = request.runId ? changeRunById(request.runId) : undefined;
  const shownRefusal = refusal ?? recordedRefusal;

  return (
    <article
      ref={cardRef}
      tabIndex={-1}
      className="dds-question dds-newdraft openspec-action"
      data-testid="change-action-card"
      data-state={request.state}
      aria-label={`Request: ${actionWords(request)}, ${state.word}`}
    >
      <div className="dds-question__head">
        <TypeChip kind="run" />
        <span className="dds-question__author">{participantName(comment.author)}</span>
        <span className="dds-stream__time" title={comment.createdAt}>
          {formatRelative(comment.createdAt)}
        </span>
      </div>
      <div className="dds-newdraft__title">
        <span className="dds-newdraft__name">{actionWords(request)}</span>
        <span className="dds-newdraft__state" data-state={request.state}>
          <span aria-hidden="true">{state.mark}</span> {state.word}
        </span>
      </div>
      {/* OCW-FR-QCPB: the instruction of a rework, as plain text. */}
      {request.action === "rework" && request.instruction && (
        <p className="openspec-action__instruction" data-testid="change-action-instruction">
          {request.instruction}
        </p>
      )}
      {/* OCW-FR-CTWP: a started request names its run and the run's state. */}
      {request.state === "started" && (
        <p className="openspec-action__run" data-testid="change-action-run">
          <span>
            {run ? `${runTitle(run)} · ${stateLabelOf(run)}` : "The run started."}
          </span>
          {request.runId && (
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              onClick={() => openGraduationRunRoute(request.runId!)}
            >
              Open run
            </button>
          )}
        </p>
      )}
      {request.state === "declined" && (
        <p className="dds-newdraft__reason" data-testid="change-action-declined">
          You declined this request.
        </p>
      )}
      {request.state === "failed" && (
        <p className="dds-newdraft__reason" data-testid="change-action-failed">
          {failureWords(request.lastRefusal)}
        </p>
      )}
      {request.state === "pending" && (
        <div className="dds-newdraft__decide">
          <input
            className="dds-newdraft__feedback"
            type="text"
            value={feedback}
            disabled={busy}
            placeholder="Feedback if you decline (optional)"
            aria-label="Feedback for the agent if you decline (optional)"
            onChange={(e) => setFeedback(e.target.value)}
          />
          <div className="dds-newdraft__actions">
            {/* While a decision is in flight both are disabled but keep their
                focus stop, so a decision by keyboard keeps the focus. */}
            <button
              type="button"
              className="btn btn--primary btn--sm"
              aria-disabled={busy ? "true" : undefined}
              onClick={() => decide(true)}
            >
              Confirm
            </button>
            <button
              type="button"
              className="btn btn--sm"
              aria-disabled={busy ? "true" : undefined}
              onClick={() => decide(false)}
            >
              Decline
            </button>
          </div>
        </div>
      )}
      {shownRefusal && (
        <RefusalStatement
          refusal={shownRefusal}
          surface="change_action_card"
          change={request.change}
          onRetry={() => decide(true)}
          className="dds-newdraft__error"
          testId="change-action-refusal"
        />
      )}
    </article>
  );
}

/**
 * CMT-FR-48: a `change_action_request` reference outside the discussion
 * stream of its change. Activating it opens the change on its stream, where
 * the card stands.
 */
export function ChangeActionReferenceControl({
  attachment,
}: {
  attachment: ChangeActionRequestReference;
}) {
  const name = actionWords(attachment);
  return (
    <button
      type="button"
      className="comment-attach__proposal"
      data-testid="comment-attachment-change-action"
      title={`${name} — show the request in the change`}
      onClick={() =>
        openOpenSpecChange({ kind: "stream", streamId: attachment.streamId }, attachment.change)
      }
    >
      <span className="comment-attach__chip-name">{name}</span>
    </button>
  );
}
