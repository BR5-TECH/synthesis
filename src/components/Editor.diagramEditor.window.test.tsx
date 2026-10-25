import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Editor as TiptapEditor } from "@tiptap/react";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { Editor } from "./Editor";
import { DiagramEditorHost, DIAGRAM_EDITOR_LABEL } from "./DiagramEditor";
import {
  closeDiagramEditor,
  isDiagramEditorOpen,
  openDiagramEditor,
  resetDiagramEditorForTest,
  setDiagramEditorOpening,
} from "./DiagramEditor/store";
import { EditSessionStore } from "../state/editSessions";
import { resetLogBufferForTest } from "../logging";
import { resetMermaidMock } from "../test/mermaidMock";
import { EDIT_DIAGRAM_LABEL } from "./markdownMermaid";
import { markdownExtensions } from "./markdownFidelity";
import { resetMermaidForTest } from "./markdownMermaidRender";

const BODY = "# Title\n\n```mermaid\ngraph TD\nA-->B\n```\n";

function wireBackend() {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_artifact_contents_by_id") return { body: BODY, checksum: "ck1" };
    if (cmd === "save_artifact_contents") return { checksum: "ck2" };
    if (cmd === "append_log_records") return undefined;
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
}

let sessions: EditSessionStore;

const dialog = () => screen.getByRole("dialog", { name: DIAGRAM_EDITOR_LABEL });

async function mount() {
  wireBackend();
  render(
    <>
      <Editor artifactId="a.md" artifactName="a.md" sessions={sessions} />
      <DiagramEditorHost />
    </>,
  );
  await screen.findByRole("button", { name: EDIT_DIAGRAM_LABEL });
}

/**
 * Open the overlay. The fence draws again when its render settles, so the
 * button is taken from the drawing that has settled.
 */
async function open() {
  await waitFor(() =>
    expect(document.querySelector(".ProseMirror pre")?.getAttribute("data-diagram-view")).toBe(
      "diagram",
    ),
  );
  fireEvent.click(screen.getByRole("button", { name: EDIT_DIAGRAM_LABEL }));
  expect(dialog()).toBeInTheDocument();
}
const close = () => fireEvent.click(within(dialog()).getByRole("button", { name: "Close" }));

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  resetDiagramEditorForTest();
  sessions = new EditSessionStore();
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 1000 });
  Object.defineProperty(window, "innerHeight", { configurable: true, value: 800 });
});

const initialWidth = window.innerWidth;
const initialHeight = window.innerHeight;

afterEach(() => {
  cleanup();
  Object.defineProperty(window, "innerWidth", { configurable: true, value: initialWidth });
  Object.defineProperty(window, "innerHeight", { configurable: true, value: initialHeight });
});

describe("the size and the place of the diagram editor overlay", () => {
  it("EDT-FR-NKGX: it opens centred at 90% × 95%, moves and resizes, and opens there again", async () => {
    await mount();
    await open();
    expect(dialog().style.cssText).toContain("left: 50px; top: 20px; width: 900px; height: 760px");

    const head = dialog().querySelector(".diagram-editor__head") as HTMLElement;
    fireEvent.pointerDown(head, { button: 0, clientX: 200, clientY: 200, pointerId: 1 });
    fireEvent.pointerMove(head, { clientX: 170, clientY: 210, pointerId: 1 });
    fireEvent.pointerUp(head, { pointerId: 1 });
    expect(dialog().style.left).toBe("20px");
    expect(dialog().style.top).toBe("30px");

    const edge = dialog().querySelector("[data-edge='e']") as HTMLElement;
    fireEvent.pointerDown(edge, { button: 0, clientX: 0, clientY: 0, pointerId: 2 });
    fireEvent.pointerMove(edge, { clientX: -100, clientY: 0, pointerId: 2 });
    fireEvent.pointerUp(edge, { pointerId: 2 });
    expect(dialog().style.width).toBe("800px");

    close();
    await open();
    expect(dialog().style.cssText).toContain("left: 50px; top: 20px; width: 900px; height: 760px");
  });

  it("EDT-FR-ULPU, EDT-FR-HVGG: the arrow keys move the divider, and each opening starts at half", async () => {
    await mount();
    await open();
    const divider = within(dialog()).getByRole("separator");
    expect(divider).toHaveAttribute("aria-orientation", "vertical");
    fireEvent.keyDown(divider, { key: "ArrowRight" });
    expect(divider).toHaveAttribute("aria-valuenow", "55");
    fireEvent.keyDown(divider, { key: "ArrowLeft" });
    fireEvent.keyDown(divider, { key: "ArrowLeft" });
    expect(divider).toHaveAttribute("aria-valuenow", "45");
    const panes = divider.parentElement as HTMLElement;
    expect(panes.style.gridTemplateColumns).toContain("0.45fr");

    close();
    await open();
    expect(within(dialog()).getByRole("separator")).toHaveAttribute("aria-valuenow", "50");
  });
});

describe("the window rules of the diagram editor overlay", () => {
  it("EDT-FR-TOVY, SNV-FR-56: the window closes its other overlays before the overlay opens", async () => {
    await mount();
    const opening = vi.fn(() => expect(isDiagramEditorOpen()).toBe(false));
    setDiagramEditorOpening(opening);
    await open();
    expect(opening).toHaveBeenCalledTimes(1);
    expect(dialog()).toBeInTheDocument();
  });

  it("EDT-FR-TOVY, SNV-FR-56: another overlay that opens closes it, and the edit stays", async () => {
    await mount();
    await open();
    fireEvent.change(screen.getByRole("textbox", { name: "Diagram source" }), {
      target: { value: "graph TD\nA-->C" },
    });
    act(() => closeDiagramEditor("other-overlay"));
    expect(screen.queryByRole("dialog")).toBeNull();
    const pm = document.querySelector(".ProseMirror") as HTMLElement;
    expect(pm.textContent).toContain("A-->C");
  });

  it("EDT-FR-HGAZ, EDT-FR-SJGJ: a press in the preview pane keeps its default, so the error text can be selected", async () => {
    await mount();
    await open();
    fireEvent.change(screen.getByRole("textbox", { name: "Diagram source" }), {
      target: { value: "INVALID" },
    });
    const error = await within(dialog()).findByRole("alert");
    const summary = error.querySelector(".mermaid-error__summary") as HTMLElement;
    expect(fireEvent.mouseDown(summary)).toBe(true);
    expect(fireEvent.mouseDown(error)).toBe(true);
    // The title bar and the scrim still keep the focus where it is.
    const head = dialog().querySelector(".diagram-editor__head") as HTMLElement;
    expect(fireEvent.mouseDown(head)).toBe(false);
    expect(fireEvent.mouseDown(screen.getByTestId("diagram-editor-scrim"))).toBe(false);
    // The dialog takes the focus of such a press, so the focus stays inside.
    // jsdom moves no focus on a press, so the test can only check that the
    // dialog is able to take the focus.
    expect(dialog()).toHaveAttribute("tabindex", "-1");
  });

  it("EDT-FR-SJGJ, EDT-FR-SRXG: a window listener added after the overlay's sees no key that the overlay answers", async () => {
    await mount();
    await open();
    const later = vi.fn();
    window.addEventListener("keydown", later, true);
    try {
      // A key outside the overlay is swallowed, and focus goes back inside.
      fireEvent.keyDown(document.body, { key: "a" });
      expect(screen.getByRole("textbox", { name: "Diagram source" })).toHaveFocus();
      fireEvent.keyDown(screen.getByRole("textbox", { name: "Diagram source" }), {
        key: "Escape",
      });
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(later).not.toHaveBeenCalled();
    } finally {
      window.removeEventListener("keydown", later, true);
    }
  });

  it("EDT-FR-HGAZ: Tab and Shift+Tab from the dialog itself stay inside the overlay", async () => {
    await mount();
    await open();
    const source = screen.getByRole("textbox", { name: "Diagram source" });
    act(() => dialog().focus());
    fireEvent.keyDown(dialog(), { key: "Tab" });
    expect(within(dialog()).getByRole("button", { name: "Close" })).toHaveFocus();
    act(() => dialog().focus());
    fireEvent.keyDown(dialog(), { key: "Tab", shiftKey: true });
    expect(within(dialog()).getByRole("separator")).toHaveFocus();
    expect(source).not.toHaveFocus();
  });

  it("EDT-FR-SRXG: the overlay keeps no state after it closes", async () => {
    await mount();
    await open();
    fireEvent.keyDown(within(dialog()).getByRole("separator"), { key: "ArrowRight" });
    close();
    expect(isDiagramEditorOpen()).toBe(false);
    await open();
    expect(within(dialog()).getByRole("separator")).toHaveAttribute("aria-valuenow", "50");
  });
});

describe("the surfaces that offer the Edit button", () => {
  function mountSurface(diagramEditor: boolean | undefined) {
    const element = document.createElement("div");
    document.body.appendChild(element);
    const editor = new TiptapEditor({
      element,
      extensions:
        diagramEditor === undefined
          ? markdownExtensions()
          : markdownExtensions([], { diagramEditor }),
      content: "```mermaid\nINVALID\n```\n",
    });
    return { element, editor };
  }

  it("EDT-FR-MOQW: an editable rich surface offers it by default", async () => {
    const { element, editor } = mountSurface(undefined);
    await waitFor(() => expect(element.querySelector(".mermaid-edit")).not.toBeNull());
    editor.destroy();
    element.remove();
  });

  it("EDT-FR-MOQW: the candidate of a review modal does not offer it", async () => {
    const { element, editor } = mountSurface(false);
    await waitFor(() =>
      expect(element.querySelector("pre")?.getAttribute("data-diagram-view")).toBe("error"),
    );
    expect(element.querySelector(".mermaid-edit")).toBeNull();
    editor.destroy();
    element.remove();
  });

  it("EDT-FR-TOVY: an overlay that is open closes when a new one opens", async () => {
    await mount();
    const pm = document.querySelector(".ProseMirror") as HTMLElement;
    const view = (pm as unknown as { editor: TiptapEditor }).editor.view;
    await open();
    const first = dialog();
    let fence = -1;
    view.state.doc.descendants((node, pos) => {
      if (node.type.name === "codeBlock") fence = pos;
    });
    act(() => openDiagramEditor(view, fence, null));
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    expect(dialog()).not.toBe(first);
  });
});
