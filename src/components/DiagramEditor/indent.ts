/**
 * EDT-FR-UYBZ: Tab and Shift+Tab in the source pane of the diagram editor.
 * They indent and outdent the source by the indent of the bundled examples,
 * as a text editor does, rather than move focus.
 */

/** The indent of the bundled examples (EDT-FR-SCSP). */
export const INDENT = "  ";

export interface SourceEdit {
  value: string;
  start: number;
  end: number;
}

interface Change {
  at: number;
  remove: number;
  insert: string;
}

/** The start offset of each line that the range from `start` to `end` touches. */
function touchedLines(value: string, start: number, end: number): number[] {
  // `lastIndexOf` reads a negative index as 0, so offset 0 needs its own case.
  const first = start === 0 ? 0 : value.lastIndexOf("\n", start - 1) + 1;
  // A selection that ends at the start of a line does not touch that line.
  const last = end > start && value[end - 1] === "\n" ? end - 1 : end;
  const lines = [first];
  for (let at = value.indexOf("\n", first); at !== -1 && at < last; at = value.indexOf("\n", at + 1)) {
    lines.push(at + 1);
  }
  return lines;
}

/**
 * Applies `changes`, sorted by offset, and moves an offset with them. An
 * offset inside a removed run goes to its start; an offset at an insert point
 * goes after the insert when `after` is true.
 */
function apply(value: string, changes: Change[], start: number, end: number): SourceEdit {
  const move = (pos: number, after: boolean) => {
    let shift = 0;
    for (const c of changes) {
      if (pos < c.at || (pos === c.at && !after && c.remove === 0)) break;
      if (pos < c.at + c.remove) return c.at + shift;
      shift += c.insert.length - c.remove;
    }
    return pos + shift;
  };
  let out = "";
  let from = 0;
  for (const c of changes) {
    out += value.slice(from, c.at) + c.insert;
    from = c.at + c.remove;
  }
  out += value.slice(from);
  const collapsed = start === end;
  return { value: out, start: move(start, collapsed), end: move(end, true) };
}

/**
 * EDT-FR-UYBZ: Tab inserts the indent at the caret, or in place of a selection
 * on one line. A selection that spans lines indents each line it touches.
 */
export function indentSource(value: string, start: number, end: number): SourceEdit {
  if (!value.slice(start, end).includes("\n")) {
    return apply(value, [{ at: start, remove: end - start, insert: INDENT }], end, end);
  }
  const changes = touchedLines(value, start, end).map((at) => ({ at, remove: 0, insert: INDENT }));
  return apply(value, changes, start, end);
}

/**
 * EDT-FR-UYBZ: Shift+Tab removes up to one indent of leading spaces from each
 * line that the caret or the selection touches.
 */
export function outdentSource(value: string, start: number, end: number): SourceEdit {
  const changes: Change[] = [];
  for (const at of touchedLines(value, start, end)) {
    let remove = 0;
    while (remove < INDENT.length && value[at + remove] === " ") remove += 1;
    if (remove > 0) changes.push({ at, remove, insert: "" });
  }
  return apply(value, changes, start, end);
}
