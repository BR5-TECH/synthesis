import { describe, expect, it } from "vitest";

import type { QuestionDraftAnswer } from "../questionAnswerDrafts";
import { makeEscalation, makePrSummaryRun, makeRun } from "../../test/graduationFixtures";
import { markPullRequestWindowOpen } from "../pullRequestSessions";
import type { GraduationEscalation } from "../../types";
import {
  composeEscalationAnswers,
  raiseEntry,
  seedRaisedEntries,
  raisesForRun,
  shownInOpenPullRequestWindow,
} from "./escalations";

// GEA-graduation-escalation-answering.md: the entry each answer sends.

const EMPTY: QuestionDraftAnswer = { selected: null, ownWords: false, typed: "", note: "" };

const threeOptions = (): GraduationEscalation =>
  makeEscalation({
    questions: [
      {
        position: 4,
        question: "Which unit should paging move by?",
        options: [
          { answer: "record", summary: "By record", description: "Fixed records." },
          { answer: "segment", summary: "By segment", description: "One phase." },
          { answer: "time", summary: "By time", description: "One minute." },
        ],
      },
    ],
  });

const compose = (escalation: GraduationEscalation, drafts: Record<number, Partial<QuestionDraftAnswer>>) =>
  composeEscalationAnswers(escalation, (position) => ({ ...EMPTY, ...drafts[position] }));

describe("composeEscalationAnswers", () => {
  it("GEA-FR-DNBI, GEA-FR-TSQO, GEA-FR-BHKU: a chosen response sends its own answer and summary, and no description", () => {
    expect(compose(threeOptions(), { 4: { selected: 2 } })).toStrictEqual([
      { position: 4, answer: "segment", summary: "By segment" },
    ]);
  });

  it("GEA-FR-TSQO, GEA-FR-PLTO: the note beside a chosen response is sent trimmed", () => {
    expect(compose(threeOptions(), { 4: { selected: 1, note: "  why not  " } })).toStrictEqual([
      { position: 4, answer: "record", summary: "By record", note: "why not" },
    ]);
  });

  it.each(["", "   ", "\n\t"])("GEA-FR-TSQO: a blank note %j is left out", (note) => {
    const [entry] = compose(threeOptions(), { 4: { selected: 3, note } })!;
    expect("note" in entry).toBe(false);
  });

  it("GEA-FR-PLTO: the own answer sends no note, also where the draft holds one", () => {
    const [entry] = compose(threeOptions(), {
      4: { selected: 1, ownWords: true, typed: "neither", note: "kept" },
    })!;
    expect(entry).toStrictEqual({ position: 4, answer: "neither", summary: "neither" });
  });

  it("GEA-FR-TSQO, GEA-FR-NXFQ: the own answer is trimmed, and its summary is its first 120 characters", () => {
    const long = "a".repeat(121);
    expect(compose(threeOptions(), { 4: { ownWords: true, typed: `  ${long}  ` } })).toStrictEqual([
      { position: 4, answer: long, summary: "a".repeat(120) },
    ]);
    const exact = "b".repeat(120);
    expect(compose(threeOptions(), { 4: { ownWords: true, typed: exact } })![0].summary).toBe(exact);
  });

  it("GEA-FR-NXFQ: the cut never splits a character in two", () => {
    const text = `${"a".repeat(119)}😀tail`;
    const { summary } = compose(threeOptions(), { 4: { ownWords: true, typed: text } })![0];
    expect(Array.from(summary)).toHaveLength(120);
    expect(summary.endsWith("😀")).toBe(true);
  });

  it("GEA-FR-ZGHY, DQA-FR-ZNOQ: a question with no options is answered by its text alone, with no note", () => {
    const alone = makeEscalation({
      questions: [{ position: 1, question: "Anything else?", options: [] }],
    });
    expect(compose(alone, { 1: { typed: "yes", note: "kept" } })).toStrictEqual([
      { position: 1, answer: "yes", summary: "yes" },
    ]);
  });

  it("GEA-FR-VWQH: nothing is composed while any question is unanswered", () => {
    expect(compose(threeOptions(), {})).toBeNull();
    expect(compose(threeOptions(), { 4: { ownWords: true, typed: "   " } })).toBeNull();
    expect(compose(threeOptions(), { 4: { note: "a note answers nothing" } })).toBeNull();
  });

  it("GEA-FR-VWQH: the entries follow the recorded order, not the order of positions", () => {
    const two = makeEscalation({
      questions: [
        { position: 5, question: "First recorded?", options: [] },
        { position: 2, question: "Second recorded?", options: [] },
      ],
    });
    const composed = compose(two, { 5: { typed: "a" }, 2: { typed: "b" } })!;
    expect(composed.map((entry) => entry.position)).toEqual([5, 2]);
  });

  it("GEA-FR-VBUG: a selection naming an option a re-read no longer records is no answer", () => {
    expect(compose(threeOptions(), { 4: { selected: 4 } })).toBeNull();
  });
});

describe("the completion of a PR summary run under its open window (CPR-FR-VOEB)", () => {
  it("CPR-FR-VOEB, GRU-FR-BLSS: a completed PR summary run whose head has its window open raises nothing; with the window closed, or for another head, it raises", () => {
    const done = makePrSummaryRun("g-ps", "completed");
    expect(raisesForRun(done)).toBe(true);
    expect(shownInOpenPullRequestWindow(done)).toBe(false);

    const close = markPullRequestWindowOpen("feature/panel");
    expect(shownInOpenPullRequestWindow(done)).toBe(true);
    expect(shownInOpenPullRequestWindow(makePrSummaryRun("g-other", "completed", {
      prSummary: { head: "feature/other" },
    }))).toBe(false);
    // A run that failed is news even under the window.
    expect(shownInOpenPullRequestWindow(makePrSummaryRun("g-ps", "failed"))).toBe(false);
    // A draft run is never held back.
    expect(shownInOpenPullRequestWindow(makeRun("g-draft", "completed"))).toBe(false);
    close();
    expect(shownInOpenPullRequestWindow(done)).toBe(false);
  });
});

describe("raiseEntry (GRU-FR-BLSS, GRU-FR-KWQX)", () => {
  const AT = "2026-11-08T09:00:00Z";
  const resting = (over: Parameters<typeof makeRun>[2] = {}) =>
    makeRun("r1", "awaiting_author", {
      rest: { reason: "plan_update", at: AT, threadId: "pt-1", setId: "qs-1" },
      ...over,
    });

  it("GRU-FR-BLSS, GRU-FR-KWQX: a change of a run that stays in its rest is the same entry", () => {
    const first = resting();
    const handedOver = resting({
      rest: {
        reason: "plan_update",
        at: AT,
        threadId: "pt-1",
        setId: "qs-1",
        answers: [{ questionPosition: 1, ownAnswer: "The header." }],
      },
      updatedAt: "2026-11-08T09:05:00Z",
    });
    expect(raiseEntry(handedOver)).toBe(raiseEntry(first));
  });

  it("GRU-FR-BLSS, GRU-FR-KWQX: a new rest, a new escalation, a new blocker attempt, or another state is a new entry", () => {
    const first = resting();
    expect(raiseEntry(resting({ rest: { reason: "plan_findings", at: AT } }))).not.toBe(raiseEntry(first));
    expect(raiseEntry(resting({ rest: { reason: "plan_update", at: "2026-11-08T10:00:00Z" } }))).not.toBe(
      raiseEntry(first),
    );
    const escalated = (raisedAt: string) =>
      makeRun("r1", "awaiting_author", { escalation: makeEscalation({ raisedAt }) });
    expect(raiseEntry(escalated(AT))).toBe(raiseEntry(escalated(AT)));
    expect(raiseEntry(escalated(AT))).not.toBe(raiseEntry(escalated("2026-11-08T10:00:00Z")));
    const blocked = (attempt: number) =>
      makeRun("r1", "blocked", { blocker: { code: "stream_dirty", message: "m", clearsBy: "c", attempt } });
    expect(raiseEntry(blocked(1))).not.toBe(raiseEntry(blocked(2)));
    expect(raiseEntry(makeRun("r1", "failed"))).not.toBe(raiseEntry(makeRun("r1", "completed")));
    expect(raiseEntry(makeRun("r2", "failed"))).not.toBe(raiseEntry(makeRun("r1", "failed")));
  });
});

describe("seedRaisedEntries (GRU-FR-BLSS)", () => {
  it("GRU-FR-BLSS: the runs that rest when a project opens count as raised, and a raised entry is kept", () => {
    const resting = makeRun("r1", "awaiting_author", {
      rest: { reason: "plan_update", at: "2026-11-08T09:00:00Z", threadId: "t", setId: "s" },
    });
    const working = makeRun("r2", "working");
    const raised = new Map<string, string>([["r3", "kept"]]);
    const again = makeRun("r3", "awaiting_author", {
      rest: { reason: "plan_findings", at: "2026-11-08T09:00:00Z", threadId: "t" },
    });
    expect(seedRaisedEntries(raised, [resting, working, again])).toBe(1);
    expect(raised.get("r1")).toBe(raiseEntry(resting));
    expect(raised.has("r2")).toBe(false);
    expect(raised.get("r3")).toBe("kept");
  });
});
