/**
 * `MIV-map-issue-view.md`: one fenced code block of a rich Markdown body.
 *
 * - A `mermaid` fence draws its diagram, inert (MIV-FR-CTJU). While it renders,
 *   and where it fails, the source shows.
 * - A fence whose language resolves (MIV-FR-ALPJ) is highlighted with the
 *   theme-aware token roles (MIV-FR-GZTZ). Its text is the source, character
 *   for character.
 * - Any other fence is plain.
 *
 * Every piece of text goes in as a React text node, so the block renders no
 * HTML of the body as elements.
 */
import { Fragment, useMemo, type ReactNode } from "react";

import { MermaidStatic } from "./DiffView/MermaidStatic";
import { diagramKindOfLanguage } from "./diagramKind";
import {
  fenceLanguage,
  tokenize,
  tokensByLine,
  type TokenSpan,
} from "../state/syntaxHighlight";

/** The text of one line, cut at its token bounds. */
function lineRuns(line: string, spans: readonly TokenSpan[]): ReactNode {
  if (spans.length === 0) return line;
  const out: ReactNode[] = [];
  let at = 0;
  spans.forEach((span, index) => {
    if (span.start > at) out.push(line.slice(at, span.start));
    out.push(
      <span key={index} className={`hl hl--${span.role}`}>
        {line.slice(span.start, span.end)}
      </span>,
    );
    at = span.end;
  });
  if (at < line.length) out.push(line.slice(at));
  return out;
}

/**
 * MIV-FR-DDAF / MIV-FR-ALPJ: the word of the info string that the label shows,
 * with a leading `.`, `{`, or `language-` removed and its case kept.
 */
function labelOf(language: string | undefined): string | null {
  const trimmed = (language ?? "").trim().replace(/^\{/, "").replace(/^\./, "").replace(/^language-/i, "");
  const word = trimmed.split(/[\s,{}]/)[0] ?? "";
  return word === "" ? null : word;
}

/**
 * MIV-FR-DDAF: the block and its language label. The label sits on the box,
 * not in the block that scrolls, so it stays at the trailing edge.
 */
function FenceBox({ label, children }: { label: string | null; children: ReactNode }) {
  return (
    <div className="code-fence-box">
      {children}
      {label !== null && (
        <span className="code-fence__lang" aria-hidden="true">
          {label}
        </span>
      )}
    </div>
  );
}

function PlainFence({ text, label }: { text: string; label: string | null }) {
  return (
    <FenceBox label={label}>
      <pre className="code-fence" data-language={label ?? undefined}>
        <code>{text}</code>
      </pre>
    </FenceBox>
  );
}

function HighlightedFence({
  text,
  language,
  label,
}: {
  text: string;
  language: string;
  label: string | null;
}) {
  const lines = useMemo(() => {
    const spans = tokenize(text, language);
    return { rows: text.split("\n"), tokens: tokensByLine(text, spans) };
  }, [text, language]);
  return (
    <FenceBox label={label}>
      <pre
        className="code-fence code-fence--highlighted"
        data-language={label ?? undefined}
        data-highlight={language}
      >
        <code>
          {lines.rows.map((row, index) => (
            <Fragment key={index}>
              {index > 0 && "\n"}
              {lineRuns(row, lines.tokens[index] ?? [])}
            </Fragment>
          ))}
        </code>
      </pre>
    </FenceBox>
  );
}

/** MIV-FR-DDAF: one fenced code block, highlighted where its language resolves. */
export function CodeFence({
  text,
  language,
}: {
  text: string;
  language: string | undefined;
}) {
  const label = labelOf(language);
  if (diagramKindOfLanguage(label?.toLowerCase()) === "mermaid") {
    return (
      <MermaidStatic source={text}>
        <PlainFence text={text} label={label} />
      </MermaidStatic>
    );
  }
  const resolved = fenceLanguage(language);
  if (resolved === null) return <PlainFence text={text} label={label} />;
  return <HighlightedFence text={text} language={resolved} label={label} />;
}

/** The renderer that `DocumentCodeContext` takes. */
export function renderCodeFence(text: string, language: string | undefined): ReactNode {
  return <CodeFence text={text} language={language} />;
}
