// The PDF viewer's page navigation, zoom, and keyboard operation
// (PDV-FR-SAIG, PDV-FR-BEQL, PDV-FR-HQTN, PDV-FR-QNVD).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetViewStateForTest } from "../../state/documentViewState";
import { ID, resetFixtures } from "../DocumentViewer/viewerFixtures";
import { PdfViewer } from ".";
import {
  page,
  pdfPayload,
  pdfReadReturns,
  pdfjsFake,
  resetPdfFixtures,
  state,
} from "./pdfFixtures";
import { CSS_UNITS, DEFAULT_ZOOM_INDEX, ZOOM_STEPS, clampPage, scaleOf, validZoomIndex } from "./zoom";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../DocumentViewer/viewerFixtures")).invokeMock,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../DocumentViewer/viewerFixtures")).listenMock,
}));
vi.mock("../../logging", async () => (await import("../DocumentViewer/viewerFixtures")).loggingMock);
vi.mock("./pdfDocument", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./pdfDocument")>()),
  loadPdfjs: async () => (await import("./pdfFixtures")).pdfjsFake,
}));

beforeEach(() => {
  resetFixtures();
  resetPdfFixtures();
  resetViewStateForTest();
  pdfjsFake.getDocument.mockReset();
  state.pages = [page("One"), page("Two"), page("Three"), page("Four")];
  pdfReadReturns(pdfPayload());
});
afterEach(cleanup);

async function opened() {
  const view = render(<PdfViewer documentId={ID} />);
  await screen.findByRole("region", { name: "PDF page" });
  await waitFor(() => expect(screen.queryByText("Loading page…")).toBeNull());
  return view;
}

const field = () => screen.getByRole("textbox", { name: "Page number" });
const lastRender = () => state.renders[state.renders.length - 1];
const layerText = () => document.querySelector(".pdf-text-layer")!.textContent;

describe("PDV-FR-SAIG: page navigation", () => {
  it("PDV-FR-SAIG, PDV-FR-BEQL: opens on the first page, with the current page and the count shown", async () => {
    await opened();
    expect(field()).toHaveValue("1");
    expect(screen.getByText("of 4")).toBeInTheDocument();
    expect(layerText()).toBe("One");
  });

  it("PDV-FR-SAIG: Previous page is disabled on the first page and Next page on the last", async () => {
    const user = userEvent.setup();
    await opened();
    const previous = screen.getByRole("button", { name: "Previous page" });
    const next = screen.getByRole("button", { name: "Next page" });
    expect(previous).toBeDisabled();
    expect(next).toBeEnabled();
    await user.click(next);
    await user.click(next);
    await user.click(next);
    expect(field()).toHaveValue("4");
    expect(next).toBeDisabled();
    expect(previous).toBeEnabled();
    await waitFor(() => expect(layerText()).toBe("Four"));
    await user.click(previous);
    expect(field()).toHaveValue("3");
    expect(next).toBeEnabled();
  });

  it("PDV-FR-SAIG: Enter in the page field shows that page, clamped to the page range", async () => {
    const user = userEvent.setup();
    await opened();
    await user.clear(field());
    await user.type(field(), "3{Enter}");
    expect(field()).toHaveValue("3");
    await waitFor(() => expect(layerText()).toBe("Three"));
    await user.clear(field());
    await user.type(field(), "99{Enter}");
    expect(field()).toHaveValue("4");
    await user.clear(field());
    await user.type(field(), "0{Enter}");
    expect(field()).toHaveValue("1");
    await user.clear(field());
    await user.type(field(), "-5{Enter}");
    expect(field()).toHaveValue("1");
  });

  it("PDV-FR-SAIG: text that is not a number leaves the page as it is, and leaving the field restores the page number", async () => {
    const user = userEvent.setup();
    await opened();
    await user.click(screen.getByRole("button", { name: "Next page" }));
    await user.clear(field());
    await user.type(field(), "abc{Enter}");
    expect(field()).toHaveValue("2");
    await user.clear(field());
    await user.type(field(), "4");
    await user.tab();
    expect(field()).toHaveValue("2");
    await waitFor(() => expect(layerText()).toBe("Two"));
  });

  it("PDV-FR-SAIG: a one-page PDF disables both page buttons", async () => {
    state.pages = [page("Only")];
    await opened();
    expect(screen.getByRole("button", { name: "Previous page" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
    expect(screen.getByText("of 1")).toBeInTheDocument();
  });
});

describe("PDV-FR-BEQL: keyboard on the page region", () => {
  it("PDV-FR-BEQL: Page Down, Page Up, Home, and End move between pages", async () => {
    const user = userEvent.setup();
    await opened();
    const region = screen.getByRole("region", { name: "PDF page" });
    expect(region).toHaveAttribute("tabindex", "0");
    region.focus();
    await user.keyboard("{PageDown}");
    expect(field()).toHaveValue("2");
    await user.keyboard("{PageDown}{PageDown}");
    expect(field()).toHaveValue("4");
    // The last page has no next page: the key changes nothing.
    await user.keyboard("{PageDown}");
    expect(field()).toHaveValue("4");
    await user.keyboard("{PageUp}");
    expect(field()).toHaveValue("3");
    await user.keyboard("{Home}");
    expect(field()).toHaveValue("1");
    await user.keyboard("{PageUp}");
    expect(field()).toHaveValue("1");
    await user.keyboard("{End}");
    expect(field()).toHaveValue("4");
    await waitFor(() => expect(layerText()).toBe("Four"));
  });

  it("PDV-FR-BEQL: the current page is announced to assistive technology when it changes", async () => {
    const user = userEvent.setup();
    await opened();
    const announcement = screen.getByText("Page 1 of 4");
    expect(announcement).toHaveAttribute("aria-live", "polite");
    await user.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByText("Page 2 of 4")).toBeInTheDocument();
  });

  it("PDV-FR-BEQL: the page keys do nothing in the page field, which keeps its own keys", async () => {
    const user = userEvent.setup();
    await opened();
    field().focus();
    await user.keyboard("{Home}");
    expect(field()).toHaveValue("1");
    expect(screen.getByText("Page 1 of 4")).toBeInTheDocument();
  });
});

describe("PDV-FR-HQTN: zoom", () => {
  it("PDV-FR-HQTN: the steps are 25, 50, 75, 100, 125, 150, 200, 300, and 400 percent, and the default is 100", async () => {
    expect([...ZOOM_STEPS]).toEqual([25, 50, 75, 100, 125, 150, 200, 300, 400]);
    expect(ZOOM_STEPS[DEFAULT_ZOOM_INDEX]).toBe(100);
    const user = userEvent.setup();
    await opened();
    expect(screen.getByText("100%")).toBeInTheDocument();
    const seen: number[] = [];
    const zoomIn = screen.getByRole("button", { name: "Zoom in" });
    while (!(zoomIn as HTMLButtonElement).disabled) {
      await user.click(zoomIn);
      seen.push(Number.parseInt(document.querySelector(".pdf-zoom-label")!.textContent!, 10));
    }
    expect(seen).toEqual([125, 150, 200, 300, 400]);
    const zoomOut = screen.getByRole("button", { name: "Zoom out" });
    const down: number[] = [];
    while (!(zoomOut as HTMLButtonElement).disabled) {
      await user.click(zoomOut);
      down.push(Number.parseInt(document.querySelector(".pdf-zoom-label")!.textContent!, 10));
    }
    expect(down).toEqual([300, 200, 150, 125, 100, 75, 50, 25]);
  });

  it("PDV-FR-HQTN: a button is disabled at its limit", async () => {
    const user = userEvent.setup();
    await opened();
    const zoomOut = screen.getByRole("button", { name: "Zoom out" });
    const zoomIn = screen.getByRole("button", { name: "Zoom in" });
    for (let i = 0; i < 3; i += 1) await user.click(zoomOut);
    expect(screen.getByText("25%")).toBeInTheDocument();
    expect(zoomOut).toBeDisabled();
    expect(zoomIn).toBeEnabled();
    for (let i = 0; i < 8; i += 1) await user.click(zoomIn);
    expect(screen.getByText("400%")).toBeInTheDocument();
    expect(zoomIn).toBeDisabled();
    expect(zoomOut).toBeEnabled();
  });

  it("PDV-FR-HQTN: Reset zoom returns to 100 percent", async () => {
    const user = userEvent.setup();
    await opened();
    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(screen.getByText("150%")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Reset zoom" }));
    expect(screen.getByText("100%")).toBeInTheDocument();
  });

  it("PDV-FR-HQTN, PDV-FR-VKRL: a zoom change keeps the current page and renders it again at the new scale", async () => {
    const user = userEvent.setup();
    await opened();
    await user.click(screen.getByRole("button", { name: "Next page" }));
    await user.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => expect(lastRender().page).toBe(3));
    expect(lastRender().scale).toBeCloseTo(scaleOf(DEFAULT_ZOOM_INDEX));
    expect(lastRender().scale).toBeCloseTo(CSS_UNITS);
    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    await waitFor(() => expect(lastRender().scale).toBeCloseTo(1.25 * CSS_UNITS));
    expect(lastRender().page).toBe(3);
    expect(field()).toHaveValue("3");
    await waitFor(() => expect(layerText()).toBe("Three"));
  });

  it("PDV-FR-HQTN: pure helpers keep the zoom index and the page inside their ranges", () => {
    expect(validZoomIndex(3)).toBe(3);
    expect(validZoomIndex(-1)).toBe(DEFAULT_ZOOM_INDEX);
    expect(validZoomIndex(99)).toBe(DEFAULT_ZOOM_INDEX);
    expect(validZoomIndex(1.5)).toBe(DEFAULT_ZOOM_INDEX);
    expect(validZoomIndex("x")).toBe(DEFAULT_ZOOM_INDEX);
    expect(clampPage(0, 5)).toBe(1);
    expect(clampPage(9, 5)).toBe(5);
    expect(clampPage(Number.NaN, 5)).toBe(1);
    expect(clampPage(3.9, 5)).toBe(3);
  });
});

describe("PDV-FR-QNVD: keyboard and names", () => {
  it("PDV-FR-QNVD: every control is a native control with an accessible name", async () => {
    await opened();
    for (const name of [
      "Previous page",
      "Next page",
      "Zoom out",
      "Zoom in",
      "Reset zoom",
      "Previous match",
      "Next match",
    ]) {
      expect(screen.getByRole("button", { name })).toBeInTheDocument();
    }
    expect(screen.getByRole("textbox", { name: "Page number" })).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: "Search text" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "PDF page" })).toBeInTheDocument();
    const toolbar = screen.getByRole("toolbar", { name: "PDF viewer" });
    for (const control of toolbar.querySelectorAll("button, input")) {
      expect(control.getAttribute("aria-label") ?? control.textContent).toBeTruthy();
    }
    expect(toolbar.querySelectorAll("[tabindex]:not([tabindex='0'])")).toHaveLength(0);
  });

  it("PDV-FR-QNVD: the whole toolbar and the page region are reached with the Tab key in order", async () => {
    const user = userEvent.setup();
    state.pages = [page("One"), page("Two"), page("Three")];
    await opened();
    // Previous page is disabled on page 1, so the first stop is the page field.
    await user.tab();
    expect(field()).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: "Next page" })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: "Zoom out" })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: "Zoom in" })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: "Reset zoom" })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("searchbox", { name: "Search text" })).toHaveFocus();
    // The match buttons are disabled while there is no match.
    await user.tab();
    expect(screen.getByRole("region", { name: "PDF page" })).toHaveFocus();
  });

  it("PDV-FR-QNVD: a button operates with Enter and Space", async () => {
    const user = userEvent.setup();
    await opened();
    const next = screen.getByRole("button", { name: "Next page" });
    next.focus();
    await user.keyboard("{Enter}");
    expect(field()).toHaveValue("2");
    await user.keyboard(" ");
    expect(field()).toHaveValue("3");
  });

  it("PDV-FR-QNVD: the viewer carries its own focus styles and uses theme tokens, and the page keeps the PDF colours", async () => {
    const { sheet, blocksFor, decl } = await import("../../test/cssRules");
    const css = sheet("components.css");
    for (const selector of [".pdf-btn:focus-visible", ".pdf-page-region:focus-visible"]) {
      const blocks = blocksFor(css, selector);
      expect(blocks.length, selector).toBeGreaterThan(0);
      expect(decl(blocks[blocks.length - 1], "box-shadow"), selector).toContain("shadow-focus");
    }
    const toolbar = blocksFor(css, ".pdf-toolbar");
    expect(decl(toolbar[0], "background")).toBe("var(--bg-chrome)");
    const sheetBlock = blocksFor(css, ".pdf-page-sheet");
    expect(decl(sheetBlock[0], "background")).toBe("#fff");
  });
});
