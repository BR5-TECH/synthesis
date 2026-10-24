import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("mermaid", async () => (await import("../../test/mermaidMock")).mermaidModule);
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import {
  holdNextRender,
  mermaidApi,
  overrideNextRender,
  resetMermaidMock,
} from "../../test/mermaidMock";
import { renderDiagram, resetMermaidForTest } from "../markdownMermaidRender";
import { MermaidStatic } from "./MermaidStatic";

const SRC = "graph TD\nA-->B";
const fallback = (
  <pre data-testid="fallback">
    <code>{SRC}</code>
  </pre>
);

beforeEach(() => {
  resetMermaidForTest();
  resetMermaidMock();
  delete document.documentElement.dataset.theme;
});

afterEach(() => {
  cleanup();
  delete document.documentElement.dataset.theme;
});

describe("an inert diagram outside an editing surface (DFV-FR-EGFA, EDT-FR-LEUI)", () => {
  it("DFV-FR-EGFA, EDT-FR-LEUI: the source shows while the diagram renders, then the diagram, which takes no input", async () => {
    const held = holdNextRender();
    render(<MermaidStatic source={SRC}>{fallback}</MermaidStatic>);
    expect(screen.getByTestId("fallback")).toBeInTheDocument();
    held.release();
    const figure = await screen.findByRole("img", { name: "Mermaid diagram" });
    expect(figure.querySelector("svg text")?.textContent).toBe(SRC);
    expect(figure.hasAttribute("tabindex")).toBe(false);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.queryByTestId("fallback")).toBeNull();
    fireEvent.click(figure);
    expect(screen.getByRole("img", { name: "Mermaid diagram" })).toBe(figure);
  });

  it("DFV-FR-EGFA, EDT-FR-XTEB, EDT-FR-JBZU: an invalid diagram shows its source and the error as text", async () => {
    overrideNextRender(async () => {
      throw new Error("<b>bad</b> INVALID");
    });
    render(<MermaidStatic source="graph TD">{fallback}</MermaidStatic>);
    const error = await screen.findByText("Diagram error: <b>bad</b> INVALID");
    expect(error.querySelector("b")).toBeNull();
    expect(screen.getByTestId("fallback")).toBeInTheDocument();
  });

  it("DFV-FR-EGFA: a diagram already rendered shows at once", async () => {
    await renderDiagram(SRC, "light");
    render(<MermaidStatic source={SRC}>{fallback}</MermaidStatic>);
    expect(screen.getByRole("img", { name: "Mermaid diagram" })).toBeInTheDocument();
    expect(screen.queryByTestId("fallback")).toBeNull();
  });

  it("EDT-FR-JIBV: a change of theme renders the diagram again in that theme", async () => {
    render(<MermaidStatic source={SRC}>{fallback}</MermaidStatic>);
    const figure = await screen.findByRole("img", { name: "Mermaid diagram" });
    expect(figure.querySelector("svg")?.getAttribute("data-theme")).toBe("default");
    act(() => {
      document.documentElement.dataset.theme = "dark";
    });
    await waitFor(() =>
      expect(
        screen.getByRole("img", { name: "Mermaid diagram" }).querySelector("svg")?.getAttribute("data-theme"),
      ).toBe("dark"),
    );
  });

  it("DFV-FR-EGFA: a new source renders a new diagram", async () => {
    const { rerender } = render(<MermaidStatic source={SRC}>{fallback}</MermaidStatic>);
    await screen.findByRole("img", { name: "Mermaid diagram" });
    rerender(<MermaidStatic source={"graph TD\nX-->Y"}>{fallback}</MermaidStatic>);
    await waitFor(() =>
      expect(
        screen.getByRole("img", { name: "Mermaid diagram" }).querySelector("svg text")?.textContent,
      ).toBe("graph TD\nX-->Y"),
    );
  });

  it("DFV-FR-EGFA: a block removed before its render's turn never renders", async () => {
    const held = holdNextRender();
    const blocker = renderDiagram("graph TD\nblocker", "light");
    const { unmount } = render(<MermaidStatic source={SRC}>{fallback}</MermaidStatic>);
    unmount();
    held.release();
    await blocker;
    // Renders run in order, so once a later one finishes, this one was decided.
    await renderDiagram("graph TD\nsentinel", "light");
    expect(mermaidApi.render.mock.calls.map((call) => call[1])).not.toContain(SRC);
  });
});
