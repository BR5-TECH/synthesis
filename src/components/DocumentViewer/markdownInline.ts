/**
 * Inline reading for the rich view of a Document tab (DTV-FR-SSQI).
 *
 * The shared parser knows code, emphasis, and links. It does not know images, so
 * this module splits the images out first and gives the rest to the parser.
 * Nothing here builds markup. It only returns plain data, and the renderer turns
 * that data into React text nodes.
 */
import { parseInline } from "../../diff/markdown";
import type { InlineSpan } from "../../diff/markdown";

export type InlinePart =
  | InlineSpan
  | { kind: "image"; alt: string; src: string };

const IMAGE = /!\[([^\]]*)\]\(([^)\s]*)\)/g;

/**
 * An image source that holds its own bytes. It is not remote, so it is the
 * only kind of image source the view may load.
 */
const DATA_IMAGE = /^data:image\/(?:png|jpeg|gif|webp);base64,[A-Za-z0-9+/=]+$/;

export function isInlineDataImage(src: string): boolean {
  return DATA_IMAGE.test(src);
}

/** Split a run of text into inline parts, with images as parts of their own. */
export function parseInlineWithImages(text: string): InlinePart[] {
  const parts: InlinePart[] = [];
  let last = 0;
  for (const match of text.matchAll(IMAGE)) {
    const at = match.index ?? 0;
    if (at > last) parts.push(...parseInline(text.slice(last, at)));
    parts.push({ kind: "image", alt: match[1], src: match[2] });
    last = at + match[0].length;
  }
  if (last < text.length) parts.push(...parseInline(text.slice(last)));
  return parts;
}

/**
 * The text a link shows as its tooltip. A link that does not point to a web
 * page, a mail address, or a relative place shows no tooltip. A script address
 * therefore never becomes an attribute.
 */
export function linkTitle(href: string): string | undefined {
  if (href === "") return undefined;
  const scheme = /^([A-Za-z][A-Za-z0-9+.-]*):/.exec(href);
  if (scheme === null) return href;
  return /^(https?|mailto)$/i.test(scheme[1]) ? href : undefined;
}
