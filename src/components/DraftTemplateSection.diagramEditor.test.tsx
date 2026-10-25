/**
 * The diagram editor overlay on the Draft template of Project settings
 * (`../../specifications/ui/SET-project-settings.md` SET-FR-16, and
 * `EDT-editor.md` EDT-FR-MOQW, EDT-FR-YARW, EDT-FR-TOVY).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { DraftTemplateSection } from "./DraftTemplateSection";
import { DiagramEditorHost } from "./DiagramEditor";
import { resetDiagramEditorForTest } from "./DiagramEditor/store";
import { resetLogBufferForTest } from "../logging";
import { resetMermaidMock } from "../test/mermaidMock";
import { resetMermaidForTest } from "./markdownMermaidRender";
import { letWriteLand } from "../test/autosave";

const TEMPLATE = "# Draft\n\n```mermaid\ngraph TD\nA-->B\n```\n";

const saved = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "save_project_config")
    .map((c) => (c[1] as { config: { draftTemplate?: string } }).config.draftTemplate ?? "");

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  resetDiagramEditorForTest();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_project_config") {
      return { lineEndings: "lf", draftTemplate: TEMPLATE, graduationConcurrencyLimit: 1 };
    }
    if (cmd === "save_project_config") return undefined;
    if (cmd === "append_log_records") return undefined;
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
});

afterEach(cleanup);

describe("the diagram editor overlay on the Draft template", () => {
  it("SET-FR-16, EDT-FR-MOQW, EDT-FR-YARW: the template's diagram fence opens the overlay, and an edit there writes the template", async () => {
    render(
      <>
        <DraftTemplateSection contentRoot="/a#0" />
        <DiagramEditorHost />
      </>,
    );
    // The fence draws again once its diagram has rendered, so the button is
    // taken from that drawing.
    await screen.findByRole("button", { name: "Mermaid diagram, edit source" });
    fireEvent.click(screen.getByRole("button", { name: "Edit diagram" }));
    const source = screen.getByRole("textbox", { name: "Diagram source" });
    expect(source).toHaveValue("graph TD\nA-->B");

    fireEvent.change(source, { target: { value: "graph TD\nA-->Z" } });

    // SET-FR-17: the template writes itself while the overlay is still open.
    await letWriteLand();
    await waitFor(() => expect(saved().some((t) => t.includes("A-->Z"))).toBe(true));
    expect(screen.getByRole("dialog", { name: "Edit Mermaid diagram" })).toBeInTheDocument();
  });
});
