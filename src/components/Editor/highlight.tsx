import { Fragment } from "react";
import type { ReactNode } from "react";
import { CURRENT_MATCH_CLASS, MATCH_CLASS } from "../findHighlight";
import {
  ANCHOR_ATTR,
  ANCHOR_CLASS,
  ANCHOR_FOCUSED_ATTR,
  FOCUSED_ANCHOR_CLASS,
  anchorLabel,
} from "../commentHighlight";
import type { YamlRole, YamlSpan } from "../../state/frontmatterYaml";
import type { FindMatch } from "../../state/findMatches";
import type { TokenSpan } from "../../state/syntaxHighlight";

/** A half-open range within a line, plus whether it is the current match. */
interface LineMatch {
  start: number;
  end: number;
  current: boolean;
}

/** A role span (EDT-FR-21) clipped to one rendered line. */
type LineSpan = { start: number; end: number; role: YamlRole };

/**
 * CMT-FR-28: one thread's anchored passage as the source layer marks it.
 *
 * The rich body carries these as ProseMirror decorations; a `<textarea>` cannot
 * carry a mark at all, so on the source surface they are drawn on the same
 * aria-hidden layer the tokens and the find matches are — which is also what
 * makes them measurable, since the rail needs a pixel offset to align a card to
 * (CMT-FR-27).
 */
export interface LayerAnchor {
  start: number;
  end: number;
  threadId: string;
  focused: boolean;
  /**
   * Whether the passage is *marked* as well as placed. A resolved thread and an
   * open draft are placed so their cards have something to align to, and neither
   * is marked: one is no longer under discussion (CMT-FR-17) and the other is
   * not a thread yet.
   */
  marked: boolean;
}

/** The element a YAML role renders as on the highlight layer. */
function roleWrap(role: YamlRole, node: ReactNode): ReactNode {
  if (role === "key") {
    return <strong className="editor__frontmatter-key">{node}</strong>;
  }
  return (
    <span
      className={
        role === "comment"
          ? "editor__frontmatter-punct editor__frontmatter-punct--comment"
          : "editor__frontmatter-punct"
      }
    >
      {node}
    </span>
  );
}

/**
 * Render `text` with three independent, possibly overlapping decorations: the
 * YAML role spans (EDT-FR-21 — keys bold, indicators and comments receding), the
 * find panel's matches (EFR-FR-DOQR), and a source file's syntax tokens
 * (ESH-FR-BABL).
 *
 * The text is cut at every decoration boundary and each resulting chunk is
 * wrapped in whatever covers it, so a match that starts mid-keyword renders
 * correctly instead of one decoration silently winning. Nothing is inserted or
 * dropped: the concatenated output reproduces `text` byte-for-byte, which is
 * what keeps every overlay presentation-only.
 *
 * A token is wrapped innermost so its colour is the one that reaches the glyphs,
 * and the match's marking — a background and a border, not a colour — is drawn
 * around it: that is how ESH-FR-GXUX has both readable at once rather than one
 * defeating the other.
 */
export function renderSegments(
  text: string,
  roles: readonly LineSpan[],
  matches: readonly LineMatch[],
  layers: {
    tokens?: readonly TokenSpan[];
    /** CMT-FR-28: the anchored passages, marked and measurable on this layer. */
    anchors?: readonly LayerAnchor[];
  } = {},
): ReactNode {
  const tokens = layers.tokens ?? [];
  const anchors = layers.anchors ?? [];
  if (
    roles.length === 0 &&
    matches.length === 0 &&
    tokens.length === 0 &&
    anchors.length === 0
  ) {
    return text;
  }

  const bounds = new Set<number>([0, text.length]);
  for (const r of roles) {
    bounds.add(r.start);
    bounds.add(r.end);
  }
  for (const m of matches) {
    bounds.add(m.start);
    bounds.add(m.end);
  }
  for (const t of tokens) {
    bounds.add(t.start);
    bounds.add(t.end);
  }
  for (const a of anchors) {
    bounds.add(a.start);
    bounds.add(a.end);
  }
  const points = [...bounds]
    .filter((p) => p >= 0 && p <= text.length)
    .sort((a, b) => a - b);

  /**
   * Which span of a sorted, non-overlapping list covers `[start, end)`.
   *
   * A pointer per list rather than a scan per chunk: the chunks are produced in
   * order and every list is already in order, so each advances at most once per
   * chunk. A `find` here would be quadratic in the number of decorations, which
   * on a source file is the number of tokens in the whole document — the surface
   * would slow down in proportion to how much of the file the grammar
   * recognises, which is exactly backwards.
   */
  const cursor = { role: 0, match: 0, token: 0, anchor: 0 };
  const covering = <T extends { start: number; end: number }>(
    list: readonly T[],
    key: keyof typeof cursor,
    start: number,
    end: number,
  ): T | undefined => {
    while (cursor[key] < list.length && list[cursor[key]].end <= start) {
      cursor[key] += 1;
    }
    const candidate = list[cursor[key]];
    return candidate && start >= candidate.start && end <= candidate.end
      ? candidate
      : undefined;
  };

  const out: ReactNode[] = [];
  for (let i = 0; i < points.length - 1; i += 1) {
    const start = points[i];
    const end = points[i + 1];
    if (start === end) continue;
    const chunk = text.slice(start, end);
    const match = covering(matches, "match", start, end);
    let node: ReactNode = chunk;
    const token = covering(tokens, "token", start, end);
    if (token) {
      node = <span className={`hl hl--${token.role}`}>{node}</span>;
    }
    if (match) {
      node = (
        <mark
          className={
            match.current ? `${MATCH_CLASS} ${CURRENT_MATCH_CLASS}` : MATCH_CLASS
          }
        >
          {node}
        </mark>
      );
    }
    const role = covering(roles, "role", start, end);
    if (role) node = roleWrap(role.role, node);
    // CMT-FR-28: outermost, so the anchor's own marking sits behind whatever the
    // passage is otherwise wearing, and carrying the thread id out to the click
    // handler and the measurement pass through the DOM.
    const anchor = covering(anchors, "anchor", start, end);
    if (anchor) {
      node = (
        <span
          className={
            !anchor.marked
              ? undefined
              : anchor.focused
                ? `${ANCHOR_CLASS} ${FOCUSED_ANCHOR_CLASS}`
                : ANCHOR_CLASS
          }
          {...{ [ANCHOR_ATTR]: anchor.threadId }}
          {...(anchor.marked
            ? {
                [ANCHOR_FOCUSED_ATTR]: String(anchor.focused),
                role: "mark",
                "aria-label": anchorLabel(anchor.threadId, anchor.focused),
              }
            : {})}
        >
          {node}
        </span>
      );
    }
    out.push(<Fragment key={start}>{node}</Fragment>);
  }
  return out;
}

/** Clip global match offsets to a `[base, base + length)` window, line-relative. */
export function matchesInWindow(
  matches: readonly FindMatch[],
  current: FindMatch | null,
  base: number,
  length: number,
  /** Rendered length, when it differs from the buffer's (a trimmed CR). */
  rendered: number = length,
): LineMatch[] {
  const out: LineMatch[] = [];
  for (const m of matches) {
    const start = m.start - base;
    const end = m.end - base;
    if (end <= 0 || start >= length) continue;
    out.push({
      start: Math.max(0, start),
      end: Math.min(rendered, end),
      // By value, not identity: the match lists are rebuilt on every render.
      current: current !== null && m.start === current.start && m.end === current.end,
    });
  }
  return out;
}

/** Clip role spans to a `[base, base + length)` line window, line-relative. */
function spansInWindow(
  spans: readonly YamlSpan[],
  base: number,
  length: number,
  rendered: number = length,
): LineSpan[] {
  const out: LineSpan[] = [];
  for (const s of spans) {
    const start = s.start - base;
    const end = s.end - base;
    if (end <= 0 || start >= length) continue;
    out.push({
      start: Math.max(0, start),
      end: Math.min(rendered, end),
      role: s.role,
    });
  }
  return out;
}

/**
 * EDT-FR-21: render the frontmatter YAML by the roles a parse of it gives each
 * piece — keys bold, scalar values normal, indicators and comments receding —
 * for the aria-hidden highlight layer behind the editable textarea. The
 * concatenated output reproduces `value` byte-for-byte (each wrapper holds only
 * the glyphs it covers), so the overlay is presentation-only and never alters
 * the buffer.
 *
 * EDT-FR-58: a block that does not parse arrives here with no spans and renders
 * as plain unhighlighted text; the find matches below still mark on it, since
 * matching does not depend on the YAML being valid.
 *
 * EFR-FR-DSKI: the find panel's matches inside the region are marked on the same
 * layer, with the current one distinguished from the rest.
 */
export function highlightYaml(
  value: string,
  spans: readonly YamlSpan[] = [],
  matches: readonly FindMatch[] = [],
  current: FindMatch | null = null,
): ReactNode {
  const lines = value.split("\n");
  let base = 0;
  return lines.map((line, i) => {
    const lineStart = base;
    // The newline `split` consumed counts toward the next line's offset.
    base += line.length + 1;
    // Drop a trailing CR (CRLF file) for the overlay only: under
    // white-space: pre-wrap a bare `\r` is itself a segment break, so leaving it
    // in would render a double line break against the textarea's single one. The
    // overlay is presentation-only — the textarea keeps the verbatim bytes.
    const text = line.endsWith("\r") ? line.slice(0, -1) : line;
    // Clip against the line as it was in the buffer, not the CR-trimmed copy,
    // so a match reaching the end of a CRLF line is not cut one glyph short.
    // Re-insert the newlines split() consumed so the overlay wraps line-for-line
    // with the textarea (white-space: pre-wrap renders them).
    return (
      <Fragment key={i}>
        {renderSegments(
          text,
          spansInWindow(spans, lineStart, line.length, text.length),
          matchesInWindow(matches, current, lineStart, line.length, text.length),
        )}
        {i < lines.length - 1 ? "\n" : null}
      </Fragment>
    );
  });
}
