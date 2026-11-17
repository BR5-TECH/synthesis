/**
 * MPV-FR-WYTL .. MPV-FR-PVXW: the body block diff of a plan proposal row.
 */
import { describe, expect, it } from "vitest";

import { bodyCountLabel, diffBody, mergedWordPieces, type BodyBlock, type BodyItem } from "./bodyDiff";

/** The blocks of a diff as `mode:kind:text`, folds as `fold(n)`. */
function shape(items: readonly BodyItem[]): string[] {
  return items.map((item) =>
    item.type === "fold"
      ? `fold(${item.blocks.length})`
      : `${item.block.mode}:${item.block.kind}:${text(item.block)}`,
  );
}

function text(block: BodyBlock): string {
  return block.pieces
    .map((p) => (p.mark === "ins" ? `[+${p.text}]` : p.mark === "del" ? `[-${p.text}]` : p.text))
    .join("");
}

describe("block alignment (MPV-FR-WYTL)", () => {
  it("MPV-FR-WYTL: an added block is `add` and a removed block is `del`", () => {
    const diff = diffBody("## A\n\nkeep\n", "## A\n\nkeep\n\n- new item\n");
    expect(shape(diff.items)).toEqual(["ctx:h2:A", "ctx:p:keep", "add:li:new item"]);
    expect(diff.added).toBe(1);
    expect(diff.removed).toBe(0);
  });

  it("MPV-FR-WYTL: a removed block keeps its kind and its text", () => {
    const diff = diffBody("## A\n\nOld paragraph that goes.\n", "## A\n");
    expect(shape(diff.items)).toEqual(["ctx:h2:A", "del:p:Old paragraph that goes."]);
    expect(diff.removed).toBe(1);
  });

  it("MPV-FR-WYTL / MPV-FR-ZFDO: a new body is every block added", () => {
    const diff = diffBody(null, "## Description\n\nText.\n\n- one\n- two\n");
    expect(diff.items.every((i) => i.type === "block" && i.block.mode === "add")).toBe(true);
    expect(diff.added).toBe(4);
  });

  it("MPV-FR-WYTL: an emptied body is every block removed", () => {
    const diff = diffBody("Text.\n\n- one\n", "");
    expect(shape(diff.items)).toEqual(["del:p:Text.", "del:li:one"]);
  });

  it("MPV-FR-WYTL: two empty bodies give no item", () => {
    expect(diffBody("", null)).toEqual({ items: [], added: 0, removed: 0, edited: 0 });
  });
});

describe("edited blocks (MPV-FR-PPHV)", () => {
  it("MPV-FR-PPHV: a reworded paragraph is one edited block with inline marks", () => {
    const diff = diffBody(
      "Workflow Engine is expected to reach out to LLM models.\n",
      "Workflow Engine calls LLM models.\n",
    );
    expect(diff.edited).toBe(1);
    expect(diff.added + diff.removed).toBe(0);
    const [only] = diff.items;
    expect(only.type === "block" && only.block.mode).toBe("edit");
    expect(shape(diff.items)[0]).toBe("edit:p:Workflow Engine [-is expected to reach out to][+calls] LLM models.");
  });

  it("MPV-FR-PPHV: an insertion inside a list item marks only the inserted words", () => {
    const diff = diffBody(
      "- Model, context size, thinking effort can be configured.\n",
      "- Model, context size, thinking effort, retries can be configured.\n",
    );
    expect(shape(diff.items)).toEqual(["edit:li:Model, context size, thinking effort[+, retries] can be configured."]);
  });

  it("MPV-FR-PPHV: a replaced block of another kind is a removal and an addition", () => {
    const diff = diffBody("Plain words here.\n", "## Plain words here\n");
    expect(shape(diff.items)).toEqual(["del:p:Plain words here.", "add:h2:Plain words here"]);
    expect(diff.edited).toBe(0);
  });

  it("MPV-FR-PPHV: a rewrite with too few shared words is a removal and an addition", () => {
    const diff = diffBody("Old paragraph that is replaced as a whole.\n", "New text unlike anything.\n");
    expect(shape(diff.items)).toEqual([
      "del:p:Old paragraph that is replaced as a whole.",
      "add:p:New text unlike anything.",
    ]);
  });

  it("MPV-FR-PPHV / MPV-FR-PVXW: inline code keeps its face inside an edited block", () => {
    const diff = diffBody(
      "The `chat` endpoint is available.\n",
      "The `chat` and `responses` endpoints are available.\n",
    );
    const block = diff.items[0].type === "block" ? diff.items[0].block : null;
    expect(block?.mode).toBe("edit");
    expect(block?.pieces.filter((p) => p.style === "code").map((p) => [p.text, p.mark ?? null])).toEqual([
      ["chat", null],
      ["responses", "ins"],
    ]);
  });

  it("MPV-FR-PPHV: mergedWordPieces keeps every character of the new text outside `del`", () => {
    const merged = mergedWordPieces(
      [{ text: "alpha beta gamma delta", style: "text" }],
      [{ text: "alpha BETA gamma delta epsilon", style: "text" }],
    );
    expect(merged).not.toBeNull();
    expect(merged!.filter((p) => p.mark !== "del").map((p) => p.text).join("")).toBe(
      "alpha BETA gamma delta epsilon",
    );
    expect(merged!.filter((p) => p.mark !== "ins").map((p) => p.text).join("")).toBe("alpha beta gamma delta");
  });
});

describe("typesetting (MPV-FR-PVXW)", () => {
  it("MPV-FR-PVXW: headings, paragraphs, and list items get their kinds; strong text keeps its face", () => {
    const diff = diffBody(null, "## Two\n\n### Three\n\nText with **bold**.\n\n  - nested\n");
    const blocks = diff.items.flatMap((i) => (i.type === "block" ? [i.block] : i.blocks));
    expect(blocks.map((b) => b.kind)).toEqual(["h2", "h3", "p", "li"]);
    expect(blocks[2].pieces).toContainEqual({ text: "bold", style: "strong" });
    expect(blocks[3].depth).toBeGreaterThan(0);
  });

  it("MPV-FR-PVXW: a rule carries no block, and a code fence shows as its plain text", () => {
    const diff = diffBody(null, "Text.\n\n---\n\n```\nlet a = 1;\n```\n");
    expect(shape(diff.items)).toEqual(["add:p:Text.", "add:code:let a = 1;"]);
  });
});

describe("folds (MPV-FR-VMYF)", () => {
  const body = (middle: string) => ["A one.", "B two.", "C three.", "D four.", middle].join("\n\n") + "\n";

  it("MPV-FR-VMYF: a run of unchanged blocks no change touches folds into one item", () => {
    const diff = diffBody(body("E five."), body("E five, changed."));
    expect(shape(diff.items)).toEqual(["fold(3)", "ctx:p:D four.", "edit:p:E five[+, changed]."]);
  });

  it("MPV-FR-VMYF: an unchanged block next to a change shows in full on each side", () => {
    const diff = diffBody("A.\n\nB.\n\nC.\n", "A.\n\nB, now.\n\nC.\n");
    expect(diff.items.every((i) => i.type === "block")).toBe(true);
  });

  it("MPV-FR-VMYF: a single unchanged block away from a change does not fold", () => {
    const before = "A one.\n\nB two.\n\nC three.\n\nD four.\n\nE five.\n\nF six.\n";
    const after = "A one.\n\nB two, changed.\n\nC three.\n\nD four.\n\nE five.\n\nF six, changed.\n";
    const diff = diffBody(before, after);
    expect(shape(diff.items)).toEqual([
      "ctx:p:A one.",
      "edit:p:B two[+, changed].",
      "ctx:p:C three.",
      "ctx:p:D four.",
      "ctx:p:E five.",
      "edit:p:F six[+, changed].",
    ]);
  });

  it("MPV-FR-VMYF: an unchanged body folds whole", () => {
    const diff = diffBody("A.\n\nB.\n\nC.\n", "A.\n\nB.\n\nC.\n");
    expect(shape(diff.items)).toEqual(["fold(3)"]);
  });
});

describe("the count of the label strip (MPV-FR-VMYF)", () => {
  it("MPV-FR-VMYF: names the added, removed, and edited blocks, the first with its noun", () => {
    expect(bodyCountLabel({ items: [], added: 3, removed: 0, edited: 2 })).toBe("3 blocks added, 2 edited");
    expect(bodyCountLabel({ items: [], added: 1, removed: 1, edited: 1 })).toBe("1 block added, 1 removed, 1 edited");
    expect(bodyCountLabel({ items: [], added: 0, removed: 2, edited: 0 })).toBe("2 blocks removed");
    expect(bodyCountLabel({ items: [], added: 0, removed: 0, edited: 1 })).toBe("1 block edited");
  });

  it("MPV-FR-VMYF: an unchanged body has no count", () => {
    expect(bodyCountLabel({ items: [], added: 0, removed: 0, edited: 0 })).toBeNull();
  });
});

describe("grouped word marks (MPV-FR-PPHV)", () => {
  it("MPV-FR-PPHV: neighbouring changed words read as one deletion and one insertion", () => {
    const diff = diffBody(
      "Card numbers are expected to be tokenized before they reach the service.\n",
      "Card numbers are tokenized in the browser before they reach the service.\n",
    );
    expect(shape(diff.items)).toEqual([
      "edit:p:Card numbers are [-expected to be tokenized][+tokenized in the browser] before they reach the service.",
    ]);
  });
});
