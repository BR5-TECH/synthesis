/**
 * Provider citation markers, hidden wherever stored model text is shown.
 *
 * OpenAI models mark the sources of a web search with private-use spans:
 * `U+E200 cite U+E202 turn0search1 U+E202 turn0search2 U+E201`. The backend
 * removes them from every reply it reads (`CVL-conversation-loop.md`
 * CVL-FR-VSSU). Text stored before that, or stored by any other path, still
 * holds them, so a surface hides them by the same rule (CMT-FR-AWIE,
 * DQA-FR-JADB). The rule matches `strip_citation_markers` in
 * `src-tauri/src/agent_conversations/citation_markers.rs`.
 */

const SPAN_OPEN = "";
const SPAN_CLOSE = "";

/** CVL-FR-VSSU: a character of the block the markers are drawn from. */
const MARKER_CHAR = /[-]/;

/**
 * CVL-FR-VSSU: `text` with every provider citation marker removed. A span from
 * `U+E200` to the next `U+E201` goes together with the spaces and tabs just
 * before it. A span that another `U+E200` interrupts, or that never closes, is
 * no span: its opening character goes alone, as any other lone character of
 * the block does. Text that holds no marker is returned as it is.
 */
export function stripCitationMarkers(text: string): string {
  if (!MARKER_CHAR.test(text)) return text;
  let out = "";
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === SPAN_OPEN) {
      let at = i + 1;
      while (at < text.length && text[at] !== SPAN_CLOSE && text[at] !== SPAN_OPEN) at++;
      if (at < text.length && text[at] === SPAN_CLOSE) {
        out = out.replace(/[ \t]+$/, "");
        i = at + 1;
        continue;
      }
    }
    if (!MARKER_CHAR.test(c)) out += c;
    i++;
  }
  return out;
}
