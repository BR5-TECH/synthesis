/**
 * The terminal of each open terminal tab (`../../specifications/ui/TRM-terminals.md`
 * TRM-FR-XBVB … TRM-FR-DTCA).
 *
 * The Viewport mounts only the active tab, but a hidden terminal tab stays
 * attached and keeps writing output (TRM-FR-ITAK). So each tab's `ghostty-web`
 * terminal lives here, outside React, in an element of its own. The tab view
 * puts that element into the page while the tab is active and takes it out
 * when the tab is hidden. Closing the tab detaches the session and disposes
 * the terminal (TRM-FR-JXLA); it never ends the session.
 */
import type { FitAddon, ITheme, Terminal } from "ghostty-web";
import * as api from "../api";
import { logDebug, logWarn } from "../logging";
import { currentTheme, subscribeTheme } from "../components/markdownMermaidRender";
import { isMacPlatform } from "../platform";
import { TERMINAL_INPUT_LIMIT } from "../api/terminals";
import { errorCode, findTerminal, subscribeTerminals } from "./terminals";

/** TRM-FR-DTCA: the scrollback of each terminal, in lines. */
export const TERMINAL_SCROLLBACK_LINES = 10_000;

/**
 * TRM-FR-DTCA: ghostty-web takes its scrollback limit in bytes of its own
 * page memory, about 14 bytes for each cell (measured on 0.4.0). The budget
 * holds 10,000 lines of 160 columns.
 */
export const TERMINAL_SCROLLBACK_BYTES = TERMINAL_SCROLLBACK_LINES * 160 * 14;

interface ViewEntry {
  id: string;
  host: HTMLDivElement;
  term: Terminal | null;
  fit: FitAddon | null;
  lastSize: { cols: number; rows: number } | null;
  /** The size the backend has; it differs from `lastSize` until a running session gets it. */
  sentSize: { cols: number; rows: number } | null;
  starting: Promise<void> | null;
  released: boolean;
  /** Counts the terminals made for this tab; output of an older one is dropped. */
  generation: number;
  dispose: (() => void)[];
}

const views = new Map<string, ViewEntry>();
type Ghostty = typeof import("ghostty-web");
let ready: Promise<Ghostty> | null = null;
let stopTheme: (() => void) | null = null;
let stopFonts: (() => void) | null = null;

/**
 * Loads the library on the first terminal, so the main bundle does not carry
 * it and its WebAssembly.
 */
function ghosttyReady(): Promise<Ghostty> {
  if (!ready) {
    ready = import("ghostty-web").then(async (ghostty) => {
      await ghostty.init();
      return ghostty;
    });
    ready.catch(() => {
      ready = null;
    });
  }
  return ready;
}

function cssVar(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value || fallback;
}

const LIGHT_ANSI: ITheme = {
  black: "#24292f", red: "#cf222e", green: "#116329", yellow: "#4d2d00",
  blue: "#0969da", magenta: "#8250df", cyan: "#1b7c83", white: "#6e7781",
  brightBlack: "#57606a", brightRed: "#a40e26", brightGreen: "#1a7f37", brightYellow: "#633c01",
  brightBlue: "#218bff", brightMagenta: "#a475f9", brightCyan: "#3192aa", brightWhite: "#8c959f",
};

const DARK_ANSI: ITheme = {
  black: "#484f58", red: "#ff7b72", green: "#3fb950", yellow: "#d29922",
  blue: "#58a6ff", magenta: "#bc8cff", cyan: "#39c5cf", white: "#b1bac4",
  brightBlack: "#6e7681", brightRed: "#ffa198", brightGreen: "#56d364", brightYellow: "#e3b341",
  brightBlue: "#79c0ff", brightMagenta: "#d2a8ff", brightCyan: "#56d4dd", brightWhite: "#f0f6fc",
};

/** TRM-FR-WRVB: the terminal colors of the app theme. */
export function terminalTheme(): ITheme {
  const dark = currentTheme() === "dark";
  return {
    ...(dark ? DARK_ANSI : LIGHT_ANSI),
    background: cssVar("--bg-canvas", dark ? "#16181d" : "#ffffff"),
    foreground: cssVar("--fg-1", dark ? "#e6e6e6" : "#1f2328"),
    cursor: cssVar("--accent", dark ? "#7aa2f7" : "#0969da"),
    selectionBackground: cssVar("--bg-selection", dark ? "#2d3a55" : "#cfe3ff"),
  };
}

/** TRM-FR-WRVB: the Source code typographic role. */
export function terminalFont(): { fontFamily: string; fontSize: number } {
  const family = cssVar("--font-source-family", "\"JetBrains Mono\", ui-monospace, monospace");
  const size = Number.parseFloat(cssVar("--font-source-size", "13px"));
  return { fontFamily: family, fontSize: Number.isFinite(size) && size > 0 ? size : 13 };
}

/** TRS-FR-AVNH: sends input in parts of at most 64 KiB. */
export function inputParts(data: string): string[] {
  const encoder = new TextEncoder();
  if (encoder.encode(data).length <= TERMINAL_INPUT_LIMIT) return [data];
  const parts: string[] = [];
  let current = "";
  let size = 0;
  for (const char of data) {
    const len = encoder.encode(char).length;
    if (size + len > TERMINAL_INPUT_LIMIT) {
      parts.push(current);
      current = "";
      size = 0;
    }
    current += char;
    size += len;
  }
  if (current) parts.push(current);
  return parts;
}

function sendInput(id: string, data: string) {
  // TRM-FR-FFPL: an ended session receives no more input.
  if (findTerminal(id)?.state !== "running") return;
  for (const part of inputParts(data)) {
    api.writeTerminalInput(id, part).catch((err) => {
      logWarn(["frontend"], "terminal input failed", { terminalId: id, code: errorCode(err) });
    });
  }
}

/** TRM-FR-RFVP, TRM-FR-DHEM: the keys the terminal handles itself. Returns true when handled. */
export function handleTerminalKey(
  event: KeyboardEvent,
  term: Pick<Terminal, "hasSelection" | "getSelection" | "paste" | "scrollPages">,
  send: (data: string) => void,
  mac: boolean = isMacPlatform(),
): boolean {
  if (event.type !== "keydown") return false;
  const copy = mac
    ? event.metaKey && !event.ctrlKey && event.code === "KeyC"
    : event.ctrlKey && event.shiftKey && !event.metaKey && event.code === "KeyC";
  if (copy) {
    if (term.hasSelection()) void navigator.clipboard?.writeText(term.getSelection()).catch(() => {});
    return true;
  }
  const paste = mac
    ? event.metaKey && !event.ctrlKey && event.code === "KeyV"
    : event.ctrlKey && event.shiftKey && !event.metaKey && event.code === "KeyV";
  if (paste) {
    void navigator.clipboard
      ?.readText()
      .then((text) => {
        if (text) term.paste(text);
      })
      .catch(() => {});
    return true;
  }
  // TRM-FR-RFVP: on Linux, Ctrl+V without Shift goes to the shell, not to a paste.
  if (!mac && event.ctrlKey && !event.shiftKey && !event.altKey && event.code === "KeyV") {
    send("\x16");
    return true;
  }
  // TRM-FR-DTCA: Shift+PageUp and Shift+PageDown scroll the scrollback.
  if (event.shiftKey && (event.code === "PageUp" || event.code === "PageDown")) {
    term.scrollPages(event.code === "PageUp" ? -1 : 1);
    return true;
  }
  return false;
}

/**
 * TRM-FR-RFVP: whether a keydown that the terminal saw must stay away from the
 * window's own handlers. A key without the platform's command modifier goes to
 * the session alone.
 */
export function keyStaysInTerminal(event: KeyboardEvent, mac: boolean = isMacPlatform()): boolean {
  if (mac) return !event.metaKey;
  return !(event.ctrlKey && event.shiftKey);
}

let stopSessions: (() => void) | null = null;

function followAppearance() {
  if (!stopSessions) {
    // TRM-FR-ILRQ: a session that becomes running gets the size its tab fitted.
    stopSessions = subscribeTerminals(() => {
      for (const entry of views.values()) sendSize(entry);
    });
  }
  if (!stopTheme) {
    // TRM-FR-WRVB: the theme of an open terminal cannot change in place, so a
    // theme change makes each terminal again. The new attach sends the history.
    stopTheme = subscribeTheme(() => {
      for (const entry of views.values()) rebuild(entry);
    });
  }
  if (!stopFonts && typeof MutationObserver !== "undefined" && typeof document !== "undefined") {
    const observer = new MutationObserver(() => {
      const font = terminalFont();
      for (const entry of views.values()) {
        if (!entry.term) continue;
        if (entry.term.options.fontFamily !== font.fontFamily) entry.term.options.fontFamily = font.fontFamily;
        if (entry.term.options.fontSize !== font.fontSize) entry.term.options.fontSize = font.fontSize;
        entry.fit?.fit();
      }
    });
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["style"] });
    stopFonts = () => observer.disconnect();
  }
}

function teardownTerm(entry: ViewEntry) {
  for (const stop of entry.dispose.splice(0)) stop();
  entry.fit?.dispose();
  entry.term?.dispose();
  entry.fit = null;
  entry.term = null;
  entry.host.replaceChildren();
}

function rebuild(entry: ViewEntry) {
  if (entry.released) return;
  teardownTerm(entry);
  entry.lastSize = null;
  entry.sentSize = null;
  entry.starting = start(entry);
}

async function start(entry: ViewEntry): Promise<void> {
  const generation = ++entry.generation;
  let ghostty: Ghostty;
  try {
    ghostty = await ghosttyReady();
  } catch (err) {
    logWarn(["frontend"], "terminal library failed to load", { error: String(err) });
    return;
  }
  if (entry.released || generation !== entry.generation) return;
  const font = terminalFont();
  const term = new ghostty.Terminal({
    scrollback: TERMINAL_SCROLLBACK_BYTES,
    fontFamily: font.fontFamily,
    fontSize: font.fontSize,
    theme: terminalTheme(),
    cursorBlink: true,
  });
  const fit = new ghostty.FitAddon();
  term.loadAddon(fit);
  term.open(entry.host);
  entry.term = term;
  entry.fit = fit;
  const id = entry.id;
  const send = (data: string) => sendInput(id, data);
  term.attachCustomKeyEventHandler((event) => handleTerminalKey(event, term, send));
  const data = term.onData(send);
  const resized = term.onResize(({ cols, rows }) => reportSize(entry, cols, rows));
  const stopKeys = (event: KeyboardEvent) => {
    if (keyStaysInTerminal(event)) event.stopPropagation();
  };
  entry.host.addEventListener("keydown", stopKeys);
  entry.dispose.push(
    () => data.dispose(),
    () => resized.dispose(),
    () => entry.host.removeEventListener("keydown", stopKeys),
  );
  fit.fit();
  fit.observeResize();
  reportSize(entry, term.cols, term.rows);
  try {
    await api.attachTerminal(id, (frame) => {
      if (generation !== entry.generation || !entry.term) return;
      const bytes = frame.length;
      // TRS-FR-VYIB: ghostty-web parses a write before it returns and paints
      // on a later frame. The acknowledgement follows the parse, so a window
      // that paints no frames (hidden or minimized) holds no process back.
      entry.term.write(frame);
      api.acknowledgeTerminalOutput(id, bytes).catch(() => {});
    });
    logDebug(["frontend"], "terminal attached", { terminalId: id });
  } catch (err) {
    logWarn(["frontend"], "terminal attach failed", { terminalId: id, code: errorCode(err) });
  }
}

/** TRM-FR-ILRQ: one resize call for each new size. */
function reportSize(entry: ViewEntry, cols: number, rows: number) {
  if (cols <= 0 || rows <= 0) return;
  entry.lastSize = { cols, rows };
  sendSize(entry);
}

/**
 * TRM-FR-ILRQ: sends the fitted size where the backend does not have it. A
 * session that starts in a container gets its size when it becomes running.
 */
function sendSize(entry: ViewEntry) {
  const size = entry.lastSize;
  if (!size) return;
  if (entry.sentSize && entry.sentSize.cols === size.cols && entry.sentSize.rows === size.rows) return;
  const id = entry.id;
  if (findTerminal(id)?.state !== "running") return;
  entry.sentSize = size;
  const { cols, rows } = size;
  api.resizeTerminal(id, cols, rows).catch((err) => {
    logWarn(["frontend"], "terminal resize failed", { terminalId: id, code: errorCode(err) });
  });
}

/**
 * TRM-FR-AULS: shows the terminal of `id` in `container`. The first call makes
 * the terminal and attaches the session; a later call only puts the element
 * back into the page.
 */
export function showTerminalView(id: string, container: HTMLElement): void {
  let entry = views.get(id);
  if (!entry) {
    const host = document.createElement("div");
    host.className = "terminal-view__host";
    host.dataset.terminalId = id;
    entry = {
      id,
      host,
      term: null,
      fit: null,
      lastSize: null,
      sentSize: null,
      starting: null,
      released: false,
      generation: 0,
      dispose: [],
    };
    views.set(id, entry);
    followAppearance();
  }
  container.appendChild(entry.host);
  if (!entry.term && !entry.starting) entry.starting = start(entry);
  else entry.fit?.fit();
}

/** TRM-FR-ITAK: the tab is hidden. The terminal stays attached. */
export function hideTerminalView(id: string): void {
  views.get(id)?.host.remove();
}

export function focusTerminalView(id: string): void {
  views.get(id)?.term?.focus();
}

/** The `start` of a view, so a test can wait for the attach. */
export function terminalViewStarted(id: string): Promise<void> {
  return views.get(id)?.starting ?? Promise.resolve();
}

export function hasTerminalView(id: string): boolean {
  return views.has(id);
}

export function terminalViewIds(): string[] {
  return [...views.keys()];
}

/**
 * TRM-FR-JXLA: the tab closed. The session is detached, unless `detach` is
 * false because the session itself is closed.
 */
export function releaseTerminalView(id: string, detach = true): void {
  const entry = views.get(id);
  if (!entry) return;
  views.delete(id);
  entry.released = true;
  teardownTerm(entry);
  entry.host.remove();
  if (!detach) return;
  api
    .detachTerminal(id)
    .then(() => logDebug(["frontend"], "terminal detached", { terminalId: id }))
    .catch((err) => logWarn(["frontend"], "terminal detach failed", { terminalId: id, code: errorCode(err) }));
}

/** Resets the views between tests. */
export function resetTerminalViewsForTest(): void {
  for (const entry of views.values()) {
    entry.released = true;
    teardownTerm(entry);
  }
  views.clear();
  stopTheme?.();
  stopTheme = null;
  stopFonts?.();
  stopFonts = null;
  stopSessions?.();
  stopSessions = null;
  ready = null;
}
