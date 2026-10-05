import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The missing-grammar diagnostic (ESH-FR-HHCR) reaches the backend through
// `api.appendLogRecords`, which is an `invoke`. Mocked at the Tauri boundary so
// the record's own shape is exercised rather than a wrapper's.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

import { invoke } from "@tauri-apps/api/core";
import { flushLogs, resetLogBufferForTest } from "../logging";
import {
  AUTODETECT_RELEVANCE_FLOOR,
  AUTODETECT_SAMPLE_CHARS,
  EXTENSION_LANGUAGES,
  detectLanguage,
  finalExtension,
  highlightFile,
  isMarkdownFile,
  languageForExtension,
  registeredScheme,
  resetMissingGrammarReportsForTest,
  resolveLanguage,
  tokenize,
  tokensByLine,
  type TokenSpan,
} from "./syntaxHighlight";

const invoked = vi.mocked(invoke);

/** Every log record appended, flattened across batches. */
const records = (): { level: string; message: string; fields: Record<string, unknown> }[] => {
  flushLogs();
  return invoked.mock.calls
    .filter(([name]) => name === "append_log_records")
    .flatMap(([, args]) => (args as { records: never[] }).records);
};

beforeEach(() => {
  resetLogBufferForTest();
  resetMissingGrammarReportsForTest();
  invoked.mockClear();
});

afterEach(() => {
  resetLogBufferForTest();
});

/** The text a span covers, so an assertion reads as the token it is about. */
const covered = (text: string, spans: TokenSpan[], role: string): string[] =>
  spans.filter((s) => s.role === role).map((s) => text.slice(s.start, s.end));

describe("file shape (ESH-FR-SSDV)", () => {
  it("reads the final extension and ignores letter case", () => {
    expect(finalExtension("main.rs")).toBe("rs");
    expect(finalExtension("Widget.Cpp")).toBe("cpp");
    expect(finalExtension("src/deep/path/app.tsx")).toBe("tsx");
    expect(finalExtension("archive.tar.gz")).toBe("gz");
  });

  it("treats a dotfile and a bare name as extensionless", () => {
    expect(finalExtension(".gitignore")).toBeNull();
    expect(finalExtension("Makefile")).toBeNull();
    expect(finalExtension("LICENSE")).toBeNull();
    // A trailing dot names no extension either.
    expect(finalExtension("odd.")).toBeNull();
  });

  it("does not let a folder's name reach the files inside it", () => {
    expect(finalExtension("vendor.rs/README")).toBeNull();
    expect(isMarkdownFile("notes.md/inner.txt")).toBe(false);
  });

  // ESH-FR-SSDV: `.md` in any letter case is a Markdown file; everything else the
  // Editor opens is a source file.
  it("names a Markdown file by its final extension, case-insensitively", () => {
    for (const name of ["notes.md", "Notes.MD", "READ.Md", "a/b/c.md"]) {
      expect(isMarkdownFile(name)).toBe(true);
    }
    for (const name of [
      "main.rs",
      "notes.txt",
      "README.markdown",
      "Makefile",
      "notes.md.txt",
      "",
    ]) {
      expect(isMarkdownFile(name)).toBe(false);
    }
  });
});

describe("extension map (ESH-FR-PBFJ)", () => {
  // ESH-FR-SSDV, ESH-FR-LKNM, ESH-FR-BABL, ESH-FR-PBFJ: the map is read without regard to the letter case of the extension.
  it("maps a mapped extension whatever its case", () => {
    expect(languageForExtension("main.RS")).toBe("rust");
    expect(languageForExtension("Widget.Cpp")).toBe("cpp");
    expect(languageForExtension("config.YML")).toBe("yaml");
  });

  // ESH-FR-SNJU, ESH-FR-PBFJ: several extensions mapping to one scheme name one grammar.
  it("gives extensions that share a scheme the same language", () => {
    expect(languageForExtension("a.cc")).toBe("cpp");
    expect(languageForExtension("b.cpp")).toBe("cpp");
    expect(languageForExtension("c.hxx")).toBe("cpp");
    // ...and keeps the two header conventions apart.
    expect(languageForExtension("d.h")).toBe("c");
    expect(languageForExtension("e.hpp")).toBe("cpp");
  });

  // ESH-FR-XNRV, ESH-FR-LMNI, ESH-FR-QFCD: Dart is the library's Dart grammar; no Flutter grammar exists.
  it("maps .dart to Dart and names no Flutter grammar anywhere", () => {
    expect(languageForExtension("widget.dart")).toBe("dart");
    expect(Object.values(EXTENSION_LANGUAGES)).not.toContain("flutter");
  });

  it("covers every scheme the spec's map names", () => {
    const expected: Record<string, string[]> = {
      c: ["c", "h"],
      cpp: ["cc", "cpp", "cxx", "hh", "hpp", "hxx"],
      csharp: ["cs"],
      go: ["go"],
      rust: ["rs"],
      typescript: ["ts", "tsx", "mts", "cts"],
      javascript: ["js", "jsx", "mjs", "cjs"],
      java: ["java"],
      yaml: ["yaml", "yml"],
      toml: ["toml"],
      json: ["json"],
      sql: ["sql"],
      scala: ["scala", "sc"],
      kotlin: ["kt", "kts"],
      dart: ["dart"],
      swift: ["swift"],
      objectivec: ["m", "mm"],
    };
    for (const [language, exts] of Object.entries(expected)) {
      for (const ext of exts) {
        expect(EXTENSION_LANGUAGES[ext]).toBe(language);
      }
    }
  });

  // ESH-FR-XNRV: every scheme the map names is registered in the installation, so
  // no ordinary source file falls back to plain for want of a grammar.
  it("registers every scheme the map names", () => {
    for (const [ext, language] of Object.entries(EXTENSION_LANGUAGES)) {
      const spans = tokenize("x", language);
      // `tokenize` returns null for an unregistered grammar; a registered one
      // returns an array (possibly empty for a one-character document).
      expect(spans, `${ext} → ${language}`).not.toBeNull();
    }
  });

  it("has no answer for an unmapped or absent extension", () => {
    expect(languageForExtension("jumble.xyz")).toBeNull();
    expect(languageForExtension("Makefile")).toBeNull();
    expect(languageForExtension("notes.txt")).toBeNull();
  });
});

describe("autodetection (ESH-FR-QBZJ)", () => {
  const python = [
    "import os",
    "from typing import List",
    "",
    "def add(a: int, b: int) -> int:",
    '    """Add two numbers."""',
    "    return a + b",
    "",
    "class Widget:",
    "    def __init__(self, name):",
    "        self.name = name",
  ].join("\n");

  // ESH-FR-QBZJ, ESH-FR-CFEQ: a text file holding recognisable source is detected from its text.
  it("detects a language from recognisable source", () => {
    // Named rather than merely non-null: "highlighted with the language
    // autodetection reports" is not satisfied by reporting Python as CSS.
    expect(detectLanguage(python)).toBe("python");
    expect(
      detectLanguage(
        '#!/usr/bin/env bash\nset -euo pipefail\nfor f in *; do\n  echo "$f"\ndone\n',
      ),
    ).toBe("bash");
  });

  // ESH-FR-QBZJ, ESH-FR-CFEQ / ESH-FR-YTPJ, ESH-FR-XXYW: prose, noise, and emptiness stay plain.
  it("reports nothing for prose, noise, and an empty file", () => {
    expect(
      detectLanguage(
        "It was a bright cold day in April, and the clocks were striking thirteen. Nothing here resembles source code at all.",
      ),
    ).toBeNull();
    expect(detectLanguage("asdkj asd kajsd 8281 ,,, ---- zzz")).toBeNull();
    expect(detectLanguage("")).toBeNull();
    expect(detectLanguage("   \n\n  ")).toBeNull();
  });

  it("keeps the floor above what a guess scores and below what a language scores", () => {
    // The floor is a judgement about relevance; pin it so a library upgrade that
    // rescales relevance is caught here rather than by a user seeing coloured
    // prose.
    expect(AUTODETECT_RELEVANCE_FLOOR).toBeGreaterThan(4);
    expect(AUTODETECT_RELEVANCE_FLOOR).toBeLessThan(20);
  });

  it("weighs a bounded sample rather than the whole of a large file", () => {
    // The bound is load-bearing rather than incidental, so it is pinned as a
    // pair: the same source inside the sample is detected and past it is not.
    // Filler in the file's own comment syntax, so what is being measured is
    // where the sample ends rather than what the padding looks like.
    const filler = "# lorem ipsum dolor sit amet\n";
    const inside = python + "\n" + filler.repeat(10);
    expect(inside.length).toBeLessThan(AUTODETECT_SAMPLE_CHARS);
    expect(detectLanguage(inside)).toBe("python");

    const pushedPast =
      filler.repeat(Math.ceil(AUTODETECT_SAMPLE_CHARS / filler.length) + 10) +
      python;
    expect(pushedPast.indexOf(python)).toBeGreaterThan(AUTODETECT_SAMPLE_CHARS);
    expect(detectLanguage(pushedPast)).not.toBe("python");
  });
});

describe("resolution order (ESH-FR-CFEQ)", () => {
  // ESH-FR-TVUT: the map is consulted first and its answer is not revisited.
  it("lets the extension win over what the text looks like", () => {
    const jsonBody = [
      "{",
      '  "name": "synthesis",',
      '  "private": true,',
      '  "version": "0.1.0",',
      '  "scripts": { "dev": "vite", "build": "tsc && vite build" },',
      '  "dependencies": { "react": "^19.1.0", "yaml": "^2.9.0" }',
      "}",
      "",
    ].join("\n");
    expect(resolveLanguage("package.json", jsonBody)).toEqual({
      language: "json",
      source: "extension",
    });
    // The same bytes under an unmapped name fall through to the text.
    const detected = resolveLanguage("package.unknownext", jsonBody);
    expect(detected?.source).toBe("detected");
  });

  // ESH-FR-JHPR: neither step is ever run over a Markdown file.
  it("resolves nothing for a Markdown file", () => {
    expect(resolveLanguage("notes.md", "# Heading\n\nfn main() {}\n")).toBeNull();
    expect(resolveLanguage("NOTES.MD", "fn main() {}\n")).toBeNull();
    expect(highlightFile("notes.md", "fn main() { let x = 1; }")).toBeNull();
  });

  it("resolves nothing for text no language fits", () => {
    expect(resolveLanguage("notes.txt", "just some words about a cat")).toBeNull();
    expect(resolveLanguage("empty.txt", "")).toBeNull();
  });
});

describe("tokenising (ESH-FR-BABL, ESH-FR-YTNN)", () => {
  it("returns spans that describe the exact text handed in", () => {
    const text = 'fn main() {\n    // greet\n    let s = "x < y & z";\n}\n';
    const spans = highlightFile("main.rs", text)!;
    expect(spans.length).toBeGreaterThan(0);
    // Every span lies inside the text, and they are disjoint and in order —
    // which is what lets a renderer cut the buffer at their boundaries without
    // deciding which of two overlapping decorations wins.
    let prevEnd = 0;
    for (const s of spans) {
      expect(s.start).toBeGreaterThanOrEqual(prevEnd);
      expect(s.end).toBeGreaterThan(s.start);
      expect(s.end).toBeLessThanOrEqual(text.length);
      prevEnd = s.end;
    }
    expect(covered(text, spans, "comment")).toContain("// greet");
    expect(covered(text, spans, "keyword")).toContain("fn");
    // The string keeps the characters the author wrote — no entity substitution
    // on the way through the library.
    expect(covered(text, spans, "string")).toContain('"x < y & z"');
  });

  it("keeps offsets aligned through characters HTML would escape", () => {
    const text = `const s = "a < b && c > d";\nconst t = 'it\\'s & more';\n`;
    const spans = highlightFile("app.ts", text)!;
    for (const s of spans) {
      // An offset that drifted would slice the buffer somewhere else entirely;
      // this is the invariant the whole overlay rests on.
      expect(text.slice(s.start, s.end).length).toBe(s.end - s.start);
    }
    expect(spans.some((s) => text.slice(s.start, s.end).includes("<"))).toBe(true);
  });

  it("keeps offsets aligned through astral characters", () => {
    const text = 'const emoji = "🎉 party";\n// 🎈\n';
    const spans = highlightFile("app.js", text)!;
    expect(covered(text, spans, "string")).toContain('"🎉 party"');
    expect(covered(text, spans, "comment")).toContain("// 🎈");
  });

  it("gives extensions that share a scheme identical spans", () => {
    const text = "class Widget {\n  int n = 1;\n};\n";
    const cc = highlightFile("a.cc", text);
    const cpp = highlightFile("b.cpp", text);
    const hxx = highlightFile("c.hxx", text);
    expect(cc).toEqual(cpp);
    expect(cc).toEqual(hxx);
  });

  it("renders an empty document and an unregistered grammar plain", () => {
    expect(tokenize("", "rust")).toBeNull();
    expect(tokenize("fn main() {}", "no-such-language")).toBeNull();
  });

  // The invariant every renderer rests on: a chunk is resolved by taking the
  // first span that covers it, so a list that was unordered or overlapping would
  // silently drop one decoration or let another swallow it. Asserted for every
  // registered grammar over text with several shapes in it, rather than for one
  // sample of one language.
  it("returns ordered, disjoint, in-range spans for every mapped scheme", () => {
    const samples = [
      "a b c { } 1 2 3 // x\n",
      'x = "str" /* c */ 42;\n\tif (a && b) { return [1, 2]; }\n',
      "# comment\nkey: value\n  - item\n",
      "<tag attr=\"v\">text</tag>\n",
      "'quoted' `back` \\escape\n\n\n",
    ];
    for (const language of new Set(Object.values(EXTENSION_LANGUAGES))) {
      for (const sample of samples) {
        let spans: ReturnType<typeof tokenize>;
        expect(() => {
          spans = tokenize(sample, language);
        }, `${language} threw on ${JSON.stringify(sample)}`).not.toThrow();
        if (!spans!) continue;
        let prevEnd = 0;
        for (const s of spans!) {
          expect(s.start, `${language}: span order`).toBeGreaterThanOrEqual(prevEnd);
          expect(s.end, `${language}: empty span`).toBeGreaterThan(s.start);
          expect(s.end, `${language}: span past the text`).toBeLessThanOrEqual(
            sample.length,
          );
          prevEnd = s.end;
        }
      }
    }
  });
});

describe("tokens by line (DFV-FR-57)", () => {
  it("gives one entry per line, in the line's own coordinates", () => {
    const text = "fn a() {}\nfn b() {}\n";
    const byLine = tokensByLine(text, highlightFile("x.rs", text));
    // Three entries: two lines of code and the empty one after the final newline.
    expect(byLine).toHaveLength(3);
    const first = byLine[0];
    expect(first.length).toBeGreaterThan(0);
    for (const span of first) {
      expect(span.start).toBeGreaterThanOrEqual(0);
      expect(span.end).toBeLessThanOrEqual("fn a() {}".length);
    }
    expect(
      first
        .filter((s) => s.role === "keyword")
        .map((s) => "fn a() {}".slice(s.start, s.end)),
    ).toEqual(["fn"]);
    // The second line's spans are relative to *it*, not to the file.
    expect(
      byLine[1]
        .filter((s) => s.role === "keyword")
        .map((s) => "fn b() {}".slice(s.start, s.end)),
    ).toEqual(["fn"]);
  });

  it("clips a token that runs across lines into a piece per line", () => {
    const text = "/* one\n   two\n   three */\nlet x = 1;\n";
    const byLine = tokensByLine(text, highlightFile("x.rs", text));
    const lines = text.split("\n");
    // The block comment covers all of the first three lines and none of the
    // fourth — a row-at-a-time pass could not have worked that out.
    for (let i = 0; i < 3; i += 1) {
      const covered = byLine[i]
        .filter((s) => s.role === "comment")
        .map((s) => lines[i].slice(s.start, s.end))
        .join("");
      expect(covered).toBe(lines[i]);
    }
    expect(byLine[3].some((s) => s.role === "comment")).toBe(false);
  });

  it("returns empty rows for text with no spans", () => {
    expect(tokensByLine("a\nb\n", null)).toEqual([[], [], []]);
    expect(tokensByLine("", [])).toEqual([[]]);
  });

  it("handles CRLF and a file with no trailing newline", () => {
    const crlf = "fn a() {}\r\nfn b() {}";
    const byLine = tokensByLine(crlf, highlightFile("x.rs", crlf));
    // Two lines, no phantom third — the file ends without a newline.
    expect(byLine).toHaveLength(2);
    const lines = crlf.split("\n");
    for (let i = 0; i < 2; i += 1) {
      for (const span of byLine[i]) {
        expect(span.end).toBeLessThanOrEqual(lines[i].length);
      }
    }
  });

  // The per-line output is what the row painters index, and they resolve a chunk
  // by taking the first span covering it — so the rows have to hold the same
  // ordered, disjoint invariant the whole-file spans do, whatever came in.
  it("keeps every row ordered and disjoint, even from unordered input", () => {
    const text = "aaa\nbbb\nccc";
    const unordered = [
      { start: 8, end: 11, role: "keyword" as const },
      { start: 0, end: 3, role: "string" as const },
    ];
    const byLine = tokensByLine(text, unordered);
    // Neither span is dropped for having arrived out of order.
    expect(byLine[0]).toEqual([{ start: 0, end: 3, role: "string" }]);
    expect(byLine[2]).toEqual([{ start: 0, end: 3, role: "keyword" }]);
  });

  it("resolves an overlap in favour of the span that opened first", () => {
    const byLine = tokensByLine("abcdef", [
      { start: 0, end: 4, role: "string" },
      { start: 2, end: 6, role: "keyword" },
    ]);
    expect(byLine[0]).toEqual([
      { start: 0, end: 4, role: "string" },
      { start: 4, end: 6, role: "keyword" },
    ]);
    // …and what comes out is disjoint, which is what the painters require.
    let prevEnd = 0;
    for (const span of byLine[0]) {
      expect(span.start).toBeGreaterThanOrEqual(prevEnd);
      prevEnd = span.end;
    }
  });

  it("drops a span wholly swallowed by the one before it", () => {
    const byLine = tokensByLine("abcdef", [
      { start: 0, end: 6, role: "comment" },
      { start: 2, end: 4, role: "keyword" },
    ]);
    expect(byLine[0]).toEqual([{ start: 0, end: 6, role: "comment" }]);
  });
});

describe("missing grammar diagnostic (ESH-FR-XNRV)", () => {
  // ESH-FR-XNRV, ESH-FR-LMNI, ESH-FR-QFCD: a mapped scheme the installation does not register leaves the
  // file plain and reports it once — no modal, nothing blocked.
  it("reports one WARN per missing scheme and leaves the scheme unusable", () => {
    // The guard the resolver applies to whatever the map answered. Called here
    // with a scheme no build bundles, which is the state ESH-FR-XNRV describes.
    expect(registeredScheme("no-such-grammar", "zz")).toBeNull();
    const first = records();
    expect(first).toHaveLength(1);
    expect(first[0].level).toBe("WARN");
    expect(first[0].message).toBe("syntax grammar is not registered");
    expect(first[0].fields).toMatchObject({
      scheme: "no-such-grammar",
      extension: "zz",
    });

    // A second file of the same kind adds no second record: one report per
    // scheme per session, so a folder of them is not a wall of identical rows.
    invoked.mockClear();
    expect(registeredScheme("no-such-grammar", "zz")).toBeNull();
    expect(records()).toHaveLength(0);
  });

  it("says nothing about a registered scheme or an unmapped extension", () => {
    expect(registeredScheme("rust", "rs")).toBe("rust");
    expect(resolveLanguage("thing.zz", "")).toBeNull();
    expect(records()).toHaveLength(0);
  });
});
