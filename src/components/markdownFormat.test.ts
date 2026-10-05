import { describe, expect, it } from "vitest";

import { applyMarkdown, type MarkdownAction } from "./markdownFormat";

/** Renders a case as `text` with the selection marked, for readable assertions. */
function run(input: string, action: MarkdownAction): string {
  const start = input.indexOf("[");
  const end = input.indexOf("]") - 1;
  const text = input.replace("[", "").replace("]", "");
  const out = applyMarkdown({ text, start, end }, action);
  return (
    out.text.slice(0, out.start) +
    "[" +
    out.text.slice(out.start, out.end) +
    "]" +
    out.text.slice(out.end)
  );
}

describe("inline marks (NAW-FR-10)", () => {
  it("wraps the selection and keeps it naming the same characters", () => {
    expect(run("[body] text", "bold")).toBe("**[body]** text");
    expect(run("[body] text", "italic")).toBe("*[body]* text");
    expect(run("[body] text", "code")).toBe("`[body]` text");
  });

  it("a second activation removes exactly what the first added", () => {
    expect(run("**[body]** text", "bold")).toBe("[body] text");
    expect(run("`[x]`", "code")).toBe("[x]");
  });

  it("italic leaves a bold span alone rather than peeling one marker off it", () => {
    // `*` is a prefix of `**`: without the guard this returned `*[body]* text`,
    // which is neither bold nor valid.
    expect(run("**[body]** text", "italic")).toBe("***[body]*** text");
  });

  it("wraps an empty selection so the markers are typed into", () => {
    expect(run("a[]b", "bold")).toBe("a**[]**b");
  });
});

describe("block marks (NAW-FR-10)", () => {
  it("prefixes the line the caret is on", () => {
    expect(run("one\nt[]wo", "h2")).toBe("one\n[## two]");
    expect(run("[]alpha", "quote")).toBe("[> alpha]");
  });

  it("replaces a prefix of the same family rather than stacking one on it", () => {
    expect(run("[## two]", "h1")).toBe("[# two]");
    expect(run("[- item]", "ordered")).toBe("[1. item]");
    expect(run("[### x]", "bullet")).toBe("[- x]");
  });

  it("a second activation of the same mark strips it", () => {
    expect(run("[## two]", "h2")).toBe("[two]");
    expect(run("[- item]", "bullet")).toBe("[item]");
    expect(run("[> q]", "quote")).toBe("[q]");
  });

  it("numbers a multi-line selection in order", () => {
    expect(run("[a\nb\nc]", "ordered")).toBe("[1. a\n2. b\n3. c]");
  });

  it("completes a partly-marked selection rather than clearing it", () => {
    // Dragging across a half-formatted list and hitting the button means "make
    // these a list", not "unmake the ones that already are".
    expect(run("[- a\nb]", "bullet")).toBe("[- a\n- b]");
  });

  it("marks an empty document rather than reading it as already marked", () => {
    // An empty document is where a heading is typed, not where one is removed:
    // treating "no line carries the mark" as "every line does" made the whole
    // toolbar inert on a fresh draft.
    expect(run("[]", "bullet")).toBe("[- ]");
    expect(run("[]", "h1")).toBe("[# ]");
    expect(run("[]", "quote")).toBe("[> ]");
  });

  it("marks a blank line inside a document", () => {
    expect(run("one\n[]\ntwo", "bullet")).toBe("one\n[- ]\ntwo");
  });

  it("keeps indentation ahead of the prefix", () => {
    expect(run("[  nested]", "bullet")).toBe("[  - nested]");
  });

  it("leaves blank lines inside a multi-line selection unmarked", () => {
    expect(run("[a\n\nb]", "bullet")).toBe("[- a\n\n- b]");
  });

  it("expands a partial-line selection to the whole line it sits in", () => {
    expect(run("he[ad]ing", "h1")).toBe("[# heading]");
  });
});
