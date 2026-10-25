import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";

import { HINTS_LABEL, HintsArea } from "./HintsArea";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("the hints area (EDT-FR-FKUH)", () => {
  it("EDT-FR-HVGG, EDT-FR-YNQF: a region named Mermaid syntax hints, which the keyboard can focus, under the name of the chart type", () => {
    render(<HintsArea source={"gantt\ntitle Plan"} />);
    const area = screen.getByRole("region", { name: HINTS_LABEL });
    expect(area).toHaveAttribute("tabindex", "0");
    expect(within(area).getByRole("heading", { level: 2 }).textContent).toBe("Gantt chart");
  });

  it("EDT-FR-FKUH: it is read-only text, with no control of any kind", () => {
    render(<HintsArea source="flowchart LR" />);
    const area = screen.getByRole("region", { name: HINTS_LABEL });
    expect(within(area).queryAllByRole("button")).toHaveLength(0);
    expect(within(area).queryAllByRole("link")).toHaveLength(0);
    expect(within(area).queryAllByRole("textbox")).toHaveLength(0);
    expect(area.querySelectorAll("[onclick], [contenteditable='true']")).toHaveLength(0);
    expect(area.textContent).toContain("-->");
  });

  it("EDT-FR-NVGA: each snippet is code text, and the hints read nothing from the network", () => {
    const fetchSpy = vi.fn(() => Promise.reject(new Error("no network")));
    vi.stubGlobal("fetch", fetchSpy);
    render(<HintsArea source="sequenceDiagram" />);
    const area = screen.getByRole("region", { name: HINTS_LABEL });
    const items = area.querySelectorAll(".diagram-hints__item");
    expect(items.length).toBeGreaterThan(5);
    for (const item of Array.from(items)) {
      expect(item.querySelector("dt code")?.textContent).not.toBe("");
      expect(item.querySelector("dd")?.textContent).not.toBe("");
    }
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("EDT-FR-NWZX: the hints change with the source", () => {
    const { rerender } = render(<HintsArea source="pie" />);
    const area = () => screen.getByRole("region", { name: HINTS_LABEL });
    expect(area()).toHaveAttribute("data-chart-type", "pie");
    rerender(<HintsArea source="mindmap" />);
    expect(area()).toHaveAttribute("data-chart-type", "mindmap");
    rerender(<HintsArea source="" />);
    expect(area()).toHaveAttribute("data-chart-type", "general");
  });
});
