import { describe, expect, it } from "vitest";

import {
  analyzeFrontmatter,
  DESCRIPTION_LIMIT,
  type YamlRole,
} from "./frontmatterYaml";

/** The substrings covered by each role, in document order. */
function rolesOf(text: string): Array<[YamlRole, string]> {
  return analyzeFrontmatter(text).spans.map((s) => [
    s.role,
    text.slice(s.start, s.end),
  ]);
}

function textFor(text: string, role: YamlRole): string[] {
  return rolesOf(text)
    .filter(([r]) => r === role)
    .map(([, t]) => t);
}

describe("analyzeFrontmatter — roles (EDT-FR-21)", () => {
  it("marks mapping keys at every nesting depth", () => {
    const src = "name: onboarding\nmetadata:\n  type: user\n  nested:\n    deep: v\n";
    expect(textFor(src, "key")).toEqual([
      "name",
      "metadata",
      "type",
      "nested",
      "deep",
    ]);
  });

  it("marks the `:` separator and list markers as receding, not as key or value", () => {
    const src = "tags:\n  - setup\n  - review\n";
    const punct = textFor(src, "punct");
    expect(punct.filter((t) => t === ":")).toHaveLength(1);
    expect(punct.filter((t) => t === "-")).toHaveLength(2);
    // The list items themselves are values — they carry no span at all.
    expect(rolesOf(src).map(([, t]) => t)).not.toContain("setup");
  });

  it("marks a key inside a list of maps as a key and the `- ` marker as receding", () => {
    const src = "list:\n  - foo: bar\n";
    expect(textFor(src, "key")).toEqual(["list", "foo"]);
    expect(textFor(src, "punct")).toContain("-");
  });

  it("marks quoting characters as receding and the quoted text as the key", () => {
    const src = '"quoted key": plain\n';
    expect(textFor(src, "key")).toEqual(["quoted key"]);
    expect(textFor(src, "punct").filter((t) => t === '"')).toHaveLength(2);
  });

  it("leaves a quoted value unmarked but recedes its quotes", () => {
    const src = 'quoted: "a: b"\n';
    // Only `quoted` is a key — the colon inside the quoted scalar is not a
    // separator, which is exactly what a parse gets right and a line scan does not.
    expect(textFor(src, "key")).toEqual(["quoted"]);
    expect(textFor(src, "punct").filter((t) => t === ":")).toHaveLength(1);
  });

  it("marks a block-scalar indicator as receding and its folded lines as value", () => {
    const src = "description: >\n  line one\n  line two\n";
    expect(textFor(src, "punct")).toContain(">");
    // The folded body carries no span at all — not part of one, not overlapping
    // one. (Asserting the whole body is absent from the span texts would be
    // vacuous: a span is always a single token's source.)
    const bodyStart = src.indexOf("  line one");
    expect(
      analyzeFrontmatter(src).spans.every(
        (s) => s.end <= bodyStart || s.start >= src.length,
      ),
    ).toBe(true);
  });

  it("marks the `?` of an explicit key, and the key after it", () => {
    expect(rolesOf("? name\n: value\n")).toEqual([
      ["punct", "?"],
      ["key", "name"],
      ["punct", ":"],
    ]);
  });

  it("recedes an anchor and a tag, and leaves an aliased value at value weight", () => {
    const src = "anchored: &a x\ntagged: !!str 42\nref: *a\n";
    expect(textFor(src, "key")).toEqual(["anchored", "tagged", "ref"]);
    const punct = textFor(src, "punct");
    expect(punct).toContain("&a");
    expect(punct).toContain("!!str");
    expect(punct).not.toContain("*a");
  });

  it("keeps offsets aligned through CRLF line endings", () => {
    const src = "name: Foo\r\ndescription: hello\r\n";
    expect(rolesOf(src)).toEqual([
      ["key", "name"],
      ["punct", ":"],
      ["key", "description"],
      ["punct", ":"],
    ]);
  });

  it("marks a comment with its own role", () => {
    const src = "tags:\n  - setup      # sequenced first\n";
    expect(textFor(src, "comment")).toEqual(["# sequenced first"]);
  });

  it("marks flow-collection indicators as receding", () => {
    const src = "tags: [a, b]\n";
    const punct = textFor(src, "punct");
    expect(punct).toContain("[");
    expect(punct).toContain("]");
    expect(punct).toContain(",");
  });

  // The Editor carves the fences off the block before the region ever sees it
  // (EDT-FR-18), so this covers the walk rather than anything on screen — it is
  // here so a `---` reaching the walk recedes instead of being read as content.
  it("marks a document-start fence as receding", () => {
    const src = "---\nname: x\n";
    expect(textFor(src, "punct")).toContain("---");
  });

  it.each([
    [
      "a block using every shape",
      'name: onboarding\ndescription: >\n  folded\nmeta:\n  "q": 1  # note\ntags: [a, b]\n',
    ],
    ["a CRLF block", "name: onboarding\r\ndescription: hello\r\ntags: [a, b]\r\n"],
  ])("reproduces %s exactly — spans never overlap and never reorder", (_label, src) => {
    const spans = analyzeFrontmatter(src).spans;
    let last = 0;
    let rebuilt = "";
    for (const s of spans) {
      // Ordered, non-empty, in bounds, and never overlapping the span before it.
      expect(s.start).toBeGreaterThanOrEqual(last);
      expect(s.end).toBeGreaterThan(s.start);
      expect(s.end).toBeLessThanOrEqual(src.length);
      rebuilt += src.slice(last, s.start) + src.slice(s.start, s.end);
      last = s.end;
    }
    // Cutting the text at every span boundary and gluing it back yields the
    // original: the overlay can only ever be presentation.
    expect(rebuilt + src.slice(last)).toBe(src);
  });

  it("does not treat `key:value` (no space) as a mapping", () => {
    // A real parse reads this as the plain scalar `nospace:x`, not a key.
    expect(textFor("nospace:x\n", "key")).toEqual([]);
  });
});

describe("analyzeFrontmatter — validity (EDT-FR-58)", () => {
  it("reports a valid block as valid, with no error", () => {
    const a = analyzeFrontmatter("name: x\ndescription: y\n");
    expect(a.valid).toBe(true);
    expect(a.error).toBeNull();
  });

  it("reports an unclosed quote as invalid, with no spans and no description", () => {
    const a = analyzeFrontmatter('description: "unclosed\nname: x\n');
    expect(a.valid).toBe(false);
    expect(a.error).toBeTruthy();
    expect(a.spans).toEqual([]);
    expect(a.description).toBeNull();
  });

  it("reports a sequence item among mapping keys as invalid", () => {
    const a = analyzeFrontmatter("name: x\n- item\n");
    expect(a.valid).toBe(false);
    expect(a.spans).toEqual([]);
  });

  it("reports a nested mapping in a compact mapping as invalid", () => {
    expect(analyzeFrontmatter("description: a: b\n").valid).toBe(false);
  });

  it("treats an empty block as valid with nothing to count", () => {
    const a = analyzeFrontmatter("");
    expect(a.valid).toBe(true);
    expect(a.description).toBeNull();
  });

  it("reports a duplicate top-level description as invalid rather than counting one", () => {
    const a = analyzeFrontmatter("description: a\ndescription: b\n");
    expect(a.valid).toBe(false);
    expect(a.description).toBeNull();
    expect(a.spans).toEqual([]);
  });

  it("reports an alias in key position as invalid", () => {
    expect(analyzeFrontmatter("*anc: v\n").valid).toBe(false);
  });

  it("reports an error anywhere in a multi-document block as invalid", () => {
    // A `---` inside the block starts a second document; its errors count too.
    expect(analyzeFrontmatter("name: x\n---\nname: y\n- item\n").valid).toBe(false);
  });
});

describe("analyzeFrontmatter — description (EDT-FR-59, EDT-FR-60)", () => {
  it("returns a plain scalar description verbatim", () => {
    expect(analyzeFrontmatter("description: hello there\n").description).toBe(
      "hello there",
    );
  });

  it("strips the quotes of a quoted description", () => {
    expect(analyzeFrontmatter('description: "hello: there"\n').description).toBe(
      "hello: there",
    );
  });

  it("folds a `>` block scalar into one line", () => {
    const a = analyzeFrontmatter(
      "description: >\n  Steps to run before\n  the first session\n",
    );
    expect(a.description).toBe("Steps to run before the first session\n");
  });

  it("keeps the line breaks of a `|` literal block scalar", () => {
    const a = analyzeFrontmatter("description: |\n  one\n  two\n");
    expect(a.description).toBe("one\ntwo\n");
  });

  it("joins a multi-line plain scalar", () => {
    const a = analyzeFrontmatter("description: one\n  two\n");
    expect(a.description).toBe("one two");
  });

  it("counts the parsed value, not the source characters it occupies", () => {
    const value = "x".repeat(40);
    const source = `description: >\n  ${value}\n`;
    // The source line carries two leading spaces and the folding indicator; the
    // parsed value carries neither (the trailing newline is YAML's clip).
    expect(analyzeFrontmatter(source).description).toBe(`${value}\n`);
    expect(source.length).toBeGreaterThan(value.length + 1);
  });

  it("returns null for a description that is a list or a nested mapping", () => {
    expect(analyzeFrontmatter("description:\n  - a\n  - b\n").description).toBeNull();
    expect(analyzeFrontmatter("description:\n  a: b\n").description).toBeNull();
  });

  it("returns null for a non-string scalar description", () => {
    expect(analyzeFrontmatter("description: 42\n").description).toBeNull();
    expect(analyzeFrontmatter("description:\n").description).toBeNull();
  });

  it("counts a tagged or anchored description by its resolved string", () => {
    // `!!str 42` resolves to the string "42"; the anchor is not part of the value.
    expect(analyzeFrontmatter("description: !!str 42\n").description).toBe("42");
    expect(analyzeFrontmatter("description: &d hello\n").description).toBe("hello");
  });

  it("returns null when the block carries no top-level description", () => {
    expect(analyzeFrontmatter("name: x\ntags: [a]\n").description).toBeNull();
    // A nested `description` is not the top-level one.
    expect(
      analyzeFrontmatter("metadata:\n  description: nested\n").description,
    ).toBeNull();
  });

  it("returns an empty string for an explicitly empty description", () => {
    expect(analyzeFrontmatter('description: ""\n').description).toBe("");
  });

  it("exposes the fixed 1024-character budget", () => {
    expect(DESCRIPTION_LIMIT).toBe(1024);
  });
});
