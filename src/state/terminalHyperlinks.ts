/**
 * The addresses of the OSC 8 hyperlinks of a terminal
 * (`../../specifications/ui/TRM-terminals.md` TRM-FR-QMVZ).
 *
 * ghostty-web 0.4.0 keeps a hyperlink id in each cell of an OSC 8 hyperlink,
 * but `getHyperlinkUri()` always returns null, so the library makes no link
 * for it. A program such as Claude Code writes the link of a pull request as
 * `ESC ] 8 ; ; <address> ST PR #7 ESC ] 8 ; ; ST`. This module finds these
 * hyperlinks in the output, keeps each address with the text that it marks,
 * and gives a link provider that finds the address of a run of linked cells
 * from its text.
 */
import type { ILink, ILinkProvider } from "ghostty-web";
import { logWarn } from "../logging";

const ESC = 0x1b;
const BEL = 0x07;
const CAN = 0x18;
const SUB = 0x1a;
const DEL = 0x7f;
const BACKSLASH = 0x5c;

/** TRM-FR-QMVZ: the hyperlinks that the tab keeps for each terminal. */
export const HYPERLINK_STORE_LIMIT = 256;
/** The longest OSC body that the tracker reads. A longer OSC 8 is not kept. */
export const HYPERLINK_BODY_LIMIT = 4096;
/** The most bytes of text that the tracker keeps for one hyperlink. A longer text is not kept. */
export const HYPERLINK_TEXT_LIMIT = 1024;

type State = "ground" | "escape" | "escape_intermediate" | "csi" | "osc" | "string" | "string_escape";

/** The text of a link as the store compares it: no whitespace and no control characters. */
export function hyperlinkKey(text: string): string {
  // eslint-disable-next-line no-control-regex
  return text.replace(/[\s\u0000-\u001f\u007f-\u009f]/gu, "");
}

export interface HyperlinkTracker {
  /** Reads one frame of output, in order. A sequence can go over frames. */
  scan(frame: Uint8Array): void;
  /** The address of the latest hyperlink with the text `text`, or undefined. */
  addressOf(text: string): string | undefined;
  /** The number of hyperlinks kept. */
  readonly size: number;
}

/** TRM-FR-QMVZ: makes the hyperlink store of one terminal. */
export function newHyperlinkTracker(limit = HYPERLINK_STORE_LIMIT): HyperlinkTracker {
  const store = new Map<string, string>();
  const decoder = new TextDecoder("utf-8", { fatal: false });
  let state: State = "ground";
  let body: number[] = [];
  let bodyTooLong = false;
  /** The address of the open hyperlink, or null where none is open. */
  let address: string | null = null;
  let text: number[] = [];
  let textTooLong = false;

  function keep() {
    if (address === null || textTooLong) return;
    const key = hyperlinkKey(decoder.decode(new Uint8Array(text)));
    if (!key) return;
    store.delete(key);
    store.set(key, address);
    while (store.size > limit) {
      const oldest = store.keys().next().value;
      if (oldest === undefined) break;
      store.delete(oldest);
    }
  }

  function close() {
    keep();
    address = null;
    text = [];
    textTooLong = false;
  }

  /** Reads a complete OSC body. Only OSC 8 changes the state of the tracker. */
  function endOsc() {
    if (body.length < 2 || body[0] !== 0x38 || body[1] !== 0x3b) return;
    // An OSC 8 that is too long is not kept, and it ends the open hyperlink,
    // so that its text does not go to the address of an older one.
    if (bodyTooLong) {
      close();
      return;
    }
    const rest = decoder.decode(new Uint8Array(body.slice(2)));
    const semicolon = rest.indexOf(";");
    if (semicolon < 0) return;
    const uri = rest.slice(semicolon + 1);
    close();
    if (uri) address = uri;
  }

  function startOsc() {
    state = "osc";
    body = [];
    bodyTooLong = false;
  }

  function addText(byte: number) {
    if (address === null || textTooLong) return;
    if (text.length >= HYPERLINK_TEXT_LIMIT) {
      textTooLong = true;
      text = [];
      return;
    }
    text.push(byte);
  }

  function escape(byte: number) {
    if (byte === 0x5b) state = "csi";
    else if (byte === 0x5d) startOsc();
    // DCS, SOS, PM, and APC hold a string until ST.
    else if (byte === 0x50 || byte === 0x58 || byte === 0x5e || byte === 0x5f) state = "string";
    else if (byte >= 0x20 && byte <= 0x2f) state = "escape_intermediate";
    else if (byte === ESC) state = "escape";
    // A C0 control after ESC runs and the escape goes on, as in a VT parser.
    else if (byte < 0x20) state = "escape";
    else state = "ground";
  }

  function step(byte: number) {
    if (byte === CAN || byte === SUB) {
      state = "ground";
      return;
    }
    switch (state) {
      case "ground":
        if (byte === ESC) state = "escape";
        else if (byte >= 0x20 && byte !== DEL) addText(byte);
        return;
      case "escape":
        escape(byte);
        return;
      case "escape_intermediate":
        if (byte === ESC) state = "escape";
        else if (byte >= 0x30 && byte <= 0x7e) state = "ground";
        return;
      case "csi":
        if (byte === ESC) state = "escape";
        else if (byte >= 0x40 && byte <= 0x7e) state = "ground";
        return;
      case "osc":
        // As in the terminal's parser, any ESC ends the OSC: `ESC \` is ST,
        // and the ESC of another sequence ends it too.
        if (byte === BEL || byte === ESC) {
          state = byte === ESC ? "escape" : "ground";
          endOsc();
        } else if (body.length >= HYPERLINK_BODY_LIMIT) bodyTooLong = true;
        else body.push(byte);
        return;
      case "string":
        if (byte === ESC) state = "string_escape";
        return;
      case "string_escape":
        if (byte === BACKSLASH) state = "ground";
        else if (byte !== ESC) state = "string";
        return;
    }
  }

  return {
    scan(frame) {
      for (let i = 0; i < frame.length; i++) step(frame[i]);
    },
    addressOf(value) {
      const key = hyperlinkKey(value);
      return key ? store.get(key) : undefined;
    },
    get size() {
      return store.size;
    },
  };
}

/** The part of a buffer line that the provider reads (the library does not export its type). */
export interface HyperlinkLine {
  readonly length: number;
  getCell(x: number): { getHyperlinkId(): number; getCodepoint(): number } | undefined;
}

/** The part of the terminal that the provider reads. */
export interface HyperlinkTerminal {
  readonly buffer: { readonly active: { readonly length: number; getLine(y: number): HyperlinkLine | undefined } };
}

interface Range {
  start: { x: number; y: number };
  end: { x: number; y: number };
}

function idAt(line: HyperlinkLine | undefined, x: number): number {
  return line?.getCell(x)?.getHyperlinkId() ?? 0;
}

/**
 * The run of cells with hyperlink `id` that holds cell (`x`, `y`). As the
 * library does, a run goes on to the next row where it reaches the end of a
 * row and the next row starts with the same id.
 */
export function hyperlinkRange(term: HyperlinkTerminal, id: number, x: number, y: number): Range {
  const buffer = term.buffer.active;
  let startY = y;
  let startX = x;
  while (startX > 0 && idAt(buffer.getLine(startY), startX - 1) === id) startX--;
  while (startX === 0 && startY > 0) {
    const above = buffer.getLine(startY - 1);
    if (!above || above.length === 0 || idAt(above, above.length - 1) !== id) break;
    startY--;
    startX = above.length - 1;
    while (startX > 0 && idAt(above, startX - 1) === id) startX--;
  }
  let endY = y;
  let endX = x;
  let line = buffer.getLine(endY);
  while (line && endX < line.length - 1 && idAt(line, endX + 1) === id) endX++;
  while (line && endX === line.length - 1 && endY + 1 < buffer.length) {
    const below = buffer.getLine(endY + 1);
    if (!below || below.length === 0 || idAt(below, 0) !== id) break;
    endY++;
    line = below;
    endX = 0;
    while (endX < line.length - 1 && idAt(line, endX + 1) === id) endX++;
  }
  return { start: { x: startX, y: startY }, end: { x: endX, y: endY } };
}

/** The text of the cells in `range`, from the code point of each cell. */
export function rangeText(term: HyperlinkTerminal, range: Range): string {
  const buffer = term.buffer.active;
  let out = "";
  for (let y = range.start.y; y <= range.end.y; y++) {
    const line = buffer.getLine(y);
    if (!line) continue;
    const from = y === range.start.y ? range.start.x : 0;
    const to = y === range.end.y ? range.end.x : line.length - 1;
    for (let x = from; x <= to; x++) {
      const code = line.getCell(x)?.getCodepoint() ?? 0;
      if (code >= 0x20) out += String.fromCodePoint(code);
    }
  }
  return out;
}

/**
 * TRM-FR-QMVZ: a link provider that gives each run of cells with one
 * hyperlink id the address that `tracker` keeps for its text. A run with no
 * such text gives no link. `activate` opens the address (TRM-FR-HWKO).
 */
export function newOsc8LinkProvider(
  term: HyperlinkTerminal,
  tracker: HyperlinkTracker,
  activate: (url: string, event: MouseEvent) => void,
): ILinkProvider {
  return {
    provideLinks(y, callback) {
      const line = term.buffer.active.getLine(y);
      if (!line || tracker.size === 0) {
        callback(undefined);
        return;
      }
      const links: ILink[] = [];
      const seen = new Set<number>();
      for (let x = 0; x < line.length; x++) {
        const id = idAt(line, x);
        if (id === 0 || seen.has(id)) continue;
        seen.add(id);
        const range = hyperlinkRange(term, id, x, y);
        const url = tracker.addressOf(rangeText(term, range));
        if (url) links.push({ text: url, range, activate: (event) => activate(url, event) });
      }
      callback(links.length > 0 ? links : undefined);
    },
  };
}

/** The part of the terminal that `registerOsc8Links` uses. */
export interface LinkProviderHost extends HyperlinkTerminal {
  registerLinkProvider?: (provider: ILinkProvider) => void;
}

/**
 * TRM-FR-QMVZ: registers the OSC 8 link provider on `term`. Where the library
 * gives no way to register it, it changes nothing and, where `warn` is true,
 * logs a `WARN` record (TRM-FR-LNOZ). Returns whether it applied.
 */
export function registerOsc8Links(
  term: LinkProviderHost,
  tracker: HyperlinkTracker,
  activate: (url: string, event: MouseEvent) => void,
  terminalId: string,
  warn = true,
): boolean {
  try {
    if (typeof term.registerLinkProvider !== "function") throw new Error("no registerLinkProvider");
    term.registerLinkProvider(newOsc8LinkProvider(term, tracker, activate));
    return true;
  } catch {
    if (warn) logWarn(["frontend"], "terminal links not applied", { terminalId, reason: "no_link_provider" });
    return false;
  }
}
