/**
 * `ESH-editor-source-files.md`: reading a file as the language it is written in.
 *
 * Two questions live here, and nothing else does. **What shape is this file** —
 * a Markdown file the rich surface renders, or a source file edited as source
 * (ESH-FR-SSDV) — and, for a source file, **what language is it and where are its
 * tokens** (ESH-FR-CFEQ, ESH-FR-LMNI).
 *
 * Both answers are derived from the file's name and its text alone: no artifact
 * type, no backend call, nothing read from disk. That is what lets the Editor and
 * the Diff tab (DFV-FR-57) colour the same file the same way without either
 * asking the other.
 *
 * The module is deliberately free of React and of the DOM: it returns token
 * *offsets*, and whichever surface asked renders them. A `<textarea>` cannot
 * carry inline marks, so the surfaces draw them on a layer behind the editable
 * text (ESH-FR-YTNN) — which only works if what comes back is offsets into the
 * exact string that went in. Every path here either produces spans that satisfy
 * that, or produces nothing at all and the file renders plain (ESH-FR-GCEN).
 */
import hljs from "highlight.js/lib/common";
import dart from "highlight.js/lib/languages/dart";
import scala from "highlight.js/lib/languages/scala";
import { logWarn } from "../logging";

/**
 * ESH-FR-LMNI: the grammars the installation registers — the library's **common**
 * set, plus the two the map below calls for that the common set omits.
 *
 * Registering the whole catalogue instead would cost bundle size on every file
 * and give autodetection ~190 grammars to weigh rather than 38, which is the
 * pass that runs over a file the map has no answer for.
 */
hljs.registerLanguage("dart", dart);
hljs.registerLanguage("scala", scala);

/**
 * The roles a token can carry. Deliberately a small set of our own rather than
 * Highlight.js's ~40 scopes: ESH-FR-YNIV requires token colours to come from the
 * application's theme-aware semantic styles, so each role is one colour the two
 * themes each define, and a scope that maps to no role renders plain.
 */
export type TokenRole =
  | "comment"
  | "keyword"
  | "string"
  | "number"
  | "type"
  | "function"
  | "variable"
  | "meta"
  | "punctuation";

/** One token, as offsets into the exact text that was tokenised. */
export interface TokenSpan {
  start: number;
  end: number;
  role: TokenRole;
}

/**
 * ESH-FR-PBFJ: the extension map. Case-insensitive, keyed by the file's **final**
 * extension, and consulted before the text is ever weighed.
 *
 * The values are the installed library's own language identifiers and aliases —
 * `toml` is an alias of its INI grammar, `objectivec` its Objective-C — so
 * several extensions mapping to one scheme highlight identically because they
 * name one grammar rather than two that resemble each other. Dart is the
 * library's Dart grammar; there is no Flutter grammar and no name for one.
 */
export const EXTENSION_LANGUAGES: Readonly<Record<string, string>> = {
  c: "c",
  h: "c",
  cc: "cpp",
  cpp: "cpp",
  cxx: "cpp",
  hh: "cpp",
  hpp: "cpp",
  hxx: "cpp",
  cs: "csharp",
  go: "go",
  rs: "rust",
  ts: "typescript",
  tsx: "typescript",
  mts: "typescript",
  cts: "typescript",
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  java: "java",
  yaml: "yaml",
  yml: "yaml",
  toml: "toml",
  json: "json",
  sql: "sql",
  scala: "scala",
  sc: "scala",
  kt: "kotlin",
  kts: "kotlin",
  dart: "dart",
  swift: "swift",
  m: "objectivec",
  mm: "objectivec",
};

/**
 * ESH-FR-QBZJ: how sure autodetection has to be before its answer is used.
 *
 * Highlight.js always names its best guess, however poor: an English paragraph
 * comes back as CSS with a relevance of 4 and a line of noise as Lua with 1,
 * while a real Python module scores 30 and a shell script 21. The floor is what
 * separates "detected" from "guessed", and is why a text file of prose renders
 * plain rather than speckled.
 */
export const AUTODETECT_RELEVANCE_FLOOR = 10;

/**
 * How much of a file autodetection weighs.
 *
 * The pass runs every registered grammar over the text, so on a large file it is
 * the one part of resolution with a cost worth bounding. A language is legible
 * from its opening screens — imports, a declaration or two — and the answer is
 * then applied to the whole file, so reading further buys accuracy nobody can
 * see at a price that grows without limit.
 */
export const AUTODETECT_SAMPLE_CHARS = 4096;

/** Highlight.js scope root → the role it renders in. Anything absent is plain. */
const SCOPE_ROLES: Readonly<Record<string, TokenRole>> = {
  comment: "comment",
  quote: "comment",
  doctag: "comment",
  keyword: "keyword",
  literal: "keyword",
  tag: "keyword",
  name: "keyword",
  "selector-tag": "keyword",
  section: "keyword",
  string: "string",
  regexp: "string",
  symbol: "string",
  char: "string",
  "template-variable": "string",
  "template-tag": "string",
  link: "string",
  number: "number",
  type: "type",
  built_in: "type",
  class: "type",
  "selector-class": "type",
  "selector-id": "type",
  "selector-attr": "type",
  "selector-pseudo": "type",
  title: "function",
  attr: "variable",
  attribute: "variable",
  property: "variable",
  variable: "variable",
  params: "variable",
  meta: "meta",
  bullet: "meta",
  operator: "punctuation",
  punctuation: "punctuation",
};

/** Schemes already reported missing, so ESH-FR-XNRV reports one record per scheme. */
const reportedMissing = new Set<string>();

/** Test seam: forget which missing schemes have already been reported. */
export function resetMissingGrammarReportsForTest(): void {
  reportedMissing.clear();
}

/**
 * ESH-FR-FKZB: the file's **final** extension, lowercased, or `null` where it has
 * none.
 *
 * A leading dot is a name and not an extension — `.gitignore` is extensionless,
 * the way every tool that reads it treats it — and a path is reduced to its last
 * segment first, so a directory called `src.rs/` cannot lend its name to the
 * files inside it.
 */
export function finalExtension(name: string): string | null {
  const base = name.split(/[/\\]/).pop() ?? "";
  const dot = base.lastIndexOf(".");
  if (dot <= 0 || dot === base.length - 1) return null;
  return base.slice(dot + 1).toLowerCase();
}

/**
 * ESH-FR-FKZB: whether the Editor opens this file as a **Markdown file** — the
 * rich document with two editing modes — or as a **source file** edited on the
 * source surface alone.
 *
 * The name decides, and nothing else: an artifact whose file is not named `.md`
 * is a source file, and an untyped file that is so named is a Markdown file. The
 * comparison ignores letter case, so `NOTES.MD` is Markdown exactly as
 * `notes.md` is.
 */
export function isMarkdownFile(name: string): boolean {
  return finalExtension(name) === "md";
}

/** ESH-FR-PBFJ step 1: the language this filename maps to, if the map names one. */
export function languageForExtension(name: string): string | null {
  const ext = finalExtension(name);
  if (ext === null) return null;
  return EXTENSION_LANGUAGES[ext] ?? null;
}

/**
 * ESH-FR-XNRV: the scheme, if the installation actually registered it.
 *
 * A scheme the map names that is not registered leaves its files plain and is
 * reported once — a diagnostic in the Logs panel rather than an interruption
 * over the document, because a grammar nobody bundled is a fact about the build
 * and not something the author can act on mid-edit.
 */
export function registeredScheme(
  language: string,
  ext: string | null,
): string | null {
  if (hljs.getLanguage(language)) return language;
  if (!reportedMissing.has(language)) {
    reportedMissing.add(language);
    logWarn(["frontend"], "syntax grammar is not registered", {
      scheme: language,
      extension: ext ?? "",
    });
  }
  return null;
}

/**
 * ESH-FR-QBZJ step 2: what the text itself looks like.
 *
 * Returns `null` where nothing is detectable — an empty file, a file of prose,
 * a file of noise — which is the answer that leaves it plain.
 */
export function detectLanguage(text: string): string | null {
  if (text.trim() === "") return null;
  const sample =
    text.length > AUTODETECT_SAMPLE_CHARS
      ? text.slice(0, AUTODETECT_SAMPLE_CHARS)
      : text;
  let result;
  try {
    result = hljs.highlightAuto(sample);
  } catch {
    // A grammar that throws on this text is a failure of detection, not of the
    // file: it renders plain (ESH-FR-GCEN).
    return null;
  }
  if (!result.language) return null;
  if (result.relevance < AUTODETECT_RELEVANCE_FLOOR) return null;
  return result.language;
}

/** Where a resolved language came from — the map, or the text (ESH-FR-CFEQ). */
export type LanguageSource = "extension" | "detected";

export interface ResolvedLanguage {
  language: string;
  source: LanguageSource;
}

/**
 * ESH-FR-CFEQ: resolve a source file's language, first answer wins.
 *
 * The extension map first, and its answer is never revisited — which is what
 * makes a given filename highlight the same way whatever it happens to contain.
 * Only where the map is silent is the text weighed.
 *
 * A Markdown file resolves to nothing at all: neither step is ever run over one,
 * because it is not highlighted (ESH-FR-ZCKP).
 */
export function resolveLanguage(
  name: string,
  text: string,
): ResolvedLanguage | null {
  if (isMarkdownFile(name)) return null;
  const mapped = languageForExtension(name);
  if (mapped !== null) {
    const usable = registeredScheme(mapped, finalExtension(name));
    // A mapped extension whose grammar is missing renders plain rather than
    // falling through to autodetection: the map's answer is deterministic
    // (ESH-FR-TVUT), and guessing behind its back would make one file's colouring
    // depend on which grammars happened to be bundled.
    return usable === null ? null : { language: usable, source: "extension" };
  }
  const detected = detectLanguage(text);
  return detected === null ? null : { language: detected, source: "detected" };
}

/** A node of the emitter's token tree: a run of text, or a scoped subtree. */
interface EmitterNode {
  scope?: string;
  children?: (EmitterNode | string)[];
}

/**
 * ESH-FR-QFCD: turn what the library produced into offsets, parsing nothing.
 *
 * Highlight.js's own output is an HTML string, which is the wrong shape twice
 * over for an editable surface: it has to be re-parsed to be positioned, and its
 * entities no longer line up character-for-character with the buffer the
 * textarea holds. The token tree behind that string carries the same information
 * already separated into scopes and literal text, so the adapter walks it and
 * counts — it arranges what the library produced rather than reading the source
 * language itself.
 *
 * Only the innermost scope covering a run is emitted, so the spans are disjoint
 * and in order: a renderer can cut the text at their boundaries without deciding
 * which of two overlapping decorations wins.
 */
function walkTokens(node: EmitterNode, spans: TokenSpan[], offset: number, inherited: TokenRole | null): { offset: number; text: string } {
  const role = node.scope
    ? (SCOPE_ROLES[node.scope.split(".")[0]] ?? inherited)
    : inherited;
  let at = offset;
  let text = "";
  for (const child of node.children ?? []) {
    if (typeof child === "string") {
      if (child.length > 0) {
        if (role) spans.push({ start: at, end: at + child.length, role });
        at += child.length;
        text += child;
      }
      continue;
    }
    const inner = walkTokens(child, spans, at, role);
    at = inner.offset;
    text += inner.text;
  }
  return { offset: at, text };
}

/**
 * ESH-FR-BABL/ESH-FR-MJRH: the token spans of `text` under `language`, or `null`
 * where it cannot be tokenised.
 *
 * `null` is not an error state anywhere — it is the plain rendering, which is a
 * complete and editable reading of the file. That is why every failure here
 * converges on it: an unregistered grammar, a grammar that throws, and an
 * emitter whose walked text does not reproduce the input exactly. The last of
 * those is the load-bearing one: the spans are offsets the surface will slice
 * the *buffer* with, so a walk that lost or gained a character would colour the
 * wrong ranges of the author's text. Verifying is cheaper than trusting.
 */
export function tokenize(text: string, language: string): TokenSpan[] | null {
  if (text === "") return null;
  if (!hljs.getLanguage(language)) return null;
  let emitted: unknown;
  try {
    emitted = hljs.highlight(text, { language, ignoreIllegals: true })._emitter;
  } catch {
    return null;
  }
  const root = (emitted as { rootNode?: EmitterNode } | undefined)?.rootNode;
  if (!root) return null;
  const spans: TokenSpan[] = [];
  const walked = walkTokens(root, spans, 0, null);
  if (walked.text !== text) return null;
  return spans;
}

/**
 * DFV-FR-57: the same tokens, cut into the lines a diff renders as rows.
 *
 * A Diff tab renders one line per row and knows a row by its line number, so it
 * needs the spans indexed that way and stated in the line's own coordinates. The
 * tokenising itself is still done over the whole revision — a string or a
 * comment that runs across three lines is one token there and three clipped
 * pieces here, which is exactly what a row-at-a-time pass could not work out.
 *
 * The result has one entry per line of `text`, so a row can index it directly.
 */
export function tokensByLine(
  text: string,
  spans: readonly TokenSpan[] | null,
): TokenSpan[][] {
  const lines = text.split("\n");
  const out: TokenSpan[][] = lines.map(() => []);
  if (!spans || spans.length === 0) return out;
  // Where each line starts in `text`, so a span can be placed without scanning.
  const starts: number[] = [];
  let at = 0;
  for (const line of lines) {
    starts.push(at);
    at += line.length + 1;
  }
  // Ordered and disjoint before anything is placed. `tokenize` already produces
  // spans that way, but every consumer of this result — the diff's row painters
  // and the Editor's layer — resolves a chunk by taking the *first* span that
  // covers it, so a list that broke either property would silently drop a
  // decoration or let one swallow another. Normalising here is O(n) on input
  // that is already in order and costs a sort on input that is not, which is a
  // fair price for the invariant holding whatever produced the spans.
  let ordered = spans as readonly TokenSpan[];
  for (let i = 1; i < ordered.length; i += 1) {
    if (ordered[i].start < ordered[i - 1].start) {
      ordered = [...spans].sort((a, b) => a.start - b.start || a.end - b.end);
      break;
    }
  }
  let line = 0;
  let previousEnd = 0;
  for (const span of ordered) {
    // An overlap is resolved in favour of the span that opened first, which is
    // the same answer a consumer's own `find` would have given it.
    const from = Math.max(span.start, previousEnd);
    if (from >= span.end) continue;
    previousEnd = span.end;
    // The spans are in order, so the search for the line one opens on resumes
    // where the last left off rather than starting over.
    while (line < lines.length - 1 && starts[line] + lines[line].length < from) {
      line += 1;
    }
    for (let i = line; i < lines.length; i += 1) {
      const lineStart = starts[i];
      if (lineStart >= span.end) break;
      const lineEnd = lineStart + lines[i].length;
      const start = Math.max(from, lineStart) - lineStart;
      const end = Math.min(span.end, lineEnd) - lineStart;
      if (end > start) out[i].push({ start, end, role: span.role });
    }
  }
  return out;
}

/**
 * ESH-FR-BABL/ESH-FR-CFEQ: the whole reading of one file — resolve, then tokenise.
 *
 * Returns `null` for a Markdown file, for a file no language resolves for, and
 * for one that cannot be tokenised; in every case the caller renders the plain
 * text it already has.
 */
export function highlightFile(name: string, text: string): TokenSpan[] | null {
  const resolved = resolveLanguage(name, text);
  if (!resolved) return null;
  return tokenize(text, resolved.language);
}
