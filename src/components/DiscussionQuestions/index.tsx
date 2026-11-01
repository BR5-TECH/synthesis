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
 * Where the set carries option descriptions, the answers stand in a left rail
 * and one description in a right rail (DQA-FR-MIBQ), so the author scans short
 * labels and reads the detail of the one they consider. The block measures its
 * own width, and the rails stack where it is narrow (DQA-FR-OGVU).
 *
 * The shape follows `../GraduationRuns/escalation.tsx` deliberately: an author
 * who has answered a graduation run's questions already knows how to answer
 * these.
 */
import {
  Fragment,
  useEffect,
  useId,
  useReducer,
  useRef,
  useState,
} from "react";

import { logError } from "../../logging";
import {
  recallAnswer,
  recallRefusal,
  rememberAnswer,
  rememberRefusal,
  type QuestionDraftAnswer,
} from "../../state/questionAnswerDrafts";
import {
  participantName,
  participantTitle,
  type PendingQuestionSet,
  type QuestionAnswerInput,
} from "../../types";
import {
  composeAnswers,
  nextUnanswered,
  DESCRIPTION_PLACEHOLDER,
  isAnswered,
  progressLine,
  setHasDescriptions,
  shownOption,
  type HoveredRow,
} from "./answers";
import { stripCitationMarkers } from "../../text/citationMarkers";
import { noteQuestionShown } from "../../voice/voiceController";
import {
  forgetSpokenPage,
  noteSpokenPage,
  registerQuestionBlock,
  spokenPageOf,
  type VoiceQuestionBlock,
} from "../../voice/voiceQuestions";

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
  // DQA-FR-ITCZ, VOM-FR-BCRD: a block that mounts again shows the page that a
  // spoken answer moved it to while it was not mounted.
  const [page, setPage] = useState(() => spokenPageOf(set.setId) ?? 0);
  const [sending, setSending] = useState(false);
  // DQA-FR-WKTP, DQA-FR-VICX: the refusal is held with the draft, so a refusal
  // that a spoken answer met while the block was not mounted shows when it mounts.
  const [refusal, setRefusalShown] = useState<string | null>(() => recallRefusal(set.setId));
  const setRefusal = (text: string | null) => {
    rememberRefusal(set.setId, text);
    setRefusalShown(text);
  };
  /**
   * DQA-FR-TKUM, DQA-FR-KBJA: the row under the pointer and the row the
   * keyboard is on, held apart so that one leaving does not clear the other.
   * The pointer wins where they differ, being the more recent intent.
   */
  const [pointed, setPointed] = useState<HoveredRow>(null);
  const [focused, setFocused] = useState<HoveredRow>(null);
  /** DQA-FR-CLHJ: unique per rendering, as one set can stand in two surfaces. */
  const instance = useId();
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
  /** What the handle of voice mode reads, from the newest render. */
  const voiceBlock = useRef<Pick<VoiceQuestionBlock, "showing" | "answer"> | null>(null);
  /** VOM-FR-BCRD: the block can be retained after it unmounts, until the set is sent. */
  const submittedRef = useRef(false);
  const mountedRef = useRef(true);
  /** The page that voice mode reads. A spoken answer moves it while unmounted too. */
  const voicePage = useRef(0);
  /**
   * DQA-FR-GGKI: the next page turn came from a spoken answer, with the button
   * that held focus then. The move can disable that button, and a browser then
   * puts focus on the document before an effect can read it.
   */
  const spokenMove = useRef<{ button: HTMLButtonElement | null } | null>(null);

  // DQA-FR-GVSA: a set the author has not paged shows its first question, and a
  // set that replaces another does the same — a page belongs to the set it was
  // turned in.
  useEffect(() => {
    setPage(spokenPageOf(set.setId) ?? 0);
    setRefusalShown(recallRefusal(set.setId));
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

  // DQA-FR-TKUM: a move to another question clears the hovered row, so the
  // rail of the question that arrived starts from its own selection.
  useEffect(() => {
    setPointed(null);
    setFocused(null);
  }, [current, set.setId]);

  useEffect(() => {
    // DQA-FR-GGKI: the record control keeps the keyboard for the next answer. A
    // Previous or Next that the move disabled loses focus, so it goes to the
    // question and not to the document.
    if (spokenMove.current) {
      if (spokenMove.current.button?.disabled) questionRef.current?.focus();
      spokenMove.current = null;
      return;
    }
    if (!paged.current) return;
    // DQA-FR-TFCE: the keyboard lands in the question that arrived rather than
    // staying on the control that moved it, because Previous and Next disable
    // themselves at the two ends of the set.
    questionRef.current?.focus();
  }, [current]);

  // VOM-FR-UALK: a set that voice mode spoke is silent on a remount. A ref per
  // set, because a new set in this mount is an arrival too.
  const shownSet = useRef<string | null>(null);
  useEffect(() => {
    if (!question) return;
    const arrived = shownSet.current !== set.setId;
    shownSet.current = set.setId;
    noteQuestionShown(set.discussionId, set.setId, question, set.askedBy, arrived);
  }, [set.discussionId, set.setId, set.askedBy, question]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  // VOM-FR-PINR: voice mode reaches the block through this, while it stands.
  useEffect(
    () =>
      registerQuestionBlock(set.discussionId, {
        setId: set.setId,
        isOpen: () => !submittedRef.current,
        showing: () => voiceBlock.current?.showing() ?? null,
        answer: (position, transcript) => voiceBlock.current?.answer(position, transcript),
      }),
    [set.discussionId, set.setId],
  );

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

  /**
   * DQA-FR-KDVU, DQA-FR-WKTP: the one submission, from Submit and from the last
   * spoken answer (DQA-FR-YMGD). It reads the draft, so a spoken answer that was
   * written a moment ago is in it.
   */
  const submit = () => {
    const answers = composeAnswers(set);
    // DQA-FR-AYNP: the guard rather than the disabled attribute is what makes
    // this single-flight. A disabled button dispatches no click in a browser, but
    // a keyboard activation and a re-entrant render can both arrive before React
    // has painted the disabled state.
    if (!answers || sendingRef.current) return;
    sendingRef.current = true;
    setSending(true);
    setRefusal(null);
    void onSubmit(answers)
      .then(() => {
        submittedRef.current = true;
        // DQA-FR-IPFD: the block and its draft go together, which the caller does
        // by publishing the set the backend now reports.
        onSubmitted();
      })
      .catch((reason: unknown) => {
        // DQA-FR-WKTP: nothing the author entered is touched.
        // ADQ-FR-LZHV: the record names no question, option, or note.
        logError(["frontend"], "the question answers were refused", {
          discussionId: set.discussionId,
          questions: total,
          // A spoken answer can submit while the block is not mounted.
          mounted: mountedRef.current,
        });
        // The typed slug rather than whatever shape the rejection took: the
        // backend refuses with a bare string, and a transport failure arrives as
        // an `Error`. `String(error)` on the latter yields "Error: <slug>", which
        // no message table matches and the author reads as a raw code.
        const raw =
          typeof reason === "string"
            ? reason
            : reason instanceof Error
              ? reason.message
              : String(reason);
        setRefusal(describeError(raw));
      })
      .finally(() => {
        // And it recovers: a refused submission is retried without a reload
        // (DQA-FR-WKTP).
        sendingRef.current = false;
        setSending(false);
      });
  };

  /**
   * DQA-FR-SUUU, DQA-FR-DYSP: a spoken answer is the own answer, whatever the
   * words are. DQA-FR-ITCZ, DQA-FR-YMGD: the block then moves on, and the last
   * answer submits. DQA-FR-VICX: a refusal does not come back here.
   */
  const answerAloud = (position: number, transcript: string) => {
    rememberAnswer(set.setId, position, {
      ...recallAnswer(set.setId, position),
      ownWords: true,
      typed: transcript,
    });
    redraw();
    const next = nextUnanswered(set, position);
    if (next === null) {
      submit();
    } else if (next !== voicePage.current) {
      voicePage.current = next;
      noteSpokenPage(set.setId, next);
      if (!mountedRef.current) {
        // VOM-FR-BCRD, VOM-FR-UALK: no effect runs while unmounted, so the
        // next question speaks from here.
        const moved = set.questions[next];
        if (moved) noteQuestionShown(set.discussionId, set.setId, moved, set.askedBy, false);
        return;
      }
      const held = document.activeElement;
      spokenMove.current = { button: held instanceof HTMLButtonElement ? held : null };
      setPage(next);
    }
  };
  voicePage.current = current;
  voiceBlock.current = {
    showing: () => set.questions[voicePage.current] ?? question ?? null,
    answer: answerAloud,
  };

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

  // DQA-FR-MIBQ, DQA-FR-EWUG: two rails only where the set carries descriptions.
  const railed = setHasDescriptions(set);
  const shown = railed
    ? shownOption(question, pointed ?? focused, draft)
    : null;
  /** DQA-FR-CLHJ: the hidden element an option row is described by. */
  const descriptionId = (position: number) =>
    `${instance}-question-${question.position}-option-${position}-description`;
  /** Leaving a row clears it only while it is still the one held. */
  const release = (row: HoveredRow) => (standing: HoveredRow) =>
    standing === row ? null : standing;

  const turnTo = (next: number) => {
    paged.current = true;
    forgetSpokenPage(set.setId);
    setPage(next);
  };

  // DQA-FR-QWTB: the two or three recorded options as rows of one answer
  // group, each reading as its recorded label, and none preselected.
  const optionRows = question.options.map((option) => (
    <Fragment key={option.position}>
      <label
        className="discussion-questions__choice"
        data-selected={
          !draft.ownWords && draft.selected === option.position
            ? "true"
            : undefined
        }
        // DQA-FR-TKUM: the right rail follows the pointer.
        onPointerEnter={railed ? () => setPointed(option.position) : undefined}
        onPointerLeave={
          railed ? () => setPointed(release(option.position)) : undefined
        }
      >
        <input
          type="radio"
          name={`discussion-question-${set.setId}-${question.position}`}
          // DQA-FR-KBJA: and the keyboard, exactly as it follows the pointer.
          onFocus={railed ? () => setFocused(option.position) : undefined}
          onBlur={
            railed ? () => setFocused(release(option.position)) : undefined
          }
          aria-describedby={
            railed && option.description
              ? descriptionId(option.position)
              : undefined
          }
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
      {/* DQA-FR-CLHJ: the row's own description, read with its label by a
            screen reader; the visible rail is not announced. Outside the label,
            so it describes the row rather than becoming part of its name. */}
      {railed && option.description && (
        <span id={descriptionId(option.position)} className="sr-only">
          {stripCitationMarkers(option.description)}
        </span>
      )}
    </Fragment>
  ));

  // DQA-FR-LFIB: every description of the question and the placeholder stand
  // in one cell, one shown and the rest hidden, so the rail is as tall as the
  // longest of them and moving between options moves nothing else.
  const descriptionLayers = [
    ...question.options
      .filter((option) => option.description)
      .map((option) => (
        <span
          key={option.position}
          className="discussion-questions__detail-text"
          data-shown={shown === option.position ? "true" : undefined}
        >
          {/* DQA-FR-JADB: less any provider citation marker. */}
          {stripCitationMarkers(option.description ?? "")}
        </span>
      )),
    <span
      key="placeholder"
      className="discussion-questions__detail-text"
      data-kind="placeholder"
      data-shown={shown === null ? "true" : undefined}
    >
      {DESCRIPTION_PLACEHOLDER}
    </span>,
  ];

  // The answer rows and the note, as one group.
  const answerGroup = (
    <div
      className={
        railed
          ? "discussion-questions__group discussion-questions__group--railed"
          : "discussion-questions__group"
      }
    >
      {/* DQA-FR-MIBQ, DQA-FR-SRMF: where the set carries descriptions, the
          option rows are the left rail and one description stands in the
          right, both on the group's own background. DQA-FR-CLHJ: the rail
          repeats what each row is described by, so it is hidden from
          assistive technology rather than announced on every move. */}
      {railed ? (
        <div
          className="discussion-questions__rails"
          data-testid="discussion-questions-rails"
        >
          <div className="discussion-questions__options">{optionRows}</div>
          <div
            className="discussion-questions__detail"
            data-testid="discussion-questions-detail"
            // DQA-FR-USTA: while the own answer is chosen and no option row
            // is hovered, the rail shows nothing, its rule included.
            data-hidden={draft.ownWords && shown === null ? "true" : undefined}
            aria-hidden="true"
          >
            {descriptionLayers}
          </div>
        </div>
      ) : (
        optionRows
      )}

      {/* DQA-FR-FCZL: the own answer, a row of the same group after the
          recorded options, so exactly one answer stands (DQA-FR-HLDS). It
          stands below the rails at the group's full width (DQA-FR-MIBQ). It is
          offered on every question — the agent cannot withhold it. The label
          wraps the radio and its words and stops there, so nothing the author
          types becomes part of the name the row announces itself by. */}
      <div
        className="discussion-questions__choice discussion-questions__choice--own"
        data-testid="discussion-questions-own"
        data-selected={draft.ownWords ? "true" : undefined}
        // DQA-FR-USTA: the own-answer row has no description, so pointing at
        // it shows the placeholder.
        onPointerEnter={railed ? () => setPointed("own") : undefined}
        onPointerLeave={railed ? () => setPointed(release("own")) : undefined}
      >
        <label className="discussion-questions__own-head">
          <input
            type="radio"
            name={`discussion-question-${set.setId}-${question.position}`}
            checked={draft.ownWords}
            disabled={locked}
            onFocus={railed ? () => setFocused("own") : undefined}
            onBlur={railed ? () => setFocused(release("own")) : undefined}
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
  );

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
        {answerGroup}
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
          onClick={submit}
        >
          Submit
        </button>
      </div>
    </section>
  );
}
