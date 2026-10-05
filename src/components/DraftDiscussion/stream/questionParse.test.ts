/**
 * Reading a submitted exchange back as one card
 * (`../../../../specifications/ui/DQA-discussion-question-answering.md`).
 *
 * The bodies these cases use are the ones
 * `../../../../src-tauri/src/comments/question_submit.rs` composes. Where a case
 * quotes a body it quotes it exactly, because the whole of the recognition is
 * that prose: ADQ-FR-NUEB forbids a marker in either comment, so a reader that
 * drifted from the composer would silently stop pairing anything.
 */
import { describe, expect, it } from "vitest";

import { questionCardOf, unescapeMarkdown } from "./questionParse";

const QUESTION =
  "If asset upload succeeds but issue creation fails, what should happen?\n\n" +
  "1. Delete uploaded assets after failure\n" +
  "2. Keep and reuse marker-scoped uploads on retry\n" +
  "3. Keep orphaned uploads and report them";

describe("the chosen option is marked in place (DQA-FR-KYWR)", () => {
  it("DQA-FR-KYWR: reads the question, its options, and which one was chosen", () => {
    const card = questionCardOf(
      QUESTION,
      "**Selected option:** Keep and reuse marker-scoped uploads on retry",
    );
    expect(card).not.toBeNull();
    expect(card!.text).toBe(
      "If asset upload succeeds but issue creation fails, what should happen?",
    );
    expect(card!.options.map((o) => o.value)).toEqual([
      "Delete uploaded assets after failure",
      "Keep and reuse marker-scoped uploads on retry",
      "Keep orphaned uploads and report them",
    ]);
    expect(card!.options.map((o) => o.chosen)).toEqual([false, true, false]);
    expect(card!.options.map((o) => o.number)).toEqual([1, 2, 3]);
  });

  it("DQA-FR-KYWR: puts the note in the footer and states the answer nowhere else", () => {
    const card = questionCardOf(
      QUESTION,
      "**Selected option:** Delete uploaded assets after failure\n\n" +
        "**Note:** Scope the marker per draft, not per repo.",
    );
    expect(card!.footer).toEqual({
      kind: "note",
      text: "Scope the marker per draft, not per repo.",
    });
    // The chosen option is the only place the answer stands.
    expect(card!.options[0].chosen).toBe(true);
  });

  it("DQA-FR-KYWR: a chosen option with no note carries no footer", () => {
    const card = questionCardOf(
      QUESTION,
      "**Selected option:** Keep orphaned uploads and report them",
    );
    expect(card!.footer).toBeNull();
    expect(card!.options[2].chosen).toBe(true);
  });

  it("DQA-FR-KYWR: an exchange with no answer yet marks nothing", () => {
    const card = questionCardOf(QUESTION, null);
    expect(card!.options.every((o) => !o.chosen)).toBe(true);
    expect(card!.footer).toBeNull();
  });
});

describe("an answer in the author's own words (DQA-FR-ZPGM)", () => {
  it("DQA-FR-ZPGM: marks no option and carries the words in the footer", () => {
    const card = questionCardOf(
      QUESTION,
      "**Own answer:** three — one per layer and one for the join",
    );
    expect(card!.options.every((o) => !o.chosen)).toBe(true);
    expect(card!.footer).toEqual({
      kind: "own",
      text: "three — one per layer and one for the join",
    });
  });
});

describe("the composer's escaping is taken back off (DQA-FR-KYWR)", () => {
  it("DQA-FR-KYWR: an option carrying Markdown punctuation reads as it was chosen", () => {
    // `escape_markdown` writes the backslashes; the card takes them out again,
    // so what the author chose is what the author reads.
    const card = questionCardOf(
      "Which style?\n\n1. use \\*bold\\* and \\`code\\`\n2. plain",
      "**Selected option:** use \\*bold\\* and \\`code\\`",
    );
    expect(card!.options[0].value).toBe("use *bold* and `code`");
    expect(card!.options[0].chosen).toBe(true);
  });

  it("DQA-FR-KYWR: a backslash the composer never wrote survives", () => {
    // Only the characters the renderer consumes are escapes. A Windows path
    // pasted into an option keeps both of its separators.
    expect(unescapeMarkdown("C:\\\\Users\\\\demo")).toBe("C:\\Users\\demo");
    expect(unescapeMarkdown("one\\.two")).toBe("one\\.two");
  });
});

describe("a body this reader cannot read (DQA-FR-TSJD)", () => {
  it("DQA-FR-TSJD: a question with no option list yields no card", () => {
    expect(questionCardOf("Just a sentence.", "**Selected option:** x")).toBeNull();
  });

  it("DQA-FR-TSJD: a body that is only an option list yields no card", () => {
    expect(questionCardOf("1. one\n2. two", null)).toBeNull();
  });

  it("DQA-FR-KYWR: a list in the question's own text stays in the text", () => {
    // The options are the run the composer appended, which is the one closest to
    // the foot. A question that quotes a numbered list keeps it.
    const card = questionCardOf(
      "The steps are:\n\n1. first\n2. second\n\nWhich one is wrong?\n\n1. first\n2. second",
      "**Selected option:** second",
    );
    expect(card!.text).toBe(
      "The steps are:\n\n1. first\n2. second\n\nWhich one is wrong?",
    );
    expect(card!.options).toHaveLength(2);
    expect(card!.options[1].chosen).toBe(true);
  });

  it("DQA-FR-TSJD: an answer body in neither fixed form marks nothing", () => {
    const card = questionCardOf(QUESTION, "I think the second one.");
    expect(card!.options.every((o) => !o.chosen)).toBe(true);
    expect(card!.footer).toBeNull();
  });
});
