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

export async function init(): Promise<void> {}

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
  hasSelection() {
    return this.selection.length > 0;
  }
  getSelection() {
    return this.selection;
  }
  scrollPages(_amount: number) {}
  focus() {}
  dispose() {
    this.disposed = true;
  }
}

export { FakeTerminal as Terminal };

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
