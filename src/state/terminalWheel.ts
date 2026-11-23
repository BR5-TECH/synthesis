/**
 * The mouse wheel of a terminal tab (`../../specifications/ui/TRM-terminals.md`
 * TRM-FR-QMXG, TRM-FR-PACR).
 *
 * ghostty-web 0.4.0 never reports the wheel to the program. In the alternate
 * screen it sends arrow keys, and elsewhere it scrolls its scrollback. A
 * program that turned mouse reporting on (Claude Code, vim, less) must get
 * wheel reports instead, so this handler sends them itself.
 */
import type { Terminal } from "ghostty-web";

/** TRM-FR-PACR: the most wheel reports that one wheel event sends. */
export const MAX_WHEEL_REPORTS = 5;

/** The wheel travel of one terminal that is not yet a whole cell. */
export interface WheelTravel {
  /** Travel towards wheel reports, in cells. */
  report: number;
  /** Travel of a Shift+wheel scroll of the scrollback, in lines. */
  scroll: number;
}

export function newWheelTravel(): WheelTravel {
  return { report: 0, scroll: 0 };
}

/** The mode in which a report uses the SGR form `ESC [ < b ; col ; row M`. */
const SGR_MOUSE_MODE = 1006;
/**
 * The legacy form `ESC [ M` holds each value as one byte. Input goes to the
 * session as UTF-8, so a value above 127 would become two bytes: the form
 * reaches a coordinate of at most 95.
 */
const LEGACY_LIMIT = 95;

type WheelTerm = Pick<Terminal, "cols" | "rows" | "hasMouseTracking" | "getMode" | "scrollLines">;

/** The cells of wheel travel, from the event's delta and its delta mode. */
function cells(delta: number, mode: number, cellHeight: number, rows: number): number {
  if (mode === 1) return delta; // DOM_DELTA_LINE
  if (mode === 2) return delta * rows; // DOM_DELTA_PAGE
  return cellHeight > 0 ? delta / cellHeight : 0;
}

/**
 * Adds `amount` to `travel` and returns the whole cells it now holds, with
 * their sign. A change of direction drops the travel of the other direction.
 */
function take(travel: number, amount: number): [number, number] {
  const start = Math.sign(travel) !== 0 && Math.sign(travel) !== Math.sign(amount) ? 0 : travel;
  const total = start + amount;
  const whole = Math.trunc(total);
  return [whole, total - whole];
}

/** TRM-FR-QMXG: one wheel report at a 1-based cell. */
export function wheelReport(up: boolean, col: number, row: number, mods: { alt: boolean; ctrl: boolean }, sgr: boolean): string | null {
  const button = (up ? 64 : 65) + (mods.alt ? 8 : 0) + (mods.ctrl ? 16 : 0);
  if (sgr) return `\x1b[<${button};${col};${row}M`;
  if (col > LEGACY_LIMIT || row > LEGACY_LIMIT) return null;
  return `\x1b[M${String.fromCharCode(32 + button, 32 + col, 32 + row)}`;
}

/**
 * TRM-FR-QMXG, TRM-FR-PACR: handles a wheel event over the terminal. Returns
 * true when it handled the event, and false to leave it to the library.
 * `rect` is the box of the terminal's cells.
 */
export function handleTerminalWheel(
  event: WheelEvent,
  term: WheelTerm,
  rect: { left: number; top: number; width: number; height: number },
  travel: WheelTravel,
  send: (data: string) => void,
): boolean {
  const rows = Math.max(1, term.rows);
  const cols = Math.max(1, term.cols);
  const cellHeight = rect.height / rows;
  if (event.shiftKey) {
    // TRM-FR-PACR: Shift with the wheel scrolls the scrollback. WebKit on
    // macOS turns a Shift+wheel of a mouse into a horizontal delta.
    const delta = event.deltaY !== 0 ? event.deltaY : event.deltaX;
    const [lines, rest] = take(travel.scroll, cells(delta, event.deltaMode, cellHeight, rows));
    travel.scroll = rest;
    if (lines !== 0) term.scrollLines(lines);
    return true;
  }
  if (!term.hasMouseTracking()) return false;
  const [steps, rest] = take(travel.report, cells(event.deltaY, event.deltaMode, cellHeight, rows));
  travel.report = rest;
  const count = Math.min(Math.abs(steps), MAX_WHEEL_REPORTS);
  if (count === 0) return true;
  const cellWidth = rect.width / cols;
  const col = clamp(Math.floor((event.clientX - rect.left) / (cellWidth || 1)), 0, cols - 1) + 1;
  const row = clamp(Math.floor((event.clientY - rect.top) / (cellHeight || 1)), 0, rows - 1) + 1;
  const report = wheelReport(steps < 0, col, row, { alt: event.altKey, ctrl: event.ctrlKey }, term.getMode(SGR_MOUSE_MODE, false));
  if (report !== null) send(report.repeat(count));
  return true;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
