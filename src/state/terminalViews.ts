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
import type { FitAddon, Ghostty as GhosttyInstance, Terminal } from "ghostty-web";
import * as api from "../api";
import { logDebug, logInfo, logWarn } from "../logging";
import { subscribeTheme } from "../components/markdownMermaidRender";
import { isMacPlatform } from "../platform";
import { TERMINAL_INPUT_LIMIT } from "../api/terminals";
import { errorCode, findTerminal, subscribeTerminals } from "./terminals";
import { handleTerminalWheel, newWheelTravel } from "./terminalWheel";
import { enforceMinimumContrast } from "./terminalContrast";
import { newColorQueryReplier, type ColorQueryReplier } from "./terminalColorQueries";
import { resolveTerminalTheme, subscribeTerminalThemePreference, type ResolvedTerminalTheme } from "./terminalThemes";

/** TRM-FR-DTCA: the scrollback of each terminal, in lines. */
export const TERMINAL_SCROLLBACK_LINES = 10_000;

/**
 * TRM-FR-DTCA: ghostty-web takes its scrollback limit in bytes of its own
 * page memory, about 14 bytes for each cell (measured on 0.4.0). The budget
 * holds 10,000 lines of 160 columns.
 */
export const TERMINAL_SCROLLBACK_BYTES = TERMINAL_SCROLLBACK_LINES * 160 * 14;

/**
 * TRM-FR-VBOY, TRM-FR-PLUX: the most WebAssembly memory that one terminal
 * keeps. ghostty-web 0.4.0 grows its memory with no limit when output with
 * many styles or graphemes moves into the scrollback.
 */
export const TERMINAL_MEMORY_CAP_BYTES = 256 * 1024 * 1024;

/**
 * TRM-FR-VBOY: the output that a terminal receives before the memory cap
 * applies to it. It is two histories (TRS-FR-IZWM, 4 MiB), so the history
 * that a new terminal replays can never make it again at once.
 */
export const TERMINAL_REMAKE_MIN_OUTPUT_BYTES = 2 * 4 * 1024 * 1024;

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
  /** TRM-FR-WRVB: the theme changed while the tab was hidden; the next show makes the terminal again. */
  stale: boolean;
  /** Why the next show makes the terminal again. */
  staleReason: RebuildReason;
  /** TRM-FR-IUMC: the key of the theme that the terminal shows, or null before the first terminal. */
  themeKey: string | null;
  /** TRM-FR-LNOZ: the WARN of a missing contrast hook was logged for this terminal. */
  contrastWarned: boolean;
  /** Counts the terminals made for this tab; output of an older one is dropped. */
  generation: number;
  /** TRM-FR-VBOY: the WebAssembly memory of the terminal's own instance, or null. */
  memory: WasmMemory | null;
  /** TRM-FR-VBOY: the bytes of output that the current terminal received. */
  received: number;
  dispose: (() => void)[];
}

const views = new Map<string, ViewEntry>();
type Ghostty = typeof import("ghostty-web");
let ready: Promise<Ghostty> | null = null;
let stopTheme: (() => void) | null = null;
let stopPreference: (() => void) | null = null;
let stopFonts: (() => void) | null = null;

/**
 * Loads the library on the first terminal, so the main bundle does not carry
 * it and its WebAssembly. It does not call `init()`: that makes the one shared
 * instance which TRM-FR-CGRR forbids, and each terminal loads its own.
 */
function ghosttyReady(): Promise<Ghostty> {
  if (!ready) {
    ready = import("ghostty-web");
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

/** Returns false where the session is not running and so takes no input. */
function sendInput(id: string, data: string): boolean {
  // TRM-FR-FFPL: an ended session receives no more input.
  if (findTerminal(id)?.state !== "running") return false;
  for (const part of inputParts(data)) {
    api.writeTerminalInput(id, part).catch((err) => {
      logWarn(["frontend"], "terminal input failed", { terminalId: id, code: errorCode(err) });
    });
  }
  return true;
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
  // TRM-FR-AWXC: ghostty-web 0.4.0 drops Shift from these keys, so the
  // terminal sends their shifted form itself.
  if (event.shiftKey && !event.ctrlKey && !event.altKey && !event.metaKey) {
    const shifted = SHIFTED_KEYS[event.code];
    if (shifted !== undefined) {
      send(shifted);
      return true;
    }
  }
  return false;
}

/** TRM-FR-AWXC: what a key with Shift alone sends to the session. */
export const SHIFTED_KEYS: Readonly<Record<string, string>> = {
  Tab: "\x1b[Z",
  Enter: "\x1b\r",
  NumpadEnter: "\x1b\r",
  Home: "\x1b[1;2H",
  End: "\x1b[1;2F",
  Insert: "\x1b[2;2~",
  Delete: "\x1b[3;2~",
  F1: "\x1b[1;2P",
  F2: "\x1b[1;2Q",
  F3: "\x1b[1;2R",
  F4: "\x1b[1;2S",
  F5: "\x1b[15;2~",
  F6: "\x1b[17;2~",
  F7: "\x1b[18;2~",
  F8: "\x1b[19;2~",
  F9: "\x1b[20;2~",
  F10: "\x1b[21;2~",
  F11: "\x1b[23;2~",
  F12: "\x1b[24;2~",
};

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
    // TRM-FR-IUMC: a session whose theme override changed gets its new theme.
    stopSessions = subscribeTerminals(() => {
      for (const entry of views.values()) sendSize(entry);
      restyle(false);
    });
  }
  if (!stopTheme) {
    // TRM-FR-WRVB: an app theme change recolours every terminal that follows it.
    // A terminal that is not made yet (its load is pending or failed) is made
    // again too, as each theme change did before themes were resolved.
    stopTheme = subscribeTheme(() => restyle(true));
  }
  if (!stopPreference) {
    // TRM-FR-IUMC, GLS-FR-VTVK: so does a change of the Terminal theme of Settings.
    stopPreference = subscribeTerminalThemePreference(() => restyle(false));
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

/** TRM-FR-IUMC: the theme that the terminal of `entry` shows now. */
function themeOf(entry: ViewEntry): ResolvedTerminalTheme {
  return resolveTerminalTheme(findTerminal(entry.id)?.theme);
}

/** TRM-FR-IUMC: the tab's surface around the terminal has the terminal's background. */
function paint(entry: ViewEntry, theme: ResolvedTerminalTheme) {
  const background = theme.theme.background ?? "";
  entry.host.style.background = background;
  if (entry.host.parentElement) entry.host.parentElement.style.background = background;
}

/**
 * TRM-FR-WRVB, TRM-FR-IUMC: the theme of an open terminal cannot change in
 * place, so a change of the theme it shows makes the terminal again. The new
 * attach sends the history. A terminal made out of the page cannot measure its
 * size, and ghostty-web 0.4.0 then fails on each write. So a hidden tab keeps
 * its terminal until it shows again.
 */
function restyle(remakeUnmade: boolean) {
  for (const entry of views.values()) {
    if (entry.released) continue;
    if (entry.themeKey === null ? !remakeUnmade : themeOf(entry).key === entry.themeKey) continue;
    if (entry.host.isConnected) rebuild(entry, "theme");
    else if (!entry.stale) {
      entry.stale = true;
      entry.staleReason = "theme";
      logDebug(["frontend"], "terminal theme deferred until shown", { terminalId: entry.id });
    }
  }
}

/** TRM-FR-HQZN: disposes the terminal and keeps no reference to it or to its instance. */
function teardownTerm(entry: ViewEntry) {
  for (const stop of entry.dispose.splice(0)) stop();
  entry.fit?.dispose();
  entry.term?.dispose();
  entry.fit = null;
  entry.term = null;
  entry.memory = null;
  entry.received = 0;
  entry.host.replaceChildren();
}

type RebuildReason = "theme" | "memory";

/**
 * Makes the terminal of `entry` again. `keepFocus` keeps the focus where it is
 * when it is outside the terminal: a remake for memory comes with no action of
 * the user (TRM-FR-VBOY), and the library's `open()` takes the focus.
 */
function rebuild(entry: ViewEntry, reason: RebuildReason, keepFocus = false) {
  if (entry.released) return;
  logDebug(["frontend"], "terminal rebuilt", { terminalId: entry.id, reason });
  // The new terminal starts at the size of the old one, so it is not made
  // smaller than the terminal that was just freed.
  const size = entry.lastSize;
  const focused = keepFocus ? focusOutside(entry) : null;
  entry.stale = false;
  teardownTerm(entry);
  // The session keeps its size, so `sentSize` stays: an unchanged size is not sent again.
  entry.lastSize = null;
  entry.starting = start(entry, size, focused);
}

/** The element that has the focus, where it is in the page and outside the terminal of `entry`. */
function focusOutside(entry: ViewEntry): HTMLElement | null {
  const active = typeof document === "undefined" ? null : document.activeElement;
  if (!(active instanceof HTMLElement) || active === document.body) return null;
  return entry.host.contains(active) ? null : active;
}

async function start(
  entry: ViewEntry,
  size: { cols: number; rows: number } | null = null,
  focused: HTMLElement | null = null,
): Promise<void> {
  const generation = ++entry.generation;
  let ghostty: Ghostty;
  try {
    ghostty = await ghosttyReady();
  } catch (err) {
    logWarn(["frontend"], "terminal library failed to load", { error: String(err) });
    return;
  }
  if (entry.released || generation !== entry.generation) return;
  // TRM-FR-CGRR: each terminal gets its own WebAssembly instance. ghostty-web
  // frees the screen memory of a disposed terminal but does not clear it, so a
  // terminal that shares an instance can show the cells of an earlier terminal.
  let instance: GhosttyInstance;
  try {
    instance = await ghostty.Ghostty.load();
  } catch (err) {
    logWarn(["frontend"], "terminal instance failed to load", { terminalId: entry.id, error: String(err) });
    return;
  }
  if (entry.released || generation !== entry.generation) return;
  // TRM-FR-WRVB: a terminal made out of the page cannot measure its size. A
  // tab that was hidden while the instance loaded makes it when it shows.
  if (!entry.host.isConnected) {
    entry.stale = true;
    logDebug(["frontend"], "terminal start deferred until shown", { terminalId: entry.id });
    return;
  }
  entry.memory = wasmMemory(instance);
  entry.received = 0;
  if (!entry.memory) logDebug(["frontend"], "terminal memory not readable", { terminalId: entry.id });
  const font = terminalFont();
  const theme = themeOf(entry);
  entry.themeKey = theme.key;
  paint(entry, theme);
  const term = new ghostty.Terminal({
    ...(size ?? {}),
    ghostty: instance,
    scrollback: TERMINAL_SCROLLBACK_BYTES,
    fontFamily: font.fontFamily,
    fontSize: font.fontSize,
    theme: theme.theme,
    cursorBlink: true,
  });
  const fit = new ghostty.FitAddon();
  term.loadAddon(fit);
  term.open(entry.host);
  if (focused?.isConnected && document.activeElement !== focused) focused.focus();
  entry.term = term;
  entry.fit = fit;
  const id = entry.id;
  const send = (data: string) => sendInput(id, data);
  // TRM-FR-DLJP: text that a program colours for another background stays
  // readable. TRM-FR-LNOZ: one WARN for each terminal, not for each remake.
  if (!enforceMinimumContrast(term, id, !entry.contrastWarned)) entry.contrastWarned = true;
  // TRM-FR-XBAP: a program that asks for the colours gets the ones of this theme.
  const colorQueries = colorQueriesFor(id, theme.theme.foreground, theme.theme.background);
  term.attachCustomKeyEventHandler((event) => handleTerminalKey(event, term, send));
  // TRM-FR-QMXG, TRM-FR-PACR: the wheel goes to a program that reports the mouse.
  const travel = newWheelTravel();
  const host = entry.host;
  term.attachCustomWheelEventHandler((event) => {
    const cellsBox = (host.querySelector("canvas") ?? host).getBoundingClientRect();
    return handleTerminalWheel(event, term, cellsBox, travel, send);
  });
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
    // The frame handler is made outside this scope, so it holds no reference
    // to the terminal: Tauri keeps the callback of a channel for the life of
    // the page (TRM-FR-HQZN).
    const attachment = await api.attachTerminal(id, frameHandler(entry, generation, colorQueries));
    logDebug(["frontend"], "terminal attached", { terminalId: id });
    if (generation === entry.generation && !entry.released) colorQueries.historyKnown(attachment?.historyBytes);
  } catch (err) {
    logWarn(["frontend"], "terminal attach failed", { terminalId: id, code: errorCode(err) });
  }
}

/**
 * TRM-FR-XBAP, TRM-FR-HQZN: the colour queries of one attach. They are made
 * outside `start()`, because the channel callback keeps them for the life of
 * the page and a closure made in `start()` shares the scope that holds the
 * terminal.
 */
function colorQueriesFor(
  id: string,
  foreground: string | undefined,
  background: string | undefined,
): ColorQueryReplier {
  return newColorQueryReplier({ terminalId: id, foreground, background, send: (data) => sendInput(id, data) });
}

/** The part of a `WebAssembly.Memory` that the memory cap reads. */
interface WasmMemory {
  readonly buffer: { readonly byteLength: number };
}

/** The WebAssembly memory of a `ghostty-web` instance, where it has one. */
function wasmMemory(instance: GhosttyInstance): WasmMemory | null {
  const memory = (instance as unknown as { memory?: Partial<WasmMemory> }).memory;
  return typeof memory?.buffer?.byteLength === "number" ? (memory as WasmMemory) : null;
}

/**
 * TRM-FR-AULS: writes each frame of one attach into the terminal, and
 * acknowledges it. Output of an older terminal of the tab is dropped. A tab
 * whose terminal is gone for its memory (TRM-FR-PLUX) acknowledges the frame
 * and writes nothing. Each frame goes to the colour queries of the attach
 * (TRM-FR-XBAP), also where the terminal is gone, because the program waits.
 */
function frameHandler(
  entry: ViewEntry,
  generation: number,
  colorQueries: ColorQueryReplier,
): (frame: Uint8Array) => void {
  const id = entry.id;
  let writeFailed = false;
  return (frame) => {
    if (entry.released || generation !== entry.generation) return;
    const bytes = frame.length;
    const term = entry.term;
    // TRS-FR-VYIB: ghostty-web parses a write before it returns and paints
    // on a later frame. The acknowledgement follows the parse, so a window
    // that paints no frames (hidden or minimized) holds no process back.
    if (term) {
      try {
        term.write(frame);
      } catch (err) {
        // TRM-FR-AULS: a failed write is acknowledged too, so a fault in the
        // view never holds the process back. One record for each terminal.
        if (!writeFailed) {
          writeFailed = true;
          logWarn(["frontend"], "terminal write failed", { terminalId: id, error: String(err) });
        }
      }
    }
    api.acknowledgeTerminalOutput(id, bytes).catch(() => {});
    colorQueries.scan(frame);
    if (term) {
      entry.received += bytes;
      capMemory(entry);
    }
  };
}

/**
 * TRM-FR-VBOY, TRM-FR-PLUX: a terminal past the memory cap is made again from
 * the history. A shown terminal is made again at once. A hidden one is
 * disposed now and made again when its tab shows, because a terminal made out
 * of the page cannot measure its size (TRM-FR-WRVB).
 */
function capMemory(entry: ViewEntry) {
  const size = entry.memory?.buffer.byteLength ?? 0;
  if (size <= TERMINAL_MEMORY_CAP_BYTES || entry.received < TERMINAL_REMAKE_MIN_OUTPUT_BYTES) return;
  const shown = entry.host.isConnected;
  logInfo(["frontend"], "terminal remade for memory", { terminalId: entry.id, memoryBytes: size, shown });
  if (shown) {
    rebuild(entry, "memory", true);
    return;
  }
  teardownTerm(entry);
  entry.stale = true;
  entry.staleReason = "memory";
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
      stale: false,
      staleReason: "theme",
      themeKey: null,
      contrastWarned: false,
      generation: 0,
      memory: null,
      received: 0,
      dispose: [],
    };
    views.set(id, entry);
    followAppearance();
  }
  container.appendChild(entry.host);
  if (entry.themeKey !== null) paint(entry, themeOf(entry));
  if (entry.stale) rebuild(entry, entry.staleReason);
  else if (!entry.term && !entry.starting) entry.starting = start(entry);
  else entry.fit?.fit();
}

/** TRM-FR-ITAK: the tab is hidden. The terminal stays attached. */
export function hideTerminalView(id: string): void {
  views.get(id)?.host.remove();
}

/**
 * Puts the focus on the terminal of a tab (TRM-FR-FYYB). A terminal that is
 * still starting takes the focus when it is ready, if its tab still shows it.
 */
export function focusTerminalView(id: string): void {
  const entry = views.get(id);
  if (!entry) return;
  if (entry.term) {
    entry.term.focus();
    return;
  }
  void entry.starting?.then(() => {
    if (views.get(id) === entry && entry.host.isConnected) entry.term?.focus();
  });
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
  stopPreference?.();
  stopPreference = null;
  stopFonts?.();
  stopFonts = null;
  stopSessions?.();
  stopSessions = null;
  ready = null;
}
