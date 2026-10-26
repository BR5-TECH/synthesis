import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
import { DiagramEditorHost, DIAGRAM_EDITOR_LABEL, SOURCE_LABEL } from "./DiagramEditor";
import { NO_DIAGRAM, PREVIEW_LABEL } from "./DiagramEditor/DiagramPreview";
import { resetDiagramEditorForTest } from "./DiagramEditor/store";
import { EditSessionStore } from "../state/editSessions";
import { resetLogBufferForTest } from "../logging";
import { holdNextRender, resetMermaidMock } from "../test/mermaidMock";
import { letWriteLand } from "../test/autosave";
import userEvent from "@testing-library/user-event";
import {
  DIAGRAM_LABEL,
  EDIT_DIAGRAM_LABEL,
  SHOW_SOURCE_LABEL,
  VIEW_DIAGRAM_LABEL,
} from "./markdownMermaid";
import { HINTS_LABEL } from "./DiagramEditor/HintsArea";
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

let sessions: EditSessionStore;

const pmEl = () => document.querySelector(".ProseMirror") as HTMLElement;
const tiptap = () => (pmEl() as unknown as { editor: TiptapEditor }).editor;
const fence = () => pmEl().querySelector("pre") as HTMLElement;
const editButton = () => screen.getByRole("button", { name: EDIT_DIAGRAM_LABEL });
const dialog = () => screen.getByRole("dialog", { name: DIAGRAM_EDITOR_LABEL });
const sourcePane = () => screen.getByRole("textbox", { name: SOURCE_LABEL }) as HTMLTextAreaElement;
const preview = () => screen.getByRole("region", { name: PREVIEW_LABEL });
const unsaved = () => screen.queryByText("● unsaved");

async function mount(body = BODY, readOnly = false, fenced = true) {
  const backend = wireBackend(body);
  const view = render(
    <>
      <Editor artifactId="a.md" artifactName="a.md" sessions={sessions} readOnly={readOnly} />
      <DiagramEditorHost />
    </>,
  );
  await waitFor(() => expect(pmEl()).not.toBeNull());
  if (fenced) await waitFor(() => expect(pmEl().querySelector("pre")).not.toBeNull());
  else await screen.findByRole("heading", { name: "Title" });
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

/**
 * Open the overlay from the Edit button. The fence draws again when its render
 * settles, so the button is taken from the drawing that has settled.
 */
async function openOverlay() {
  await fencesSettled();
  fireEvent.click(editButton());
  return dialog();
}

async function saveNow() {
  await act(async () => {
    await sessions.flush("a.md", { force: true });
  });
}

const typeSource = (next: string) => fireEvent.change(sourcePane(), { target: { value: next } });

beforeEach(() => {
  invokeMock.mockReset();
  listeners = {};
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  resetDiagramEditorForTest();
  sessions = new EditSessionStore();
});

afterEach(() => {
  cleanup();
});

describe("the Edit button of a diagram fence", () => {
  it("EDT-FR-ADNU, EDT-FR-YVAI: an icon button named Edit diagram sits between the View button and the view toggle, in both views", async () => {
    await mount();
    await screen.findByRole("button", { name: DIAGRAM_LABEL });
    const button = editButton();
    expect(button.title).toBe(EDIT_DIAGRAM_LABEL);
    expect(button.textContent).toBe("");
    expect(button.querySelector("svg")).not.toBeNull();
    const controls = button.parentElement as HTMLElement;
    expect(controls.className).toBe("mermaid-controls");
    expect(within(controls).getByRole("button", { name: SHOW_SOURCE_LABEL })).toBeInTheDocument();
    // The View button comes first, then the Edit button; the toggle keeps the corner.
    expect(Array.from(controls.children).map((el) => el.getAttribute("aria-label"))).toEqual([
      VIEW_DIAGRAM_LABEL,
      EDIT_DIAGRAM_LABEL,
      SHOW_SOURCE_LABEL,
    ]);

    fireEvent.click(screen.getByRole("button", { name: SHOW_SOURCE_LABEL }));
    expect(fence().getAttribute("data-diagram-view")).toBe("source");
    expect(editButton()).toBeInTheDocument();
  });

  it("EDT-FR-QTHL, EDT-FR-BQIU: an empty fence and a fence with an error have the Edit button alone", async () => {
    await mount("```mermaid\n```\n\n```mermaid\nINVALID\n```\n");
    await waitFor(() =>
      expect(pmEl().querySelectorAll("pre")[1].getAttribute("data-diagram-view")).toBe("error"),
    );
    const buttons = screen.getAllByRole("button", { name: EDIT_DIAGRAM_LABEL });
    expect(buttons).toHaveLength(2);
    for (const button of buttons) {
      expect(button.parentElement?.children).toHaveLength(1);
    }
    expect(screen.queryByRole("button", { name: SHOW_SOURCE_LABEL })).toBeNull();
  });

  it("EDT-FR-QTHL, EDT-FR-OXQC: a fence shown as a Find peek keeps the Edit button and the View button and has no toggle", async () => {
    await mount();
    await screen.findByRole("button", { name: DIAGRAM_LABEL });
    act(() => {
      sessions.setFind("a.md", { form: "find" });
    });
    fireEvent.change(screen.getByLabelText("Find"), { target: { value: "A-->B" } });
    await waitFor(() => expect(fence().getAttribute("data-diagram-view")).toBe("source"));
    expect(screen.queryByRole("button", { name: SHOW_SOURCE_LABEL })).toBeNull();
    const controls = editButton().parentElement as HTMLElement;
    expect(Array.from(controls.children).map((el) => el.getAttribute("aria-label"))).toEqual([
      VIEW_DIAGRAM_LABEL,
      EDIT_DIAGRAM_LABEL,
    ]);
  });

  it("EDT-FR-LEUI, EDT-FR-QTHL: a surface that takes no input has no Edit button", async () => {
    await mount(BODY, true);
    await waitFor(() => expect(fence().getAttribute("data-diagram-view")).toBe("diagram"));
    expect(screen.queryByRole("button", { name: EDIT_DIAGRAM_LABEL })).toBeNull();
  });

  it("EDT-FR-ADNU: the keyboard reaches and operates the Edit button", async () => {
    await mount();
    await screen.findByRole("button", { name: DIAGRAM_LABEL });
    await fencesSettled();
    const button = editButton();
    expect(button.tagName).toBe("BUTTON");
    expect(button.tabIndex).toBe(0);
    button.focus();
    expect(document.activeElement).toBe(button);
    await userEvent.keyboard("{Enter}");
    expect(dialog()).toBeInTheDocument();
    fireEvent.keyDown(sourcePane(), { key: "Escape" });
    expect(document.activeElement).toBe(editButton());
    await userEvent.keyboard(" ");
    expect(dialog()).toBeInTheDocument();
  });
});

describe("the diagram editor overlay", () => {
  it("EDT-FR-SJGJ, EDT-FR-HVGG, EDT-FR-ULPU: a modal dialog over a scrim, with two named panes and a divider", async () => {
    await mount();
    const modal = await openOverlay();
    expect(modal).toHaveAttribute("aria-modal", "true");
    expect(modal.closest(".scrim")).not.toBeNull();
    expect(within(modal).getByRole("textbox", { name: SOURCE_LABEL })).toBeInTheDocument();
    expect(within(modal).getByRole("region", { name: PREVIEW_LABEL })).toBeInTheDocument();
    const divider = within(modal).getByRole("separator");
    expect(divider).toHaveAttribute("tabindex", "0");
    expect(divider).toHaveAttribute("aria-valuenow", "50");
    expect(within(modal).getByRole("button", { name: "Close" })).toBeInTheDocument();
    // The source pane comes first, the diagram pane last.
    const panes = sourcePane().parentElement as HTMLElement;
    expect(panes.firstElementChild).toBe(sourcePane());
    expect(panes.lastElementChild).toBe(preview());
  });

  it("EDT-FR-SJGJ: a click on the scrim does nothing", async () => {
    await mount();
    await openOverlay();
    const scrim = screen.getByTestId("diagram-editor-scrim");
    fireEvent.mouseDown(scrim);
    fireEvent.click(scrim);
    expect(dialog()).toBeInTheDocument();
  });

  it("EDT-FR-HGAZ: focus moves into the source pane with the caret, and Tab stays inside", async () => {
    await mount();
    await openOverlay();
    const pane = sourcePane();
    expect(document.activeElement).toBe(pane);
    expect(pane.selectionStart).toBe(0);
    expect(pane.value).toBe("graph TD\nA-->B");

    const close = within(dialog()).getByRole("button", { name: "Close" });
    const hints = within(dialog()).getByRole("region", { name: HINTS_LABEL });
    // The order of focus: Close, the source pane, the divider, the hints area.
    expect(hints).toHaveAttribute("tabindex", "0");
    hints.focus();
    fireEvent.keyDown(hints, { key: "Tab" });
    expect(document.activeElement).toBe(close);
    fireEvent.keyDown(close, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(hints);
  });

  it("EDT-FR-SRXG, EDT-FR-HGAZ: Close and Escape close it, keep the edit, and give focus to the Edit button", async () => {
    await mount();
    await openOverlay();
    typeSource("graph TD\nA-->C");
    fireEvent.click(within(dialog()).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(editButton());
    expect(tiptap().getText()).toContain("A-->C");

    await openOverlay();
    expect(sourcePane().value).toBe("graph TD\nA-->C");
    fireEvent.keyDown(sourcePane(), { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(editButton());
    expect(tiptap().getText()).toContain("A-->C");
  });

  it("EDT-FR-EBEZ, EDT-FR-YARW, EDT-FR-MIQE: an edit in the source pane is an edit of the buffer, which marks it dirty and saves", async () => {
    const { backend } = await mount();
    await openOverlay();
    expect(unsaved()).toBeNull();
    typeSource("graph TD\nA-->C");
    await waitFor(() => expect(unsaved()).toBeInTheDocument());
    // The fence behind the scrim renders the new source.
    await waitFor(() =>
      expect(fence().querySelector("svg")?.getAttribute("data-source")).toBe("graph TD\nA-->C"),
    );
    // The overlay has no Save control of its own.
    expect(within(dialog()).queryByRole("button", { name: /save/i })).toBeNull();
    await saveNow();
    expect(backend.saves[backend.saves.length - 1]).toContain(
      "# Title\n\n```mermaid\ngraph TD\nA-->C\n```\n\nAfter the diagram.",
    );
    expect(dialog()).toBeInTheDocument();
  });

  it("EDT-FR-YARW, EDT-FR-70: the write schedule of the tab writes the edit while the overlay is open", async () => {
    const { backend } = await mount();
    await openOverlay();
    typeSource("graph TD\nA-->E");
    await letWriteLand();
    await waitFor(() => expect(backend.saves.some((body) => body.includes("A-->E"))).toBe(true));
    expect(dialog()).toBeInTheDocument();
  });

  it("EDT-FR-YARW: an edit that makes the source invalid is still written", async () => {
    const { backend } = await mount();
    await openOverlay();
    typeSource("INVALID");
    await within(preview()).findByRole("alert");
    await saveNow();
    expect(backend.saves[backend.saves.length - 1]).toContain("```mermaid\nINVALID\n```");
  });

  it("EDT-FR-HBEB: undo and redo in the source pane use the one history of the artifact", async () => {
    await mount();
    await openOverlay();
    typeSource("graph TD\nA-->C");
    await waitFor(() => expect(unsaved()).toBeInTheDocument());

    fireEvent.keyDown(sourcePane(), { key: "z", metaKey: true });
    expect(sourcePane().value).toBe("graph TD\nA-->B");
    expect(tiptap().getText()).toContain("A-->B");

    await waitFor(() =>
      expect(preview().querySelector("svg")?.getAttribute("data-source")).toBe("graph TD\nA-->B"),
    );

    fireEvent.keyDown(sourcePane(), { key: "z", metaKey: true, shiftKey: true });
    expect(sourcePane().value).toBe("graph TD\nA-->C");
    expect(tiptap().getText()).toContain("A-->C");
    await waitFor(() =>
      expect(preview().querySelector("svg")?.getAttribute("data-source")).toBe("graph TD\nA-->C"),
    );
  });

  it("EDT-FR-HBEB: an edit made in the overlay is undone in the document after it closes", async () => {
    await mount();
    await openOverlay();
    typeSource("graph TD\nA-->C");
    fireEvent.click(within(dialog()).getByRole("button", { name: "Close" }));
    fireEvent.keyDown(pmEl(), { key: "z", metaKey: true });
    expect(tiptap().getText()).toContain("A-->B");
    expect(tiptap().getText()).not.toContain("A-->C");
  });

  it("EDT-FR-HBEB: the Undo of the native Edit menu in the source pane uses the history of the artifact", async () => {
    await mount();
    await openOverlay();
    typeSource("graph TD\nA-->C");
    const event = new InputEvent("beforeinput", {
      inputType: "historyUndo",
      bubbles: true,
      cancelable: true,
    });
    act(() => {
      sourcePane().dispatchEvent(event);
    });
    expect(event.defaultPrevented).toBe(true);
    expect(sourcePane().value).toBe("graph TD\nA-->B");
  });

  it("EDT-FR-IYTC, EDT-FR-CQBG: opening, moving, resizing, the divider and closing change nothing in the buffer", async () => {
    const { backend } = await mount();
    await openOverlay();
    const head = dialog().querySelector(".diagram-editor__head") as HTMLElement;
    fireEvent.pointerDown(head, { button: 0, clientX: 100, clientY: 100, pointerId: 1 });
    fireEvent.pointerMove(head, { clientX: 90, clientY: 95, pointerId: 1 });
    fireEvent.pointerUp(head, { pointerId: 1 });
    const edge = dialog().querySelector("[data-edge='se']") as HTMLElement;
    fireEvent.pointerDown(edge, { button: 0, clientX: 0, clientY: 0, pointerId: 2 });
    fireEvent.pointerMove(edge, { clientX: -40, clientY: -40, pointerId: 2 });
    fireEvent.pointerUp(edge, { pointerId: 2 });
    fireEvent.keyDown(within(dialog()).getByRole("separator"), { key: "ArrowLeft" });
    fireEvent.click(within(dialog()).getByRole("button", { name: "Close" }));

    expect(unsaved()).toBeNull();
    // No undo step was taken: an undo has nothing to reverse.
    fireEvent.keyDown(pmEl(), { key: "z", metaKey: true });
    expect(tiptap().getText()).toContain("A-->B");
    expect(unsaved()).toBeNull();
    await saveNow();
    expect(backend.saves).toEqual([BODY]);
  });
});

describe("the diagram pane", () => {
  it("EDT-FR-FKUH, EDT-FR-NKGX, EDT-FR-NWZX, EDT-FR-HVGG: the hints area under the panes follows the chart type while the author types", async () => {
    await mount();
    await openOverlay();
    const hints = within(dialog()).getByRole("region", { name: HINTS_LABEL });
    // EDT-FR-NKGX: the hints area sits under the panes, as a part of the
    // overlay itself, so its 20% is a share of the whole overlay.
    expect(hints.previousElementSibling).toBe(sourcePane().parentElement);
    expect(hints.parentElement).toBe(dialog());
    expect(within(hints).getByRole("heading", { level: 2 }).textContent).toBe("Flowchart");
    typeSource("sequenceDiagram\nA->>B: hi");
    expect(within(hints).getByRole("heading", { level: 2 }).textContent).toBe("Sequence diagram");
    typeSource("");
    expect(within(hints).getByRole("heading", { level: 2 }).textContent).toBe("General hints");
  });

  it("EDT-FR-IYTC: reading, selecting, and scrolling the hints change nothing in the buffer", async () => {
    const { backend } = await mount();
    await openOverlay();
    const hints = within(dialog()).getByRole("region", { name: HINTS_LABEL });
    hints.focus();
    fireEvent.mouseDown(hints);
    fireEvent.scroll(hints, { target: { scrollTop: 40 } });
    const snippet = hints.querySelector("code") as HTMLElement;
    const range = document.createRange();
    range.selectNodeContents(snippet);
    window.getSelection()?.addRange(range);
    fireEvent.click(snippet);
    fireEvent.keyDown(hints, { key: "ArrowDown" });
    fireEvent.click(within(dialog()).getByRole("button", { name: "Close" }));
    expect(unsaved()).toBeNull();
    await saveNow();
    expect(backend.saves).toEqual([BODY]);
  });

  it("EDT-FR-QSOQ: it shows the diagram of the source and renders again while the author types", async () => {
    await mount();
    await openOverlay();
    await waitFor(() =>
      expect(preview().querySelector("svg")?.getAttribute("data-source")).toBe("graph TD\nA-->B"),
    );
    typeSource("graph TD\nA-->D");
    await waitFor(() =>
      expect(preview().querySelector("svg")?.getAttribute("data-source")).toBe("graph TD\nA-->D"),
    );
    expect(preview().querySelector("[data-stale]")).toBeNull();
  });

  it("EDT-FR-QSOQ: while a render is in progress the pane keeps what it shows", async () => {
    await mount();
    await openOverlay();
    await waitFor(() => expect(preview().querySelector("svg")).not.toBeNull());
    const held = [holdNextRender(), holdNextRender()];
    typeSource("graph TD\nA-->H");
    expect(preview().querySelector("svg")?.getAttribute("data-source")).toBe("graph TD\nA-->B");
    expect(within(preview()).queryByText(NO_DIAGRAM)).toBeNull();
    act(() => held.forEach((h) => h.release()));
    await waitFor(() =>
      expect(preview().querySelector("svg")?.getAttribute("data-source")).toBe("graph TD\nA-->H"),
    );
  });

  it("EDT-FR-STFK, EDT-FR-SRXG: the last diagram is kept only during one opening", async () => {
    await mount();
    await openOverlay();
    await waitFor(() => expect(preview().querySelector("svg")).not.toBeNull());
    typeSource("graph TD\nINVALID");
    await within(preview()).findByRole("alert");
    expect(preview().querySelector("[data-stale='true']")).not.toBeNull();
    fireEvent.click(within(dialog()).getByRole("button", { name: "Close" }));

    await openOverlay();
    await within(preview()).findByRole("alert");
    expect(preview().querySelector("svg")).toBeNull();
    expect(within(preview()).getByText(NO_DIAGRAM)).toBeInTheDocument();
  });

  it("EDT-FR-STFK, EDT-FR-XKMP, EDT-FR-VIOP: an invalid source keeps the last diagram dimmed, with the error under it", async () => {
    await mount();
    await openOverlay();
    await waitFor(() => expect(preview().querySelector("svg")).not.toBeNull());
    typeSource("graph TD\nINVALID");
    const alert = await within(preview()).findByRole("alert");
    expect(alert.querySelector(".mermaid-error__summary")?.textContent).toMatch(/^Diagram error: /);
    expect(alert.querySelector(".mermaid-error__detail")?.textContent).toContain("INVALID");
    const figure = preview().querySelector("[data-stale='true']") as HTMLElement;
    expect(figure.querySelector("svg")?.getAttribute("data-source")).toBe("graph TD\nA-->B");
    // The error comes after the diagram.
    expect(figure.compareDocumentPosition(alert) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("EDT-FR-UDVE: an empty source shows the statement and no error", async () => {
    await mount();
    await openOverlay();
    await waitFor(() => expect(preview().querySelector("svg")).not.toBeNull());
    typeSource("   ");
    expect(within(preview()).getByText(NO_DIAGRAM)).toBeInTheDocument();
    expect(preview().querySelector("svg")).toBeNull();
    expect(within(preview()).queryByRole("alert")).toBeNull();
  });

  it("EDT-FR-UDVE: a source that is invalid when the overlay opens shows the statement and the error", async () => {
    await mount("```mermaid\nINVALID\n```\n");
    await waitFor(() => expect(fence().getAttribute("data-diagram-view")).toBe("error"));
    await openOverlay();
    const alert = await within(preview()).findByRole("alert");
    const statement = within(preview()).getByText(NO_DIAGRAM);
    // The error shows under the statement.
    expect(statement.compareDocumentPosition(alert) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(preview().querySelector("svg")).toBeNull();
  });
});

describe("when the fence or the surface changes under the overlay", () => {
  it("EDT-FR-SHYX: an undo that removes the fence closes the overlay and gives focus to the surface", async () => {
    await mount("# Title\n", false, false);
    // The fence is an edit of its own, so an undo can take it away.
    act(() => {
      tiptap()
        .chain()
        .focus("end")
        .insertContent({
          type: "codeBlock",
          attrs: { language: "mermaid" },
          content: [{ type: "text", text: "graph TD\nA-->B" }],
        })
        .run();
    });
    await openOverlay();
    fireEvent.keyDown(sourcePane(), { key: "z", metaKey: true });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(pmEl().querySelector("pre")).toBeNull();
    expect(pmEl().contains(document.activeElement) || document.activeElement === pmEl()).toBe(
      true,
    );
  });

  it("EDT-FR-WPGE: the external-change modal closes the overlay and keeps the edit", async () => {
    await mount();
    await openOverlay();
    typeSource("graph TD\nA-->C");
    act(() => {
      listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
        payload: { artifactId: "a.md", checksum: "ck-other" },
      });
    });
    expect(await screen.findByRole("button", { name: "Keep my version" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: DIAGRAM_EDITOR_LABEL })).toBeNull();
    expect(tiptap().getText()).toContain("A-->C");
  });

  it("EDT-FR-WPGE, EDT-FR-LEUI: a surface that becomes read-only closes the overlay and loses the Edit button", async () => {
    const { view } = await mount();
    await openOverlay();
    view.rerender(
      <>
        <Editor artifactId="a.md" artifactName="a.md" sessions={sessions} readOnly />
        <DiagramEditorHost />
      </>,
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.queryByRole("button", { name: EDIT_DIAGRAM_LABEL })).toBeNull();
  });

  it("EDT-FR-SHYX: an undo of an edit elsewhere in the document keeps the overlay on its fence", async () => {
    await mount();
    await fencesSettled();
    act(() => {
      tiptap().chain().focus("end").insertContent("More text.").run();
    });
    await openOverlay();
    fireEvent.keyDown(sourcePane(), { key: "z", metaKey: true });
    expect(tiptap().getText()).not.toContain("More text.");
    expect(dialog()).toBeInTheDocument();
    expect(sourcePane().value).toBe("graph TD\nA-->B");
  });

  it("EDT-FR-HGAZ: the Edit button keeps the focus when the fence draws again after the close", async () => {
    await mount();
    await openOverlay();
    const held = [holdNextRender(), holdNextRender()];
    typeSource("graph TD\nA-->F");
    fireEvent.click(within(dialog()).getByRole("button", { name: "Close" }));
    const before = editButton();
    expect(document.activeElement).toBe(before);
    act(() => held.forEach((h) => h.release()));
    await waitFor(() => expect(before.isConnected).toBe(false));
    await waitFor(() => expect(document.activeElement).toBe(editButton()));
  });

  it("EDT-FR-SRXG, EDT-FR-SHYX: an overlay whose surface ends closes and keeps no state", async () => {
    const { view } = await mount();
    await openOverlay();
    typeSource("graph TD\nA-->C");
    view.rerender(<DiagramEditorHost />);
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("the keys and the divider of the source pane", () => {
  it("EDT-FR-UYBZ, EDT-FR-HGAZ: Tab in the source pane indents by two spaces, keeps focus there, and edits the buffer", async () => {
    const { backend } = await mount();
    await openOverlay();
    const pane = sourcePane();
    pane.setSelectionRange(9, 9);
    const pressed = fireEvent.keyDown(pane, { key: "Tab" });
    expect(pressed).toBe(false);
    expect(document.activeElement).toBe(pane);
    expect(pane.value).toBe("graph TD\n  A-->B");
    expect(pane.selectionStart).toBe(11);
    expect(pane.selectionEnd).toBe(11);
    await waitFor(() => expect(unsaved()).toBeInTheDocument());
    await saveNow();
    expect(backend.saves[backend.saves.length - 1]).toContain("```mermaid\ngraph TD\n  A-->B\n```");
  });

  it("EDT-FR-UYBZ: Tab indents each line of a selection that spans lines, and Shift+Tab outdents them again", async () => {
    await mount("```mermaid\ngraph TD\nA-->B\nB-->C\n```\n");
    await openOverlay();
    const pane = sourcePane();
    pane.setSelectionRange(9, pane.value.length);
    fireEvent.keyDown(pane, { key: "Tab" });
    expect(pane.value).toBe("graph TD\n  A-->B\n  B-->C");
    expect([pane.selectionStart, pane.selectionEnd]).toEqual([9, pane.value.length]);
    expect(document.activeElement).toBe(pane);

    expect(fireEvent.keyDown(pane, { key: "Tab", shiftKey: true })).toBe(false);
    expect(pane.value).toBe("graph TD\nA-->B\nB-->C");
    expect([pane.selectionStart, pane.selectionEnd]).toEqual([9, pane.value.length]);
    expect(document.activeElement).toBe(pane);
  });

  it("EDT-FR-UYBZ: Shift+Tab on a line with no leading space changes nothing and keeps focus", async () => {
    await mount();
    await openOverlay();
    const pane = sourcePane();
    pane.setSelectionRange(3, 3);
    // The key is still the source pane's: the browser does not move focus.
    expect(fireEvent.keyDown(pane, { key: "Tab", shiftKey: true })).toBe(false);
    expect(pane.value).toBe("graph TD\nA-->B");
    expect(document.activeElement).toBe(pane);
    expect(sessions.get("a.md")?.dirty ?? false).toBe(false);
  });

  it("EDT-FR-UYBZ: a Tab that ends an IME composition is the composition's, not an indent", async () => {
    await mount();
    await openOverlay();
    const pane = sourcePane();
    pane.setSelectionRange(9, 9);
    expect(fireEvent.keyDown(pane, { key: "Tab", keyCode: 229 })).toBe(true);
    expect(fireEvent.keyDown(pane, { key: "Tab", isComposing: true })).toBe(true);
    expect(pane.value).toBe("graph TD\nA-->B");
  });

  it("EDT-FR-UYBZ, EDT-FR-HBEB: an indent is an edit in the one history of the artifact, which undo reverses", async () => {
    await mount();
    await openOverlay();
    const pane = sourcePane();
    pane.setSelectionRange(9, 9);
    fireEvent.keyDown(pane, { key: "Tab" });
    expect(pane.value).toBe("graph TD\n  A-->B");
    await waitFor(() => expect(tiptap().state.doc.textContent).toContain("graph TD\n  A-->B"));
    fireEvent.keyDown(pane, { key: "z", metaKey: true });
    await waitFor(() => expect(sourcePane().value).toBe("graph TD\nA-->B"));
    expect(tiptap().state.doc.textContent).toContain("graph TD\nA-->B");
  });

  it("EDT-FR-CFCQ: Ctrl+Tab and Ctrl+Shift+Tab move focus out of the source pane, and change no text", async () => {
    await mount();
    await openOverlay();
    const pane = sourcePane();
    fireEvent.keyDown(pane, { key: "Tab", ctrlKey: true });
    expect(document.activeElement).toBe(within(dialog()).getByRole("separator"));

    pane.focus();
    fireEvent.keyDown(pane, { key: "Tab", ctrlKey: true, shiftKey: true });
    expect(document.activeElement).toBe(within(dialog()).getByRole("button", { name: "Close" }));
    expect(pane.value).toBe("graph TD\nA-->B");
  });

  it("EDT-FR-SPAG, EDT-FR-UPYC: a drag of the divider shows the split in a tooltip, holds at the middle near it, and the tooltip goes when the drag ends", async () => {
    await mount();
    await openOverlay();
    const divider = within(dialog()).getByRole("separator");
    const panes = divider.parentElement as HTMLElement;
    vi.spyOn(panes, "getBoundingClientRect").mockReturnValue({
      left: 0,
      top: 0,
      right: 1000,
      bottom: 400,
      width: 1000,
      height: 400,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
    expect(screen.queryByTestId("diagram-split-tip")).toBeNull();

    fireEvent.pointerDown(divider, { button: 0, clientX: 500, clientY: 120, pointerId: 3 });
    expect(screen.getByTestId("diagram-split-tip")).toHaveTextContent("50 / 50");
    expect(divider).toContainElement(screen.getByTestId("diagram-split-tip"));

    fireEvent.pointerMove(divider, { clientX: 623, clientY: 150, pointerId: 3 });
    expect(screen.getByTestId("diagram-split-tip")).toHaveTextContent("62 / 38");
    expect(divider).toHaveAttribute("aria-valuenow", "62");
    expect(divider).toHaveAttribute("aria-valuetext", "62 / 38");
    expect(screen.getByTestId("diagram-split-tip").style.top).toBe("150px");

    // EDT-FR-UPYC: 1.5 points from the middle, the divider holds at 50/50.
    fireEvent.pointerMove(divider, { clientX: 515, clientY: 150, pointerId: 3 });
    expect(divider).toHaveAttribute("aria-valuenow", "50");
    expect(screen.getByTestId("diagram-split-tip")).toHaveTextContent("50 / 50");
    fireEvent.pointerMove(divider, { clientX: 530, clientY: 150, pointerId: 3 });
    expect(divider).toHaveAttribute("aria-valuenow", "53");

    fireEvent.pointerUp(divider, { pointerId: 3 });
    expect(screen.queryByTestId("diagram-split-tip")).toBeNull();
    expect(divider).toHaveAttribute("aria-valuenow", "53");

    // A drag that the system cancels also ends the tooltip.
    fireEvent.pointerDown(divider, { button: 0, clientX: 530, clientY: 120, pointerId: 4 });
    expect(screen.getByTestId("diagram-split-tip")).toBeInTheDocument();
    fireEvent.pointerCancel(divider, { pointerId: 4 });
    expect(screen.queryByTestId("diagram-split-tip")).toBeNull();
  });
});
