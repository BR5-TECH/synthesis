import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./logging", () => ({ logDebug: vi.fn() }));

import { installContextMenuGuard } from "./contextMenuGuard";
import { TEXT_INPUT_TYPES } from "./editableTarget";
import { logDebug } from "./logging";

/** Sends a secondary click on `target` and tells whether it was prevented. */
function rightClick(target: EventTarget, x = 0, y = 0): boolean {
  const event = new MouseEvent("contextmenu", {
    bubbles: true,
    cancelable: true,
    clientX: x,
    clientY: y,
  });
  target.dispatchEvent(event);
  return event.defaultPrevented;
}

function add<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, string> = {},
  parent: Element = document.body,
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  parent.appendChild(el);
  return el;
}

/** Selects the whole text content of `node`. */
function select(node: Node) {
  const range = document.createRange();
  range.selectNodeContents(node);
  const selection = document.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
}

/** Makes the engine report the caret under the pointer at `node`/`offset`. */
function stubCaret(kind: "position" | "range", at: { node: Node; offset: number } | null) {
  const value =
    kind === "position"
      ? vi.fn(() => (at ? { offsetNode: at.node, offset: at.offset } : null))
      : vi.fn(() => {
          if (!at) return null;
          const range = document.createRange();
          range.setStart(at.node, at.offset);
          return range;
        });
  const name = kind === "position" ? "caretPositionFromPoint" : "caretRangeFromPoint";
  Object.defineProperty(document, name, { configurable: true, value });
  return value;
}

describe("context menu guard", () => {
  let remove: () => void;

  beforeEach(() => {
    vi.mocked(logDebug).mockClear();
    document.body.innerHTML = "";
    remove = installContextMenuGuard(window);
  });

  afterEach(() => {
    remove();
    document.getSelection()?.removeAllRanges();
    delete (document as unknown as Record<string, unknown>).caretPositionFromPoint;
    delete (document as unknown as Record<string, unknown>).caretRangeFromPoint;
    document.body.innerHTML = "";
  });

  it("SNV-FR-WCQH: a secondary click on chrome opens no menu and is logged once", () => {
    const button = add("button");
    button.textContent = "Run";
    expect(rightClick(button)).toBe(true);
    expect(rightClick(add("div"))).toBe(true);
    expect(logDebug).toHaveBeenCalledTimes(2);
    expect(logDebug).toHaveBeenCalledWith(["frontend"], "native context menu suppressed", {
      tag: "button",
    });
  });

  it("SNV-FR-WCQH: a secondary click on an open application menu opens no system menu", () => {
    const menu = add("div", { role: "menu", class: "menu" });
    const item = add("div", { role: "menuitem", class: "menu-item" }, menu);
    item.textContent = "Rename";
    expect(rightClick(item)).toBe(true);
  });

  it("SNV-FR-WCQH: a click that an application menu took is left as it is", () => {
    const row = add("div");
    row.addEventListener("contextmenu", (e) => e.preventDefault());
    expect(rightClick(row)).toBe(true);
    expect(logDebug).not.toHaveBeenCalled();
  });

  it("SNV-FR-WCQH: a secondary click on the window itself opens no menu", () => {
    expect(rightClick(window)).toBe(true);
    expect(logDebug).toHaveBeenCalledWith(["frontend"], "native context menu suppressed", {
      tag: "none",
    });
  });

  it.each([...TEXT_INPUT_TYPES.filter((t) => t !== ""), "TEXT"])(
    "SNV-FR-WCQH: a text input of type %s keeps the system text menu",
    (type) => {
      expect(rightClick(add("input", { type }))).toBe(false);
    },
  );

  it("SNV-FR-WCQH: an input with no type and a textarea keep the system text menu", () => {
    expect(rightClick(add("input"))).toBe(false);
    expect(rightClick(add("textarea"))).toBe(false);
    expect(logDebug).not.toHaveBeenCalled();
  });

  it("SNV-FR-WCQH: a read-only input and textarea keep the system text menu, for Copy", () => {
    expect(rightClick(add("input", { readonly: "" }))).toBe(false);
    expect(rightClick(add("textarea", { readonly: "" }))).toBe(false);
  });

  it("SNV-FR-WCQH: a disabled field and a non-text input open no menu", () => {
    expect(rightClick(add("input", { disabled: "" }))).toBe(true);
    expect(rightClick(add("textarea", { disabled: "" }))).toBe(true);
    for (const type of ["checkbox", "number", "button", "range"]) {
      expect(rightClick(add("input", { type }))).toBe(true);
    }
  });

  it("SNV-FR-WCQH: an editable rich-text region and its children keep the system text menu", () => {
    const editor = add("div", { contenteditable: "true" });
    const paragraph = add("p", {}, editor);
    expect(rightClick(editor)).toBe(false);
    expect(rightClick(paragraph)).toBe(false);
    const plain = add("div", { contenteditable: "plaintext-only" });
    expect(rightClick(plain)).toBe(false);
  });

  it("SNV-FR-WCQH: a terminal keeps the system text menu", () => {
    const view = add("div", { class: "terminal-view" });
    const host = add("div", { class: "terminal-view__host", contenteditable: "true" }, view);
    const canvas = add("canvas", {}, host);
    expect(rightClick(canvas)).toBe(false);
  });

  it("SNV-FR-WCQH: a non-editable island inside an editor opens no menu", () => {
    const editor = add("div", { contenteditable: "true" });
    const island = add("div", { contenteditable: "false" }, editor);
    const inner = add("span", {}, island);
    expect(rightClick(inner)).toBe(true);
    const upper = add("div", { contenteditable: "FALSE" });
    expect(rightClick(upper)).toBe(true);
  });

  it("SNV-FR-WCQH: a text node target counts as its parent element", () => {
    const editor = add("div", { contenteditable: "true" });
    const text = document.createTextNode("hello");
    editor.appendChild(text);
    expect(rightClick(text)).toBe(false);
    const label = add("span");
    const chromeText = document.createTextNode("Label");
    label.appendChild(chromeText);
    expect(rightClick(chromeText)).toBe(true);
  });

  describe("text selection", () => {
    it("SNV-FR-WCQH, SNV-FR-TLXS: a click on selected text keeps the system menu", () => {
      const message = add("p");
      message.textContent = "selected words";
      select(message);
      expect(rightClick(message)).toBe(false);
    });

    it("SNV-FR-WCQH: a click outside the selection opens no menu", () => {
      const message = add("p");
      message.textContent = "selected words";
      const other = add("p");
      other.textContent = "other";
      select(message);
      expect(rightClick(other)).toBe(true);
    });

    it("SNV-FR-WCQH: an empty selection does not count", () => {
      const message = add("p");
      message.textContent = "words";
      const range = document.createRange();
      range.setStart(message.firstChild!, 2);
      range.collapse(true);
      document.getSelection()!.addRange(range);
      expect(rightClick(message)).toBe(true);
    });

    it("SNV-FR-WCQH: with the caret under the pointer in the selection, the system menu opens", () => {
      const message = add("p");
      message.textContent = "selected words";
      select(message.firstChild!);
      const caret = stubCaret("position", { node: message.firstChild!, offset: 3 });
      expect(rightClick(message, 10, 20)).toBe(false);
      expect(caret).toHaveBeenCalledWith(10, 20);
    });

    it("SNV-FR-WCQH: a caret outside the selected part of the target opens no menu", () => {
      const panel = add("div");
      const selected = add("p", {}, panel);
      selected.textContent = "selected";
      const rest = add("p", {}, panel);
      rest.textContent = "not selected";
      select(selected);
      stubCaret("position", { node: rest.firstChild!, offset: 2 });
      expect(rightClick(panel)).toBe(true);
    });

    it("SNV-FR-WCQH: the end of the selection counts as in the selection", () => {
      const message = add("p");
      message.textContent = "words";
      select(message.firstChild!);
      stubCaret("position", { node: message.firstChild!, offset: 5 });
      expect(rightClick(message)).toBe(false);
    });

    it("SNV-FR-WCQH: the older caret call of WebKit is used where the newer one is absent", () => {
      const panel = add("div");
      const selected = add("p", {}, panel);
      selected.textContent = "selected";
      const rest = add("p", {}, panel);
      rest.textContent = "rest";
      select(selected);
      const legacy = stubCaret("range", { node: selected.firstChild!, offset: 1 });
      expect(rightClick(panel, 4, 8)).toBe(false);
      expect(legacy).toHaveBeenCalledWith(4, 8);
      stubCaret("range", { node: rest.firstChild!, offset: 1 });
      expect(rightClick(panel)).toBe(true);
    });

    it("SNV-FR-WCQH: no caret under the pointer falls back to the target touching the selection", () => {
      const message = add("p");
      message.textContent = "words";
      select(message);
      stubCaret("position", null);
      expect(rightClick(message)).toBe(false);
    });

    it("SNV-FR-WCQH: a caret in another document opens no menu and does not throw", () => {
      const message = add("p");
      message.textContent = "words";
      select(message);
      const other = document.implementation.createHTMLDocument("other");
      stubCaret("position", { node: other.doctype!, offset: 0 });
      expect(rightClick(message)).toBe(true);
    });
  });

  it("SNV-FR-WCQH: after removal, the guard prevents nothing", () => {
    remove();
    expect(rightClick(add("div"))).toBe(false);
    remove = () => {};
  });
});
