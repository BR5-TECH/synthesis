/**
 * CMT-FR-09: render a comment's Markdown source as rich text.
 *
 * Built on the same `../diff/markdown` parser the diff viewer uses rather than a
 * new dependency: a comment is short-form prose with emphasis, code spans, lists
 * and links — exactly the subset that parser already covers — and a second
 * Markdown implementation in the app would be two things to keep agreeing about
 * what `**bold**` means.
 *
 * A link renders as its text and is deliberately inert, matching the diff
 * viewer's treatment (DFV-FR-06): a comment body is content another author wrote,
 * and turning it into something clickable inside the rail is a navigation
 * decision this surface does not make.
 */
import { Fragment } from "react";
import type { ReactNode } from "react";
import { parseInline, parseMarkdownBlocks } from "../diff/markdown";
import { rosterOf, segmentBody } from "./agentTags";
import type { Roster } from "./agentTags";
import { stripCitationMarkers } from "../text/citationMarkers";

const EMPTY: readonly string[] = [];

function Inline({
  text,
  roster = EMPTY,
}: {
  text: string;
  roster?: Roster;
}): ReactNode {
  return (
    <>
      {parseInline(text).map((span, i) => {
        switch (span.kind) {
          case "strong":
            return <strong key={i}>{span.text}</strong>;
          case "em":
            return <em key={i}>{span.text}</em>;
          case "code":
            return <code key={i}>{span.text}</code>;
          case "link":
            return (
              <a key={i} title={span.href} aria-disabled>
                {span.text}
              </a>
            );
          default:
            return (
              <Fragment key={i}>
                <Tagged text={span.text} roster={roster} />
              </Fragment>
            );
        }
      })}
    </>
  );
}

/**
 * AGT-FR-29: bold the tags in a run of inline text that resolve to at least one
 * enrolled agent, so a reader can tell at a glance who a message was addressed
 * to.
 *
 * Applied to the plain spans of the parse alone: a tag inside a code span is a
 * code sample rather than an address, and a tag inside a link's text belongs to
 * the link. One that resolves to nobody is left exactly as it was written
 * (AGT-FR-28) — an email address, a `@media` rule, an `@all` in a project with
 * nobody to hear it (AGT-FR-38) — which is what lets an author type an `@`
 * without being interrupted.
 */
function Tagged({ text, roster }: { text: string; roster: Roster }): ReactNode {
  const { nicknames, ready } = rosterOf(roster);
  // Nothing enrolled resolves nothing, handle included, so the text stands as
  // written and the parse below is work with no possible output.
  if (nicknames.length === 0 && ready.length === 0) return text;
  const segments = segmentBody(text, roster);
  if (segments.length === 1 && segments[0].kind === "text") return text;
  return (
    <>
      {segments.map((segment, i) =>
        segment.kind === "tag" ? (
          <span key={i} className="comment__tag" data-testid="comment-tag">
            {segment.text}
          </span>
        ) : (
          <Fragment key={i}>{segment.text}</Fragment>
        ),
      )}
    </>
  );
}

export function CommentMarkdown({
  body,
  agentRoster = EMPTY,
  className = "comment__body",
  testId,
}: {
  body: string;
  /**
   * AGT-FR-24: the agents the open project enrolled, and which of them can
   * answer. A tag reaches only these, so the same body rendered in a project that
   * has not enrolled `@arch` marks nothing — which is why the roster is passed in
   * rather than inferred from the text.
   *
   * The ready subset matters here and not only at dispatch: AGT-FR-38 leaves an
   * `@all` unemphasised in a project where nobody can answer it, because marking
   * it would promise a reader participants who were never in the conversation.
   */
  agentRoster?: Roster;
  /**
   * The surface's own class for the rendered body. A Flow node renders its
   * inline prompt through here too (`FLO-flow.md` FLO-FR-13, on the terms
   * CMT-FR-09 sets), and a node is not a card — the block treatments below are
   * shared, the box around them is not.
   */
  className?: string;
  testId?: string;
}) {
  // CMT-FR-AWIE: a provider citation marker is hidden, and the stored body
  // is not changed.
  const blocks = parseMarkdownBlocks(stripCitationMarkers(body));
  return (
    // Every surface that renders a comment or a note body is a narrow one — a
    // card in the rail (`CMT-comments.md`), a row in the Comments panel
    // (`CMP-comments-panel.md` CMP-FR-12), a row in the Notes panel
    // (`NTS-notes.md` NTS-FR-25) — and all three set a body at the UI scale, so
    // a body is the same size wherever it is read. There is no document-scale
    // rendering of one to select between.
    <div className={`${className} doc doc--ui`} data-testid={testId}>
      {blocks.map((block, i) => {
        switch (block.kind) {
          case "heading": {
            // A heading inside a comment is rendered at a size that belongs in a
            // card rather than at the document scale its level would imply — the
            // rail is a narrow column beside the artifact, not a second document.
            return (
              <p key={i} className="comment__body-heading">
                <Inline text={block.text} roster={agentRoster} />
              </p>
            );
          }
          case "code":
            return (
              <pre key={i}>
                <code>{block.text}</code>
              </pre>
            );
          case "quote":
            return (
              <blockquote key={i}>
                <Inline text={block.text} roster={agentRoster} />
              </blockquote>
            );
          case "list-item":
            return (
              <div
                key={i}
                className="comment__body-list"
                style={{ paddingLeft: 12 * ((block.depth ?? 0) + 1) }}
              >
                <span className="comment__body-marker">
                  {block.ordered ? `${block.marker ?? "1."}` : "•"}
                </span>{" "}
                <Inline text={block.text} roster={agentRoster} />
              </div>
            );
          case "rule":
            return <hr key={i} />;
          case "table-row":
            // A table in a comment is rare and a real table in a narrow rail is
            // unreadable; the row's cells render as plain text rather than being
            // dropped, so nothing the author wrote disappears.
            return <p key={i}>{(block.cells ?? [block.text]).join(" · ")}</p>;
          default:
            return (
              <p key={i}>
                <Inline text={block.text} roster={agentRoster} />
              </p>
            );
        }
      })}
    </div>
  );
}
