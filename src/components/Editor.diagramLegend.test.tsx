import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);
vi.mock("@xyflow/react", async (importOriginal) =>
  (await import("../test/calmMock")).xyflowModule(await importOriginal()),
);

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { Editor } from "./Editor";
import { CALM_EDITOR_LABEL, DIAGRAM_EDITOR_LABEL, DiagramEditorHost } from "./DiagramEditor";
import { resetDiagramEditorForTest } from "./DiagramEditor/store";
import { CALM_HINTS_LABEL, HINTS_LABEL } from "./DiagramEditor/HintsArea";
import {
  HIDE_DETAILS_LABEL,
  HIDE_HINTS_LABEL,
  SHOW_DETAILS_LABEL,
  SHOW_HINTS_LABEL,
} from "./DiagramEditor/LegendToggle";
import { EDIT_DIAGRAM_LABEL, VIEW_DIAGRAM_LABEL } from "./markdownMermaid";
import { EditSessionStore } from "../state/editSessions";
import { resetLogBufferForTest } from "../logging";
import { resetMermaidMock } from "../test/mermaidMock";
import { resetMermaidForTest } from "./markdownMermaidRender";
import { readCalm, resetCalmForTest } from "../calm/read";
import { CALM_TEMPLATES } from "../calm/templates";
import { diagramLegendHidden, restoreDiagramLegend } from "../state/diagramLegend";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import { letWriteLand } from "../test/autosave";
import type { LayoutPreferences } from "../types";

const MERMAID_BODY = "# Title\n\n```mermaid\ngraph TD\nA-->B\n```\n\nAfter the diagram.\n";
const FLOW = CALM_TEMPLATES.find((t) => t.key === "flow")!;
const CALM_BODY = "```calm\n" + FLOW.source + "\n```\n";

interface Backend {
  /** The layout record of the project slot. */
  layout: LayoutPreferences | null;
  saved: LayoutPreferences[];
}

/** A strict backend: it answers what the surfaces ask for and rejects the rest. */
function wireBackend(body: string): Backend {
  const backend: Backend = { layout: null, saved: [] };
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "load_artifact_contents_by_id") return { body, checksum: "ck1" };
    if (cmd === "save_artifact_contents") return { checksum: "ck2" };
    if (cmd === "load_layout_preferences") return backend.layout && { ...backend.layout };
    if (cmd === "save_layout_preferences") {
      const record = args?.preferences as LayoutPreferences;
      backend.saved.push(record);
      backend.layout = record;
      return undefined;
    }
    if (cmd === "append_log_records") return undefined;
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
  return backend;
}

let sessions: EditSessionStore;
const pmEl = () => document.querySelector(".ProseMirror") as HTMLElement;

async function mount(body: string): Promise<Backend> {
  const backend = wireBackend(body);
  render(
    <>
      <Editor artifactId="a.md" artifactName="a.md" sessions={sessions} readOnly={false} />
      <DiagramEditorHost />
    </>,
  );
  await waitFor(() => expect(pmEl()?.querySelector("pre")).toBeTruthy());
  return backend;
}

/** Wait until every fence has its render result, so it draws no more. */
async function fencesSettled() {
  await waitFor(() => {
    for (const pre of Array.from(pmEl().querySelectorAll("pre"))) {
      expect(pre.getAttribute("data-diagram-view")).not.toBe("pending");
    }
  });
}

async function openMermaidEditor(): Promise<HTMLElement> {
  await fencesSettled();
  fireEvent.click(screen.getByRole("button", { name: EDIT_DIAGRAM_LABEL }));
  return screen.findByRole("dialog", { name: DIAGRAM_EDITOR_LABEL });
}

/** The CALM fence draws its inline canvas before its buttons are final. */
async function calmDrawn() {
  await waitFor(() =>
    expect(document.querySelector(".calm-figure .calm-inline")).toBeTruthy(),
  );
}

async function openCalmViewer(): Promise<HTMLElement> {
  await calmDrawn();
  fireEvent.click(screen.getByRole("button", { name: VIEW_DIAGRAM_LABEL }));
  return screen.findByRole("dialog", { name: "View CALM architecture" });
}

async function closeDialog(dialog: HTMLElement) {
  fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
}

/** The labels of the controls of the title bar, in order. */
const titleControls = (dialog: HTMLElement) =>
  Array.from(dialog.querySelectorAll(".diagram-overlay__controls button")).map((b) =>
    b.getAttribute("aria-label"),
  );

beforeAll(async () => {
  // The first read loads the validator and the layout engine under the hook
  // limit, so no test waits for it under the shorter limit of a `findBy`.
  await readCalm(FLOW.source);
});

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  resetCalmForTest();
  resetDiagramEditorForTest();
  resetLayoutPreferencesCache();
  sessions = new EditSessionStore();
});

afterEach(() => {
  cleanup();
  resetLayoutPreferencesCache();
});

describe("the legend toggle of the diagram editor overlay", () => {
  it("EDT-FR-JFGN, EDT-FR-UOXS, EDT-FR-FKUH, EDT-FR-NKGX: it sits before Close and hides and shows the hints area", async () => {
    await mount(MERMAID_BODY);
    const dialog = await openMermaidEditor();
    expect(titleControls(dialog)).toEqual([HIDE_HINTS_LABEL, "Close"]);
    const toggle = within(dialog).getByRole("button", { name: HIDE_HINTS_LABEL });
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    expect(toggle.title).toBe(HIDE_HINTS_LABEL);
    expect(within(dialog).getByRole("region", { name: HINTS_LABEL })).toBeInTheDocument();

    fireEvent.click(toggle);
    expect(within(dialog).queryByRole("region", { name: HINTS_LABEL })).toBeNull();
    const shown = within(dialog).getByRole("button", { name: SHOW_HINTS_LABEL });
    expect(shown).toHaveAttribute("aria-pressed", "false");
    expect(shown.title).toBe(SHOW_HINTS_LABEL);
    // EDT-FR-UOXS: the panes are the last part of the overlay, so they take
    // the height that the hints area leaves.
    const panes = dialog.querySelector(".diagram-editor__panes") as HTMLElement;
    expect(panes.parentElement).toBe(dialog);
    expect(dialog.querySelector(".diagram-hints")).toBeNull();

    fireEvent.click(shown);
    expect(within(dialog).getByRole("region", { name: HINTS_LABEL })).toBeInTheDocument();
  });

  it("EDT-FR-NGCW, EDT-FR-SRXG: the hidden state outlives the overlay and the next opening starts hidden", async () => {
    await mount(MERMAID_BODY);
    const first = await openMermaidEditor();
    fireEvent.click(within(first).getByRole("button", { name: HIDE_HINTS_LABEL }));
    await closeDialog(first);
    const second = await openMermaidEditor();
    expect(within(second).queryByRole("region", { name: HINTS_LABEL })).toBeNull();
    expect(within(second).getByRole("button", { name: SHOW_HINTS_LABEL })).toBeInTheDocument();
  });

  it("EDT-FR-DNVJ, EDT-FR-NGCW: a toggle leaves the buffer unchanged and writes no file of the project", async () => {
    await mount(MERMAID_BODY);
    const dialog = await openMermaidEditor();
    const source = within(dialog).getByRole("textbox", { name: "Diagram source" }) as HTMLTextAreaElement;
    const before = source.value;
    fireEvent.click(within(dialog).getByRole("button", { name: HIDE_HINTS_LABEL }));
    fireEvent.click(within(dialog).getByRole("button", { name: SHOW_HINTS_LABEL }));
    expect(source.value).toBe(before);
    await letWriteLand();
    expect(screen.queryByText("● unsaved")).toBeNull();
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "save_artifact_contents")).toBe(false);
    await closeDialog(dialog);
    // An undo in the document has no step of the toggle to take back.
    fireEvent.keyDown(pmEl(), { key: "z", metaKey: true });
    expect(pmEl().querySelector("pre")?.textContent).toContain("A-->B");
    expect(diagramLegendHidden()).toBe(false);
  });

  it("EDT-FR-VWNJ: the Mermaid viewer has no legend toggle", async () => {
    await mount(MERMAID_BODY);
    await fencesSettled();
    fireEvent.click(screen.getByRole("button", { name: VIEW_DIAGRAM_LABEL }));
    const viewer = await screen.findByRole("dialog", { name: "View Mermaid diagram" });
    expect(titleControls(viewer)).toEqual(["Fit to window", "Close"]);
  });
});

describe("the legend toggle of the CALM viewer", () => {
  it("CAD-FR-RLDM, CAD-FR-XHZP, CAD-FR-KUBU: it sits between Fit and Close and hides and shows the details panel", async () => {
    await mount(CALM_BODY);
    const viewer = await openCalmViewer();
    expect(titleControls(viewer)).toEqual(["Fit to window", HIDE_DETAILS_LABEL, "Close"]);
    const toggle = within(viewer).getByRole("button", { name: HIDE_DETAILS_LABEL });
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    expect(toggle.title).toBe(HIDE_DETAILS_LABEL);

    fireEvent.click(toggle);
    expect(within(viewer).queryByRole("complementary", { name: "Details" })).toBeNull();
    const shown = within(viewer).getByRole("button", { name: SHOW_DETAILS_LABEL });
    expect(shown).toHaveAttribute("aria-pressed", "false");
    expect(shown.title).toBe(SHOW_DETAILS_LABEL);
    // CAD-FR-XHZP: the diagram area is the only part of the stage.
    const stage = viewer.querySelector(".calm-stage") as HTMLElement;
    expect(Array.from(stage.children).map((c) => c.className.split(" ")[0])).toEqual([
      "calm-stage__area",
    ]);

    fireEvent.click(shown);
    expect(within(viewer).getByRole("complementary", { name: "Details" })).toBeInTheDocument();
  });

  it("CAD-FR-XHZP, CAD-FR-MZSV: a hide and a show keep the chosen flow", async () => {
    await mount(CALM_BODY);
    const viewer = await openCalmViewer();
    const select = within(viewer).getByRole("combobox", { name: "Flow" }) as HTMLSelectElement;
    const flowId = select.options[1].value;
    fireEvent.change(select, { target: { value: flowId } });
    const details = within(viewer).getByRole("complementary", { name: "Details" });
    expect(within(details).getByText("Pay for an order")).toBeInTheDocument();

    fireEvent.click(within(viewer).getByRole("button", { name: HIDE_DETAILS_LABEL }));
    expect(select.value).toBe(flowId);
    fireEvent.click(within(viewer).getByRole("button", { name: SHOW_DETAILS_LABEL }));
    expect(select.value).toBe(flowId);
    const again = within(viewer).getByRole("complementary", { name: "Details" });
    expect(within(again).getByText("Pay for an order")).toBeInTheDocument();
  });

  it("CAD-FR-XHZP: a hide and a show keep the selected item and the zoom", async () => {
    await mount(CALM_BODY);
    const viewer = await openCalmViewer();
    const details = within(viewer).getByRole("complementary", { name: "Details" });
    const entry = details.querySelector("button.calm-details__entry") as HTMLButtonElement;
    const name = entry.textContent ?? "";
    fireEvent.click(entry);
    const selected = within(viewer).getByRole("complementary", { name: "Details" });
    expect(within(selected).getByRole("heading", { name })).toBeInTheDocument();
    const area = within(viewer).getByRole("group", { name: "CALM architecture diagram" });
    const fitted = within(viewer).getByTestId("calm-zoom").textContent;
    fireEvent.keyDown(area, { key: "+" });
    const zoom = within(viewer).getByTestId("calm-zoom").textContent;
    expect(zoom).not.toBe(fitted);

    fireEvent.click(within(viewer).getByRole("button", { name: HIDE_DETAILS_LABEL }));
    expect(within(viewer).getByTestId("calm-zoom").textContent).toBe(zoom);
    fireEvent.click(within(viewer).getByRole("button", { name: SHOW_DETAILS_LABEL }));
    expect(within(viewer).getByTestId("calm-zoom").textContent).toBe(zoom);
    const again = within(viewer).getByRole("complementary", { name: "Details" });
    expect(within(again).getByRole("heading", { name })).toBeInTheDocument();
  });

  it("EDT-FR-MSCV: a legend hidden in the CALM viewer is hidden in the CALM editor overlay", async () => {
    await mount(CALM_BODY);
    const viewer = await openCalmViewer();
    fireEvent.click(within(viewer).getByRole("button", { name: HIDE_DETAILS_LABEL }));
    await closeDialog(viewer);

    await calmDrawn();
    fireEvent.click(screen.getByRole("button", { name: EDIT_DIAGRAM_LABEL }));
    const editor = await screen.findByRole("dialog", { name: CALM_EDITOR_LABEL });
    expect(within(editor).queryByRole("region", { name: CALM_HINTS_LABEL })).toBeNull();
    const toggle = within(editor).getByRole("button", { name: SHOW_HINTS_LABEL });
    expect(toggle).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(toggle);
    expect(within(editor).getByRole("region", { name: CALM_HINTS_LABEL })).toBeInTheDocument();
  });
});

describe("the stored legend state", () => {
  it("EDT-FR-NGCW, SNV-FR-08: a toggle in an overlay is stored in the layout record of the project", async () => {
    const backend = await mount(MERMAID_BODY);
    backend.layout = { bottomPanelHeight: 210 };
    await act(() => restoreDiagramLegend("/p"));
    const dialog = await openMermaidEditor();
    fireEvent.click(within(dialog).getByRole("button", { name: HIDE_HINTS_LABEL }));
    await waitFor(() =>
      expect(backend.saved[backend.saved.length - 1]).toEqual({ bottomPanelHeight: 210, diagramLegendHidden: true }),
    );
  });

  it("EDT-FR-DNVJ: an opening reads the stored value again and shows what the other window stored", async () => {
    const backend = await mount(MERMAID_BODY);
    backend.layout = { diagramLegendHidden: false };
    await act(() => restoreDiagramLegend("/p"));
    // The Settings window hides the legend while the main window shows it.
    backend.layout = { diagramLegendHidden: true };
    const dialog = await openMermaidEditor();
    await waitFor(() =>
      expect(within(dialog).queryByRole("region", { name: HINTS_LABEL })).toBeNull(),
    );
    expect(diagramLegendHidden()).toBe(true);
    expect(within(dialog).getByRole("button", { name: SHOW_HINTS_LABEL })).toBeInTheDocument();
  });
});
