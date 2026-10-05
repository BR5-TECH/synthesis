import { FRONTMATTER_RE } from "../markdownFidelity";

interface SplitDoc {
  fm: string | null;
  body: string;
  raw: string;
}

/**
 * EDT-FR-18: split a leading YAML frontmatter block off the body. `raw` is the
 * verbatim block (fences + trailing newline) so an untouched frontmatter
 * round-trips byte-for-byte (`raw + body === original`); `fm` is the editable
 * inner text. Files without frontmatter yield `{ fm: null }`.
 */
export function splitFrontmatter(text: string): SplitDoc {
  const m = FRONTMATTER_RE.exec(text);
  if (!m) return { fm: null, body: text, raw: "" };
  return { fm: m[1], body: text.slice(m[0].length), raw: m[0] };
}

/**
 * Reassemble the full Markdown for save. An unedited frontmatter reuses the
 * verbatim original block (byte-preserving, EDT-FR-18); an edited one is
 * reconstructed in canonical `---\n…\n---\n` form.
 */
export function joinFrontmatter(
  fm: string | null,
  body: string,
  orig: { fm: string | null; raw: string },
): string {
  if (fm === null) return body;
  if (orig.fm !== null && fm === orig.fm) return orig.raw + body;
  // Edited: reconstruct a canonical block, matching the file's line endings so a
  // CRLF document doesn't gain mixed endings.
  const nl = orig.raw.includes("\r\n") ? "\r\n" : "\n";
  return `---${nl}${fm}${nl}---${nl}${body}`;
}

