/**
 * The diagram editor overlay on the prompt of a New Artifact tab
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-11, and
 * `EDT-editor.md` EDT-FR-MOQW, EDT-FR-YARW).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { makeStubs, renderWorkspace } from "../test/newArtifactFixtures";
import { DiagramEditorHost } from "./DiagramEditor";
import { resetDiagramEditorForTest } from "./DiagramEditor/store";
import { hunkCandidateBuffers } from "../state/candidateBuffers";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { resetLogBufferForTest } from "../logging";
import { resetMermaidMock } from "../test/mermaidMock";
import { resetMermaidForTest } from "./markdownMermaidRender";
import { letWriteLand } from "../test/autosave";

const { stub } = makeStubs(invokeMock);

const PROMPT_BODY = "# Prompt\n\n```mermaid\ngraph TD\nA-->B\n```\n";

const writes = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "save_draft_file_contents")
    .map((c) => (c[1] as { body: string }).body);

beforeEach(() => {
  invokeMock.mockReset();
  hunkCandidateBuffers.clear();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  resetDiagramEditorForTest();
  stub();
  const base = invokeMock.getMockImplementation()!;
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "load_draft_file_contents") return { body: PROMPT_BODY, checksum: "c1" };
    if (cmd === "append_log_records") return undefined;
    return base(cmd, args);
  });
});

afterEach(cleanup);

describe("the diagram editor overlay on a draft prompt", () => {
  it("NAW-FR-11, EDT-FR-MOQW, EDT-FR-YARW: the prompt's diagram fence opens the overlay, and an edit there writes the prompt", async () => {
    renderWorkspace();
    render(<DiagramEditorHost />);
    // The fence draws again once its diagram has rendered, so the button is
    // taken from that drawing.
    await screen.findByRole("button", { name: "Mermaid diagram, edit source" });
    fireEvent.click(screen.getByRole("button", { name: "Edit diagram" }));
    const source = screen.getByRole("textbox", { name: "Diagram source" });
    expect(source).toHaveValue("graph TD\nA-->B");

    fireEvent.change(source, { target: { value: "graph TD\nA-->Z" } });

    // NAW-FR-13: the prompt writes itself while the overlay is still open.
    await letWriteLand();
    await waitFor(() => expect(writes().some((body) => body.includes("A-->Z"))).toBe(true));
    expect(screen.getByRole("dialog", { name: "Edit Mermaid diagram" })).toBeInTheDocument();
  });

  it("NAW-FR-11, EDT-FR-MOQW, EDT-FR-YVAI: the prompt's diagram fence opens the viewer, which writes nothing", async () => {
    renderWorkspace();
    render(<DiagramEditorHost />);
    await screen.findByRole("button", { name: "Mermaid diagram, edit source" });
    fireEvent.click(await screen.findByRole("button", { name: "View diagram" }));
    const viewer = screen.getByRole("dialog", { name: "View Mermaid diagram" });
    await waitFor(() => expect(viewer.querySelector("svg")).not.toBeNull());
    fireEvent.click(within(viewer).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    await letWriteLand();
    expect(writes()).toEqual([]);
  });
});
