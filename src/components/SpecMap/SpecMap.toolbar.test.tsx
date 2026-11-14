import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { SpecMap } from ".";
import { makeStore, mockCanvasRect, setWindowWidth } from "../../test/specMapRender";

vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

/**
 * jsdom does no layout, so the toolbar has no scroll width. Each test gives
 * the width that each form of the toolbar needs, and the toolbar's own width
 * is the width of the tab that `mockCanvasRect` gives.
 */
type Needs = { full: number; short: number; icons: number };
let needs: Needs;
let tabWidth: number;

function toolbar() {
  return screen.getByRole("toolbar", { name: "Map controls" });
}

beforeEach(() => {
  needs = { full: 0, short: 0, icons: 0 };
  tabWidth = 0;
  vi.spyOn(HTMLElement.prototype, "scrollWidth", "get").mockImplementation(function (this: HTMLElement) {
    if (!this.classList.contains("smap-toolbar")) return 0;
    const form = this.getAttribute("data-form") as keyof Needs | null;
    return form ? Math.max(needs[form], tabWidth) : tabWidth;
  });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) {
    return this.classList.contains("smap-toolbar") ? tabWidth : 0;
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

async function renderAt(width: number, withUnplanned = true) {
  tabWidth = width;
  mockCanvasRect({ width, height: 800 });
  setWindowWidth(1600);
  const { store } = makeStore();
  const unplanned = withUnplanned ? { on: false, onToggle: vi.fn() } : undefined;
  const view = render(<SpecMap store={store} onOpenArtifact={vi.fn()} unplanned={unplanned} />);
  await screen.findByTestId("spec-map-canvas");
  return view;
}

describe("the toolbar fits the Map tab (SMP-FR-GJEW, SMP-FR-CVIB)", () => {
  it("SMP-FR-GJEW: a tab of 1180px or more that fits the full toolbar shows the label and the full counts", async () => {
    needs = { full: 1300, short: 1000, icons: 800 };
    const { container } = await renderAt(1400);
    expect(toolbar()).toHaveAttribute("data-form", "full");
    expect(screen.getByText("Completeness")).toBeInTheDocument();
    expect(container.querySelector(".smap-toolbar__counts")).toHaveTextContent("2 verified · 2 built · 1 drafted · 1 gaps");
    expect(screen.getByRole("button", { name: "Unplanned" })).toHaveTextContent("unplanned");
  });

  it("SMP-FR-GJEW: a tab of 1180px or more that the full toolbar overflows takes the short form, and the toggles keep their names", async () => {
    // The browser check at a 1600px window: the tab is 1214px and the full toolbar needs 1368px.
    needs = { full: 1368, short: 1030, icons: 800 };
    const { container } = await renderAt(1214);
    expect(toolbar()).toHaveAttribute("data-form", "short");
    expect(screen.queryByText("Completeness")).toBeNull();
    expect(container.querySelector('.smap-bar[data-variant="project"]')).toHaveAttribute("data-narrow", "true");
    expect(container.querySelector(".smap-toolbar__counts")).toHaveTextContent("33% verified · 1 gaps");
    expect(screen.getByRole("button", { name: "Dependencies" })).toHaveTextContent("dependencies");
    expect(screen.getByRole("button", { name: "Gaps only" })).toHaveTextContent("gaps only");
    expect(screen.getByRole("button", { name: "Unplanned" })).toHaveTextContent("unplanned");
  });

  it("SMP-FR-GJEW: where the short form also overflows, the counts hide and each toggle shows its icon with its name as the accessible name and tooltip", async () => {
    needs = { full: 1368, short: 1100, icons: 800 };
    const { container } = await renderAt(1050);
    expect(toolbar()).toHaveAttribute("data-form", "icons");
    expect(container.querySelector(".smap-toolbar__counts")).toBeNull();
    for (const name of ["Dependencies", "Gaps only", "Unplanned"]) {
      const toggle = screen.getByRole("button", { name });
      expect(toggle).toHaveTextContent("");
      expect(toggle).toHaveAttribute("title", name);
      expect(toggle.querySelector("svg")).not.toBeNull();
    }
  });

  it("SMP-FR-GJEW: below 1000px the toolbar shows the icon form even where a wider form fits", async () => {
    needs = { full: 0, short: 0, icons: 0 };
    await renderAt(900);
    expect(toolbar()).toHaveAttribute("data-form", "icons");
  });

  it("SMP-FR-GJEW: a tab that becomes wide enough again takes the full form back", async () => {
    needs = { full: 1368, short: 1030, icons: 800 };
    await renderAt(1214);
    expect(toolbar()).toHaveAttribute("data-form", "short");
    tabWidth = 1463;
    mockCanvasRect({ width: 1463, height: 800 });
    act(() => {
      window.dispatchEvent(new Event("resize"));
    });
    expect(toolbar()).toHaveAttribute("data-form", "full");
    expect(screen.getByText("Completeness")).toBeInTheDocument();
  });

  it("SMP-FR-GJEW: without the unplanned toggle a toolbar that fits keeps the full form", async () => {
    // The toolbar without the unplanned toggle needs less width.
    needs = { full: 1200, short: 1000, icons: 800 };
    await renderAt(1214, false);
    expect(toolbar()).toHaveAttribute("data-form", "full");
    expect(screen.queryByRole("button", { name: "Unplanned" })).toBeNull();
  });
});
