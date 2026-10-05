import { beforeEach, describe, expect, it } from "vitest";

import {
  draftedPositions,
  forgetEveryDraft,
  forgetRun,
  forgetRunsExcept,
  recallAnswer,
  reconcile,
  rememberAnswer,
} from "./escalationDrafts";

// GRU-graduation-runs.md GEA-FR-IHIZ: the unsent answer draft, and the one thing
// a re-read does not replace.

beforeEach(forgetEveryDraft);

describe("the unsent answer draft (GEA-FR-IHIZ)", () => {
  it("holds every part of what was entered, keyed by run and position", () => {
    // Not the effective answer computed from them (GEA-FR-UWLK): a re-render
    // restores exactly what the author left rather than a value they never
    // typed — including the response they had selected before they moved to
    // their own words, so changing their mind back finds it still chosen.
    rememberAnswer("run-a", 3, {
      selected: "fail",
      ownWords: true,
      typed: "and also this",
    });
    expect(recallAnswer("run-a", 3)).toEqual({
      selected: "fail",
      ownWords: true,
      typed: "and also this",
    });
    // Keyed by the recorded position, never by the page showing.
    expect(recallAnswer("run-a", 1)).toEqual({
      selected: null,
      ownWords: false,
      typed: "",
    });
    // And no draft of one run is ever read for another.
    expect(recallAnswer("run-b", 3)).toEqual({
      selected: null,
      ownWords: false,
      typed: "",
    });
  });

  it("removes an entry that holds nothing at all", () => {
    // Clearing a field the author had filled in leaves the question
    // unanswered rather than answered with nothing.
    rememberAnswer("run-a", 1, {
      selected: null,
      ownWords: false,
      typed: "something",
    });
    expect(draftedPositions("run-a")).toEqual([1]);
    rememberAnswer("run-a", 1, { selected: null, ownWords: false, typed: "" });
    expect(draftedPositions("run-a")).toEqual([]);
  });

  it("keeps an own-words row that is selected but not yet written in", () => {
    // GEA-FR-UWLK: the author pressed that row, so it is pressed. Dropped as an
    // empty entry, the radio would render unpressed under them.
    rememberAnswer("run-a", 1, { selected: null, ownWords: true, typed: "" });
    expect(draftedPositions("run-a")).toEqual([1]);
    expect(recallAnswer("run-a", 1).ownWords).toBe(true);
  });

  it("reconciles a re-read rather than replacing it", () => {
    // GEA-FR-VBUG, GEA-FR-HAKV, GEA-FR-FFKD, GRU-FR-ZVTC: every entry whose position the re-read run still records is
    // kept against that position, and one for a position it no longer records
    // is dropped with it.
    rememberAnswer("run-a", 1, { selected: "fail", ownWords: false, typed: "" });
    rememberAnswer("run-a", 2, {
      selected: null,
      ownWords: true,
      typed: "in the ui spec",
    });
    rememberAnswer("run-a", 4, { selected: "both", ownWords: false, typed: "" });

    reconcile("run-a", [1, 2, 3]);
    expect(draftedPositions("run-a").sort()).toEqual([1, 2]);
    expect(recallAnswer("run-a", 1)).toEqual({
      selected: "fail",
      ownWords: false,
      typed: "",
    });
    expect(recallAnswer("run-a", 2)).toEqual({
      selected: null,
      ownWords: true,
      typed: "in the ui spec",
    });
    expect(recallAnswer("run-a", 4)).toEqual({
      selected: null,
      ownWords: false,
      typed: "",
    });
  });

  it("is discarded when its run is answered, and when its run leaves the queue", () => {
    rememberAnswer("run-a", 1, { selected: "fail", ownWords: false, typed: "" });
    rememberAnswer("run-b", 1, { selected: "drop", ownWords: false, typed: "" });

    forgetRun("run-a");
    expect(draftedPositions("run-a")).toEqual([]);
    // Scoped to the run: forgetting one leaves the other whole.
    expect(draftedPositions("run-b")).toEqual([1]);

    rememberAnswer("run-c", 2, { selected: null, ownWords: true, typed: "yes" });
    forgetRunsExcept(["run-c"]);
    expect(draftedPositions("run-b")).toEqual([]);
    expect(draftedPositions("run-c")).toEqual([2]);
  });
});
