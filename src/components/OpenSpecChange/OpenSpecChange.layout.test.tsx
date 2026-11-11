/**
 * The narrow-window layout and the Rich images of a change
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-GCME,
 * OCW-FR-FMDZ).
 *
 * jsdom lays nothing out, so the width the list collapses at is given to the
 * workspace through a stand-in `ResizeObserver`. What only a browser can show
 * is the real width of each column at 800 px with a side panel open, the
 * discussion column giving up width first, and the wrap of a path and of the
 * Source text.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { changeGridTemplate } from "./ChangeDiscussion";
import { collapsesAt, railStacksAt } from "./ArtifactNav";
import { insideChangeFolder } from "./ChangeMarkdown";
import {
  changeBackend,
  drawChange,
  resetChangeWorkspaceStores,
  wireChangeBackend,
  type ChangeBackend,
} from "../../test/changeWorkspaceFixtures";

/** A stand-in that lets a test say how wide each observed element is. */
class FakeResizeObserver {
  static all: FakeResizeObserver[] = [];
  elements: Element[] = [];
  constructor(private readonly callback: ResizeObserverCallback) {
    FakeResizeObserver.all.push(this);
  }
  observe(element: Element) {
    this.elements.push(element);
  }
  unobserve() {}
  disconnect() {
    this.elements = [];
  }
  /** Report a width for each element this observer watches that the test names. */
  resize(width: number, which: (element: Element) => boolean = () => true) {
    const entries = this.elements
      .filter(which)
      .map((target) => ({ target, contentRect: { width, height: 600 } }) as unknown as ResizeObserverEntry);
    if (entries.length > 0) this.callback(entries, this as unknown as ResizeObserver);
  }
}

function resizeAll(width: number) {
  act(() => {
    for (const observer of FakeResizeObserver.all) observer.resize(width);
  });
}

/** Report a width for the stage alone, and another for the body of the list. */
function resizeStage(stage: number, body: number) {
  act(() => {
    for (const observer of FakeResizeObserver.all) {
      observer.resize(stage, (e) => e.classList.contains("openspec-change__stage"));
      observer.resize(body, (e) => e.classList.contains("openspec-change__body"));
    }
  });
}

/** The flex factors of a grid template, in order. */
function flexFactors(template: string): number[] {
  return [...template.matchAll(/([0-9.]+)fr\b/g)].map((m) => Number(m[1]));
}

let backend: ChangeBackend;

beforeEach(() => {
  resetChangeWorkspaceStores();
  invokeMock.mockReset();
  FakeResizeObserver.all = [];
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  backend = changeBackend({ proposals: [], discussions: [] });
  wireChangeBackend(invokeMock, backend);
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  expect(backend.unknown).toEqual([]);
});

describe("the narrow window (OCW-FR-GCME)", () => {
  it("OCW-FR-GCME: the list collapses where the reading pane beside it would be narrower than 360 px", () => {
    // The list is 200 px wide, so the body needs 560 px for both.
    expect(collapsesAt(560)).toBe(false);
    expect(collapsesAt(559)).toBe(true);
    expect(collapsesAt(1200)).toBe(false);
    // A body that is not measured yet keeps the list.
    expect(collapsesAt(0)).toBe(false);
  });

  it("OCW-FR-GCME: below that width the artifact list is a menu above the reading pane, and the menu selects the artifact", async () => {
    const user = userEvent.setup();
    drawChange();
    await screen.findAllByTestId("openspec-artifact");
    resizeAll(520);
    const menu = await screen.findByTestId("openspec-artifact-menu");
    expect(screen.queryByRole("navigation", { name: "Artifacts" })).toBeNull();
    const reading = screen.getByTestId("openspec-reading");
    expect(menu.compareDocumentPosition(reading) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const select = within(menu).getByRole("combobox", { name: "Artifacts" });
    expect(within(select).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "proposal · done",
      "specs · done",
      "design · ready",
      "tasks · blocked",
    ]);
    await user.selectOptions(select, "tasks");
    expect(await screen.findByTestId("openspec-no-file")).toHaveTextContent("The change holds no file for tasks.");

    // Wide again, the list comes back with the same selection.
    resizeAll(900);
    await waitFor(() => expect(screen.getByRole("navigation", { name: "Artifacts" })).toBeInTheDocument());
    expect(screen.queryByTestId("openspec-artifact-menu")).toBeNull();
    const tasks = screen.getAllByTestId("openspec-artifact").find((b) => b.textContent?.startsWith("tasks"))!;
    expect(tasks).toHaveAttribute("aria-current", "true");
  });

  it("OCW-FR-GCME: the menu names the undecided hunks of an artifact", async () => {
    backend = changeBackend({ discussions: [] });
    wireChangeBackend(invokeMock, backend);
    drawChange();
    await screen.findByTestId("openspec-review-bar");
    resizeAll(400);
    const select = within(await screen.findByTestId("openspec-artifact-menu")).getByRole("combobox");
    expect(within(select).getAllByRole("option")[0]).toHaveTextContent("proposal · done · 1 undecided change");
  });

  it("OCW-FR-GCME: the discussion column gives up width before the reading pane, down to its own minimum", () => {
    const template = changeGridTemplate("even");
    expect(template).toMatch(/^minmax\(min\(var\(--ocw-doc-min\), calc\(100% - var\(--ocw-discussion-min\) - var\(--dds-splitter-w\)\)\), 50fr\)/);
    expect(template).toMatch(/minmax\(var\(--ocw-discussion-min\), 50fr\)$/);
  });

  it("OCW-FR-GCME: no flex factor of the grid is less than 1, so a column at its minimum leaves the whole rest of the width to the other", () => {
    // A factor less than 1 takes only that part of the free space, so a band
    // of the width stays empty when the other column stops at its minimum.
    for (const ratio of ["even", "doc70", "chat70", 0.25, 0.75, 1 / 3, 0.123, 2, Number.NaN] as const) {
      const factors = flexFactors(changeGridTemplate(ratio));
      expect(factors, String(ratio)).toHaveLength(2);
      for (const factor of factors) expect(factor, String(ratio)).toBeGreaterThanOrEqual(1);
      // The proportion of the two is the author's.
      expect(factors[0] + factors[1], String(ratio)).toBeCloseTo(100, 6);
    }
    expect(flexFactors(changeGridTemplate("doc70"))).toEqual([70, 30]);
    expect(flexFactors(changeGridTemplate("chat70"))).toEqual([30, 70]);
    expect(flexFactors(changeGridTemplate(1 / 3))).toEqual([33.33, 66.67]);
    // A drag past the limits is clamped, and a ratio that is not a number is even.
    expect(flexFactors(changeGridTemplate(0.123))).toEqual([25, 75]);
    expect(flexFactors(changeGridTemplate(2))).toEqual([75, 25]);
    expect(flexFactors(changeGridTemplate(Number.NaN))).toEqual([50, 50]);
  });

  it("OCW-FR-GCME: the History rail moves above the document where the reading pane beside it would be narrower than 360 px", () => {
    // The rail is 220 px wide, so the stage needs 580 px for both.
    expect(railStacksAt(580)).toBe(false);
    expect(railStacksAt(579)).toBe(true);
    expect(railStacksAt(300)).toBe(true);
    expect(railStacksAt(1200)).toBe(false);
    // A stage that is not measured yet keeps the rail beside the document.
    expect(railStacksAt(0)).toBe(false);
  });

  it("OCW-FR-GCME, OCW-FR-RKCC: the open History rail stands beside the document on a wide stage, above it on a narrow one, and beside it again when the stage is wide", async () => {
    const user = userEvent.setup();
    drawChange();
    await screen.findAllByTestId("openspec-artifact");
    const stage = screen.getByTestId("openspec-change-stage");
    const split = screen.getByTestId("openspec-change-split");
    expect(stage).not.toHaveAttribute("data-rail");
    expect(split).not.toHaveAttribute("data-history");
    await user.click(screen.getByRole("button", { name: "History" }));
    const rail = await screen.findByTestId("openspec-history");
    expect(stage).toHaveAttribute("data-rail", "beside");
    // The document column keeps the width of the rail too, so the discussion
    // column gives up width first (`openspec-change.css`).
    expect(split).toHaveAttribute("data-history", "open");

    // 700 px: the rail stays beside, and the list beside it is a menu.
    resizeStage(700, 480);
    expect(stage).toHaveAttribute("data-rail", "beside");
    expect(await screen.findByTestId("openspec-artifact-menu")).toBeInTheDocument();

    // 500 px: the rail moves above the document, which takes the whole width.
    resizeStage(500, 500);
    await waitFor(() => expect(stage).toHaveAttribute("data-rail", "above"));
    expect((await within(rail).findAllByTestId("openspec-version")).length).toBeGreaterThan(0);

    resizeStage(1000, 780);
    await waitFor(() => expect(stage).toHaveAttribute("data-rail", "beside"));
    expect(screen.getByRole("navigation", { name: "Artifacts" })).toBeInTheDocument();

    // Closed, the stage and the grid state no rail.
    await user.click(screen.getByRole("button", { name: "History" }));
    await waitFor(() => expect(screen.queryByTestId("openspec-history")).toBeNull());
    expect(stage).not.toHaveAttribute("data-rail");
    expect(split).not.toHaveAttribute("data-history");
  });

  it("OCW-FR-GCME, OCW-FR-RKCC: History opened on a stage that is already narrow shows the rail above the document at once", async () => {
    const user = userEvent.setup();
    drawChange();
    await screen.findAllByTestId("openspec-artifact");
    const stage = screen.getByTestId("openspec-change-stage");
    resizeStage(500, 500);
    expect(stage).not.toHaveAttribute("data-rail");
    await user.click(screen.getByRole("button", { name: "History" }));
    await screen.findByTestId("openspec-history");
    expect(stage).toHaveAttribute("data-rail", "above");
  });
});

describe("Rich images (OCW-FR-FMDZ)", () => {
  it("OCW-FR-FMDZ: an image that is not inside the change folder renders its alt text and its destination as a link that is not fetched", async () => {
    backend.detail.artifacts[0].files[0].text = "# Why\n\n![remote](https://example.com/x.png)\n";
    drawChange({ file: "proposal.md" });
    const image = await screen.findByTestId("openspec-image");
    expect(within(image).getByRole("img", { name: "remote" })).toHaveTextContent("remote");
    const link = within(image).getByText("https://example.com/x.png");
    expect(link).not.toHaveAttribute("href");
    expect(link.closest("a")).toBeNull();
    expect(image).not.toHaveAttribute("data-inside");
    expect(document.querySelector(".openspec-file img")).toBeNull();
  });

  it("OCW-FR-FMDZ, OSM-FR-BLEY: an image inside the change folder renders the bytes that \"read change image\" returns for its change-relative path", async () => {
    backend.images["images/entry.png"] = { data: "iVBORw==", mediaType: "image/png" };
    backend.detail.artifacts[0].files[0].text = "# Why\n\n![The entry](./images/entry.png)\n";
    drawChange({ file: "proposal.md" });
    const shown = await screen.findByTestId("openspec-image-shown");
    expect(shown).toHaveAttribute("src", "data:image/png;base64,iVBORw==");
    expect(shown).toHaveAttribute("alt", "The entry");
    expect(backend.imageReads).toEqual(["images/entry.png"]);
  });

  it("OCW-FR-FMDZ: an image inside the change folder whose read refuses renders its alt text and its destination, marked as inside", async () => {
    backend.detail.artifacts[0].files[0].text = "# Why\n\n![The entry](./images/entry.png)\n";
    drawChange({ file: "proposal.md" });
    await waitFor(() => expect(backend.imageReads).toEqual(["images/entry.png"]));
    const image = await screen.findByTestId("openspec-image");
    expect(image).toHaveAttribute("data-inside", "true");
    expect(image).toHaveTextContent("The entry");
    expect(image).toHaveTextContent("./images/entry.png");
    expect(screen.queryByTestId("openspec-image-shown")).toBeNull();
  });

  it("OCW-FR-FMDZ: a destination is inside the change folder only where it is relative and stays in it", () => {
    expect(insideChangeFolder("images/a.png", "")).toBe(true);
    expect(insideChangeFolder("./a.png", "")).toBe(true);
    expect(insideChangeFolder("../a.png", "specs/auth")).toBe(true);
    expect(insideChangeFolder("a.png?x=1#y", "")).toBe(true);
    expect(insideChangeFolder("../a.png", "")).toBe(false);
    expect(insideChangeFolder("../../../a.png", "specs/auth")).toBe(false);
    expect(insideChangeFolder("/abs/a.png", "")).toBe(false);
    expect(insideChangeFolder("https://example.com/a.png", "")).toBe(false);
    expect(insideChangeFolder("data:image/png;base64,AAAA", "")).toBe(false);
    expect(insideChangeFolder("C:/a.png", "")).toBe(false);
    expect(insideChangeFolder("", "")).toBe(false);
    expect(insideChangeFolder(".", "")).toBe(false);
  });
});
