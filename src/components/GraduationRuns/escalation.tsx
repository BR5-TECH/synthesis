/**
 * Where a run that stopped to ask something is answered
 * (`../../../specifications/ui/GEA-graduation-escalation-answering.md`).
 *
 * The reason the run raised, then every question it recorded in the order it
 * recorded them (GEA-FR-SXXB). Each question's proposed responses and its own
 * free-text field are rows of **one** answer group rather than a list with a
 * field beside it (GEA-FR-ZSDY), so what is selected is the answer
 * (GEA-FR-UWLK) rather than something inferred from which field holds a value.
 *
 * What the author has entered and not yet sent is held outside this component
 * (`../../state/escalationDrafts.ts`), so a reload, a look at the agent output,
 * a move to another run and back, and a refusal of the whole set all leave the
 * work where it was (GEA-FR-YFMY).
 *
 * The whole ordered set goes back in one call (GEA-FR-VWQH), and until the
 * backend has accepted it the run is still rendered as waiting on the author
 * (GEA-FR-VIPR).
 *
 * GEA-FR-LWQD: it serves a work stream update's escalation on the same terms. An
 * update belongs to a stream and to no run, so what this component is given is
 * the recorded escalation, an owner id to hold the draft against, and the send
 * that belongs to whichever record holds it — never a run. A merge run's
 * escalation is a run's (GEA-FR-MRDK) and is answered here through the run.
 */

import { useEffect, useReducer, useState, type ReactNode } from "react";

import * as api from "../../api";
import { logError } from "../../logging";
import {
  forgetRun,
  recallAnswer,
  reconcile,
  rememberAnswer,
  type DraftAnswer,
} from "../../state/escalationDrafts";
import {
  answeredCount,
  composeAnswers,
  graduationErrorMessage,
} from "../../state/graduation";
import type {
  EscalationAnswerInput,
  GraduationEscalation,
  GraduationEscalationQuestion,
  GraduationRun,
} from "../../types";

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
  onError: (message: string) => void;
  /**
   * GEA-FR-WQZH: what a merge run's escalation additionally names, rendered
   * under the reason and above the questions.
   */
  context?: ReactNode;
  /**
   * How a refusal of this owner's operation reads.
   *
   * A run's refusals and a work stream update's are different vocabularies, and
   * a code one of them does not know renders as the bare code. The owner that
   * made the call is the one that can name it.
   */
  describeError?: (reason: string) => string;
}

/** The run-shaped entry point, for the run region that already had one. */
export function RunEscalationForm({
  run,
  busy,
  onAnswered,
  onError,
}: {
  run: GraduationRun;
  busy: boolean;
  onAnswered: () => void;
  onError: (message: string) => void;
}) {
  return (
    <EscalationForm
      ownerId={run.id}
      escalation={run.escalation!}
      busy={busy}
      onSend={(answers) => api.answerGraduationEscalation(run.id, answers)}
      onAnswered={onAnswered}
      onError={onError}
      // GEA-FR-MRDK / GEA-FR-WQZH: a merge run is answered here like any run,
      // and additionally names its unresolved paths and its publication.
      context={run.merge ? <MergeEscalationContext run={run} /> : undefined}
    />
  );
}

export function EscalationForm({
  ownerId,
  escalation,
  busy,
  onSend,
  onAnswered,
  onError,
  context,
  describeError = graduationErrorMessage,
}: EscalationFormProps) {
  // The draft lives outside this component, so a write to it is what the
  // render has to be told about.
  const [, redraw] = useReducer((count: number) => count + 1, 0);
  const [sending, setSending] = useState(false);
  const runId = ownerId;

  // A set in flight belongs to the run it was composed for: the region hands
  // this form the run the author selects, and a call still running for the run
  // before it must not hold the new one's controls closed.
  useEffect(() => {
    setSending(false);
  }, [runId]);

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

  const answers: Record<number, string> = {};
  const summaries: Record<number, string> = {};
  for (const question of escalation.questions) {
    const draft = recallAnswer(ownerId, question.position);
    const answer = effectiveAnswer(question, draft);
    if (answer !== "") answers[question.position] = answer;
    const summary = effectiveSummary(question, draft);
    if (summary !== null) summaries[question.position] = summary;
  }
  const composed = composeAnswers(escalation, answers, summaries);
  const answered = answeredCount(answers, escalation);
  const outstanding = escalation.questions.length - answered;
  const locked = busy || sending;

  const write = (position: number, draft: DraftAnswer) => {
    rememberAnswer(ownerId, position, draft);
    redraw();
  };

  return (
    <section className="graduation__escalation" data-testid="graduation-escalation">
      {/* GEA-FR-SXXB / ESU-FR-12: the reason, verbatim. GRU-FR-MRPE: escaped
          plain text, with no Markdown and no embedded markup. */}
      <p className="graduation__reason">{escalation.reason}</p>

      {/* GEA-FR-WQZH: a merge run's affected paths and publication choice. */}
      {context}

      {escalation.questions.map((question, index) => {
        const draft = recallAnswer(ownerId, question.position);
        const alone = question.options.length === 0;
        return (
          <div
            className="graduation__question"
            key={question.position}
            data-testid="graduation-question"
          >
            <div className="graduation__question-head">
              <span className="graduation__question-count">
                {/* Counted over the set as it was recorded. A question's
                    `position` is a stable id rather than an ordinal, so a set
                    recorded at positions 2 and 5 would otherwise read as
                    "Question 5 of 2". */}
                Question {index + 1} of {escalation.questions.length}
              </span>
            </div>
            <fieldset className="graduation__answer">
              <legend className="graduation__ask">{question.question}</legend>

              {/* GEA-FR-DNBI: each proposed response as its summary, with its
                  description as supporting text beneath. GEA-FR-BHKU: the
                  description is display-only and is submitted nowhere. */}
              {question.options.map((option) => (
                <label
                  className="graduation__choice"
                  key={option.answer}
                  data-selected={
                    !draft.ownWords && draft.selected === option.answer
                      ? "true"
                      : undefined
                  }
                >
                  <input
                    type="radio"
                    name={`graduation-answer-${ownerId}-${question.position}`}
                    checked={!draft.ownWords && draft.selected === option.answer}
                    disabled={locked}
                    // GEA-FR-GCTA: choosing a response leaves the words the
                    // author typed where they were.
                    onChange={() =>
                      write(question.position, {
                        ...draft,
                        selected: option.answer,
                        ownWords: false,
                      })
                    }
                  />
                  <span className="graduation__option">
                    <span className="graduation__option-summary">
                      {option.summary}
                    </span>
                    {option.description && (
                      <span className="graduation__option-description">
                        {option.description}
                      </span>
                    )}
                  </span>
                </label>
              ))}

              {/* GEA-FR-ZGHY / GEA-FR-LRCD: a question recorded with no
                  proposed responses has this row and no other, so its text
                  answers it whether or not a choice was ever pressed — there is
                  nothing else it could have meant. */}
              <div
                className="graduation__choice graduation__choice--own"
                data-selected={draft.ownWords ? "true" : undefined}
                data-alone={alone ? "true" : undefined}
              >
                {!alone && (
                  <label className="graduation__own-head">
                    <input
                      type="radio"
                      name={`graduation-answer-${ownerId}-${question.position}`}
                      checked={draft.ownWords}
                      disabled={locked}
                      onChange={() =>
                        write(question.position, { ...draft, ownWords: true })
                      }
                    />
                    <span>In my own words</span>
                  </label>
                )}
                <textarea
                  className="draft-review__feedback graduation__typed"
                  aria-label={`Your answer to question ${question.position}`}
                  value={draft.typed}
                  disabled={locked}
                  // GEA-FR-UEFS: typing selects this row, and GEA-FR-GCTA: it
                  // clears no proposed response the author had chosen.
                  onChange={(event) =>
                    write(question.position, {
                      ...draft,
                      typed: event.target.value,
                      ownWords: true,
                    })
                  }
                />
              </div>
            </fieldset>
          </div>
        );
      })}

      <div className="graduation__question-foot">
        {/* GEA-FR-YMLV / GEA-FR-FZMW: which of the set are answered, and how
            many still need one, so a disabled control is never a mystery. */}
        <span
          className="graduation__question-count"
          data-testid="graduation-answered-count"
        >
          {answered} of {escalation.questions.length} answered
          {outstanding > 0 &&
            ` · ${outstanding === 1 ? "1 question needs" : `${outstanding} questions need`} an answer`}
        </span>
        <span className="spacer" />
        <button
          type="button"
          className="btn btn--sm btn--primary"
          // GEA-FR-FFKD: enabled only once every recorded question is answered.
          disabled={locked || composed === null}
          data-testid="graduation-send-answers"
          onClick={() => {
            if (!composed) return;
            setSending(true);
            void onSend(composed)
              .then(() => {
                // The set is with the backend, so the draft it was composed
                // from has nothing left to protect.
                forgetRun(ownerId);
                onAnswered();
              })
              // GEA-FR-TEMG: a refusal leaves every entered answer intact, so
              // the author corrects what was refused rather than typing the
              // whole set again.
              .catch((reason) => {
                logError(["frontend"], "the answers were refused", {});
                onError(describeError(String(reason)));
              })
              .finally(() => setSending(false));
          }}
        >
          Send answers
        </button>
      </div>
    </section>
  );
}

/**
 * GEA-FR-UWLK: the answer is what the selected row holds.
 *
 * GEA-FR-LRCD: a question with no proposed responses is answered by its text
 * whether or not its row was ever pressed.
 */
function effectiveAnswer(
  question: GraduationEscalationQuestion,
  draft: DraftAnswer,
): string {
  if (question.options.length === 0) return draft.typed.trim() === "" ? "" : draft.typed;
  if (draft.ownWords) return draft.typed.trim() === "" ? "" : draft.typed;
  const chosen = question.options.find(
    (option) => option.answer === draft.selected,
  );
  return chosen ? chosen.answer : "";
}

/**
 * GEA-FR-TSQO: a selected response submits its own summary. The author's own
 * words carry none, so the operation makes one from the text.
 */
function effectiveSummary(
  question: GraduationEscalationQuestion,
  draft: DraftAnswer,
): string | null {
  if (draft.ownWords) return null;
  const chosen = question.options.find(
    (option) => option.answer === draft.selected,
  );
  return chosen ? chosen.summary : null;
}
