import { describe, expect, it } from "vitest";
import { indentSource, outdentSource } from "./indent";

/** The text with `[` and `]` marking the selection, or `|` the caret. */
function parse(marked: string) {
  const caret = marked.indexOf("|");
  if (caret !== -1) return { value: marked.replace("|", ""), start: caret, end: caret };
  const start = marked.indexOf("[");
  const end = marked.indexOf("]") - 1;
  return { value: marked.replace("[", "").replace("]", ""), start, end };
}

function show({ value, start, end }: { value: string; start: number; end: number }) {
  if (start === end) return value.slice(0, start) + "|" + value.slice(start);
  return value.slice(0, start) + "[" + value.slice(start, end) + "]" + value.slice(end);
}

const indent = (marked: string) => {
  const { value, start, end } = parse(marked);
  return show(indentSource(value, start, end));
};
const outdent = (marked: string) => {
  const { value, start, end } = parse(marked);
  return show(outdentSource(value, start, end));
};

describe("indentSource (EDT-FR-UYBZ)", () => {
  it("EDT-FR-UYBZ: Tab inserts two spaces at the caret, and the caret goes after them", () => {
    expect(indent("graph TD\n|A-->B")).toBe("graph TD\n  |A-->B");
    expect(indent("A|B")).toBe("A  |B");
    expect(indent("|")).toBe("  |");
  });

  it("EDT-FR-UYBZ: a selection on one line is replaced by the indent", () => {
    expect(indent("A[-->]B")).toBe("A  |B");
  });

  it("EDT-FR-UYBZ: a selection that spans lines indents each line it touches, and keeps the selection over them", () => {
    expect(indent("graph TD\nA[-->B\nB-->]C\nD")).toBe("graph TD\n  A[-->B\n  B-->]C\nD");
    // A selection from the start of a line keeps its start at the line start.
    expect(indent("x\n[A\nB]\ny")).toBe("x\n[  A\n  B]\ny");
  });

  it("EDT-FR-UYBZ: a selection that ends at the start of a line does not indent that line", () => {
    expect(indent("[A\nB\n]C")).toBe("[  A\n  B\n]C");
  });
});

describe("outdentSource (EDT-FR-UYBZ)", () => {
  it("EDT-FR-UYBZ: Shift+Tab removes up to two leading spaces from the line of the caret", () => {
    expect(outdent("x\n    A|-->B")).toBe("x\n  A|-->B");
    expect(outdent(" A|")).toBe("A|");
    // A caret inside the removed spaces goes to the start of the line.
    expect(outdent("  | A")).toBe("| A");
    expect(outdent(" | A")).toBe("|A");
  });

  it("EDT-FR-UYBZ: Shift+Tab outdents each line of a selection, and a line with no leading space stays", () => {
    expect(outdent("  [A\nB\n    C]\nD")).toBe("[A\nB\n  C]\nD");
  });

  it("EDT-FR-UYBZ: at offset 0 of a source that starts with an empty line, the empty first line is the line of the caret", () => {
    expect(outdent("|\n  abc")).toBe("|\n  abc");
    expect(indent("[\nA]")).toBe("[  \n  A]");
    expect(outdent("[\n  A]")).toBe("[\nA]");
  });

  it("EDT-FR-UYBZ: a line that starts with a tab or text is not changed", () => {
    expect(outdent("\tA|")).toBe("\tA|");
    expect(outdent("A|")).toBe("A|");
  });
});
