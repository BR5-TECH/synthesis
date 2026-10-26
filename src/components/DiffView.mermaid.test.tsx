import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

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
import { DiffView } from "./DiffView";
import { DiagramEditorHost } from "./DiagramEditor";
import { resetDiagramEditorForTest } from "./DiagramEditor/store";
import { EditSessionStore } from "../state/editSessions";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import type { Discussion } from "../types";

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
  resetDiagramEditorForTest();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
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

  it("DFV-FR-EGFA, EDT-FR-ADNU: the target's fence has the Edit button and the original's has none", async () => {
    renderDiff(target("notes.md"));
    const [left, right] = await fenceRow();
    expect(within(right).getByRole("button", { name: "Edit diagram" })).toBeInTheDocument();
    expect(within(left).queryByRole("button", { name: "Edit diagram" })).toBeNull();
  });

  it("DFV-FR-EGFA, EDT-FR-YVAI: the target's fence has the View button and the original's has none", async () => {
    renderDiff(target("notes.md"));
    const [left, right] = await fenceRow();
    expect(await within(right).findByRole("button", { name: "View diagram" })).toBeInTheDocument();
    expect(within(left).queryByRole("button", { name: "View diagram" })).toBeNull();
    expect(left.querySelector(".mermaid-controls")).toBeNull();
  });

  it("DFV-FR-EGFA, EDT-FR-MOQW, EDT-FR-EBEZ, EDT-FR-HBEB: the overlay edits the target, which writes the file, and undo uses its history", async () => {
    const sessions = new EditSessionStore();
    render(
      <>
        <DiffView target={target("notes.md")} sessions={sessions} />
        <DiagramEditorHost />
      </>,
    );
    const [, right] = await fenceRow();
    fireEvent.click(within(right).getByRole("button", { name: "Edit diagram" }));
    const source = screen.getByRole("textbox", { name: "Diagram source" }) as HTMLTextAreaElement;
    expect(source.value).toBe("graph TD\nA-->C");

    fireEvent.change(source, { target: { value: "graph TD\nA-->D" } });
    await act(async () => {
      await sessions.flush("notes.md", { force: true });
    });
    const saves = callsTo("save_artifact_contents");
    expect((saves[saves.length - 1][1] as { body: string }).body).toContain("A-->D");

    fireEvent.keyDown(source, { key: "z", metaKey: true });
    await waitFor(() =>
      expect(
        (screen.getByRole("textbox", { name: "Diagram source" }) as HTMLTextAreaElement).value,
      ).toBe("graph TD\nA-->C"),
    );
  });

  it("DFV-FR-EGFA, EDT-FR-SJGJ: in Unified, the overlay on an unchanged fence stays open while the author edits it", async () => {
    const SAME = "# Title\n\nOld text.\n\n```mermaid\ngraph TD\nA-->B\n```\n\nText.\n";
    const CHANGED = "# Title\n\nNew text.\n\n```mermaid\ngraph TD\nA-->B\n```\n\nText.\n";
    wireBackend({
      revisions: revs(SAME, CHANGED),
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "unified" },
    });
    const sessions = new EditSessionStore();
    render(
      <>
        <DiffView target={target("notes.md")} sessions={sessions} />
        <DiagramEditorHost />
      </>,
    );
    const view = await screen.findByTestId("diff-rich-unified");
    await within(view).findByRole("button", { name: DIAGRAM_LABEL });
    fireEvent.click(within(view).getByRole("button", { name: "Edit diagram" }));
    const source = () =>
      screen.getByRole("textbox", { name: "Diagram source" }) as HTMLTextAreaElement;

    // One edit that adds a line, and one that does not.
    fireEvent.change(source(), { target: { value: "graph TD\nA-->B\nB-->C" } });
    fireEvent.change(source(), { target: { value: "graph TD\nA-->B\nB-->D" } });
    await letWriteLand();
    expect(screen.getByRole("dialog", { name: "Edit Mermaid diagram" })).toBeInTheDocument();
    expect(source().value).toBe("graph TD\nA-->B\nB-->D");
    const saves = callsTo("save_artifact_contents");
    expect((saves[saves.length - 1][1] as { body: string }).body).toContain("B-->D");

    // The comparison catches up once the overlay closes.
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() =>
      expect(within(view).getAllByRole("img", { name: "Mermaid diagram" }).length).toBeGreaterThan(0),
    );
  });

  it("DFV-FR-EGFA, EDT-FR-SJGJ, EDT-FR-SRXG: with the discussion panel open, one Escape closes the overlay alone", async () => {
    const opened: Discussion = {
      id: "d1",
      target: { kind: "artifact", artifactId: "notes.md" },
      fragmentTarget: null,
      comments: [
        {
          id: "c1",
          author: { kind: "human", login: "raver119" },
          body: "Is the diagram right?",
          quotes: [],
          attachments: [],
          createdAt: "2026-01-01T00:00:00Z",
        },
      ],
      resolved: false,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    };
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "list_discussions") return [opened];
      if (cmd === "list_project_agents" || cmd === "list_agent_turns") return [];
      return base(cmd, args);
    });
    render(
      <>
        <DiffView target={target("notes.md")} sessions={new EditSessionStore()} />
        <DiagramEditorHost />
      </>,
    );
    fireEvent.click(await screen.findByRole("button", { name: /Show discussion/ }));
    expect(screen.getByRole("dialog", { name: "Discussion" })).toBeInTheDocument();

    const [, right] = await fenceRow();
    fireEvent.click(within(right).getByRole("button", { name: "Edit diagram" }));
    const overlay = screen.getByRole("dialog", { name: "Edit Mermaid diagram" });

    fireEvent.keyDown(within(overlay).getByRole("textbox", { name: "Diagram source" }), {
      key: "Escape",
    });
    expect(screen.queryByRole("dialog", { name: "Edit Mermaid diagram" })).toBeNull();
    expect(screen.getByRole("dialog", { name: "Discussion" })).toBeInTheDocument();

    // Without the overlay, Escape is the panel's again.
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Discussion" })).toBeNull());

    // EDT-FR-SJGJ: a press in the overlay does not reach the tab, so the
    // actions menu behind the overlay stays as it is.
    fireEvent.click(screen.getByRole("button", { name: "Actions" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();
    fireEvent.click(within(right).getByRole("button", { name: "Edit diagram" }));
    fireEvent.mouseDown(screen.getByRole("textbox", { name: "Diagram source" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    fireEvent.mouseDown(document.body);
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  });
});
