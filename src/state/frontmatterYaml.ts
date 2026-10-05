import {
  Composer,
  isMap,
  isScalar,
  Parser,
  type CST,
  type Document,
} from "yaml";

/**
 * EDT-FR-59: the description budget the agents that read these files accept. A
 * fixed UI-side constant — no backend lookup.
 */
export const DESCRIPTION_LIMIT = 1024;

/**
 * EDT-FR-21: the roles the parse hands the renderer. Anything not covered by a
 * span renders as a value (normal weight): plain scalars, the folded lines of a
 * block scalar, and whitespace.
 *
 * - `key`   — a mapping key, at any nesting depth (bold).
 * - `punct` — the `---` fences, `:` separators, list markers, quoting
 *             characters, block-scalar indicators (`|`, `>`) and flow
 *             indicators (receding).
 * - `comment` — a `#` comment (receding, distinct from key and value alike).
 */
export type YamlRole = "key" | "punct" | "comment";

/** A half-open `[start, end)` range of the frontmatter text and its role. */
export interface YamlSpan {
  start: number;
  end: number;
  role: YamlRole;
}

export interface FrontmatterAnalysis {
  /** EDT-FR-58: whether the block parses as YAML at all. */
  valid: boolean;
  /** The first parse error, for the invalid-block indication. `null` when valid. */
  error: string | null;
  /** Role spans in document order; empty when the block does not parse. */
  spans: readonly YamlSpan[];
  /**
   * EDT-FR-60: the parsed scalar value of a top-level `description` — quotes
   * stripped, block scalars folded, continuation lines joined. `null` when the
   * block does not parse, carries no top-level `description`, or gives it a
   * value that is not a scalar string.
   */
  description: string | null;
}

/**
 * Whitespace tokens carry no role of their own — they render as the value
 * weight, like the text around them.
 */
const IGNORED_TOKENS: ReadonlySet<string> = new Set(["space", "newline"]);

/**
 * The shape every CST token shares. Typed structurally rather than as
 * `CST.SourceToken` so the same marker accepts the loosely-typed `props` of a
 * block scalar and the top-level stream: a token with no `source` of its own (a
 * collection) simply contributes no span.
 */
interface LeafToken {
  type: string;
  offset: number;
  source?: string;
}

/**
 * Push a span for a leaf token: comments get their own role, every other
 * indicator (`-`, `:`, `---`, `{`, `[`, `,`, an anchor, a tag, a block-scalar
 * header) recedes. Zero-length and whitespace tokens contribute nothing.
 */
function markSource(token: LeafToken, out: YamlSpan[]): void {
  if (IGNORED_TOKENS.has(token.type)) return;
  const source = token.source ?? "";
  if (source.length === 0) return;
  out.push({
    start: token.offset,
    end: token.offset + source.length,
    role: token.type === "comment" ? "comment" : "punct",
  });
}

function markSources(
  tokens: readonly LeafToken[] | undefined,
  out: YamlSpan[],
): void {
  if (!tokens) return;
  for (const token of tokens) markSource(token, out);
}

/**
 * A quoted scalar: the quote characters recede and only the text between them
 * is the key. The unterminated case is defensive — a block with an unclosed
 * quote does not parse, and an invalid block never reaches the span walk — but
 * it keeps the walk total rather than emitting a span past the end of the text.
 */
function markQuoted(token: CST.FlowScalar, asKey: boolean, out: YamlSpan[]): void {
  const { offset, source } = token;
  const quote = source[0];
  const closed = source.length > 1 && source.endsWith(quote);
  out.push({ start: offset, end: offset + 1, role: "punct" });
  if (closed) {
    out.push({
      start: offset + source.length - 1,
      end: offset + source.length,
      role: "punct",
    });
  }
  const innerEnd = offset + source.length - (closed ? 1 : 0);
  if (asKey && innerEnd > offset + 1) {
    out.push({ start: offset + 1, end: innerEnd, role: "key" });
  }
}

/** Walk a collection's items, keys marked as keys and values recursed into. */
function visitItems(items: readonly CST.CollectionItem[], out: YamlSpan[]): void {
  for (const item of items) {
    markSources(item.start, out);
    if (item.key) visitToken(item.key, true, out);
    markSources(item.sep, out);
    if (item.value) visitToken(item.value, false, out);
  }
}

/**
 * EDT-FR-21: map one CST token onto role spans. `asKey` distinguishes a mapping
 * key from a value — the same `scalar` token type serves both, and only the key
 * position is bold.
 */
function visitToken(token: CST.Token, asKey: boolean, out: YamlSpan[]): void {
  switch (token.type) {
    case "scalar":
      // A plain scalar in value position is the value weight — no span.
      if (asKey && token.source.length > 0) {
        out.push({
          start: token.offset,
          end: token.offset + token.source.length,
          role: "key",
        });
      }
      break;
    case "single-quoted-scalar":
    case "double-quoted-scalar":
      markQuoted(token, asKey, out);
      break;
    case "block-scalar":
      // The `|` / `>` header (with any chomp or indent indicator) recedes; the
      // folded lines beneath it are the value.
      markSources(token.props, out);
      break;
    case "block-map":
    case "block-seq":
      visitItems(token.items, out);
      break;
    case "flow-collection":
      // The opening indicator only — the closing one lives in `end`, marked by
      // the trailing pass below (marking it here too would double-span it).
      markSource(token.start, out);
      visitItems(token.items, out);
      break;
    default:
      break;
  }
  // Trailing tokens on a scalar or collection — a newline, or a comment sitting
  // after the value on the same line.
  markSources((token as { end?: LeafToken[] }).end, out);
}

/** EDT-FR-21: role spans for the whole block, in document order. */
function spansOf(tokens: readonly CST.Token[]): YamlSpan[] {
  const out: YamlSpan[] = [];
  for (const token of tokens) {
    if (token.type === "document") {
      markSources(token.start, out);
      if (token.value) visitToken(token.value, false, out);
      markSources(token.end, out);
    } else {
      // A stray top-level token — a directive line, a lone comment, a doc-end.
      markSource(token, out);
    }
  }
  return out.sort((a, b) => a.start - b.start);
}

/**
 * The parsed value of a top-level string key, or `null` when there is none to
 * read — no such key, or a value that is a list, a nested mapping, or a
 * non-string scalar.
 */
function scalarField(
  doc: Document.Parsed | undefined,
  key: string,
): string | null {
  if (!doc) return null;
  const contents = doc.contents;
  if (!isMap(contents)) return null;
  for (const pair of contents.items) {
    if (!isScalar(pair.key) || pair.key.value !== key) continue;
    const value = pair.value;
    if (isScalar(value) && typeof value.value === "string") return value.value;
    return null;
  }
  return null;
}

/**
 * EDT-FR-21 / EDT-FR-58 / EDT-FR-60: read the frontmatter block as the YAML it
 * is. The parse is read-only — it yields role spans and the description's
 * resolved value, and never rewrites, re-indents, re-quotes or reorders the
 * text it was handed.
 */
export function analyzeFrontmatter(text: string): FrontmatterAnalysis {
  let tokens: CST.Token[];
  let docs: Document.Parsed[];
  try {
    // One tokenization feeds both halves: the role spans are read off these
    // tokens, and the documents are composed from the same ones rather than
    // from a second pass over the source.
    tokens = Array.from(new Parser().parse(text));
    docs = Array.from(new Composer().compose(tokens, true));
  } catch (err) {
    // A parse that throws outright is as invalid as one that reports errors.
    return {
      valid: false,
      error: err instanceof Error ? err.message : String(err),
      spans: [],
      description: null,
    };
  }
  // A `---` inside the block starts a second document; an error anywhere in the
  // stream makes the block invalid.
  const failed = docs.find((doc) => doc.errors.length > 0);
  if (failed) {
    return {
      valid: false,
      error: failed.errors[0].message,
      spans: [],
      description: null,
    };
  }
  return {
    valid: true,
    error: null,
    spans: spansOf(tokens),
    description: scalarField(docs[0], "description"),
  };
}
