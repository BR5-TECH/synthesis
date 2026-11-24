// TRM-FR-HQZN against the real, patched `ghostty-web` (`patches/ghostty-web@0.4.0.patch`):
// a disposed terminal leaves no listener on the document, the window, or its
// host, so nothing keeps its WebAssembly instance alive.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

type GhosttyWeb = typeof import("ghostty-web");

interface Registration {
  target: EventTarget;
  type: string;
  listener: EventListenerOrEventListenerObject;
  capture: boolean;
}

let real: GhosttyWeb;
let wasm: WebAssembly.Module;

beforeAll(async () => {
  real = await vi.importActual<GhosttyWeb>("ghostty-web");
  // Loaded once: a compile can take more than a second on a slow runner.
  wasm = await WebAssembly.compile(readFileSync(resolve(process.cwd(), "node_modules/ghostty-web/dist/ghostty-vt.wasm")));
}, 30_000);

/** A 2D context that draws nothing and measures each glyph as 8 by 13 pixels. */
function fakeContext(): CanvasRenderingContext2D {
  const values: Record<string | symbol, unknown> = {};
  return new Proxy(values, {
    get(target, prop) {
      if (prop === "measureText") {
        return () => ({
          width: 8,
          actualBoundingBoxAscent: 10,
          actualBoundingBoxDescent: 3,
          fontBoundingBoxAscent: 10,
          fontBoundingBoxDescent: 3,
        });
      }
      if (prop in target) return target[prop];
      return () => undefined;
    },
    set(target, prop, value) {
      target[prop] = value;
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
}

let live: Registration[] = [];
let host: HTMLDivElement;
let watched: EventTarget[] = [];

function sameRegistration(a: Registration, b: Registration) {
  return a.target === b.target && a.type === b.type && a.listener === b.listener && a.capture === b.capture;
}

function captureOf(options?: boolean | AddEventListenerOptions | EventListenerOptions): boolean {
  return typeof options === "boolean" ? options : Boolean(options?.capture);
}

beforeEach(() => {
  live = [];
  host = document.createElement("div");
  document.body.appendChild(host);
  watched = [document, window, host];
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(
    (() => fakeContext()) as unknown as HTMLCanvasElement["getContext"],
  );
  const add = EventTarget.prototype.addEventListener;
  const remove = EventTarget.prototype.removeEventListener;
  vi.spyOn(EventTarget.prototype, "addEventListener").mockImplementation(function (
    this: EventTarget,
    type: string,
    listener: EventListenerOrEventListenerObject | null,
    options?: boolean | AddEventListenerOptions,
  ) {
    const once = typeof options === "object" && options?.once;
    if (listener && !once && watched.includes(this)) {
      const entry = { target: this, type, listener, capture: captureOf(options) };
      if (!live.some((l) => sameRegistration(l, entry))) live.push(entry);
    }
    if (listener && once && watched.includes(this)) {
      live.push({ target: this, type: `${type} (once)`, listener, capture: captureOf(options) });
    }
    return add.call(this, type, listener, options);
  });
  vi.spyOn(EventTarget.prototype, "removeEventListener").mockImplementation(function (
    this: EventTarget,
    type: string,
    listener: EventListenerOrEventListenerObject | null,
    options?: boolean | EventListenerOptions,
  ) {
    if (listener) {
      const capture = captureOf(options);
      live = live.filter(
        (l) =>
          !(
            l.target === this &&
            (l.type === type || l.type === `${type} (once)`) &&
            l.listener === listener &&
            l.capture === capture
          ),
      );
    }
    return remove.call(this, type, listener, options);
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  host.remove();
});

async function newGhostty() {
  const instance = await WebAssembly.instantiate(wasm, { env: { log: () => {} } });
  return new real.Ghostty(instance as unknown as ConstructorParameters<GhosttyWeb["Ghostty"]>[0]);
}

async function openTerminal() {
  const ghostty = await newGhostty();
  const term = new real.Terminal({ ghostty, cols: 80, rows: 24, cursorBlink: false, scrollback: 10_000 });
  term.open(host);
  return term;
}

/** A ResizeObserver for jsdom that records what it observes. */
class RecordingResizeObserver {
  static live = new Set<RecordingResizeObserver>();
  observe() {
    RecordingResizeObserver.live.add(this);
  }
  unobserve() {}
  disconnect() {
    RecordingResizeObserver.live.delete(this);
  }
}

function leftover() {
  const name = (t: EventTarget) => (t === document ? "document" : t === window ? "window" : "host");
  return live.map((l) => `${name(l.target)} ${l.type}`).sort();
}

describe("ghostty-web dispose (patched)", () => {
  it("TRM-FR-HQZN: a disposed terminal leaves no listener on the document, the window, or its host", async () => {
    const term = await openTerminal();
    try {
      // The three listeners that 0.4.0 left are there while the terminal is open.
      expect(leftover()).toEqual(expect.arrayContaining(["document mousedown", "document mouseup", "host beforeinput"]));
    } finally {
      term.dispose();
    }
    expect(leftover()).toEqual([]);
  }, 20_000);

  it("TRM-FR-HQZN: a terminal opened as the app opens it disconnects its resize observer", async () => {
    RecordingResizeObserver.live.clear();
    vi.stubGlobal("ResizeObserver", RecordingResizeObserver);
    const term = await openTerminal();
    const fit = new real.FitAddon();
    try {
      term.loadAddon(fit);
      fit.observeResize();
      expect(RecordingResizeObserver.live.size).toBe(1);
    } finally {
      fit.dispose();
      term.dispose();
      vi.unstubAllGlobals();
    }
    expect(RecordingResizeObserver.live.size).toBe(0);
    expect(leftover()).toEqual([]);
  }, 20_000);

  it("TRM-FR-HQZN: the selection manager of a disposed terminal holds no terminal, renderer, or instance", async () => {
    const term = await openTerminal();
    const manager = (term as unknown as { selectionManager: Record<string, unknown> }).selectionManager;
    expect(manager.wasmTerm).toBeTruthy();
    term.dispose();
    // A browser can keep the last right-clicked canvas, and its listeners
    // reach the manager: the manager must reach nothing large.
    expect(manager.wasmTerm).toBeNull();
    expect(manager.renderer).toBeNull();
    expect(manager.terminal).toBeNull();
  }, 20_000);

  it("TRM-FR-VBOY: the real instance gives the WebAssembly memory that the cap reads", async () => {
    const ghostty = await newGhostty();
    const memory = (ghostty as unknown as { memory?: { buffer?: { byteLength?: number } } }).memory;
    expect(memory?.buffer?.byteLength).toBeGreaterThan(0);
  }, 20_000);

  it("TRM-FR-HQZN: a terminal disposed after a selection leaves no listener", async () => {
    const term = await openTerminal();
    try {
      const canvas = host.querySelector("canvas") as HTMLCanvasElement;
      canvas.dispatchEvent(new MouseEvent("mousedown", { button: 0, bubbles: true }));
      canvas.dispatchEvent(new MouseEvent("mousemove", { bubbles: true }));
      document.dispatchEvent(new MouseEvent("mouseup", { button: 0, bubbles: true }));
    } finally {
      term.dispose();
    }
    expect(leftover()).toEqual([]);
  }, 20_000);

  it("TRM-FR-HQZN: a terminal disposed after a context menu leaves no listener", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const term = await openTerminal();
    try {
      const canvas = host.querySelector("canvas") as HTMLCanvasElement;
      canvas.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
      vi.advanceTimersByTime(10);
      expect(leftover()).toEqual(expect.arrayContaining(["document click (once)", "document contextmenu (once)"]));
    } finally {
      term.dispose();
    }
    expect(leftover()).toEqual([]);
  }, 20_000);

  it("TRM-FR-HQZN: a terminal disposed before its context menu timer fires adds no listener later", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const term = await openTerminal();
    try {
      const canvas = host.querySelector("canvas") as HTMLCanvasElement;
      canvas.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    } finally {
      term.dispose();
    }
    vi.advanceTimersByTime(10);
    expect(leftover()).toEqual([]);
  }, 20_000);
});
