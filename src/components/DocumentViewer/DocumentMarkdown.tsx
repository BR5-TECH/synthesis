/**
 * The rich view of a Markdown document in a Document tab (DTV-FR-SSQI).
 *
 * The view is read-only and safe by construction:
 * - Every piece of text from the document goes in as a React text node. The
 *   code never uses `dangerouslySetInnerHTML`, so an HTML tag in the text shows
 *   as text and never becomes an element.
 * - A link is inert text. It has no `href`, so it cannot navigate.
 * - An image is a placeholder with its alt text. Only an image that holds its
 *   own bytes (a `data:` source) becomes an `<img>`. No other image source is
 *   ever given to an element, so the view loads no remote resource.
 */
import { memo, useMemo } from "react";
import type { ReactNode } from "react";
import {
  isInlineDataImage,
  linkTitle,
  parseInlineWithImages,
} from "./markdownInline";
import { buildDocument } from "./markdownStructure";
import type { DocNode, ListNode } from "./markdownStructure";

function Inline({ text }: { text: string }) {
  const parts = parseInlineWithImages(text);
  return (
    <>
      {parts.map((part, index): ReactNode => {
        switch (part.kind) {
          case "strong":
            return <strong key={index}>{part.text}</strong>;
          case "em":
            return <em key={index}>{part.text}</em>;
          case "code":
            return <code key={index}>{part.text}</code>;
          case "link":
            return (
              <span
                key={index}
                className="doc-link"
                title={linkTitle(part.href)}
              >
                {part.text}
              </span>
            );
          case "image":
            return isInlineDataImage(part.src) ? (
              <img
                key={index}
                className="doc-picture"
                src={part.src}
                alt={part.alt}
              />
            ) : (
              <span
                key={index}
                className="doc-image"
                role="img"
                aria-label={part.alt || "Image"}
              >
                <svg
                  className="doc-image__glyph"
                  viewBox="0 0 16 16"
                  width="14"
                  height="14"
                  aria-hidden="true"
                  focusable="false"
                >
                  <rect
                    x="1.5"
                    y="2.5"
                    width="13"
                    height="11"
                    rx="1.5"
                    fill="none"
                    stroke="currentColor"
                  />
                  <circle cx="5.5" cy="6" r="1.2" fill="currentColor" />
                  <path
                    d="M2 12.5l4-4 3 3 2-2 3 3"
                    fill="none"
                    stroke="currentColor"
                  />
                </svg>
                <span aria-hidden="true">{part.alt || "Image"}</span>
              </span>
            );
          default:
            return <span key={index}>{part.text}</span>;
        }
      })}
    </>
  );
}

function List({ list }: { list: ListNode }) {
  const items = list.items.map((item, index) => (
    <li key={index}>
      <Inline text={item.text} />
      {item.children.map((child, at) => (
        <List key={at} list={child} />
      ))}
    </li>
  ));
  if (!list.ordered) return <ul>{items}</ul>;
  return <ol start={list.start}>{items}</ol>;
}

const HEADINGS = ["h1", "h2", "h3", "h4", "h5", "h6"] as const;

function Node({ node }: { node: DocNode }) {
  switch (node.kind) {
    case "frontmatter":
      return (
        <pre className="doc-frontmatter">
          <code>{node.text}</code>
        </pre>
      );
    case "list":
      return <List list={node.list} />;
    case "table":
      return (
        <div className="tableWrapper">
          <table>
            {node.header && (
              <thead>
                <tr>
                  {node.header.map((cell, at) => (
                    <th key={at}>
                      <Inline text={cell} />
                    </th>
                  ))}
                </tr>
              </thead>
            )}
            <tbody>
              {node.rows.map((row, index) => (
                <tr key={index}>
                  {row.map((cell, at) => (
                    <td key={at}>
                      <Inline text={cell} />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    default: {
      const { block } = node;
      switch (block.kind) {
        case "heading": {
          const Tag = HEADINGS[Math.min(Math.max(block.level ?? 1, 1), 6) - 1];
          return (
            <Tag>
              <Inline text={block.text} />
            </Tag>
          );
        }
        case "code":
          return (
            <pre data-language={block.language}>
              <code>{block.text}</code>
            </pre>
          );
        case "quote":
          return (
            <blockquote>
              <p>
                <Inline text={block.text} />
              </p>
            </blockquote>
          );
        case "rule":
          return <hr />;
        default:
          return (
            <p>
              <Inline text={block.text} />
            </p>
          );
      }
    }
  }
}

function DocumentMarkdownView({ text }: { text: string }) {
  const nodes = useMemo(() => buildDocument(text), [text]);
  return (
    <>
      {nodes.map((node, index) => (
        <Node key={index} node={node} />
      ))}
    </>
  );
}

/** The rendered Markdown. It re-renders only when the text changes. */
export const DocumentMarkdown = memo(DocumentMarkdownView);
