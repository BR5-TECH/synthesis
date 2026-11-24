/**
 * The answers to the colour queries of a program (`../../specifications/ui/TRM-terminals.md`
 * TRM-FR-XBAP).
 *
 * ghostty-web 0.4.0 does not answer `OSC 10 ; ?` and `OSC 11 ; ?`. A program
 * that gets no answer thinks that the background is dark, and on a light theme
 * its colours are hard to read. So the terminal tab finds these queries in the
 * live output and answers them with the colours that the terminal shows.
 */
import { logDebug } from "../logging";
import { parseColor, type Rgb } from "./terminalContrast";

const ESC = 0x1b;
const BEL = 0x07;
const OSC_START = 0x5d; // ]
const BACKSLASH = 0x5c;
const CAN = 0x18;
const SUB = 0x1a;

/** The longest OSC body that the scanner keeps. A query is 4 bytes; longer ones are not queries. */
const BODY_LIMIT = 8;

type State = "ground" | "escape" | "osc" | "osc_escape";
type Kind = "foreground" | "background";

interface Reply {
  /** The byte offset just after the query's terminator. */
  end: number;
  kind: Kind;
  data: string;
}

export interface ColorQueryReplier {
  /** Reads one frame of output, in order. */
  scan(frame: Uint8Array): void;
  /**
   * The attach sent `bytes` of history first; queries in it get no answer.
   * Without a size, all output read until now counts as history.
   */
  historyKnown(bytes?: number): void;
}

/** `rgb:rrrr/gggg/bbbb`, the form of xterm. */
export function xtermColor([r, g, b]: Rgb): string {
  const hex = (c: number) => c.toString(16).padStart(2, "0").repeat(2);
  return `rgb:${hex(r)}/${hex(g)}/${hex(b)}`;
}

/**
 * TRM-FR-XBAP: a scanner for one attach. It answers each query that ends after
 * the history, through `send`, with the terminator of the query.
 */
export function newColorQueryReplier(options: {
  terminalId: string;
  foreground: string | undefined;
  background: string | undefined;
  /** Sends input to the session. Returns false where the session took none. */
  send: (data: string) => boolean;
}): ColorQueryReplier {
  const colors: Record<Kind, Rgb | null> = {
    foreground: parseColor(options.foreground),
    background: parseColor(options.background),
  };
  let state: State = "ground";
  let body: number[] = [];
  let overflow = false;
  let offset = 0;
  let history: number | null = null;
  const pending: Reply[] = [];

  const answer = (reply: Reply) => {
    if (!options.send(reply.data)) return;
    logDebug(["frontend"], "terminal colour query answered", { terminalId: options.terminalId, kind: reply.kind });
  };

  const finish = (end: number, terminator: string) => {
    const text = overflow ? "" : String.fromCharCode(...body);
    body = [];
    overflow = false;
    const kind: Kind | null = text === "10;?" ? "foreground" : text === "11;?" ? "background" : null;
    if (!kind) return;
    const color = colors[kind];
    if (!color) return;
    const reply: Reply = { end, kind, data: `\x1b]${kind === "foreground" ? 10 : 11};${xtermColor(color)}${terminator}` };
    if (history === null) pending.push(reply);
    else if (end > history) answer(reply);
  };

  const collect = (byte: number) => {
    if (body.length < BODY_LIMIT) body.push(byte);
    else overflow = true;
  };

  return {
    scan(frame) {
      for (let i = 0; i < frame.length; i++) {
        const byte = frame[i];
        const end = offset + i + 1;
        switch (state) {
          case "ground":
            if (byte === ESC) state = "escape";
            break;
          case "escape":
            if (byte === OSC_START) {
              state = "osc";
              body = [];
              overflow = false;
            } else state = byte === ESC ? "escape" : "ground";
            break;
          case "osc":
            if (byte === BEL) {
              state = "ground";
              finish(end, "\x07");
            } else if (byte === ESC) state = "osc_escape";
            else if (byte === CAN || byte === SUB) state = "ground";
            else collect(byte);
            break;
          case "osc_escape":
            if (byte === BACKSLASH) {
              state = "ground";
              finish(end, "\x1b\\");
            } else if (byte === OSC_START) {
              // An ESC cancels the OSC; this one starts the next.
              state = "osc";
              body = [];
              overflow = false;
            } else state = byte === ESC ? "escape" : "ground";
            break;
        }
      }
      offset += frame.length;
    },
    historyKnown(bytes = offset) {
      history = bytes;
      for (const reply of pending.splice(0)) if (reply.end > bytes) answer(reply);
    },
  };
}
