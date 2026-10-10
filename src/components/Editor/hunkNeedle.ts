/**
 * The text a proposed change's Markdown renders to
 * (`../../../specifications/ui/DCR-draft-change-review.md` DCR-FR-LGHZ,
 * DCR-FR-TSNW).
 *
 * A change quotes the prompt's **source**, and the surface it is found in holds
 * the **rendered** text. Removing syntax characters one by one closes only the
 * gaps someone thought of: a link keeps its destination, an escape keeps its
 * backslash, and an entity keeps its name, so a change quoting any of them is
 * never drawn. Here the quoted Markdown goes through the parser the document
 * itself went through, so the two sides meet on the same text for every
 * construct the Editor knows.
 */
import { createDocument, type Editor as TiptapEditor } from "@tiptap/react";

import { docText } from "../findHighlight";
import { logWarn } from "../../logging";

/** The part of `tiptap-markdown`'s storage that reads Markdown in. */
type MarkdownStorage = {
  markdown?: { parser?: { parse: (content: string) => string } };
};

/**
 * DCR-FR-LGHZ: the text `markdown` renders to in this editor, in the form
 * `docText` gives the document, or null when it cannot be rendered.
 *
 * Null is not an empty text: the caller falls back to the source, which is
 * what it searched before this existed.
 */
export function renderedText(
  editor: TiptapEditor,
  markdown: string,
): string | null {
  if (markdown === "") return "";
  const parser = (editor.storage as MarkdownStorage).markdown?.parser;
  if (!parser) return null;
  try {
    const html = parser.parse(markdown);
    return docText(createDocument(html, editor.schema)).text;
  } catch (error) {
    // The Markdown is the user's prompt text, and a parser's message can quote
    // it, so only the length and the kind of error are logged.
    logWarn(["frontend"], "could not render a proposed change's text to find it", {
      length: markdown.length,
      error: error instanceof Error ? error.name : "unknown",
    });
    return null;
  }
}

/**
 * DCR-FR-TSNW: a lead without the line it starts in, or null when that would
 * leave nothing to find.
 *
 * The backend cuts a lead at a fixed length, so its first line can start inside
 * a link destination or a code span. Rendered on its own, such a line holds text
 * the document does not. Only the end of a lead places an insertion, so its
 * first line is the part that is safe to lose.
 */
export function leadTail(lead: string): string | null {
  const newline = lead.indexOf("\n");
  if (newline === -1) return null;
  const rest = lead.slice(newline + 1);
  return rest.trim() === "" ? null : rest;
}
