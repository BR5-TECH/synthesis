import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { SpecMap } from ".";
import { INITIAL_VIEW } from "../../state/specMap/session";
import { buildTree } from "../../state/specMap/tree";
import type { SpecificationIndex } from "../../state/specMap/types";
import { layoutMap, scopeOf } from "./layout";
import { smallIndex } from "../../test/specMapFixtures";
import {
  canvas,
  makeStore,
  mockCanvasRect,
  nodeButton,
  renderMap,
  setView,
  setWindowWidth,
} from "../../test/specMapRender";

vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("the tab frame (SMP)", () => {
  it("SMP-FR-PMCX: the tab loads the index once, shows loading meanwhile, and a remount loads nothing", async () => {
    let resolve!: (index: SpecificationIndex) => void;
    const { store, ops } = makeStore(undefined, {
      load: vi.fn(() => new Promise<SpecificationIndex>((r) => (resolve = r))),
    });
    setWindowWidth(1440);
    const props = { onOpenArtifact: vi.fn(), onNewDraft: vi.fn(), onOpenDraft: vi.fn() };
    const first = render(<SpecMap store={store} {...props} />);
    expect(await screen.findByRole("status")).toHaveTextContent("Loading the specification map");
    await act(async () => resolve(smallIndex()));
    await screen.findByTestId("spec-map-canvas");
    first.unmount();
    render(<SpecMap store={store} {...props} />);
    await screen.findByTestId("spec-map-canvas");
    expect(ops.load).toHaveBeenCalledTimes(1);
  });

  it("SMP-FR-ONSD: a failed load shows the error with Retry, disables the toolbar, and Retry loads again", async () => {
    const { ops } = await renderMap({
      ops: { load: vi.fn<() => Promise<SpecificationIndex>>().mockRejectedValueOnce(new Error("x")).mockResolvedValue(smallIndex()) },
    });
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("The specification map could not be loaded.");
    expect(screen.queryByTestId("spec-map-canvas")).toBeNull();
    expect(screen.getByRole("button", { name: "Dependencies" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Gaps only" })).toBeDisabled();
    fireEvent.click(within(alert).getByRole("button", { name: "Retry" }));
    await screen.findByTestId("spec-map-canvas");
    expect(ops.load).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("button", { name: "Dependencies" })).toBeEnabled();
  });

  it("SMP-FR-LRAX: an index with no root nodes shows the empty state", async () => {
    await renderMap({ index: { ...smallIndex(), roots: [], dependencies: [] } });
    expect(screen.getByRole("status")).toHaveTextContent("No specifications are indexed.");
    expect(screen.queryByTestId("spec-map-canvas")).toBeNull();
  });

  it("SMP-FR-CVIB, SMZ-FR-VYOS, SMP-FR-QMRE: the toolbar holds its controls in order, with a pill per level name", async () => {
    mockCanvasRect({ width: 1440, height: 800 });
    const { container } = await renderMap();
    const toolbar = screen.getByRole("toolbar", { name: "Map controls" });
    const order = [...toolbar.children].map((el) =>
      [el.classList[0], el.getAttribute("data-filter")].filter(Boolean).join(":"),
    );
    expect(order).toEqual([
      "smap-levels",
      "smap-toolbar__divider",
      "smap-toolbar__label",
      "smap-bar",
      "smap-toolbar__counts",
      "smap-toolbar__spacer",
      "smap-filter:deps",
      "smap-filter:gaps",
    ]);
    const radios = screen.getAllByRole("radio");
    expect(radios.map((r) => r.textContent)).toEqual(["domains", "features", "groups", "specs"]);
    expect(radios[0]).toHaveAttribute("aria-checked", "true");
    expect(container.querySelector(".smap-levels__option[data-active]")).toHaveTextContent("domains");
  });

  it("SMP-FR-YDKM: the project bar and counts cover every spec node", async () => {
    mockCanvasRect({ width: 1440, height: 800 });
    const { container } = await renderMap();
    expect(screen.getByText("2 verified · 2 built · 1 drafted · 1 gaps")).toBeInTheDocument();
    const bar = container.querySelector('.smap-bar[data-variant="project"]')!;
    expect(bar).toHaveAttribute("aria-label", "2 verified, 2 built, 1 drafted, 1 gaps");
    const widths = [...bar.children].map((s) => (s as HTMLElement).style.width);
    expect(widths).toEqual([`${(2 / 6) * 100}%`, `${(2 / 6) * 100}%`, `${(1 / 6) * 100}%`, `${(1 / 6) * 100}%`]);
  });

  it("SMP-FR-GJEW: below a tab width of 1180px the label hides, the bar narrows and the counts shorten", async () => {
    // A wide window with a narrower tab: the tab's width is what counts.
    mockCanvasRect({ width: 1100, height: 800 });
    const { container } = await renderMap({ windowWidth: 1440 });
    expect(screen.queryByText("Completeness")).toBeNull();
    expect(container.querySelector('.smap-bar[data-variant="project"]')).toHaveAttribute("data-narrow", "true");
    expect(container.querySelector(".smap-toolbar__counts")).toHaveTextContent("33% verified · 1 gaps");
    expect(screen.getByRole("button", { name: "Dependencies" })).toHaveTextContent("dependencies");
  });

  it("SMP-FR-GJEW: below a tab width of 1000px the counts hide and the toggles keep only their icon and tooltip", async () => {
    mockCanvasRect({ width: 900, height: 800 });
    const { container } = await renderMap({ windowWidth: 1440 });
    expect(container.querySelector(".smap-toolbar__counts")).toBeNull();
    const deps = screen.getByRole("button", { name: "Dependencies" });
    expect(deps).toHaveTextContent("");
    expect(deps).toHaveAttribute("title", "Dependencies");
    expect(screen.getByRole("button", { name: "Gaps only" })).toHaveAttribute("title", "Gaps only");
  });
});

describe("the canvas (SMZ)", () => {
  it("SMZ-FR-NUOB: first paint frames level 0 at 0.8 from the measured canvas height", async () => {
    mockCanvasRect({ width: 500, height: 100 });
    await renderMap();
    const transform = screen.getByTestId("spec-map-stage").style.transform;
    const [, tx, ty, scale] = transform.match(/translate\(([-\d.]+)px, ([-\d.]+)px\) scale\(([-\d.]+)\)/)!;
    // One column of two 176px cards 30px apart: 382px of content.
    expect(Number(tx)).toBe(0);
    expect(Number(ty)).toBeCloseTo((382 / 2) * 0.8 - 50 + 26);
    expect(Number(scale)).toBe(0.8);
  });

  it("SMZ-FR-VYOS, SMZ-FR-EPTR, SMZ-FR-QOTA, SMZ-FR-RNIT: a chosen level renders that level, top-aligned or centred on the selection", async () => {
    const { store } = await renderMap();
    fireEvent.click(screen.getByRole("radio", { name: "groups" }));
    // SMZ-FR-QOTA: nothing is selected, so the level is framed top-aligned.
    const groups = layoutMap(scopeOf(buildTree(store.snapshot().index!), null), 2, 0);
    expect(store.snapshot().view).toEqual({
      level: 2,
      scale: 0.8,
      tx: 0,
      ty: Math.max(0, (groups.height / 2) * 0.8 + 26),
      touched: true,
    });
    expect(nodeButton("g1 label, group")).toHaveAttribute("data-shape", "collapsed");
    expect(screen.queryByRole("button", { name: "A1 a1 label" })).toBeNull();

    act(() => store.select({ kind: "spec", code: "A3" }));
    fireEvent.click(screen.getByRole("radio", { name: "specs" }));
    const chip = layoutMap(scopeOf(buildTree(store.snapshot().index!), null), 3, 0).chip.get("A3")!;
    expect(store.snapshot().view).toEqual({
      level: 3,
      scale: 0.85,
      tx: -(chip.x + chip.w / 2) * 0.85,
      ty: -(chip.y + chip.h / 2) * 0.85,
      touched: true,
    });
    expect(nodeButton("A1 a1 label")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("radio", { name: "domains" }));
    expect(store.snapshot().view).toEqual(INITIAL_VIEW);
  });

  it("SMZ-FR-ZQLP, SMZ-FR-GMEB: a wheel step over the canvas crosses the scale window into the next level", async () => {
    const { store } = await renderMap();
    setView(store, { level: 0, scale: 1.29, tx: 10 });
    fireEvent.wheel(canvas(), { deltaY: -120 });
    expect(store.snapshot().view).toMatchObject({ level: 1, scale: 0.66 });
    fireEvent.wheel(canvas(), { deltaY: 120 });
    expect(store.snapshot().view).toMatchObject({ level: 0, scale: 1.3 });
  });

  it("SMZ-FR-KAHU: a drag on empty canvas pans, and a drag that starts on a node does not", async () => {
    const { store } = await renderMap();
    setView(store, { level: 0, tx: 5, ty: 7 });
    fireEvent.pointerDown(canvas(), { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(canvas(), { pointerId: 1, clientX: 40, clientY: 30 });
    fireEvent.pointerUp(canvas(), { pointerId: 1 });
    expect(store.snapshot().view).toMatchObject({ tx: 35, ty: 27, touched: true });

    // Under the drag threshold, so this press is neither a pan nor a drag.
    fireEvent.pointerDown(nodeButton("d1 label, domain"), { button: 0, pointerId: 2, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(canvas(), { pointerId: 2, clientX: 12, clientY: 11 });
    fireEvent.pointerUp(window, { pointerId: 2 });
    expect(store.snapshot().view).toMatchObject({ tx: 35, ty: 27 });
    expect(store.snapshot().index!.roots.map((r) => r.id)).toEqual(["d1", "d2"]);
  });

  it("SMZ-FR-JWXA: plus and minus zoom by 1.18, and the percentage resets to first paint", async () => {
    const { store } = await renderMap();
    setView(store, { level: 1, scale: 1 });
    fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(store.snapshot().view.scale).toBeCloseTo(1.18);
    expect(screen.getByRole("button", { name: "Reset view" })).toHaveTextContent("118%");
    fireEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    expect(store.snapshot().view.scale).toBeCloseTo(1);
    fireEvent.click(screen.getByRole("button", { name: "Reset view" }));
    expect(store.snapshot().view).toEqual(INITIAL_VIEW);
    expect(screen.getByRole("button", { name: "Reset view" })).toHaveTextContent("80%");
  });

  it("SMZ-FR-AILK: the legend names the current level and the four states", async () => {
    const { store, container } = await renderMap();
    const legend = container.querySelector(".smap-legend") as HTMLElement;
    expect(legend).toHaveTextContent("domainsverifiedbuiltdraftedgap");
    setView(store, { level: 2 });
    expect(legend).toHaveTextContent(/^groups/);
    expect(legend.querySelectorAll(".smap-dot")).toHaveLength(4);
  });

  it("SMZ-FR-FOXU, SMZ-FR-BRCT, SMZ-FR-LWEQ: a focused node is the only root, with a breadcrumb back out", async () => {
    const { store, container } = await renderMap();
    act(() => store.select({ kind: "index", id: "f1" }));
    setView(store, { level: 2, scale: 1.1, tx: 40, ty: 40 });
    fireEvent.click(screen.getByRole("button", { name: "Open feature" }));

    expect(store.snapshot().focusId).toBe("f1");
    expect(store.snapshot().view).toEqual(INITIAL_VIEW);
    expect(container.querySelector(".smap-legend__level")).toHaveTextContent("features");
    expect(nodeButton("f1 label, feature")).toHaveAttribute("data-shape", "overview");
    expect(screen.queryByRole("button", { name: "d2 label, domain" })).toBeNull();
    expect(screen.getAllByRole("radio").map((r) => r.textContent)).toEqual(["features", "groups", "specs"]);
    const crumbs = screen.getByRole("navigation", { name: "Focused subtree" });
    expect(crumbs).toHaveTextContent("specifications→d1 label→f1 label");

    setView(store, { level: 1, scale: 1.2 });
    fireEvent.click(within(crumbs).getByRole("button", { name: "d1 label" }));
    expect(store.snapshot().focusId).toBe("d1");
    expect(store.snapshot().view).toEqual(INITIAL_VIEW);

    setView(store, { level: 1, scale: 1.2 });
    fireEvent.click(within(screen.getByRole("navigation", { name: "Focused subtree" })).getByRole("button", { name: "specifications" }));
    expect(store.snapshot().focusId).toBeNull();
    expect(store.snapshot().view).toEqual(INITIAL_VIEW);
    expect(nodeButton("d2 label, domain")).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Focused subtree" })).toBeNull();
    expect(store.snapshot().selection).toEqual({ kind: "index", id: "f1" });
  });
});

describe("nodes (SMN)", () => {
  it("SMN-FR-XCOK, SMN-FR-QHVM: level 0 shows root cards with meta, summary, bar and counts", async () => {
    await renderMap();
    const d1 = nodeButton("d1 label, domain");
    expect(d1).toHaveAttribute("data-shape", "overview");
    expect(d1).toHaveTextContent("4 specs · 2 features");
    expect(d1).toHaveTextContent("d1 summary.");
    for (const text of ["1 verified", "1 built", "1 drafted", "1 gaps"]) expect(d1).toHaveTextContent(text);
    expect(within(d1).getByRole("img")).toHaveAttribute("aria-label", "1 verified, 1 built, 1 drafted, 1 gaps");
    // SMN-FR-QHVM: a gaps count of zero is hidden.
    expect(nodeButton("d2 label, domain")).not.toHaveTextContent("gaps");
  });

  it("SMN-FR-EYRV, SMN-FR-KTZB: a click selects a node, and hovering marks the root that contains it", async () => {
    const { store, container } = await renderMap();
    setView(store, { level: 1 });
    fireEvent.click(nodeButton("f1 label, feature"));
    expect(store.snapshot().selection).toEqual({ kind: "index", id: "f1" });
    expect(nodeButton("f1 label, feature")).toHaveAttribute("data-selected", "true");
    expect(nodeButton("d1 label, domain")).toHaveAttribute("data-context", "true");

    fireEvent.mouseOver(nodeButton("f3 label, feature"));
    expect(nodeButton("d2 label, domain")).toHaveAttribute("data-context", "true");
    expect(nodeButton("d1 label, domain")).not.toHaveAttribute("data-context");

    fireEvent.mouseLeave(canvas());
    expect(nodeButton("d1 label, domain")).toHaveAttribute("data-context", "true");
    expect(container.querySelector('[data-context][data-selected]')).toBeNull();
  });

  it("Enter on a focused node selects it, as a click does", async () => {
    const { store } = await renderMap();
    fireEvent.keyDown(nodeButton("d2 label, domain"), { key: "Enter" });
    expect(store.snapshot().selection).toEqual({ kind: "index", id: "d2" });
  });

  it("SMN-FR-HLDQ: gaps only dims nodes with no gap or drafted spec, and chips that are not gaps", async () => {
    const { store } = await renderMap();
    fireEvent.click(screen.getByRole("button", { name: "Gaps only" }));
    expect(screen.getByRole("button", { name: "Gaps only" })).toHaveAttribute("aria-pressed", "true");
    expect(nodeButton("d2 label, domain")).toHaveAttribute("data-dim", "gaps");
    expect(nodeButton("d1 label, domain")).not.toHaveAttribute("data-dim");
    setView(store, { level: 3 });
    expect(nodeButton("A1 a1 label")).toHaveAttribute("data-dim", "gaps");
    expect(nodeButton("A2 a2 label")).not.toHaveAttribute("data-dim");
  });

  it("SMN-FR-ISBE, SMN-FR-WAGD: a spec chip carries its folder's badge, its label and its state dot", async () => {
    const { store } = await renderMap();
    setView(store, { level: 3 });
    const chip = nodeButton("A2 a2 label");
    expect(chip.querySelector(".smap-code")).toHaveAttribute("data-folder", "core");
    expect(chip.querySelector(".smap-code")).toHaveTextContent("A2");
    expect(chip.querySelector(".smap-dot")).toHaveAttribute("data-state", "gap");
  });

  it("SMN-FR-MVWA, SMN-FR-FXRL, SMN-FR-YSKQ: hovering shows the card for the node, and leaving hides it", async () => {
    const { store } = await renderMap();
    fireEvent.mouseOver(nodeButton("d1 label, domain"));
    const card = screen.getByRole("tooltip");
    expect(card).toHaveTextContent("4 specs");
    expect(card).toHaveTextContent("28 reqs · 14 scenarios · 1 gaps");

    setView(store, { level: 3 });
    fireEvent.mouseOver(nodeButton("A1 a1 label"));
    expect(screen.getByRole("tooltip")).toHaveTextContent("10 reqs · 5 scenarios · cites 1");
    expect(screen.getByRole("tooltip")).toHaveTextContent("A1 summary.");

    fireEvent.mouseLeave(canvas());
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("SMN-FR-YSKQ: no hover card shows while the canvas pans", async () => {
    await renderMap();
    fireEvent.mouseOver(nodeButton("d1 label, domain"));
    fireEvent.pointerDown(canvas(), { button: 0, pointerId: 1, clientX: 0, clientY: 0 });
    fireEvent.mouseOver(nodeButton("d1 label, domain"));
    expect(screen.queryByRole("tooltip")).toBeNull();
    fireEvent.pointerUp(canvas(), { pointerId: 1 });
  });

  it("SMD-FR-ULTF, SMD-FR-CGNW: a planned draft counts on a collapsed node and shows as a chip when expanded", async () => {
    const { store } = await renderMap();
    act(() => {
      store.dispatch({ kind: "attachDraft", nodeId: "g1", draft: { draftId: "dr", name: "Untitled" } });
    });
    setView(store, { level: 1 });
    expect(nodeButton("f1 label, feature")).toHaveTextContent("3 specs · +1 planned");
    setView(store, { level: 3 });
    const planned = nodeButton("draft Untitled");
    expect(planned.querySelector(".smap-code--draft")).toHaveTextContent("draft");
  });
});

describe("edges (SME)", () => {
  const resolved = (root: HTMLElement) =>
    root.querySelectorAll('path.smap-edge:not([data-weight="unresolved"])').length;
  const unresolved = (root: HTMLElement) =>
    root.querySelectorAll('path.smap-edge[data-weight="unresolved"]').length;

  it("SME-FR-OKUH, SME-FR-FKIW: the dependencies toggle removes every resolved edge and keeps unresolved citations", async () => {
    const { container } = await renderMap();
    // One resolved link between the roots; the unresolved citation draws only dashed.
    expect(resolved(container)).toBe(1);
    expect(unresolved(container)).toBe(1);
    const toggle = screen.getByRole("button", { name: "Dependencies" });
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(resolved(container)).toBe(0);
    expect(unresolved(container)).toBe(1);
  });

  it("SME-FR-TJMY, SME-FR-DZHE: an edge touching the active node draws in the active class with its own arrowhead", async () => {
    const { container } = await renderMap();
    expect(container.querySelectorAll('path.smap-edge[data-weight="active"]')).toHaveLength(0);
    fireEvent.mouseOver(nodeButton("d2 label, domain"));
    const active = container.querySelectorAll('path.smap-edge[data-weight="active"]');
    expect(active).toHaveLength(1);
    expect(active[0].getAttribute("marker-end")).toMatch(/^url\(#smap-.*-hot\)$/);
    expect(active[0].getAttribute("d")).toMatch(/^M[-\d.]+ [-\d.]+Q/);
  });

  it("SME-FR-WRPX: at the last level a selected spec dims the chips it has no dependency with", async () => {
    const { store, container } = await renderMap();
    setView(store, { level: 3 });
    fireEvent.click(nodeButton("A1 a1 label"));
    expect(nodeButton("B1 b1 label")).toHaveAttribute("data-dim", "related");
    expect(nodeButton("C1 c1 label")).not.toHaveAttribute("data-dim");
    expect(nodeButton("A1 a1 label")).not.toHaveAttribute("data-dim");
    expect(container.querySelectorAll('path.smap-edge[data-weight="active"]')).toHaveLength(1);
  });
});

describe("level names, framing and render rules", () => {
  it("SMP-FR-QMRE: every level name on the surface comes from the index", async () => {
    const renamed = {
      ...smallIndex(),
      levels: [
        { plural: "areas", singular: "area" },
        { plural: "themes", singular: "theme" },
        { plural: "sets", singular: "set" },
        { plural: "documents", singular: "document" },
      ],
    };
    const { store, container } = await renderMap({ index: renamed });
    expect(screen.getAllByRole("radio").map((r) => r.textContent)).toEqual(["areas", "themes", "sets", "documents"]);
    expect(container.querySelector(".smap-legend__level")).toHaveTextContent("areas");
    expect(nodeButton("d1 label, area")).toHaveTextContent("4 specs · 2 themes");
    const pane = screen.getByRole("complementary", { name: "Inspector" });
    expect(within(pane).getByRole("button", { name: "New area" })).toBeInTheDocument();
    act(() => store.select({ kind: "index", id: "f1" }));
    expect(container.querySelector(".smap-inspector__kind")).toHaveTextContent("theme");
    expect(within(pane).getByText("Sets")).toBeInTheDocument();
    expect(within(pane).getByRole("button", { name: "Open theme" })).toBeInTheDocument();
    expect(within(pane).getByRole("button", { name: "New set" })).toBeInTheDocument();
    fireEvent.click(within(pane).getByRole("button", { name: "Rename" }));
    expect(screen.getByRole("dialog")).toHaveAccessibleName("Rename theme");
    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.click(within(pane).getByRole("button", { name: "Delete" }));
    expect(screen.getByRole("dialog")).toHaveAccessibleName("Delete theme");
  });

  it("SMZ-FR-NUOB: first paint follows each new measured height, and stops once the author pans", async () => {
    mockCanvasRect({ width: 500, height: 100 });
    const { store } = await renderMap();
    const ty = () =>
      Number(screen.getByTestId("spec-map-stage").style.transform.match(/translate\([-\d.]+px, ([-\d.]+)px\)/)![1]);
    expect(ty()).toBeCloseTo((382 / 2) * 0.8 - 50 + 26);

    mockCanvasRect({ width: 500, height: 300 });
    act(() => {
      window.dispatchEvent(new Event("resize"));
    });
    expect(ty()).toBeCloseTo((382 / 2) * 0.8 - 150 + 26);

    setView(store, { level: 0, scale: 0.8, tx: 0, ty: 5 });
    mockCanvasRect({ width: 500, height: 100 });
    act(() => {
      window.dispatchEvent(new Event("resize"));
    });
    expect(ty()).toBe(5);
  });

  it("SMN-FR-JEQO: an expanded root node hides its summary and its counts", async () => {
    const { store } = await renderMap();
    setView(store, { level: 1 });
    const d1 = nodeButton("d1 label, domain");
    expect(d1).toHaveAttribute("data-shape", "expanded");
    expect(d1.querySelector(".smap-root__summary")).toBeNull();
    expect(d1.querySelector(".smap-counts")).toBeNull();
    expect(d1).toHaveTextContent("4 specs · 2 features");
  });

  it("SMN-FR-HLDQ: the toggle starts off; a node with only drafted specs stays, one with only built specs dims", async () => {
    const { store } = await renderMap();
    expect(screen.getByRole("button", { name: "Gaps only" })).toHaveAttribute("aria-pressed", "false");
    setView(store, { level: 2 });
    fireEvent.click(screen.getByRole("button", { name: "Gaps only" }));
    expect(nodeButton("g3 label, group")).not.toHaveAttribute("data-dim");
    expect(nodeButton("g2 label, group")).toHaveAttribute("data-dim", "gaps");
  });
});
