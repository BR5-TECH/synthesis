/**
 * DCR-FR-JKVA / DCR-FR-ESIY: the source of the diagram in the fence text of a
 * diagram hunk.
 *
 * A diagram hunk carries the whole fence of the diagram it changes, and the
 * review draws and compares the diagram, so it needs the source alone. The
 * fence can be indented (a list item), can use backticks or tildes, and can
 * have no closing fence when the file ends inside it.
 */

/**
 * DCR-FR-JKVA: what a diagram hunk changes, as the source of each diagram.
 * `oldSource` is null for an addition and `newSource` is null for a removal.
 */
export interface DiagramChange {
  oldSource: string | null;
  newSource: string | null;
}

/** The opening line of a fence: its indent, its marker, and its info string. */
const OPENING = /^([ \t]*)(`{3,}|~{3,})[ \t]*(.*)$/;

/** The number of columns that an indent takes. A tab counts as four. */
function columns(indent: string): number {
  let count = 0;
  for (const ch of indent) count += ch === "\t" ? 4 : 1;
  return count;
}

/** `line` without the first `width` columns of its indent. */
function dedent(line: string, width: number): string {
  let used = 0;
  let at = 0;
  while (at < line.length && used < width) {
    const ch = line[at];
    if (ch === " ") used += 1;
    else if (ch === "\t") used += 4;
    else break;
    at += 1;
  }
  return line.slice(at);
}

/**
 * The source of the Mermaid diagram that `fenceText` holds, or null when the
 * text is not the fence of a Mermaid diagram.
 *
 * The opening and the closing fence lines, the indent of the fence, and the
 * line breaks after the fence are removed. A line break inside the source is
 * one `\n`, whatever the file used.
 */
export function diagramSourceOf(fenceText: string | null | undefined): string | null {
  if (fenceText === null || fenceText === undefined) return null;
  const lines = fenceText.split(/\r\n|\r|\n/);
  // The line breaks after the fence, and any blank line before it.
  while (lines.length > 0 && lines[lines.length - 1].trim() === "") lines.pop();
  while (lines.length > 0 && lines[0].trim() === "") lines.shift();
  if (lines.length === 0) return null;

  const opening = OPENING.exec(lines[0]);
  if (opening === null) return null;
  const [, indent, marker, info] = opening;
  // A fence of backticks has no backtick in its info string.
  if (marker[0] === "`" && info.includes("`")) return null;
  if (!/^mermaid(?:\s|$|\{)/i.test(info.trim())) return null;
  const width = columns(indent);

  const body = lines.slice(1);
  const last = body[body.length - 1];
  if (last !== undefined) {
    const closing = new RegExp(`^[ \\t]*\\${marker[0]}{${marker.length},}[ \\t]*$`);
    if (closing.test(last)) body.pop();
  }
  while (body.length > 0 && body[body.length - 1].trim() === "") body.pop();
  return body.map((line) => dedent(line, width)).join("\n");
}
