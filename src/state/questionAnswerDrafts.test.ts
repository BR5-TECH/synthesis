import { beforeEach, describe, expect, it } from "vitest";
import {
  draftedPositions,
  forgetEveryDraft,
  forgetSet,
  recallAnswer,
  reconcile,
  rememberAnswer,
} from "./questionAnswerDrafts";

describe("the unsent answer draft", () => {
  beforeEach(() => {
    forgetEveryDraft();
  });

  it("DQA-FR-EGZS: holds the selected option and the note, per question", () => {
    rememberAnswer("s1", 2, { selected: 1, ownWords: false, typed: "", note: "keep the diagram here" });
    expect(recallAnswer("s1", 2)).toEqual({
      selected: 1,
      ownWords: false,
      typed: "",
      note: "keep the diagram here",
    });
  });

  it("DQA-FR-EGZS: a question nothing was entered against reads as empty", () => {
    expect(recallAnswer("s1", 1)).toEqual({ selected: null, ownWords: false, typed: "", note: "" });
  });

  it("DQA-FR-CBQK: is keyed by the set id and the recorded position", () => {
    rememberAnswer("s1", 1, { selected: 2, ownWords: false, typed: "", note: "" });
    // The same position under another set reads nothing: a set that replaced
    // another starts clean (DQA-FR-GVSA).
    expect(recallAnswer("s2", 1)).toEqual({ selected: null, ownWords: false, typed: "", note: "" });
    // And another position under the same set likewise.
    expect(recallAnswer("s1", 2)).toEqual({ selected: null, ownWords: false, typed: "", note: "" });
  });

  it("DQA-FR-EGZS: holds the author's own words as well as a selection", () => {
    rememberAnswer("s1", 1, {
      selected: null,
      ownWords: true,
      typed: "three, one per layer",
      note: "",
    });
    expect(recallAnswer("s1", 1)).toEqual({
      selected: null,
      ownWords: true,
      typed: "three, one per layer",
      note: "",
    });
  });

  it("DQA-FR-FCZL: the own row chosen with nothing typed is a state, and is kept", () => {
    // Dropping the entry would put the radio back to no answer chosen, which
    // the author never did.
    rememberAnswer("s1", 1, { selected: null, ownWords: true, typed: "", note: "" });
    expect(recallAnswer("s1", 1).ownWords).toBe(true);
  });

  it("DQA-FR-EGZS: a selection is kept beneath the own words that stand over it", () => {
    // The two are one choice (DQA-FR-HLDS), and keeping the selection is what
    // lets an author try their own words and go back to the option they had.
    rememberAnswer("s1", 1, { selected: 2, ownWords: true, typed: "mine", note: "" });
    const held = recallAnswer("s1", 1);
    expect(held.selected).toBe(2);
    expect(held.ownWords).toBe(true);
  });

  it("DQA-FR-DYFR, DQA-FR-HCTM: own words survive a reconcile and go with the set", () => {
    rememberAnswer("s1", 1, { selected: null, ownWords: true, typed: "mine", note: "" });
    rememberAnswer("s1", 2, { selected: null, ownWords: true, typed: "other", note: "" });
    reconcile("s1", [1]);
    expect(recallAnswer("s1", 1).typed).toBe("mine");
    expect(recallAnswer("s1", 2)).toEqual({
      selected: null,
      ownWords: false,
      typed: "",
      note: "",
    });
    forgetSet("s1");
    expect(recallAnswer("s1", 1).typed).toBe("");
  });

  it("DQA-FR-UAKC: a note alone is held, and leaves the question unanswered", () => {
    rememberAnswer("s1", 1, { selected: null, ownWords: false, typed: "", note: "a thought" });
    const held = recallAnswer("s1", 1);
    expect(held.note).toBe("a thought");
    // The note never stands in for a choice.
    expect(held.selected).toBeNull();
  });

  it("DQA-FR-EGZS: an entry holding nothing at all leaves no trace", () => {
    rememberAnswer("s1", 1, { selected: 1, ownWords: false, typed: "", note: "typed" });
    rememberAnswer("s1", 1, { selected: null, ownWords: false, typed: "", note: "" });
    expect(draftedPositions("s1")).toEqual([]);
  });

  it("DQA-FR-EGZS: clearing a note keeps the selection that stands beside it", () => {
    rememberAnswer("s1", 1, { selected: 2, ownWords: false, typed: "", note: "typed" });
    rememberAnswer("s1", 1, { selected: 2, ownWords: false, typed: "", note: "" });
    expect(recallAnswer("s1", 1)).toEqual({ selected: 2, ownWords: false, typed: "", note: "" });
  });

  it("DQA-FR-DYFR: a re-read reconciles rather than replaces", () => {
    rememberAnswer("s1", 1, { selected: 1, ownWords: false, typed: "", note: "one" });
    rememberAnswer("s1", 2, { selected: 2, ownWords: false, typed: "", note: "two" });
    rememberAnswer("s1", 3, { selected: 1, ownWords: false, typed: "", note: "three" });

    // The re-read set no longer records position 2.
    reconcile("s1", [1, 3]);

    expect(recallAnswer("s1", 1)).toEqual({ selected: 1, ownWords: false, typed: "", note: "one" });
    expect(recallAnswer("s1", 3)).toEqual({ selected: 1, ownWords: false, typed: "", note: "three" });
    expect(recallAnswer("s1", 2)).toEqual({ selected: null, ownWords: false, typed: "", note: "" });
    // Nothing moved: an entry belongs to a position, not to a place in a list.
    expect(draftedPositions("s1").sort()).toEqual([1, 3]);
  });

  it("DQA-FR-LSNW: survives a re-read that records every position", () => {
    rememberAnswer("s1", 1, { selected: 1, ownWords: false, typed: "", note: "one" });
    reconcile("s1", [1, 2, 3]);
    expect(recallAnswer("s1", 1)).toEqual({ selected: 1, ownWords: false, typed: "", note: "one" });
  });

  it("DQA-FR-HCTM: the draft goes when the set it belongs to does", () => {
    rememberAnswer("s1", 1, { selected: 1, ownWords: false, typed: "", note: "one" });
    rememberAnswer("s2", 1, { selected: 2, ownWords: false, typed: "", note: "other" });

    forgetSet("s1");

    expect(recallAnswer("s1", 1)).toEqual({ selected: null, ownWords: false, typed: "", note: "" });
    // And no other set's draft went with it.
    expect(recallAnswer("s2", 1)).toEqual({ selected: 2, ownWords: false, typed: "", note: "other" });
  });
});
