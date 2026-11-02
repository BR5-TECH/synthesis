/**
 * Where a run that stopped to ask something is answered
 * (`../../../specifications/ui/GEA-graduation-escalation-answering.md`).
 *
 * The questions render in the application's one question block
 * (`../QuestionSet/index.tsx`, GEA-FR-QBWN), so an escalation is answered
 * exactly as a discussion's set is. This host supplies what an escalation owns
 * (`DQA-discussion-question-answering.md` DQA-FR-RTWE): the turn that asked as
 * the head (GEA-FR-KMVR), the reason behind a closed disclosure (GEA-FR-SXXB),
 * the merge or update context (GEA-FR-WQZH, GEA-FR-ZRGP), the draft keyed by the
 * owner (GEA-FR-XTUE, GEA-FR-LWQD), and the entry each answer sends
 * (GEA-FR-VWQH, GEA-FR-TSQO).
 *
 * The whole ordered set goes back in one call, and until the backend has
 * accepted it the run is still rendered as waiting on the author (GEA-FR-VIPR).
 */

import { useEffect, useMemo, type ReactNode } from "react";

import * as api from "../../api";
import {
  adoptEscalation,
  forgetRun,
  recallAnswer,
  recallRefusal,
  reconcile,
  rememberAnswer,
  rememberRefusal,
} from "../../state/escalationDrafts";
import {
  composeEscalationAnswers,
  graduationErrorMessage,
} from "../../state/graduation";
import type {
  EscalationAnswerInput,
  GraduationEscalation,
  GraduationEscalationOrigin,
  GraduationRun,
} from "../../types";
import {
  QuestionSetBlock,
  type QuestionSetDrafts,
  type SetQuestion,
} from "../QuestionSet";

import { MergeEscalationContext } from "./merge";

export interface EscalationFormProps {
  /**
   * GEA-FR-XTUE: what the unsent answer draft is held against —
   * a run's id for a run's escalation, a stream's id for an update's.
   */
  ownerId: string;
  escalation: GraduationEscalation;
  busy: boolean;
  /** GEA-FR-VWQH: the whole ordered set, in one call. */
  onSend: (answers: EscalationAnswerInput[]) => Promise<unknown>;
  onAnswered: () => void;
  /**
   * GEA-FR-WQZH, GEA-FR-ZRGP: what a merge run's or an update's escalation
   * additionally names, between the head and the question.
   */
  context?: ReactNode;
  /**
   * How a refusal of this owner's operation reads. A run's refusals and a work
   * stream update's are different vocabularies, and the owner that made the call
   * is the one that can name it.
   */
  describeError?: (reason: string) => string;
  /** What stopped: a run, or a work stream update (GEA-FR-LWQD). */
  subject?: "run" | "update";
}

/** The run-shaped entry point, for the run region. */
export function RunEscalationForm({
  run,
  busy,
  onAnswered,
}: {
  run: GraduationRun;
  busy: boolean;
  onAnswered: () => void;
}) {
  return (
    <EscalationForm
      ownerId={run.id}
      escalation={run.escalation!}
      busy={busy}
      onSend={(answers) => api.answerGraduationEscalation(run.id, answers)}
      onAnswered={onAnswered}
      // GEA-FR-MRDK / GEA-FR-WQZH: a merge run is answered here like any run,
      // and additionally names its unresolved paths and its publication.
      context={run.merge ? <MergeEscalationContext run={run} /> : undefined}
    />
  );
}

/** GEA-FR-KMVR: the turn that asked, from the escalation's recorded origin. */
export function askingTurn(origin: GraduationEscalationOrigin): string {
  switch (origin) {
    case "work":
      return "Work turn";
    case "review":
      return "Review turn";
    case "semantic_merge":
      return "Merge turn";
  }
}

export function EscalationForm({
  ownerId,
  escalation,
  busy,
  onSend,
  onAnswered,
  context,
  describeError = graduationErrorMessage,
  subject = "run",
}: EscalationFormProps) {
  // GEA-FR-KTME: a draft entered against an earlier escalation of this owner is
  // not offered for a later one. Before the draft is read, so no render shows it.
  adoptEscalation(ownerId, escalation.raisedAt);

  // GEA-FR-VBUG: a re-read of the run is reconciled rather than replaced. Every
  // entry whose position the escalation still records is kept against that
  // position, and an entry for a position it no longer records is dropped with
  // the question it was entered against.
  useEffect(() => {
    reconcile(
      ownerId,
      escalation.questions.map((question) => question.position),
    );
  }, [ownerId, escalation]);

  // GEA-FR-DNBI: each proposed response is one option, its summary the label and
  // its description what the rails show. GEA-FR-BHKU: the description is
  // display-only. GEA-FR-SKWP: no internal field name is rendered.
  const questions: SetQuestion[] = useMemo(
    () =>
      escalation.questions.map((question) => ({
        position: question.position,
        text: question.question,
        options: question.options.map((option, index) => ({
          position: index + 1,
          label: option.summary,
          description: option.description,
        })),
      })),
    [escalation.questions],
  );

  // GEA-FR-XTUE, GEA-FR-LWQD: the draft is keyed by the owner and the position.
  const drafts: QuestionSetDrafts = {
    recall: (position) => recallAnswer(ownerId, position),
    remember: (position, answer) => rememberAnswer(ownerId, position, answer),
    recallRefusal: () => recallRefusal(ownerId),
    rememberRefusal: (text) => rememberRefusal(ownerId, text),
  };

  const turn = askingTurn(escalation.origin);

  return (
    <div className="graduation__escalation" data-testid="graduation-escalation">
      <QuestionSetBlock
        // DQA-FR-GVSA: a later escalation of the same run is a set that
        // replaces another, so it opens on its first question.
        setKey={`${ownerId}:${escalation.raisedAt}`}
        questions={questions}
        drafts={drafts}
        label={`Questions from the ${turn.toLowerCase()}`}
        asker={<span className="discussion-questions__handle">{turn}</span>}
        headDetail={
          // GEA-FR-SXXB / ESU-FR-12: the reason, verbatim, behind a disclosure
          // that is closed by default. GRU-FR-MRPE: escaped plain text.
          <details className="discussion-questions__reason" data-testid="graduation-reason">
            <summary>{subject === "update" ? "Why the update stopped" : "Why the run stopped"}</summary>
            <p className="discussion-questions__reason-text">{escalation.reason}</p>
          </details>
        }
        context={context}
        disabled={busy}
        compose={() => composeEscalationAnswers(escalation, drafts.recall)}
        onSubmit={onSend}
        onSubmitted={() => {
          // GEA-FR-KTME: the set is with the backend, so the draft it was
          // composed from has nothing left to protect.
          forgetRun(ownerId);
          onAnswered();
        }}
        describeError={describeError}
        logFields={{ ownerId, origin: escalation.origin }}
      />
    </div>
  );
}
