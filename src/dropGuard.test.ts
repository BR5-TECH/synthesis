import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./logging", () => ({ logDebug: vi.fn() }));

import { installDropGuard } from "./dropGuard";
import { DRAG_TYPE, DRAG_ZONE_ATTRIBUTE } from "./components/draftsTree";
import { logDebug } from "./logging";

interface FakeTransfer {
  types: string[];
  dropEffect: string;
}

/** jsdom has no `DragEvent`, so the event carries a plain transfer object. */
function dragEvent(type: "dragover" | "drop", types: string[]) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  const dataTransfer: FakeTransfer = { types, dropEffect: "move" };
  Object.defineProperty(event, "dataTransfer", { value: dataTransfer });
  return { event, dataTransfer };
}

describe("drop guard", () => {
  let remove: () => void;
  let plain: HTMLDivElement;

  beforeEach(() => {
    vi.mocked(logDebug).mockClear();
    document.body.innerHTML = "";
    plain = document.createElement("div");
    document.body.appendChild(plain);
    remove = installDropGuard(window);
  });

  afterEach(() => {
    remove();
    document.body.innerHTML = "";
  });

  it("SNV-FR-UKER: a file dragged over a place that takes no drop is refused", () => {
    const { event, dataTransfer } = dragEvent("dragover", ["Files"]);
    plain.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(dataTransfer.dropEffect).toBe("none");
  });

  it("SNV-FR-UKER: a link released over a place that takes no drop does not load", () => {
    const { event } = dragEvent("drop", ["text/uri-list", "text/plain"]);
    plain.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(logDebug).toHaveBeenCalledTimes(1);
  });

  it("SNV-FR-UKER: a drop target that took the drag keeps its own drop effect", () => {
    plain.addEventListener("dragover", (e) => {
      e.preventDefault();
      (e as unknown as { dataTransfer: FakeTransfer }).dataTransfer.dropEffect = "copy";
    });
    const { event, dataTransfer } = dragEvent("dragover", ["Files"]);
    plain.dispatchEvent(event);
    expect(dataTransfer.dropEffect).toBe("copy");
  });

  it("SNV-FR-UKER, EDT-FR-86: a file over a text field reaches the field's own drop", () => {
    const field = document.createElement("textarea");
    document.body.appendChild(field);
    const over = dragEvent("dragover", ["Files"]);
    field.dispatchEvent(over.event);
    // Accepted, so the engine loads nothing and the field sees the drop.
    expect(over.event.defaultPrevented).toBe(true);
    expect(over.dataTransfer.dropEffect).toBe("copy");

    // The field takes an image: the guard leaves that drop alone.
    const take = (e: Event) => e.preventDefault();
    field.addEventListener("drop", take);
    const taken = dragEvent("drop", ["Files"]);
    field.dispatchEvent(taken.event);
    expect(logDebug).not.toHaveBeenCalled();
    field.removeEventListener("drop", take);

    // A file the field does not take is dropped on nothing.
    const left = dragEvent("drop", ["Files"]);
    field.dispatchEvent(left.event);
    expect(left.event.defaultPrevented).toBe(true);
    expect(logDebug).toHaveBeenCalledTimes(1);
  });

  it("SNV-FR-UKER: a link over a text field is inserted on the field's own terms", () => {
    const linkTypes = ["text/uri-list", "text/plain"];
    const rich = document.createElement("div");
    rich.setAttribute("contenteditable", "true");
    const inner = document.createElement("p");
    rich.appendChild(inner);
    const input = document.createElement("input");
    document.body.append(rich, input);
    for (const target of [inner, input]) {
      const over = dragEvent("dragover", linkTypes);
      target.dispatchEvent(over.event);
      expect(over.event.defaultPrevented).toBe(false);
      const drop = dragEvent("drop", linkTypes);
      target.dispatchEvent(drop.event);
      expect(drop.event.defaultPrevented).toBe(false);
    }
  });

  it("SNV-FR-UKER: a link over a place that takes no text is refused", () => {
    const rich = document.createElement("div");
    rich.setAttribute("contenteditable", "true");
    const island = document.createElement("span");
    island.setAttribute("contenteditable", "false");
    rich.appendChild(island);
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    const readOnly = document.createElement("textarea");
    readOnly.readOnly = true;
    document.body.append(rich, checkbox, readOnly);
    for (const target of [island, checkbox, readOnly]) {
      const { event, dataTransfer } = dragEvent("dragover", ["text/uri-list"]);
      target.dispatchEvent(event);
      expect(event.defaultPrevented, target.tagName).toBe(true);
      expect(dataTransfer.dropEffect, target.tagName).toBe("none");
    }
  });

  it("SNV-FR-UKER: a target that accepts the drag but does not take the drop does not load it", () => {
    // As the comment composer does: it accepts every drag and takes files alone.
    plain.addEventListener("dragover", (e) => e.preventDefault());
    const over = dragEvent("dragover", ["text/uri-list"]);
    plain.dispatchEvent(over.event);
    expect(over.dataTransfer.dropEffect).toBe("move");
    const drop = dragEvent("drop", ["text/uri-list"]);
    plain.dispatchEvent(drop.event);
    expect(drop.event.defaultPrevented).toBe(true);
  });

  it("SNV-FR-UKER: an event with no transfer is left alone", () => {
    const event = new Event("dragover", { bubbles: true, cancelable: true });
    plain.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
  });

  it("DRP-FR-PXMK, DRP-FR-27: a row drag inside the Drafts panel's region is never touched", () => {
    const zone = document.createElement("div");
    zone.setAttribute(DRAG_ZONE_ATTRIBUTE, "");
    const row = document.createElement("div");
    zone.appendChild(row);
    document.body.appendChild(zone);
    const { event, dataTransfer } = dragEvent("dragover", [DRAG_TYPE]);
    row.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
    expect(dataTransfer.dropEffect).toBe("move");
  });

  it("DRP-FR-PXMK: a row drag outside the Drafts panel reaches no other surface", () => {
    // As the rich editor and the comment composer do: they accept every drag.
    const editor = document.createElement("div");
    editor.setAttribute("contenteditable", "true");
    const seen = vi.fn((e: Event) => e.preventDefault());
    editor.addEventListener("dragover", seen);
    editor.addEventListener("drop", seen);
    document.body.appendChild(editor);

    const over = dragEvent("dragover", [DRAG_TYPE]);
    editor.dispatchEvent(over.event);
    expect(over.event.defaultPrevented).toBe(true);
    expect(over.dataTransfer.dropEffect).toBe("none");
    const drop = dragEvent("drop", [DRAG_TYPE]);
    editor.dispatchEvent(drop.event);
    expect(drop.event.defaultPrevented).toBe(true);
    expect(seen).not.toHaveBeenCalled();
  });

  it("SNV-FR-UKER: a refused drag is logged once, not on every dragover", () => {
    for (let i = 0; i < 5; i++) plain.dispatchEvent(dragEvent("dragover", ["Files"]).event);
    expect(logDebug).toHaveBeenCalledTimes(1);
    // The next drag is logged again.
    plain.dispatchEvent(new Event("dragend", { bubbles: true }));
    plain.dispatchEvent(dragEvent("dragover", ["Files"]).event);
    expect(logDebug).toHaveBeenCalledTimes(2);
  });

  it("SNV-FR-UKER: a link with no plain-text form is not dropped into a text field", () => {
    const field = document.createElement("textarea");
    document.body.appendChild(field);
    const { event } = dragEvent("drop", ["text/uri-list"]);
    field.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });

  it("SNV-FR-UKER: the guard is gone once it is removed", () => {
    remove();
    const { event } = dragEvent("dragover", ["Files"]);
    plain.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
    remove = () => {};
  });
});

// SNV-FR-RQLF: with the native handler off, nothing may still wait for its
// events, or a drop would reach a listener that never fires.
describe("native drag-drop events", () => {
  const sources = (dir: string, ext: RegExp): string[] =>
    readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) return sources(path, ext);
      return ext.test(entry.name) && !/\.test\.tsx?$/.test(entry.name) ? [path] : [];
    });

  it("SNV-FR-RQLF: the shell listens for no native drag-drop event", () => {
    const files = [
      ...sources("src", /\.tsx?$/),
      ...sources(join("src-tauri", "src"), /\.rs$/),
    ];
    const native = /onDragDropEvent|tauri:\/\/drag|DragDropEvent|WindowEvent::DragDrop/;
    const offenders = files.filter((f) => native.test(readFileSync(f, "utf8")));
    expect(offenders).toEqual([]);
  });
});
