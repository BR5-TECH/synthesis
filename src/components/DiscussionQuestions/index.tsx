/**
 * Where the author answers a discussion agent that asked several things at once
 * (`../../../specifications/ui/DQA-discussion-question-answering.md`).
 *
 * The block sits between the conversation's message list and its composer, in
 * every presentation that renders a discussion (DQA-FR-NRZB). It is **outside
 * the discussion's history** until the author submits (DQA-FR-TVMH).
 *
 * The block itself is the application's one question block
 * (`../QuestionSet/index.tsx`, DQA-FR-HSBK). This host supplies what a
 * discussion's set owns (DQA-FR-RTWE): the agent that asked as the head
 * (DQA-FR-PDLN), the draft keyed by the set's id (DQA-FR-CBQK), the submission
 * of the discussion's own payload (DQA-FR-KDVU), and voice mode, which is a
 * discussion's alone (DQA-FR-CYVA).
 */
import { useMemo, useRef } from "react";

import {
  recallAnswer,
  recallRefusal,
  rememberAnswer,
  rememberRefusal,
} from "../../state/questionAnswerDrafts";
import {
  participantName,
  participantTitle,
  type PendingQuestionSet,
  type QuestionAnswerInput,
} from "../../types";
import { composeAnswers } from "./answers";
import { stripCitationMarkers } from "../../text/citationMarkers";
import { noteQuestionShown } from "../../voice/voiceController";
import {
  forgetSpokenPage,
  noteSpokenPage,
  registerQuestionBlock,
  spokenPageOf,
} from "../../voice/voiceQuestions";
import {
  QuestionSetBlock,
  type QuestionSetDrafts,
  type QuestionSetVoice,
  type SetQuestion,
} from "../QuestionSet";

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
  // The newest set, read by voice mode at the moment it asks rather than when
  // the wiring was built.
  const setRef = useRef(set);
  setRef.current = set;

  // DQA-FR-JADB: every text the block renders, less any provider citation
  // marker. The recorded set, and the answer a submission sends, are unchanged.
  const questions: SetQuestion[] = useMemo(
    () =>
      set.questions.map((question) => ({
        position: question.position,
        text: stripCitationMarkers(question.text),
        options: question.options.map((option) => ({
          position: option.position,
          label: stripCitationMarkers(option.value),
          description:
            option.description === undefined || option.description === null
              ? option.description
              : stripCitationMarkers(option.description),
        })),
      })),
    [set.questions],
  );

  // DQA-FR-CBQK: the draft is keyed by the set's id and the recorded position.
  const drafts: QuestionSetDrafts = {
    recall: (position) => recallAnswer(set.setId, position),
    remember: (position, answer) => rememberAnswer(set.setId, position, answer),
    recallRefusal: () => recallRefusal(set.setId),
    rememberRefusal: (text) => rememberRefusal(set.setId, text),
  };

  // DQA-FR-CYVA, VOM-FR-PINR: voice mode is a discussion's alone.
  const voice: QuestionSetVoice = useMemo(
    () => ({
      spokenPage: () => spokenPageOf(setRef.current.setId),
      notePage: (page) => noteSpokenPage(setRef.current.setId, page),
      forgetPage: () => forgetSpokenPage(setRef.current.setId),
      shown: (position, arrived) => {
        const current = setRef.current;
        const question = current.questions.find((q) => q.position === position);
        if (question) {
          noteQuestionShown(current.discussionId, current.setId, question, current.askedBy, arrived);
        }
      },
      register: (handle) =>
        registerQuestionBlock(setRef.current.discussionId, {
          setId: setRef.current.setId,
          isOpen: handle.isOpen,
          showing: () => {
            const position = handle.showing();
            return setRef.current.questions.find((q) => q.position === position) ?? null;
          },
          answer: handle.answer,
        }),
    }),
    [],
  );

  return (
    <QuestionSetBlock
      setKey={set.setId}
      questions={questions}
      drafts={drafts}
      // DQA-FR-MQZE: the block names the agent that asked.
      label={`Questions from ${participantName(set.askedBy)}`}
      asker={
        // DQA-FR-PDLN: the agent that asked, from the set's own recorded
        // participant snapshot. CTA-FR-KFUF, CTA-FR-KYPK: its title where the
        // snapshot carries one, and no line and no placeholder where it does not.
        <>
          <span className="discussion-questions__handle">{participantName(set.askedBy)}</span>
          {participantTitle(set.askedBy) && (
            <span className="discussion-questions__title">{participantTitle(set.askedBy)}</span>
          )}
        </>
      }
      compose={() => composeAnswers(set)}
      onSubmit={onSubmit}
      onSubmitted={onSubmitted}
      describeError={describeError}
      logFields={{ discussionId: set.discussionId }}
      voice={voice}
    />
  );
}
