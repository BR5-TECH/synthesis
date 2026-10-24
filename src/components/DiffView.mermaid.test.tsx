import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { resetDiffModes } from "../state/diffModes";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetLogBufferForTest } from "../logging";
import { letWriteLand } from "../test/autosave";
import { makeWireBackend, renderDiff, revs, target } from "../test/diffViewFixtures";
import { resetMermaidMock } from "../test/mermaidMock";
import { DIAGRAM_LABEL, SHOW_DIAGRAM_LABEL, SHOW_SOURCE_LABEL } from "./markdownMermaid";
import { resetMermaidForTest } from "./markdownMermaidRender";

const wireBackend = makeWireBackend(invokeMock);
const callsTo = (name: string) => invokeMock.mock.calls.filter((c) => c[0] === name);

const OLD = "# Title\n\n```mermaid\ngraph TD\nA-->B\n```\n\nText.\n";
const NEW = "# Title\n\n```mermaid\ngraph TD\nA-->C\n```\n\nText.\n";

beforeEach(() => {
  invokeMock.mockReset();
  resetDiffModes();
  resetAppPreferencesCache();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  wireBackend({
    revisions: revs(OLD, NEW),
    prefs: { diffRenderingMode: "rich", diffVisualizationMode: "side_by_side" },
  });
});

afterEach(cleanup);

/** The two panes of the row that holds the changed fence. */
async function fenceRow(): Promise<[HTMLElement, HTMLElement]> {
  const view = await screen.findByTestId("diff-rich-side-by-side");
  await within(view).findByRole("button", { name: DIAGRAM_LABEL });
  const row = Array.from(view.querySelectorAll<HTMLElement>(".diff-sbs__row")).find((r) =>
    r.querySelector(".mermaid-block"),
  )!;
  const [left, right] = Array.from(row.children) as HTMLElement[];
  return [left, right];
}

describe("diagram fences in a Rich diff (DFV-FR-EGFA)", () => {
  it("DFV-FR-EGFA, EDT-FR-LEUI: the original shows an inert diagram and the target a diagram with its toggle", async () => {
    renderDiff(target("notes.md"));
    const [left, right] = await fenceRow();

    const inert = await within(left).findByRole("img", { name: "Mermaid diagram" });
    expect(inert.querySelector("svg text")?.textContent).toBe("graph TD\nA-->B");
    expect(within(left).queryByRole("button")).toBeNull();
    expect(left).toHaveAttribute("aria-readonly", "true");
    // DFV-FR-19, DFV-FR-32: the block keeps its marking, and its word marks
    // show only on a source, which the diagram replaces.
    expect(left.getAttribute("data-mark")).toBeTruthy();
    expect(left.querySelector(".diff-word")).toBeNull();

    expect(within(right).getByRole("button", { name: SHOW_SOURCE_LABEL })).toBeInTheDocument();
    await waitFor(() =>
      expect(right.querySelector(".mermaid-diagram svg text")?.textContent).toBe(
        "graph TD\nA-->C",
      ),
    );
  });

  it("DFV-FR-EGFA, DFV-FR-19: the target's fence keeps its block marking in both views", async () => {
    renderDiff(target("notes.md"));
    const [, right] = await fenceRow();
    const pre = () => right.querySelector("pre")!;
    await waitFor(() => expect(pre().getAttribute("data-diagram-view")).toBe("diagram"));
    const mark = pre().getAttribute("data-mark");
    expect(mark).toBeTruthy();

    fireEvent.click(within(right).getByRole("button", { name: SHOW_SOURCE_LABEL }));
    expect(pre().getAttribute("data-diagram-view")).toBe("source");
    expect(pre().getAttribute("data-mark")).toBe(mark);
    // DFV-FR-32: the word marks show on the source.
    expect(pre().querySelector(".diff-word")).not.toBeNull();
  });

  it("DFV-FR-EGFA, EDT-FR-CQBG: toggling the target's fence writes nothing", async () => {
    renderDiff(target("notes.md"));
    const [, right] = await fenceRow();
    fireEvent.click(within(right).getByRole("button", { name: SHOW_SOURCE_LABEL }));
    fireEvent.click(within(right).getByRole("button", { name: SHOW_DIAGRAM_LABEL }));
    await letWriteLand();
    expect(callsTo("save_artifact_contents")).toHaveLength(0);
  });

  it("DFV-FR-EGFA: in Unified, the removed revision of a fence is an inert diagram above the editable one", async () => {
    wireBackend({
      revisions: revs(OLD, NEW),
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "unified" },
    });
    renderDiff(target("notes.md"));
    const view = await screen.findByTestId("diff-rich-unified");
    const inert = await within(view).findByRole("img", { name: "Mermaid diagram" });
    expect(inert.querySelector("svg text")?.textContent).toBe("graph TD\nA-->B");
    expect(inert.closest(".diff-block")).toHaveAttribute("aria-readonly", "true");
    expect(within(view).getByRole("button", { name: SHOW_SOURCE_LABEL })).toBeInTheDocument();
    await waitFor(() =>
      expect(view.querySelector(".ProseMirror .mermaid-diagram svg text")?.textContent).toBe(
        "graph TD\nA-->C",
      ),
    );
  });

  it("DFV-FR-EGFA: a fence in another language is drawn as code on both sides", async () => {
    wireBackend({
      revisions: revs("# T\n\n```js\nlet a = 1;\n```\n", "# T\n\n```js\nlet a = 2;\n```\n"),
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "side_by_side" },
    });
    renderDiff(target("notes.md"));
    const view = await screen.findByTestId("diff-rich-side-by-side");
    await waitFor(() => expect(view.querySelectorAll("pre").length).toBeGreaterThan(1));
    expect(view.querySelector(".mermaid-block")).toBeNull();
    expect(view.querySelector(".mermaid-diagram")).toBeNull();
  });
});
