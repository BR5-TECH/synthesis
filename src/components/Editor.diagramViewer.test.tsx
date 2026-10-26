import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Editor as TiptapEditor } from "@tiptap/react";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);

const invokeMock = vi.fn();
let listeners: Record<string, (event: { payload: ArtifactChangedPayload }) => void> = {};
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (event: { payload: ArtifactChangedPayload }) => void) => {
    listeners[event] = cb;
    return () => {};
  }),
}));

import { Editor } from "./Editor";
import { DiagramEditorHost } from "./DiagramEditor";
import {
  DIAGRAM_VIEWER_LABEL,
  FIT_LABEL,
  VIEWER_DIAGRAM_LABEL,
} from "./DiagramEditor/DiagramViewer";
import { sendHistoryKey } from "./DiagramEditor/fence";
import { resetDiagramEditorForTest } from "./DiagramEditor/store";
import { MAX_ZOOM, MIN_ZOOM } from "./DiagramEditor/viewerGeometry";
import { EditSessionStore } from "../state/editSessions";
import { resetLogBufferForTest } from "../logging";
import { holdNextRender, resetMermaidMock } from "../test/mermaidMock";
import {
  DIAGRAM_LABEL,
  EDIT_DIAGRAM_LABEL,
  SHOW_SOURCE_LABEL,
  VIEW_DIAGRAM_LABEL,
} from "./markdownMermaid";
import { resetMermaidForTest } from "./markdownMermaidRender";
import { ARTIFACT_CHANGED_EXTERNALLY } from "../events";
import type { ArtifactChangedPayload } from "../types";

const BODY = "# Title\n\n```mermaid\ngraph TD\nA-->B\n```\n\nAfter the diagram.\n";

interface Backend {
  body: string;
  saves: string[];
}

/** A strict backend: it answers what the Editor asks for and rejects the rest. */
function wireBackend(body = BODY): Backend {
  const backend: Backend = { body, saves: [] };
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    if (cmd === "load_artifact_contents_by_id") return { body: backend.body, checksum: "ck1" };
    if (cmd === "save_artifact_contents") {
      backend.saves.push((args as { body: string }).body);
      return { checksum: `ck-saved-${backend.saves.length}` };
    }
    if (cmd === "append_log_records") return undefined;
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
  return backend;
}

/**
 * jsdom has no layout. The diagram area reports the size a test gives it, and
 * a stand-in `ResizeObserver` tells the viewer when that size changes.
 */
let areaSize = { width: 1000, height: 500 };
const observers = new Set<() => void>();
class FakeResizeObserver {
  private readonly callback: () => void;
  constructor(callback: () => void) {
    this.callback = callback;
  }
  observe() {
    observers.add(this.callback);
  }
  disconnect() {
    observers.delete(this.callback);
  }
  unobserve() {}
}
const isArea = (el: unknown) =>
  el instanceof HTMLElement && el.classList.contains("diagram-viewer__area");
const widthDescriptor = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
const heightDescriptor = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight");

let sessions: EditSessionStore;

const pmEl = () => document.querySelector(".ProseMirror") as HTMLElement;
const tiptap = () => (pmEl() as unknown as { editor: TiptapEditor }).editor;
const fence = () => pmEl().querySelector("pre") as HTMLElement;
const viewButton = () => screen.getByRole("button", { name: VIEW_DIAGRAM_LABEL });
const viewer = () => screen.getByRole("dialog", { name: DIAGRAM_VIEWER_LABEL });
const area = () => within(viewer()).getByRole("img", { name: VIEWER_DIAGRAM_LABEL });
const stage = () => screen.getByTestId("diagram-viewer-stage");
const unsaved = () => screen.queryByText("● unsaved");

/** The position at the end of the source of the first diagram fence. */
function fenceEnd(): number {
  let end = -1;
  tiptap().state.doc.descendants((node, pos) => {
    if (end < 0 && node.type.name === "codeBlock") end = pos + 1 + node.content.size;
  });
  return end;
}

/** The scale, x, and y of the transform of the stage. */
function placed(): { scale: number; x: number; y: number } {
  const match = stage().style.transform.match(
    /translate\((-?[\d.e+-]+)px, (-?[\d.e+-]+)px\) scale\(([\d.e+-]+)\)/,
  );
  if (!match) throw new Error(`no transform: ${stage().style.transform}`);
  return { x: Number(match[1]), y: Number(match[2]), scale: Number(match[3]) };
}

async function mount(body = BODY, readOnly = false) {
  const backend = wireBackend(body);
  const view = render(
    <>
      <Editor artifactId="a.md" artifactName="a.md" sessions={sessions} readOnly={readOnly} />
      <DiagramEditorHost />
    </>,
  );
  await waitFor(() => expect(pmEl()).not.toBeNull());
  await waitFor(() => expect(pmEl().querySelector("pre")).not.toBeNull());
  return { backend, view };
}

/** Wait until every fence has its render result, so it draws no more. */
async function fencesSettled() {
  await waitFor(() => {
    for (const pre of Array.from(pmEl().querySelectorAll("pre"))) {
      expect(pre.getAttribute("data-diagram-view")).not.toBe("pending");
    }
  });
}

async function openViewer() {
  await fencesSettled();
  await waitFor(() => expect(screen.getByRole("button", { name: VIEW_DIAGRAM_LABEL })).toBeTruthy());
  fireEvent.click(viewButton());
  await waitFor(() => expect(stage().querySelector("svg")).not.toBeNull());
  return viewer();
}

async function saveNow() {
  await act(async () => {
    await sessions.flush("a.md", { force: true });
  });
}

beforeEach(() => {
  delete document.documentElement.dataset.theme;
  invokeMock.mockReset();
  listeners = {};
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  resetDiagramEditorForTest();
  sessions = new EditSessionStore();
  areaSize = { width: 1000, height: 500 };
  observers.clear();
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get(this: HTMLElement) {
      return isArea(this) ? areaSize.width : 0;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return isArea(this) ? areaSize.height : 0;
    },
  });
});

afterEach(() => {
  cleanup();
  delete document.documentElement.dataset.theme;
  vi.unstubAllGlobals();
  if (widthDescriptor) Object.defineProperty(HTMLElement.prototype, "clientWidth", widthDescriptor);
  if (heightDescriptor) {
    Object.defineProperty(HTMLElement.prototype, "clientHeight", heightDescriptor);
  }
});

describe("the View button of a diagram fence", () => {
  it("EDT-FR-YVAI: an icon button named View diagram, before the Edit button, in both views", async () => {
    await mount();
    await screen.findByRole("button", { name: DIAGRAM_LABEL });
    await fencesSettled();
    const button = viewButton();
    expect(button.title).toBe(VIEW_DIAGRAM_LABEL);
    expect(button.textContent).toBe("");
    expect(button.querySelector("svg")).not.toBeNull();
    expect(button.nextElementSibling?.getAttribute("aria-label")).toBe(EDIT_DIAGRAM_LABEL);

    fireEvent.click(screen.getByRole("button", { name: SHOW_SOURCE_LABEL }));
    expect(fence().getAttribute("data-diagram-view")).toBe("source");
    expect(viewButton()).toBeInTheDocument();
  });

  it("EDT-FR-YVAI: the keyboard reaches and operates the View button", async () => {
    await mount();
    await fencesSettled();
    const button = viewButton();
    expect(button.tagName).toBe("BUTTON");
    expect(button.tabIndex).toBe(0);
    button.focus();
    await userEvent.keyboard("{Enter}");
    expect(viewer()).toBeInTheDocument();
  });

  it("EDT-FR-OXQC: an empty fence and a fence with a diagram error have no View button", async () => {
    await mount("```mermaid\n```\n\n```mermaid\nINVALID\n```\n");
    await waitFor(() =>
      expect(pmEl().querySelectorAll("pre")[1].getAttribute("data-diagram-view")).toBe("error"),
    );
    expect(screen.getAllByRole("button", { name: EDIT_DIAGRAM_LABEL })).toHaveLength(2);
    expect(screen.queryByRole("button", { name: VIEW_DIAGRAM_LABEL })).toBeNull();
  });

  it("EDT-FR-OXQC, EDT-FR-LEUI: an inert fence has no View button", async () => {
    await mount(BODY, true);
    await waitFor(() => expect(fence().getAttribute("data-diagram-view")).toBe("diagram"));
    expect(screen.queryByRole("button", { name: VIEW_DIAGRAM_LABEL })).toBeNull();
  });

  it("EDT-FR-OXQC: a fence whose first render is in progress has no View button", async () => {
    const held = holdNextRender();
    await mount();
    expect(fence().getAttribute("data-diagram-view")).toBe("pending");
    expect(screen.queryByRole("button", { name: VIEW_DIAGRAM_LABEL })).toBeNull();
    act(() => held.release());
    await waitFor(() => expect(viewButton()).toBeInTheDocument());
  });

  it("EDT-FR-RCAV: while a changed source renders the View button stays, and an error removes it", async () => {
    await mount();
    await fencesSettled();
    fireEvent.click(screen.getByRole("button", { name: SHOW_SOURCE_LABEL }));
    const held = holdNextRender();
    act(() => {
      tiptap().chain().insertContentAt(fenceEnd(), { type: "text", text: "-->C" }).run();
    });
    await waitFor(() => expect(tiptap().getText()).toContain("-->C"));
    // The render of the new source waits; the button keeps its state.
    expect(viewButton()).toBeInTheDocument();
    act(() => held.release());
    await fencesSettled();
    expect(viewButton()).toBeInTheDocument();

    act(() => {
      tiptap().chain().insertContentAt(fenceEnd(), { type: "text", text: " INVALID" }).run();
    });
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: VIEW_DIAGRAM_LABEL })).toBeNull(),
    );
    // An undo restores a valid source, and the button comes back.
    fireEvent.keyDown(pmEl(), { key: "z", metaKey: true });
    await waitFor(() => expect(viewButton()).toBeInTheDocument());
  });
});

describe("the diagram viewer overlay", () => {
  it("EDT-FR-KMSI, EDT-FR-CCZU: a modal dialog over a scrim, with Fit and Close controls", async () => {
    await mount();
    const modal = await openViewer();
    expect(modal).toHaveAttribute("aria-modal", "true");
    expect(modal.closest(".scrim")).not.toBeNull();
    const fit = within(modal).getByRole("button", { name: FIT_LABEL });
    expect(fit.title).toBe(FIT_LABEL);
    expect(fit).toBeVisible();
    expect(within(modal).getByRole("button", { name: "Close" })).toBeInTheDocument();
    expect(modal.querySelector(".modal__title")?.textContent).toBe(DIAGRAM_VIEWER_LABEL);
  });

  it("EDT-FR-KMSI: it opens centred at 90% x 95%, moves, resizes, and keeps no geometry", async () => {
    await mount();
    const modal = await openViewer();
    const opened = { left: modal.style.left, top: modal.style.top, width: modal.style.width };
    expect(parseFloat(modal.style.width)).toBeCloseTo(window.innerWidth * 0.9);
    expect(parseFloat(modal.style.height)).toBeCloseTo(window.innerHeight * 0.95);
    expect(parseFloat(modal.style.left)).toBeCloseTo(window.innerWidth * 0.05);
    const head = modal.querySelector(".diagram-viewer__head") as HTMLElement;
    fireEvent.pointerDown(head, { button: 0, clientX: 100, clientY: 100, pointerId: 1 });
    fireEvent.pointerMove(head, { clientX: 80, clientY: 90, pointerId: 1 });
    fireEvent.pointerUp(head, { pointerId: 1 });
    expect(modal.style.left).not.toBe(opened.left);
    const edge = modal.querySelector("[data-edge='e']") as HTMLElement;
    fireEvent.pointerDown(edge, { button: 0, clientX: 0, clientY: 0, pointerId: 2 });
    fireEvent.pointerMove(edge, { clientX: -60, clientY: 0, pointerId: 2 });
    fireEvent.pointerUp(edge, { pointerId: 2 });
    expect(modal.style.width).not.toBe(opened.width);

    fireEvent.click(within(modal).getByRole("button", { name: "Close" }));
    const again = await openViewer();
    expect({ left: again.style.left, top: again.style.top, width: again.style.width }).toEqual(
      opened,
    );
  });

  it("EDT-FR-ELUJ: focus goes into the diagram area, Tab stays inside, and Close and Escape give focus to the View button", async () => {
    await mount();
    await openViewer();
    expect(document.activeElement).toBe(area());
    const close = within(viewer()).getByRole("button", { name: "Close" });
    const fit = within(viewer()).getByRole("button", { name: FIT_LABEL });
    // The order of focus: Fit, Close, the diagram area.
    fireEvent.keyDown(area(), { key: "Tab" });
    expect(document.activeElement).toBe(fit);
    fireEvent.keyDown(fit, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(area());

    fireEvent.click(close);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(viewButton());

    await openViewer();
    fireEvent.keyDown(area(), { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(viewButton());
  });

  it("EDT-FR-VWNJ: it shows the diagram of the fence alone, from the same render", async () => {
    await mount();
    const modal = await openViewer();
    expect(within(modal).queryByRole("textbox")).toBeNull();
    expect(within(modal).queryByRole("region", { name: "Mermaid syntax hints" })).toBeNull();
    expect(within(modal).queryByRole("button", { name: EDIT_DIAGRAM_LABEL })).toBeNull();
    expect(area()).toHaveAttribute("tabindex", "0");
    const svg = stage().querySelector("svg") as SVGElement;
    expect(svg.getAttribute("data-source")).toBe("graph TD\nA-->B");
    // The natural size of the drawing; the transform sets the size on screen.
    expect(svg.getAttribute("width")).toBe("200");
    expect(svg.getAttribute("height")).toBe("100");
  });

  it("EDT-FR-LJAP: the diagram opens fitted and centred, and Fit and 0 fit it again", async () => {
    await mount();
    await openViewer();
    // Area 1000 x 500, diagram 200 x 100, 16 px of margin: the height decides.
    const fitted = { scale: (500 - 32) / 100, x: (1000 - 200 * 4.68) / 2, y: (500 - 100 * 4.68) / 2 };
    await waitFor(() => expect(placed().scale).toBeCloseTo(fitted.scale));
    expect(placed().x).toBeCloseTo(fitted.x);
    expect(placed().y).toBeCloseTo(fitted.y);

    fireEvent.keyDown(area(), { key: "+" });
    expect(placed().scale).not.toBeCloseTo(fitted.scale);
    fireEvent.click(within(viewer()).getByRole("button", { name: FIT_LABEL }));
    expect(placed().scale).toBeCloseTo(fitted.scale);

    fireEvent.keyDown(area(), { key: "-" });
    fireEvent.keyDown(area(), { key: "ArrowLeft" });
    fireEvent.keyDown(area(), { key: "0" });
    expect(placed()).toEqual(expect.objectContaining({ scale: expect.closeTo(fitted.scale) }));
    expect(placed().x).toBeCloseTo(fitted.x);
  });

  it("EDT-FR-CCZU: the zoom readout shows the current size in percent, at each zoom and each fit", async () => {
    await mount();
    await openViewer();
    const readout = () => within(viewer()).getByTestId("diagram-viewer-zoom");
    // Fitted: (500 - 32) / 100 of the natural size.
    await waitFor(() => expect(readout().textContent).toBe("468%"));
    expect(readout().closest("button")).toBeNull();
    fireEvent.keyDown(area(), { key: "+" });
    expect(readout().textContent).toBe("585%");
    fireEvent.click(within(viewer()).getByRole("button", { name: FIT_LABEL }));
    expect(readout().textContent).toBe("468%");
  });

  it("EDT-FR-MZIF: the wheel zooms about the pointer, and + and - about the centre, inside the limits", async () => {
    await mount();
    await openViewer();
    await waitFor(() => expect(placed().scale).toBeGreaterThan(1));
    const before = placed();
    // The point of the diagram under the pointer stays where it is.
    const px = 300;
    const py = 200;
    const diagramX = (px - before.x) / before.scale;
    fireEvent.wheel(area(), { deltaY: -100, clientX: px, clientY: py });
    const after = placed();
    expect(after.scale).toBeGreaterThan(before.scale);
    expect((px - after.x) / after.scale).toBeCloseTo(diagramX);

    fireEvent.keyDown(area(), { key: "-" });
    expect(placed().scale).toBeLessThan(after.scale);
    fireEvent.keyDown(area(), { key: "=" });
    expect(placed().scale).toBeCloseTo(after.scale);

    for (let i = 0; i < 40; i++) fireEvent.keyDown(area(), { key: "+" });
    expect(placed().scale).toBe(MAX_ZOOM);
    for (let i = 0; i < 80; i++) fireEvent.keyDown(area(), { key: "-" });
    expect(placed().scale).toBe(MIN_ZOOM);
    expect(screen.getByTestId("diagram-viewer-zoom").textContent).toBe("10%");
  });

  it("EDT-FR-PDBC: a drag and the arrow keys pan the diagram", async () => {
    await mount();
    await openViewer();
    await waitFor(() => expect(placed().scale).toBeGreaterThan(1));
    const start = placed();
    fireEvent.pointerDown(area(), { button: 0, clientX: 100, clientY: 100, pointerId: 1 });
    fireEvent.pointerMove(area(), { clientX: 130, clientY: 80, pointerId: 1 });
    fireEvent.pointerUp(area(), { pointerId: 1 });
    expect(placed().x).toBeCloseTo(start.x + 30);
    expect(placed().y).toBeCloseTo(start.y - 20);
    expect(placed().scale).toBe(start.scale);

    const dragged = placed();
    fireEvent.keyDown(area(), { key: "ArrowRight" });
    expect(placed().x).toBeGreaterThan(dragged.x);
    fireEvent.keyDown(area(), { key: "ArrowUp" });
    expect(placed().y).toBeLessThan(dragged.y);
  });

  it("EDT-FR-JJDP: a fitted diagram fits again after a resize; a zoomed one keeps its zoom", async () => {
    await mount();
    await openViewer();
    await waitFor(() => expect(placed().scale).toBeCloseTo(4.68));
    areaSize = { width: 400, height: 400 };
    act(() => observers.forEach((notify) => notify()));
    // The width decides now: (400 - 32) / 200.
    expect(placed().scale).toBeCloseTo(1.84);

    fireEvent.keyDown(area(), { key: "+" });
    const zoomed = placed();
    areaSize = { width: 900, height: 600 };
    act(() => observers.forEach((notify) => notify()));
    expect(placed()).toEqual(zoomed);

    // A new opening starts fitted again.
    fireEvent.keyDown(area(), { key: "Escape" });
    await openViewer();
    expect(area()).toHaveAttribute("data-fitted", "true");
    // The width decides: (900 - 32) / 200.
    expect(placed().scale).toBeCloseTo((900 - 32) / 200);
  });

  it("EDT-FR-LVFQ: an edit and an undo render the viewer again, and it keeps the last diagram during a render", async () => {
    await mount();
    await openViewer();
    const svgSource = () => stage().querySelector("svg")?.getAttribute("data-source");
    expect(svgSource()).toBe("graph TD\nA-->B");
    const held = holdNextRender();
    act(() => {
      tiptap().chain().insertContentAt(fenceEnd(), { type: "text", text: "-->D" }).run();
    });
    await waitFor(() => expect(tiptap().getText()).toContain("-->D"));
    // The render waits; the viewer keeps the diagram it shows.
    expect(svgSource()).toBe("graph TD\nA-->B");
    act(() => held.release());
    await waitFor(() => expect(svgSource()).toBe("graph TD\nA-->B-->D"));

    // The undo of the native Edit menu reaches the surface behind the scrim.
    act(() => sendHistoryKey(tiptap().view, "undo", { metaKey: true, ctrlKey: false }));
    await waitFor(() => expect(svgSource()).toBe("graph TD\nA-->B"));
    expect(viewer()).toBeInTheDocument();
  });

  it("EDT-FR-LVFQ, EDT-FR-JIBV: a change of theme renders the viewer again in that theme", async () => {
    await mount();
    await openViewer();
    const svgTheme = () => stage().querySelector("svg")?.getAttribute("data-theme");
    expect(svgTheme()).toBe("default");
    act(() => {
      document.documentElement.dataset.theme = "dark";
    });
    await waitFor(() => expect(svgTheme()).toBe("dark"));
    expect(viewer()).toBeInTheDocument();
  });

  it("EDT-FR-JRDD: a source that stops being a valid diagram closes the viewer and gives focus to the surface", async () => {
    await mount();
    await openViewer();
    act(() => {
      tiptap().chain().insertContentAt(fenceEnd(), { type: "text", text: " INVALID" }).run();
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(pmEl().contains(document.activeElement) || document.activeElement === pmEl()).toBe(true);
  });

  it("EDT-FR-JRDD: a change that removes the fence closes the viewer and gives focus to the surface", async () => {
    await mount();
    await openViewer();
    act(() => {
      const { state } = tiptap();
      let from = -1;
      let to = -1;
      state.doc.descendants((node, pos) => {
        if (node.type.name === "codeBlock") {
          from = pos;
          to = pos + node.nodeSize;
        }
      });
      tiptap().chain().deleteRange({ from, to }).run();
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(pmEl().querySelector("pre")).toBeNull();
    expect(pmEl().contains(document.activeElement) || document.activeElement === pmEl()).toBe(true);
  });

  it("EDT-FR-EJBZ: a surface that becomes read-only closes the viewer", async () => {
    const { view } = await mount();
    await openViewer();
    view.rerender(
      <>
        <Editor artifactId="a.md" artifactName="a.md" sessions={sessions} readOnly />
        <DiagramEditorHost />
      </>,
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.queryByRole("button", { name: VIEW_DIAGRAM_LABEL })).toBeNull();
  });

  it("EDT-FR-EJBZ: the external-change modal closes the viewer", async () => {
    await mount();
    await openViewer();
    act(() => {
      listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
        payload: { artifactId: "a.md", checksum: "ck-other" },
      });
    });
    expect(await screen.findByRole("button", { name: "Keep my version" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: DIAGRAM_VIEWER_LABEL })).toBeNull();
  });

  it("EDT-FR-EAOC: opening, zooming, panning, fitting, moving, resizing, and closing change nothing in the buffer", async () => {
    const { backend } = await mount();
    const modal = await openViewer();
    fireEvent.wheel(area(), { deltaY: -100, clientX: 10, clientY: 10 });
    fireEvent.keyDown(area(), { key: "+" });
    fireEvent.keyDown(area(), { key: "ArrowDown" });
    fireEvent.pointerDown(area(), { button: 0, clientX: 0, clientY: 0, pointerId: 1 });
    fireEvent.pointerMove(area(), { clientX: 20, clientY: 20, pointerId: 1 });
    fireEvent.pointerUp(area(), { pointerId: 1 });
    fireEvent.keyDown(area(), { key: "0" });
    const head = modal.querySelector(".diagram-viewer__head") as HTMLElement;
    fireEvent.pointerDown(head, { button: 0, clientX: 100, clientY: 100, pointerId: 2 });
    fireEvent.pointerMove(head, { clientX: 90, clientY: 95, pointerId: 2 });
    fireEvent.pointerUp(head, { pointerId: 2 });
    const edge = modal.querySelector("[data-edge='se']") as HTMLElement;
    fireEvent.pointerDown(edge, { button: 0, clientX: 0, clientY: 0, pointerId: 3 });
    fireEvent.pointerMove(edge, { clientX: -40, clientY: -40, pointerId: 3 });
    fireEvent.pointerUp(edge, { pointerId: 3 });
    fireEvent.click(within(modal).getByRole("button", { name: "Close" }));

    expect(unsaved()).toBeNull();
    // No undo step was taken: an undo has nothing to reverse.
    fireEvent.keyDown(pmEl(), { key: "z", metaKey: true });
    expect(tiptap().getText()).toContain("A-->B");
    expect(unsaved()).toBeNull();
    await saveNow();
    expect(backend.saves).toEqual([BODY]);
  });
});
