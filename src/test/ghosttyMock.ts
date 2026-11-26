/**
 * A stand-in for `ghostty-web` in Vitest. The real library loads WebAssembly,
 * which jsdom does not run. Each fake terminal records what the code under test
 * did to it, so a test can read it from `fakeTerminals`.
 */
type Listener<T> = (value: T) => void;

function emitter<T>() {
  const listeners = new Set<Listener<T>>();
  const event = (listener: Listener<T>) => {
    listeners.add(listener);
    return { dispose: () => listeners.delete(listener) };
  };
  return { event, fire: (value: T) => listeners.forEach((l) => l(value)) };
}

export const fakeTerminals: FakeTerminal[] = [];

/** Test hook: while set, `write()` of every fake terminal throws it. */
export const fakeWriteFailure: { error: Error | null } = { error: null };

/** How many times `init()` ran. It makes the one shared instance that TRM-FR-CGRR forbids. */
export const fakeInitCalls = { count: 0 };

export async function init(): Promise<void> {
  fakeInitCalls.count += 1;
}

/** Test hook: while true, `open()` makes no renderer, as a library without the contrast hook. */
export const fakeNoRenderer = { on: false };

/** Test hook: while true, `open()` makes no link detector, as a library without the link hook. */
export const fakeNoLinkHook = { on: false };

/** A link as the library's link detector gives it (TRM-FR-HWKO). */
export interface FakeLink {
  text: string;
  range: { start: { x: number; y: number }; end: { x: number; y: number } };
  activate(event: MouseEvent): void;
}

/**
 * A stand-in for the link detector that `open()` makes. `links` maps
 * `"col,row"` to the link at that cell, and `getLinkAt` gives the same object
 * each time, as the library's cache does.
 */
export class FakeLinkDetector {
  links = new Map<string, FakeLink>();
  async getLinkAt(col: number, row: number): Promise<FakeLink | undefined> {
    return this.links.get(`${col},${row}`);
  }
}

/** Each `Ghostty.load()` in order, so a test can check that no two terminals share one. */
export const fakeInstances: FakeGhostty[] = [];

/** Test hook: while set, `Ghostty.load()` rejects with it. */
export const fakeLoadFailure: { error: Error | null } = { error: null };

/**
 * Test hook: while set, each `Ghostty.load()` waits for this promise, so a test
 * can hold a load open and end it by hand.
 */
export const fakeLoadGate: { promise: Promise<void> | null; waiting: number } = { promise: null, waiting: 0 };

/** A stand-in for the WebAssembly instance that each terminal owns (TRM-FR-CGRR). */
export class FakeGhostty {
  /** Test hook: the size of the instance's WebAssembly memory (TRM-FR-VBOY). */
  memory = { buffer: { byteLength: 2 * 1024 * 1024 } };

  static async load(): Promise<FakeGhostty> {
    const gate = fakeLoadGate.promise;
    if (gate) {
      fakeLoadGate.waiting += 1;
      await gate;
      fakeLoadGate.waiting -= 1;
    }
    if (fakeLoadFailure.error) throw fakeLoadFailure.error;
    const instance = new FakeGhostty();
    fakeInstances.push(instance);
    return instance;
  }
}

export { FakeGhostty as Ghostty };

export class FakeTerminal {
  cols = 80;
  rows = 24;
  options: Record<string, unknown>;
  written: Uint8Array[] = [];
  pasted: string[] = [];
  opened: HTMLElement | null = null;
  /** Whether the parent was in the page when `open()` ran. */
  openedConnected = false;
  disposed = false;
  selection = "";
  keyHandler: ((event: KeyboardEvent) => boolean) | null = null;
  wheelHandler: ((event: WheelEvent) => boolean) | null = null;
  /** Test hook: whether the program turned mouse reporting on. */
  mouseTracking = false;
  /** Test hook: the DEC private modes that are on, for `getMode`. */
  modes = new Set<number>();
  /** The pages `scrollPages` moved, in order. */
  scrolled: number[] = [];
  /**
   * A stand-in for the canvas renderer that `open()` makes. `drawn` holds the
   * cells that its `renderCellText` got, so a test can read the colours drawn.
   */
  renderer: FakeRenderer | undefined = undefined;
  /** The link detector that `open()` makes (TRM-FR-HWKO). */
  linkDetector: FakeLinkDetector | undefined = undefined;
  private data = emitter<string>();
  private resizes = emitter<{ cols: number; rows: number }>();
  readonly onData = this.data.event;
  readonly onResize = this.resizes.event;

  constructor(options: Record<string, unknown> = {}) {
    this.options = { ...options };
    if (typeof options.cols === "number") this.cols = options.cols;
    if (typeof options.rows === "number") this.rows = options.rows;
    fakeTerminals.push(this);
  }
  open(parent: HTMLElement) {
    if (!fakeNoRenderer.on) this.renderer = new FakeRenderer(String((this.options.theme as ITheme | undefined)?.background ?? "#1e1e1e"));
    if (!fakeNoLinkHook.on) this.linkDetector = new FakeLinkDetector();
    this.opened = parent;
    this.openedConnected = parent.isConnected;
    const canvas = document.createElement("canvas");
    canvas.dataset.fakeTerminal = "true";
    parent.appendChild(canvas);
  }
  write(data: string | Uint8Array, callback?: () => void) {
    if (fakeWriteFailure.error) throw fakeWriteFailure.error;
    this.written.push(typeof data === "string" ? new TextEncoder().encode(data) : data);
    callback?.();
  }
  paste(text: string) {
    this.pasted.push(text);
    this.data.fire(text);
  }
  resize(cols: number, rows: number) {
    if (cols === this.cols && rows === this.rows) return;
    this.cols = cols;
    this.rows = rows;
    this.resizes.fire({ cols, rows });
  }
  /** Test hook: the user typed. */
  type(data: string) {
    this.data.fire(data);
  }
  writtenText(): string {
    return this.written.map((w) => new TextDecoder().decode(w)).join("");
  }
  loadAddon(addon: { activate(term: FakeTerminal): void }) {
    addon.activate(this);
  }
  attachCustomKeyEventHandler(handler: (event: KeyboardEvent) => boolean) {
    this.keyHandler = handler;
  }
  attachCustomWheelEventHandler(handler?: (event: WheelEvent) => boolean) {
    this.wheelHandler = handler ?? null;
  }
  hasMouseTracking() {
    return this.mouseTracking;
  }
  getMode(mode: number, _isAnsi = false) {
    return this.modes.has(mode);
  }
  hasSelection() {
    return this.selection.length > 0;
  }
  getSelection() {
    return this.selection;
  }
  scrollPages(amount: number) {
    this.scrolled.push(amount);
  }
  focus() {}
  dispose() {
    this.disposed = true;
  }
}

export { FakeTerminal as Terminal };

/** Test hook: while true, `open()` makes a renderer with no `render` and no `resize`. */
export const fakeNoRenderHook = { on: false };

/**
 * A stand-in for the 2D context of the canvas. Setting the size of its
 * `FakeCanvas` resets its transform; `scale()` multiplies it.
 */
export class FakeContext {
  a = 1;
  d = 1;
  scale(x: number, y: number) {
    this.a *= x;
    this.d *= y;
  }
  getTransform() {
    return { a: this.a, d: this.d };
  }
  /** Test hook: the WebView lost the drawing surface and made it again. */
  resetTransform() {
    this.a = 1;
    this.d = 1;
  }
}

/** A stand-in for the canvas. As in a real canvas, setting a size resets the context. */
export class FakeCanvas {
  private w = 0;
  private h = 0;
  constructor(private readonly ctx: FakeContext) {}
  get width() {
    return this.w;
  }
  set width(value: number) {
    this.w = value;
    this.ctx.resetTransform();
  }
  get height() {
    return this.h;
  }
  set height(value: number) {
    this.h = value;
    this.ctx.resetTransform();
  }
  toJSON() {
    return { width: this.w, height: this.h };
  }
}

/** The parts of the ghostty-web 0.4.0 `CanvasRenderer` that the app reaches into. */
export class FakeRenderer {
  theme: { background: string };
  drawn: Record<string, number>[] = [];
  devicePixelRatio = window.devicePixelRatio || 1;
  metrics = { width: 8, height: 16 };
  ctx = new FakeContext();
  canvas = new FakeCanvas(this.ctx);
  /** Each `resize()`, in order. */
  resized: { cols: number; rows: number }[] = [];
  /** The `forceAll` of each `render()`, in order. */
  rendered: boolean[] = [];
  declare render: ((buffer: FakeBuffer, forceAll?: boolean) => void) | undefined;
  declare resize: ((cols: number, rows: number) => void) | undefined;
  constructor(background: string) {
    this.theme = { background };
    if (!fakeNoRenderHook.on) {
      this.render = FakeRenderer.prototype.renderFrame;
      this.resize = FakeRenderer.prototype.resizeCanvas;
    }
  }
  /** As the library: the canvas size alone decides a resize; the transform is not checked. */
  renderFrame(buffer: FakeBuffer, forceAll = false) {
    const { cols, rows } = buffer.getDimensions();
    const ratio = this.devicePixelRatio;
    if (this.canvas.width !== cols * this.metrics.width * ratio || this.canvas.height !== rows * this.metrics.height * ratio) {
      this.resizeCanvas(cols, rows);
      forceAll = true;
    }
    this.rendered.push(forceAll);
  }
  resizeCanvas(cols: number, rows: number) {
    this.resized.push({ cols, rows });
    this.canvas.width = cols * this.metrics.width * this.devicePixelRatio;
    this.canvas.height = rows * this.metrics.height * this.devicePixelRatio;
    this.ctx.scale(this.devicePixelRatio, this.devicePixelRatio);
  }
  /** Test hook: one frame of the render loop of `term`. */
  frame(term: { cols: number; rows: number }) {
    this.render?.({ getDimensions: () => ({ cols: term.cols, rows: term.rows }) });
  }
  renderCellText(cell: Record<string, number>, _col: number, _row: number) {
    this.drawn.push({ ...cell });
  }
}

export class FitAddon {
  term: FakeTerminal | null = null;
  activate(term: FakeTerminal) {
    this.term = term;
  }
  fit() {}
  observeResize() {}
  dispose() {}
}

export type ITheme = Record<string, string>;

/** The part of the frame buffer that the renderer reads. */
export interface FakeBuffer {
  getDimensions(): { cols: number; rows: number };
}
