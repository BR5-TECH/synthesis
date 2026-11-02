import { beforeEach, describe, expect, it } from "vitest";

import {
  adoptEscalation,
  draftedPositions,
  forgetEveryDraft,
  forgetRun,
  forgetRunsExcept,
  recallAnswer,
  recallRefusal,
  reconcile,
  rememberAnswer,
  rememberRefusal,
  streamOwner,
} from "./escalationDrafts";

// GEA-graduation-escalation-answering.md: the unsent answer draft, and the one
// thing a re-read does not replace.

beforeEach(forgetEveryDraft);

const EMPTY = { selected: null, ownWords: false, typed: "", note: "" };

describe("the unsent answer draft (GEA-FR-IHIZ)", () => {
  it("GEA-FR-IHIZ, GEA-FR-XTUE, GEA-FR-THNY: holds every part of what was entered, keyed by owner and position", () => {
    // The option chosen before the author moved to their own words is kept, so
    // changing their mind back finds it still chosen, and the note with it.
    const entry = { selected: 2, ownWords: true, typed: "and also this", note: "why" };
    rememberAnswer("run-a", 3, entry);
    expect(recallAnswer("run-a", 3)).toEqual(entry);
    // Keyed by the recorded position, never by the page showing.
    expect(recallAnswer("run-a", 1)).toEqual(EMPTY);
    // And no draft of one run is ever read for another.
    expect(recallAnswer("run-b", 3)).toEqual(EMPTY);
  });

  it("GEA-FR-IHIZ: removes an entry that holds nothing at all", () => {
    rememberAnswer("run-a", 1, { ...EMPTY, typed: "something" });
    expect(draftedPositions("run-a")).toEqual([1]);
    rememberAnswer("run-a", 1, EMPTY);
    expect(draftedPositions("run-a")).toEqual([]);
  });

  it("GEA-FR-IHIZ: keeps an entry that holds only a note", () => {
    // A note alone answers nothing (DQA-FR-UAKC), but it is what the author
    // wrote, and the draft keeps it.
    rememberAnswer("run-a", 1, { ...EMPTY, note: "later" });
    expect(recallAnswer("run-a", 1).note).toBe("later");
  });

  it("GEA-FR-IHIZ: keeps an own-answer row that is chosen but not yet written in", () => {
    // The author pressed that row, so it stays pressed.
    rememberAnswer("run-a", 1, { ...EMPTY, ownWords: true });
    expect(draftedPositions("run-a")).toEqual([1]);
    expect(recallAnswer("run-a", 1).ownWords).toBe(true);
  });

  it("GEA-FR-VBUG: reconciles a re-read rather than replacing it", () => {
    // Every entry whose position the re-read run still records is kept against
    // that position, and one for a position it no longer records is dropped.
    rememberAnswer("run-a", 1, { ...EMPTY, selected: 1 });
    rememberAnswer("run-a", 2, { ...EMPTY, ownWords: true, typed: "in the ui spec" });
    rememberAnswer("run-a", 4, { ...EMPTY, selected: 2 });

    reconcile("run-a", [1, 2, 3]);
    expect(draftedPositions("run-a").sort()).toEqual([1, 2]);
    expect(recallAnswer("run-a", 1)).toEqual({ ...EMPTY, selected: 1 });
    expect(recallAnswer("run-a", 2)).toEqual({
      ...EMPTY,
      ownWords: true,
      typed: "in the ui spec",
    });
    expect(recallAnswer("run-a", 4)).toEqual(EMPTY);
  });

  it("GEA-FR-KTME: is discarded when its run is answered, and when its run leaves the queue", () => {
    rememberAnswer("run-a", 1, { ...EMPTY, selected: 1 });
    rememberRefusal("run-a", "refused");
    rememberAnswer("run-b", 1, { ...EMPTY, selected: 2 });
    rememberRefusal("run-b", "refused too");

    forgetRun("run-a");
    expect(draftedPositions("run-a")).toEqual([]);
    expect(recallRefusal("run-a")).toBeNull();
    // Scoped to the run: forgetting one leaves the other whole.
    expect(draftedPositions("run-b")).toEqual([1]);

    rememberAnswer("run-c", 2, { ...EMPTY, ownWords: true, typed: "yes" });
    forgetRunsExcept(["run-c"]);
    expect(draftedPositions("run-b")).toEqual([]);
    expect(recallRefusal("run-b")).toBeNull();
    expect(draftedPositions("run-c")).toEqual([2]);
  });

  it("GEA-FR-TEMG: holds the refusal the last submission met, per owner, until it is cleared", () => {
    rememberRefusal("run-a", "A branch moved.");
    expect(recallRefusal("run-a")).toBe("A branch moved.");
    expect(recallRefusal("run-b")).toBeNull();
    rememberRefusal("run-a", null);
    expect(recallRefusal("run-a")).toBeNull();
  });

  it("GEA-FR-KTME: a later escalation of the same owner does not read the draft or refusal of an earlier one", () => {
    adoptEscalation("run-a", "2026-01-01T00:00:00Z");
    rememberAnswer("run-a", 1, { ...EMPTY, selected: 1 });
    rememberRefusal("run-a", "refused");
    // The same escalation re-read keeps everything.
    adoptEscalation("run-a", "2026-01-01T00:00:00Z");
    expect(recallAnswer("run-a", 1).selected).toBe(1);
    expect(recallRefusal("run-a")).toBe("refused");
    // Another escalation drops both.
    adoptEscalation("run-a", "2026-01-02T00:00:00Z");
    expect(recallAnswer("run-a", 1)).toEqual(EMPTY);
    expect(recallRefusal("run-a")).toBeNull();
  });

  it("GEA-FR-LWQD, GEA-FR-KTME: the run listing's clean-up leaves a work stream update's draft alone", () => {
    const owner = streamOwner("w1");
    rememberAnswer(owner, 1, { ...EMPTY, typed: "kept", ownWords: true });
    rememberRefusal(owner, "refused");
    forgetRunsExcept(["r1"]);
    expect(recallAnswer(owner, 1).typed).toBe("kept");
    expect(recallRefusal(owner)).toBe("refused");
  });

  it("GEA-FR-IHIZ: the empty answer an unknown owner reads cannot be changed by its reader", () => {
    const empty = recallAnswer("nobody", 1);
    expect(() => {
      (empty as { typed: string }).typed = "x";
    }).toThrow();
    expect(recallAnswer("nobody", 2).typed).toBe("");
  });
});
