/**
 * Where the author answers a discussion agent that asked several things at once
 * (`../../../specifications/ui/DQA-discussion-question-answering.md`).
 *
 * The block sits between the conversation's message list and its composer, in
 * every presentation that renders a discussion (DQA-FR-NRZB). It is **outside
 * the discussion's history** until the author submits (DQA-FR-TVMH): it
 * contributes no comment, occupies no position in the message list, and no
 * reader of the conversation sees it.
 *
 * It is answered **one question at a time**, with pagination both ways
 * (DQA-FR-BXHU), because a set presented all at once is a form rather than a
 * conversation, and because an author who has answered six of seven should see
 * which one is still open.
 *
 * It is an in-panel block and never a floating overlay and never an application
 * modal (DQA-FR-CKYP): it takes no focus trap, and the rest of the window stays
 * operable while it stands.
 *
 * What the author has entered and not yet sent is held outside this component
 * (`../../state/questionAnswerDrafts.ts`), so paging, a re-read, a re-render, a
 * move between presentations, and a refusal of the whole set all leave the work
 * where it was (DQA-FR-LSNW).
 *
 * The shape follows `../GraduationRuns/escalation.tsx` deliberately: an author
 * who has answered a graduation run's questions already knows how to answer
 * these.
 */
import { useEffect, useReducer, useRef, useState } from "react";

import { logError } from "../../logging";
import {
  recallAnswer,
  rememberAnswer,
  type QuestionDraftAnswer,
} from "../../state/questionAnswerDrafts";
import {
  participantName,
  participantTitle,
  type PendingQuestionSet,
  type QuestionAnswerInput,
} from "../../types";
import { composeAnswers, isAnswered, progressLine } from "./answers";
import { stripCitationMarkers } from "../../text/citationMarkers";

export interface DiscussionQuestionsProps {
  set: PendingQuestionSet;
  /** DQA-FR-KDVU: the whole ordered set of answers, in one call. */
  onSubmit: (answers: QuestionAnswerInput[]) => Promise<unknown>;
  /** DQA-FR-IPFD: the submission was accepted, so the block goes. */
  onSubmitted: () => void;
  /** DQA-FR-WKTP: how a typed refusal reads, rendered at the foot of the block. */
  describeError: (reason: string) => string;
}

export function DiscussionQuestions({
  set,
  onSubmit,
  onSubmitted,
  describeError,
}: DiscussionQuestionsProps) {
  // The draft lives outside this component, so a write to it is what the render
  // has to be told about.
  const [, redraw] = useReducer((count: number) => count + 1, 0);
  const [page, setPage] = useState(0);
  const [sending, setSending] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const questionRef = useRef<HTMLFieldSetElement | null>(null);
  const ownFieldRef = useRef<HTMLTextAreaElement | null>(null);
  const noteFieldRef = useRef<HTMLTextAreaElement | null>(null);
  /** Set when the author chooses the row that opens a field, so focus follows. */
  const opened = useRef(false);
  /** DQA-FR-AYNP: held outside state, so a second activation in the same tick sees it. */
  const sendingRef = useRef(false);
  // DQA-FR-TFCE: focus moves with the page, but not on the first render — the
  // block appearing must not take the keyboard away from whatever the author is
  // doing (DQA-FR-CKYP).
  const paged = useRef(false);

  // DQA-FR-GVSA: a set the author has not paged shows its first question, and a
  // set that replaces another does the same — a page belongs to the set it was
  // turned in.
  useEffect(() => {
    setPage(0);
    setRefusal(null);
    paged.current = false;
  }, [set.setId]);

  // DQA-FR-DYFR: a re-read that records fewer questions must not leave the
  // block showing a page that is no longer there.
  const total = set.questions.length;
  const current = Math.min(page, Math.max(total - 1, 0));
  const question = set.questions[current];

  // DQA-FR-FCZL: the own-answer field takes the keyboard the moment its row is
  // chosen, so writing follows choosing without a second act. It runs on the
  // press alone, never on a page turn, which is what keeps DQA-FR-TFCE's focus
  // on the question that arrived.
  useEffect(() => {
    if (opened.current) ownFieldRef.current?.focus();
    opened.current = false;
  });

  useEffect(() => {
    if (!paged.current) return;
    // DQA-FR-TFCE: the keyboard lands in the question that arrived rather than
    // staying on the control that moved it, because Previous and Next disable
    // themselves at the two ends of the set.
    questionRef.current?.focus();
  }, [current]);

  // Read before the early return below, because the effect after it is a hook:
  // a re-read that records no questions at all takes that return (DQA-FR-DYFR),
  // and a hook the component reaches on some renders and not others changes the
  // order React matches them by. Position 0 is no recorded position (per
  // `../../../specifications/tools/ADQ-ask-discussion-questions-tool.md`
  // ADQ-FR-TWNS, which counts from 1), so this reads as the empty draft.
  const draft = recallAnswer(set.setId, question?.position ?? 0);

  /**
   * Both fields are one line until there is more than one line in them, and
   * grow to the cap the stylesheet sets before they start to scroll.
   *
   * The kit's optional field (`.draft-review__feedback`) is drawn as a rule to
   * write on rather than as a box, and its height is the writer's to set from
   * the content — `../PromptChangeReview.tsx` does the same for the same field.
   * Without it the field stays at two lines while the author's own answer runs
   * to ten, and they read their answer through a slot.
   *
   * It runs when what is written changes and when a field arrives — choosing the
   * own answer or an option renders one that was not there — rather than on
   * every render: each pass forces two reflows per field, and a page turn or a
   * redraw changes neither field's content.
   */
  useEffect(() => {
    for (const field of [ownFieldRef.current, noteFieldRef.current]) {
      if (field === null) continue;
      field.style.height = "auto";
      const border = field.offsetHeight - field.clientHeight;
      field.style.height = `${field.scrollHeight + border}px`;
    }
  }, [draft.typed, draft.note, draft.ownWords, draft.selected, current]);

  if (!question) return null;

  const composed = composeAnswers(set);
  const locked = sending;

  const write = (patch: Partial<QuestionDraftAnswer>) => {
    rememberAnswer(set.setId, question.position, { ...draft, ...patch });
    redraw();
  };

  // DQA-FR-JJON: the note is offered only where a **recorded option** is
  // selected. A question answered in the author's own words offers none.
  const noteOffered = !draft.ownWords && draft.selected !== null;

  const turnTo = (next: number) => {
    paged.current = true;
    setPage(next);
  };

  return (
    <section
      className="discussion-questions"
      data-testid="discussion-questions"
      // DQA-FR-MQZE: the arrival of a set, a move between questions, an accepted
      // submission and a refusal are each announced, naming what changed.
      aria-label={`Questions from ${participantName(set.askedBy)}`}
    >
      <header className="discussion-questions__head">
        {/* DQA-FR-PDLN: the agent that asked, from the set's own recorded
            participant snapshot, with its title on the terms every agent
            contribution renders one. */}
        <span className="discussion-questions__asker">
          <span className="discussion-questions__handle">
            {participantName(set.askedBy)}
          </span>
          {/* CTA-FR-KFUF, CTA-FR-KYPK: the title where the snapshot carries one,
              and no line and no placeholder where it does not. */}
          {participantTitle(set.askedBy) && (
            <span className="discussion-questions__title">
              {participantTitle(set.askedBy)}
            </span>
          )}
        </span>
        {/* DQA-FR-MJPV: which question of how many is showing and how many still
            need an answer, so a disabled Submit is never a mystery. */}
        <span
          className="discussion-questions__progress"
          data-testid="discussion-questions-progress"
          aria-live="polite"
        >
          {progressLine(set, current)}
        </span>
      </header>

      <fieldset
        className="discussion-questions__question"
        data-testid="discussion-questions-question"
        ref={questionRef}
        tabIndex={-1}
      >
        {/* DQA-FR-XQOR: the recorded text, exactly as it was recorded, with no
            prose composed around it. DQA-FR-JADB: less any provider citation
            marker, which is hidden and not removed from the set. */}
        <legend className="discussion-questions__ask">
          {stripCitationMarkers(question.text)}
        </legend>

        {/* The answer rows and the note, as one group.
            A wrapper rather than the fieldset itself, because the fieldset also
            holds the question, and a surface that draws the group as a card
            needs the card to surround the answers alone — a legend notches the
            border of the box it is inside. It is `display: contents` by
            default, so every presentation that draws no card is unaffected. */}
        <div className="discussion-questions__group">
        {/* DQA-FR-QWTB: the two or three recorded options as rows of one answer
            group, each reading as its recorded value, and none preselected. */}
        {question.options.map((option) => (
          <label
            className="discussion-questions__choice"
            key={option.position}
            data-selected={
              !draft.ownWords && draft.selected === option.position ? "true" : undefined
            }
          >
            <input
              type="radio"
              name={`discussion-question-${set.setId}-${question.position}`}
              // DQA-FR-HLDS: exactly one option stands selected. Choosing
              // another replaces the selection rather than adding to it, which
              // one radio group per question is what guarantees.
              checked={!draft.ownWords && draft.selected === option.position}
              disabled={locked}
              // DQA-FR-ROXG: choosing leaves the note the author typed where it
              // was — the note belongs to the question, not to the option.
              // DQA-FR-HLDS: and it takes the answer off the own-answer row,
              // the two being rows of one group.
              onChange={() => write({ selected: option.position, ownWords: false })}
            />
            <span className="discussion-questions__option">
              {stripCitationMarkers(option.value)}
            </span>
          </label>
        ))}

        {/* DQA-FR-FCZL: the own answer, a row of the same group after the
            recorded options, so exactly one answer stands (DQA-FR-HLDS). It is
            offered on every question — the agent cannot withhold it. The label
            wraps the radio and its words and stops there, so nothing the author
            types becomes part of the name the row announces itself by. */}
        <div
          className="discussion-questions__choice discussion-questions__choice--own"
          data-testid="discussion-questions-own"
          data-selected={draft.ownWords ? "true" : undefined}
        >
          <label className="discussion-questions__own-head">
            <input
              type="radio"
              name={`discussion-question-${set.setId}-${question.position}`}
              checked={draft.ownWords}
              disabled={locked}
              onChange={() => {
                opened.current = true;
                write({ ownWords: true });
              }}
            />
            <span className="discussion-questions__option">
              Something else — I will say
            </span>
          </label>
          {draft.ownWords && (
            <textarea
              ref={ownFieldRef}
              className="draft-review__feedback discussion-questions__typed"
              data-testid="discussion-questions-own-field"
              // One line until there is more than one line in it. Without this a
              // textarea falls back to its own two rows when the effect above
              // sets the height to `auto`, and the field measures itself as two
              // lines tall however little is written in it.
              rows={1}
              aria-label={`Your own answer to question ${current + 1} of ${total}`}
              value={draft.typed}
              disabled={locked}
              onChange={(event) => write({ typed: event.target.value })}
            />
          )}
        </div>
        {/* DQA-FR-ROXG: the note below the answer rows, marked by its placeholder
            alone — there is nothing to press and nothing to open.
            DQA-FR-JJON: and it is offered only beside a chosen option.
            DQA-FR-UAKC: typing here chooses nothing, so a question carrying a note
            and no answer is still unanswered. */}
        {noteOffered && (
          <textarea
            ref={noteFieldRef}
            className="draft-review__feedback discussion-questions__note-field"
            data-testid="discussion-questions-note"
            rows={1}
            placeholder="Add a note…"
            aria-label={`Note for question ${current + 1} of ${total}`}
            value={draft.note}
            disabled={locked}
            onChange={(event) => write({ note: event.target.value })}
          />
        )}
        </div>
      </fieldset>


      {refusal && (
        // DQA-FR-WKTP: the typed error inline at the foot of the block, with
        // every selection and every note exactly as the author left them.
        <p className="discussion-questions__error" role="alert">
          {refusal}
        </p>
      )}

      <div className="discussion-questions__foot">
        <button
          type="button"
          className="btn btn--sm"
          data-testid="discussion-questions-previous"
          // DQA-FR-TFCE: disabled at the two ends of the set, which is what lets
          // focus move to the question rather than stay on the control.
          disabled={locked || current === 0}
          onClick={() => turnTo(current - 1)}
        >
          ‹ Previous
        </button>
        <span
          className="discussion-questions__dots"
          data-testid="discussion-questions-dots"
          aria-hidden="true"
        >
          {set.questions.map((entry, index) => (
            <span
              key={entry.position}
              className="discussion-questions__dot"
              data-current={index === current ? "true" : undefined}
              // DQA-FR-SEBN: what counts as answered is one rule, so the dots,
              // the progress line and Submit cannot disagree — own words are an
              // answer, and `selected` stays null for a question answered in
              // them.
              data-answered={
                isAnswered(set.setId, entry.position) ? "true" : undefined
              }
            />
          ))}
        </span>
        <button
          type="button"
          className="btn btn--sm"
          data-testid="discussion-questions-next"
          disabled={locked || current >= total - 1}
          onClick={() => turnTo(current + 1)}
        >
          Next ›
        </button>
        <span className="spacer" />
        <button
          type="button"
          className="btn btn--sm btn--primary"
          data-testid="discussion-questions-submit"
          // DQA-FR-WGQY: enabled only once every recorded question is answered.
          // DQA-FR-AYNP: and single-flight — `locked` is set the instant it is
          // activated, so every further activation is ignored until the request
          // settles.
          disabled={locked || composed === null}
          onClick={() => {
            // DQA-FR-AYNP: the guard rather than the disabled attribute is what
            // makes this single-flight. A disabled button dispatches no click in
            // a browser, but a keyboard activation and a re-entrant render can
            // both arrive before React has painted the disabled state.
            if (!composed || sendingRef.current) return;
            sendingRef.current = true;
            setSending(true);
            setRefusal(null);
            void onSubmit(composed)
              .then(() => {
                // DQA-FR-IPFD: the block and its draft go together, which the
                // caller does by publishing the set the backend now reports.
                onSubmitted();
              })
              .catch((reason: unknown) => {
                // DQA-FR-WKTP: nothing the author entered is touched.
                // ADQ-FR-LZHV: the record names no question, option, or note.
                logError(["frontend"], "the question answers were refused", {
                  discussionId: set.discussionId,
                  questions: total,
                });
                // The typed slug rather than whatever shape the rejection took:
                // the backend refuses with a bare string, and a transport
                // failure arrives as an `Error`. `String(error)` on the latter
                // yields "Error: <slug>", which no message table matches and the
                // author reads as a raw code.
                const raw =
                  typeof reason === "string"
                    ? reason
                    : reason instanceof Error
                      ? reason.message
                      : String(reason);
                setRefusal(describeError(raw));
              })
              .finally(() => {
                // And it recovers: a refused submission is retried without a
                // reload (DQA-FR-WKTP).
                sendingRef.current = false;
                setSending(false);
              });
          }}
        >
          Submit
        </button>
      </div>
    </section>
  );
}
