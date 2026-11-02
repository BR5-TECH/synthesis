/**
 * The application's one block for answering a set of questions
 * (`../../../specifications/ui/DQA-discussion-question-answering.md` DQA-FR-HSBK).
 *
 * A discussion's pending set, a graduation run's escalation, a merge run's
 * escalation and a work stream update's escalation all render this block, so
 * one act looks and works the same wherever the author meets it. The **host**
 * supplies four parts (DQA-FR-RTWE): the head that names who asks, an optional
 * context between the head and the question, the unsent answer draft, and the
 * send operation. Everything else is here: paging, answer rows, rails, the own
 * answer, the note, the counts, Submit, single flight, and the inline refusal.
 *
 * It is answered **one question at a time**, with pagination both ways
 * (DQA-FR-BXHU). It is an in-panel block, never a floating overlay and never an
 * application modal (DQA-FR-CKYP): it takes no focus trap.
 *
 * What the author has entered and not yet sent is held outside this component
 * by the host's draft (DQA-FR-LSNW, GEA-FR-QULY), so paging, a re-read, a
 * re-render and a refusal of the whole set all leave the work where it was.
 *
 * Voice mode is a discussion's alone (DQA-FR-CYVA): a host that passes no
 * `voice` gets a block that speaks nothing and takes no spoken answer.
 */
import {
  Fragment,
  useEffect,
  useId,
  useReducer,
  useRef,
  useState,
  type ReactNode,
} from "react";

import { logError } from "../../logging";
import type { QuestionDraftAnswer } from "../../state/questionAnswerDrafts";
import {
  DESCRIPTION_PLACEHOLDER,
  hasDescriptions,
  isQuestionAnswered,
  nextUnansweredOf,
  progressLineOf,
  shownOption,
  type HoveredRow,
} from "./answers";

/** One option of a question, as the block renders it. */
export interface SetOption {
  /** The option's recorded position; what the draft holds as `selected`. */
  position: number;
  /** DQA-FR-QWTB: the recorded label the row reads as. */
  label: string;
  /** DQA-FR-MIBQ: what the right rail shows for it, where there is one. */
  description?: string | null;
}

/** One recorded question, as the block renders it. */
export interface SetQuestion {
  /** The question's recorded position, stable across re-reads. */
  position: number;
  /** DQA-FR-XQOR: the recorded text. */
  text: string;
  /** DQA-FR-QWTB, DQA-FR-ZNOQ: the recorded options, possibly none. */
  options: SetOption[];
}

/** DQA-FR-RTWE: the host's unsent answer draft for this set. */
export interface QuestionSetDrafts {
  recall(position: number): QuestionDraftAnswer;
  remember(position: number, answer: QuestionDraftAnswer): void;
  /** DQA-FR-WKTP: the refusal the last submission met, held with the draft. */
  recallRefusal(): string | null;
  rememberRefusal(text: string | null): void;
}

/** What a mounted block lends to voice mode (VOM-FR-PINR). */
export interface QuestionSetVoiceHandle {
  /** False once the set was submitted, so the block is not retained. */
  isOpen(): boolean;
  /** The recorded position of the question that shows now. */
  showing(): number | null;
  /** DQA-FR-SUUU: a transcript answers the question at this position. */
  answer(position: number, transcript: string): void;
}

/** DQA-FR-CYVA: the voice-mode wiring a discussion host supplies. */
export interface QuestionSetVoice {
  /** DQA-FR-ITCZ, VOM-FR-BCRD: the page a spoken answer moved the set to. */
  spokenPage(): number | null;
  notePage(page: number): void;
  forgetPage(): void;
  /** VOM-FR-UALK: the question at this position shows. */
  shown(position: number, arrived: boolean): void;
  /** VOM-FR-PINR: lend the handle while the block stands. */
  register(handle: QuestionSetVoiceHandle): () => void;
}

export interface QuestionSetBlockProps<Answer> {
  /** The identity of the set; a new key is a set that replaces another. */
  setKey: string;
  questions: SetQuestion[];
  drafts: QuestionSetDrafts;
  /** DQA-FR-MQZE: what the block announces itself as. */
  label: string;
  /** DQA-FR-RTWE: who asks, on the left of the head. */
  asker: ReactNode;
  /** DQA-FR-RTWE: what stands under the head row, at its full width. */
  headDetail?: ReactNode;
  /** DQA-FR-RTWE: what stands between the head and the question. */
  context?: ReactNode;
  /** The host is busy, so no control of the block can be used. */
  disabled?: boolean;
  /**
   * DQA-FR-WGQY: the whole ordered set of answers from the draft, or `null`
   * while it is not complete. Submit is enabled on exactly this.
   */
  compose: () => Answer[] | null;
  /** DQA-FR-KDVU: the whole ordered set, in one call. */
  onSubmit: (answers: Answer[]) => Promise<unknown>;
  /** DQA-FR-IPFD: the submission was accepted. */
  onSubmitted: () => void;
  /** DQA-FR-WKTP: how a typed refusal reads. */
  describeError: (reason: string) => string;
  /** What the refusal record names besides the counts. Never user content. */
  logFields?: Record<string, string | number | boolean>;
  /** DQA-FR-CYVA: voice mode, for a discussion's set alone. */
  voice?: QuestionSetVoice;
}

export function QuestionSetBlock<Answer>({
  setKey,
  questions,
  drafts,
  label,
  asker,
  headDetail,
  context,
  disabled = false,
  compose,
  onSubmit,
  onSubmitted,
  describeError,
  logFields = {},
  voice,
}: QuestionSetBlockProps<Answer>) {
  // The draft lives outside this component, so a write to it is what the render
  // has to be told about.
  const [, redraw] = useReducer((count: number) => count + 1, 0);
  // DQA-FR-ITCZ, VOM-FR-BCRD: a block that mounts again shows the page that a
  // spoken answer moved it to while it was not mounted.
  //
  // DQA-FR-GVSA: the page belongs to the set it was turned in. It is held with
  // the key of that set, so a set that replaces another never shows the page of
  // the one before it, not even for one frame.
  const [pageOf, setPageOf] = useState(() => ({
    key: setKey,
    page: voice?.spokenPage() ?? 0,
  }));
  const page = pageOf.key === setKey ? pageOf.page : (voice?.spokenPage() ?? 0);
  const setPage = (next: number) => setPageOf({ key: setKey, page: next });
  const [sending, setSending] = useState(false);
  // DQA-FR-WKTP, DQA-FR-VICX: the refusal is held with the draft, so a refusal
  // that a spoken answer met while the block was not mounted shows when it mounts.
  // It is held with the key of its set for the reason the page is.
  const [refusalOf, setRefusalOf] = useState(() => ({
    key: setKey,
    text: drafts.recallRefusal(),
  }));
  const refusal = refusalOf.key === setKey ? refusalOf.text : drafts.recallRefusal();
  const setRefusal = (text: string | null) => {
    drafts.rememberRefusal(text);
    setRefusalOf({ key: setKey, text });
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
  const submitRef = useRef<HTMLButtonElement | null>(null);
  /**
   * DQA-FR-EWLB: a refused submission gives the keyboard back to Submit. The
   * button is disabled while the request runs, and a browser moves focus off a
   * control that becomes disabled.
   */
  const refocusSubmit = useRef(false);
  /**
   * The set a submission belongs to. A submission that settles after another
   * set replaced its own changes nothing: its acceptance must not drop the
   * draft of the set that replaced it, and its refusal is not that set's.
   */
  const submissionKey = useRef(setKey);
  // DQA-FR-TFCE: focus moves with the page, but not on the first render — the
  // block appearing must not take the keyboard away from whatever the author is
  // doing (DQA-FR-CKYP).
  const paged = useRef(false);
  /** What the handle of voice mode reads, from the newest render. */
  const voiceBlock = useRef<Pick<QuestionSetVoiceHandle, "showing" | "answer"> | null>(null);
  /** VOM-FR-BCRD: the block can be retained after it unmounts, until the set is sent. */
  const submittedRef = useRef(false);
  const mountedRef = useRef(true);
  /** The page that voice mode reads. A spoken answer moves it while unmounted too. */
  const voicePage = useRef(0);
  /** The newest voice wiring, read by the handle and by the unmounted path. */
  const voiceRef = useRef(voice);
  voiceRef.current = voice;
  /**
   * DQA-FR-GGKI: the next page turn came from a spoken answer, with the button
   * that held focus then. The move can disable that button, and a browser then
   * puts focus on the document before an effect can read it.
   */
  const spokenMove = useRef<{ button: HTMLButtonElement | null } | null>(null);

  // DQA-FR-GVSA: a set that replaces another starts unpaged, and a send still
  // running for the set before does not hold the new one's controls closed.
  useEffect(() => {
    paged.current = false;
    submissionKey.current = setKey;
    sendingRef.current = false;
    setSending(false);
  }, [setKey]);

  // DQA-FR-DYFR: a re-read that records fewer questions must not leave the
  // block showing a page that is no longer there.
  const total = questions.length;
  const current = Math.min(page, Math.max(total - 1, 0));
  const question = questions[current];

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
  }, [current, setKey]);

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
    if (!question || !voiceRef.current) return;
    const arrived = shownSet.current !== setKey;
    shownSet.current = setKey;
    voiceRef.current.shown(question.position, arrived);
  }, [setKey, question]);

  // DQA-FR-EWLB: once Submit is enabled again after a refusal, it takes back the
  // keyboard, unless the author has moved focus somewhere meanwhile.
  useEffect(() => {
    if (sending || !refocusSubmit.current) return;
    refocusSubmit.current = false;
    const held = document.activeElement;
    if (held === null || held === document.body) submitRef.current?.focus();
  }, [sending]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  // VOM-FR-PINR: voice mode reaches the block through this, while it stands.
  const voiced = voice !== undefined;
  useEffect(() => {
    if (!voiced) return;
    return voiceRef.current?.register({
      isOpen: () => !submittedRef.current,
      showing: () => voiceBlock.current?.showing() ?? null,
      answer: (position, transcript) => voiceBlock.current?.answer(position, transcript),
    });
  }, [setKey, voiced]);

  // Read before the early return below, because the effect after it is a hook:
  // a re-read that records no questions at all takes that return (DQA-FR-DYFR),
  // and a hook the component reaches on some renders and not others changes the
  // order React matches them by. Position 0 is no recorded position (both a
  // discussion's set and an escalation count from 1), so this reads as the
  // empty draft.
  const draft = drafts.recall(question?.position ?? 0);

  /**
   * Both fields are one line until there is more than one line in them, and
   * grow to the cap the stylesheet sets before they start to scroll.
   *
   * The kit's optional field (`.draft-review__feedback`) is drawn as a rule to
   * write on rather than as a box, and its height is the writer's to set from
   * the content. It runs when what is written changes and when a field arrives,
   * rather than on every render: each pass forces two reflows per field.
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
    const answers = compose();
    // DQA-FR-AYNP: the guard rather than the disabled attribute is what makes
    // this single-flight. A disabled button dispatches no click in a browser, but
    // a keyboard activation and a re-entrant render can both arrive before React
    // has painted the disabled state.
    if (!answers || sendingRef.current) return;
    sendingRef.current = true;
    setSending(true);
    setRefusal(null);
    const key = setKey;
    const current = () => submissionKey.current === key;
    void onSubmit(answers)
      .then(() => {
        if (!current()) return;
        submittedRef.current = true;
        // DQA-FR-IPFD: the block and its draft go together, which the host does
        // once the backend has accepted the set.
        onSubmitted();
      })
      .catch((reason: unknown) => {
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
        // DQA-FR-WKTP: nothing the author entered is touched.
        // ADQ-FR-LZHV: the record names no question, option, answer, or note —
        // only the typed code before any detail the refusal carries.
        logError(["frontend"], "the question answers were refused", {
          ...logFields,
          reason: raw.split(":")[0].trim() || "unknown",
          questions: total,
          // A spoken answer can submit while the block is not mounted.
          mounted: mountedRef.current,
          stale: !current(),
        });
        if (!current()) return;
        refocusSubmit.current = mountedRef.current;
        setRefusal(describeError(raw));
      })
      .finally(() => {
        if (!current()) return;
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
    drafts.remember(position, {
      ...drafts.recall(position),
      ownWords: true,
      typed: transcript,
    });
    redraw();
    const next = nextUnansweredOf(questions, drafts.recall, position);
    if (next === null) {
      submit();
    } else if (next !== voicePage.current) {
      voicePage.current = next;
      voiceRef.current?.notePage(next);
      if (!mountedRef.current) {
        // VOM-FR-BCRD, VOM-FR-UALK: no effect runs while unmounted, so the
        // next question speaks from here.
        const moved = questions[next];
        if (moved) voiceRef.current?.shown(moved.position, false);
        return;
      }
      const held = document.activeElement;
      spokenMove.current = { button: held instanceof HTMLButtonElement ? held : null };
      setPage(next);
    }
  };
  voicePage.current = current;
  voiceBlock.current = {
    showing: () => (questions[voicePage.current] ?? question)?.position ?? null,
    answer: answerAloud,
  };

  if (!question) return null;

  const composed = compose();
  const locked = sending || disabled;

  const write = (patch: Partial<QuestionDraftAnswer>) => {
    drafts.remember(question.position, { ...draft, ...patch });
    redraw();
  };

  // DQA-FR-ZNOQ: a question recorded with no options has the own-answer field
  // alone, open, with no row to choose and no rails.
  const alone = question.options.length === 0;
  // DQA-FR-JJON: the note is offered only where a **recorded option** is
  // selected. A question answered in the author's own words offers none.
  const noteOffered = !alone && !draft.ownWords && draft.selected !== null;

  // DQA-FR-MIBQ, DQA-FR-EWUG: two rails only where the set carries descriptions.
  const railed = !alone && hasDescriptions(questions);
  const shown = railed ? shownOption(question, pointed ?? focused, draft) : null;
  /** DQA-FR-CLHJ: the hidden element an option row is described by. */
  const descriptionId = (position: number) =>
    `${instance}-question-${question.position}-option-${position}-description`;
  /** Leaving a row clears it only while it is still the one held. */
  const release = (row: HoveredRow) => (standing: HoveredRow) =>
    standing === row ? null : standing;
  const groupName = `question-set-${instance}-${question.position}`;

  const turnTo = (next: number) => {
    paged.current = true;
    voiceRef.current?.forgetPage();
    setPage(next);
  };

  // DQA-FR-QWTB: the recorded options as rows of one answer group, each
  // reading as its recorded label, and none preselected.
  const optionRows = question.options.map((option) => (
    <Fragment key={option.position}>
      <label
        className="discussion-questions__choice"
        data-selected={
          !draft.ownWords && draft.selected === option.position ? "true" : undefined
        }
        // DQA-FR-TKUM: the right rail follows the pointer.
        onPointerEnter={railed ? () => setPointed(option.position) : undefined}
        onPointerLeave={railed ? () => setPointed(release(option.position)) : undefined}
      >
        <input
          type="radio"
          name={groupName}
          // DQA-FR-KBJA: and the keyboard, exactly as it follows the pointer.
          onFocus={railed ? () => setFocused(option.position) : undefined}
          onBlur={railed ? () => setFocused(release(option.position)) : undefined}
          aria-describedby={
            railed && option.description ? descriptionId(option.position) : undefined
          }
          // DQA-FR-HLDS: exactly one answer stands. One radio group per question
          // is what guarantees it.
          checked={!draft.ownWords && draft.selected === option.position}
          disabled={locked}
          // DQA-FR-ROXG: choosing leaves the note the author typed where it
          // was — the note belongs to the question, not to the option.
          onChange={() => write({ selected: option.position, ownWords: false })}
        />
        <span className="discussion-questions__option">{option.label}</span>
      </label>
      {/* DQA-FR-CLHJ: the row's own description, read with its label by a
            screen reader; the visible rail is not announced. */}
      {railed && option.description && (
        <span id={descriptionId(option.position)} className="sr-only">
          {option.description}
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
          {option.description ?? ""}
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

  const ownField = (
    <textarea
      ref={ownFieldRef}
      className="draft-review__feedback discussion-questions__typed"
      data-testid="discussion-questions-own-field"
      // One line until there is more than one line in it.
      rows={1}
      aria-label={`Your own answer to question ${current + 1} of ${total}`}
      value={draft.typed}
      disabled={locked}
      onChange={(event) =>
        // DQA-FR-ZNOQ: with no options, the field is the answer row itself.
        write(alone ? { typed: event.target.value, ownWords: true } : { typed: event.target.value })
      }
    />
  );

  // The answer rows and the note, as one group.
  const answerGroup = alone ? (
    <div className="discussion-questions__group" data-alone="true">
      {ownField}
    </div>
  ) : (
    <div
      className={
        railed
          ? "discussion-questions__group discussion-questions__group--railed"
          : "discussion-questions__group"
      }
    >
      {/* DQA-FR-MIBQ, DQA-FR-SRMF: the option rows are the left rail and one
          description stands in the right. DQA-FR-CLHJ: the rail is hidden from
          assistive technology rather than announced on every move. */}
      {railed ? (
        <div className="discussion-questions__rails" data-testid="discussion-questions-rails">
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
          recorded options, offered on every question. The label wraps the radio
          and its words and stops there, so nothing the author types becomes
          part of the row's name. */}
      <div
        className="discussion-questions__choice discussion-questions__choice--own"
        data-testid="discussion-questions-own"
        data-selected={draft.ownWords ? "true" : undefined}
        // DQA-FR-USTA: the own-answer row has no description.
        onPointerEnter={railed ? () => setPointed("own") : undefined}
        onPointerLeave={railed ? () => setPointed(release("own")) : undefined}
      >
        <label className="discussion-questions__own-head">
          <input
            type="radio"
            name={groupName}
            checked={draft.ownWords}
            disabled={locked}
            onFocus={railed ? () => setFocused("own") : undefined}
            onBlur={railed ? () => setFocused(release("own")) : undefined}
            onChange={() => {
              opened.current = true;
              write({ ownWords: true });
            }}
          />
          <span className="discussion-questions__option">Something else — I will say</span>
        </label>
        {draft.ownWords && ownField}
      </div>
      {/* DQA-FR-ROXG, DQA-FR-JJON, DQA-FR-UAKC: the note below the answer rows,
          marked by its placeholder alone, offered only beside a chosen option,
          and never an answer. */}
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
      // DQA-FR-MQZE: the block names what it is.
      aria-label={label}
    >
      <header className="discussion-questions__head">
        {/* DQA-FR-RTWE: who asks is the host's. */}
        <span className="discussion-questions__asker">{asker}</span>
        {/* DQA-FR-MJPV: which question of how many is showing and how many still
            need an answer, so a disabled Submit is never a mystery. */}
        <span
          className="discussion-questions__progress"
          data-testid="discussion-questions-progress"
          aria-live="polite"
        >
          {progressLineOf(questions, drafts.recall, current)}
        </span>
        {headDetail}
      </header>

      {context}

      <fieldset
        className="discussion-questions__question"
        data-testid="discussion-questions-question"
        ref={questionRef}
        tabIndex={-1}
      >
        {/* DQA-FR-XQOR: the recorded text, with no prose composed around it. */}
        <legend className="discussion-questions__ask">{question.text}</legend>

        {/* A wrapper rather than the fieldset itself, because a surface that
            draws the group as a card needs the card to surround the answers
            alone — a legend notches the border of the box it is inside. */}
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
          // DQA-FR-TFCE: disabled at the two ends of the set.
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
          {questions.map((entry, index) => (
            <span
              key={entry.position}
              className="discussion-questions__dot"
              data-current={index === current ? "true" : undefined}
              // DQA-FR-SEBN: what counts as answered is one rule, so the dots,
              // the progress line and Submit cannot disagree.
              data-answered={
                isQuestionAnswered(entry, drafts.recall(entry.position)) ? "true" : undefined
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
          ref={submitRef}
          data-testid="discussion-questions-submit"
          // DQA-FR-WGQY: enabled only once every recorded question is answered.
          // DQA-FR-AYNP: and single-flight.
          disabled={locked || composed === null}
          onClick={submit}
        >
          Submit
        </button>
      </div>
    </section>
  );
}
