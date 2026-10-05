import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import { typeChip } from "../../artifactTypes";
import { nodeButton, renderMap, setView } from "../../test/specMapRender";
import { node, smallIndex, spec } from "../../test/specMapFixtures";

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

const inspector = () => screen.getByRole("complementary", { name: "Inspector" });

describe("placement (SMI-FR-QWOP, SMI-FR-BNHI)", () => {
  it("SMI-FR-QWOP: from 1160px of window the inspector docks and is open by default", async () => {
    await renderMap({ windowWidth: 1160 });
    expect(inspector()).not.toHaveAttribute("data-overlay");
    expect(screen.queryByRole("button", { name: "inspector" })).toBeNull();
  });

  it("SMI-FR-QWOP, SMI-FR-BNHI: below 1160px it starts closed, and its button opens it as an overlay", async () => {
    const { store } = await renderMap({ windowWidth: 1159 });
    expect(screen.queryByRole("complementary", { name: "Inspector" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "inspector" }));
    expect(inspector()).toHaveAttribute("data-overlay", "true");
    expect(store.snapshot().inspectorOpen).toBe(true);
    // The zoom controls move clear of the overlay.
    expect(screen.getByTestId("spec-map-canvas")).toHaveAttribute("data-inspector-overlay", "true");
  });

  it("SMI-FR-BNHI: the close button hides the inspector and the canvas button brings it back", async () => {
    await renderMap();
    fireEvent.click(within(inspector()).getByRole("button", { name: "Hide inspector" }));
    expect(screen.queryByRole("complementary", { name: "Inspector" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "inspector" }));
    expect(inspector()).toBeInTheDocument();
  });
});

describe("content (SMI)", () => {
  it("SMI-FR-KDGX, SMI-FR-TRUZ: with no selection the body asks for one and the header names no kind", async () => {
    const { container } = await renderMap();
    expect(inspector()).toHaveTextContent("Select a node to inspect it.");
    expect(container.querySelector(".smap-inspector__kind")).toHaveTextContent("");
    expect(within(inspector()).getByRole("button", { name: "New domain" })).toBeInTheDocument();
  });

  it("SMI-FR-TRUZ, SMI-FR-MFSA: a leaf group shows its breadcrumb, badge, summary, counts and totals", async () => {
    const { store, container } = await renderMap();
    act(() => store.select({ kind: "index", id: "g1" }));
    const pane = inspector();
    expect(container.querySelector(".smap-inspector__kind")).toHaveTextContent("group");
    expect(container.querySelector(".smap-inspector__crumbs")).toHaveTextContent("d1 label → f1 label");
    expect(container.querySelector(".smap-inspector__heading")).toHaveTextContent("2 specsg1 label");
    expect(within(pane).getByText("g1 summary.")).toBeInTheDocument();
    expect(container.querySelector(".smap-inspector__grid")).toHaveTextContent("1 verified0 built0 drafted1 gaps");
    expect(pane).toHaveTextContent("14 requirements · 7 scenarios");
  });

  it("SMI-FR-MFSA: a root node's breadcrumb is the corpus itself", async () => {
    const { store, container } = await renderMap();
    act(() => store.select({ kind: "index", id: "d2" }));
    expect(container.querySelector(".smap-inspector__crumbs")).toHaveTextContent("specifications");
  });

  it("SMI-FR-CEVL: an index node lists its children under the next level's name, and a row selects its child", async () => {
    const { store } = await renderMap();
    act(() => store.select({ kind: "index", id: "d1" }));
    expect(within(inspector()).getByText("Features")).toBeInTheDocument();
    const row = within(inspector()).getByRole("button", { name: /f2 label/ });
    expect(row).toHaveTextContent("1");
    fireEvent.click(row);
    expect(store.snapshot().selection).toEqual({ kind: "index", id: "f2" });
  });

  it("SMI-FR-CEVL: a spec node's one child row is its path", async () => {
    const { store } = await renderMap();
    act(() => store.select({ kind: "spec", code: "A1" }));
    expect(within(inspector()).getByText("Path")).toBeInTheDocument();
    expect(within(inspector()).getByText("specifications/ui/A1-a1.md")).toBeInTheDocument();
  });

  it("SMI-FR-YHNT, SMI-FR-PNGA: a spec's outgoing dependencies list most citations first, and a resolved row selects", async () => {
    const { store } = await renderMap();
    act(() => store.select({ kind: "spec", code: "A1" }));
    const row = within(inspector()).getByRole("button", { name: /c1 label/ });
    expect(row).toHaveTextContent("C1c1 label5");
    fireEvent.click(row);
    expect(store.snapshot().selection).toEqual({ kind: "spec", code: "C1" });
  });

  it("SMI-FR-YHNT, SMI-FR-PNGA, SMI-FR-ZLOA: an unresolved dependency is a danger row that selects nothing, and needs attention", async () => {
    const { store, container } = await renderMap();
    act(() => store.select({ kind: "spec", code: "C2" }));
    const danger = container.querySelector(".smap-rows .smap-row[data-danger]")!;
    expect(danger).toHaveTextContent("A1a1 label1");
    fireEvent.click(danger);
    expect(store.snapshot().selection).toEqual({ kind: "spec", code: "C2" });
    expect(within(inspector()).getByRole("note")).toHaveTextContent("cites A1 — identifier resolves to nothing");
  });

  it("SMI-FR-YHNT: an index node lists its outgoing links at its own depth", async () => {
    const { store } = await renderMap();
    act(() => store.select({ kind: "index", id: "d1" }));
    const link = within(inspector()).getByRole("button", { name: /d2 label/ });
    expect(link).toHaveTextContent("2d2 label5");
    fireEvent.click(link);
    expect(store.snapshot().selection).toEqual({ kind: "index", id: "d2" });
  });

  it("SMI-FR-ZLOA: an index node lists up to four gap specs beneath it, and a node with none shows no card", async () => {
    const { store } = await renderMap();
    act(() => store.select({ kind: "index", id: "d1" }));
    expect(within(inspector()).getByRole("note")).toHaveTextContent("A2 — a2 label: no coverage yet");
    act(() => store.select({ kind: "index", id: "d2" }));
    expect(within(inspector()).queryByRole("note")).toBeNull();
  });
});

describe("actions (SMI)", () => {
  it("SMI-FR-GAJD: Open in editor opens the spec file through the shell's open route", async () => {
    const { store, onOpenArtifact } = await renderMap();
    act(() => store.select({ kind: "spec", code: "A2" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "Open in editor" }));
    expect(onOpenArtifact).toHaveBeenCalledWith({
      id: "specifications/core/A2-a2.md",
      name: "A2-a2.md",
      artifactType: "spec",
      chip: typeChip("spec"),
    });
  });

  it("SMI-FR-EUXP: an index node's primary action names its level and focuses it", async () => {
    const { store } = await renderMap();
    act(() => store.select({ kind: "index", id: "g4" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "Open group" }));
    expect(store.snapshot().focusId).toBe("g4");
  });

  it("SMI-FR-RPCO: Zoom to shows an index node at its own depth, and a spec at the last level", async () => {
    const { store } = await renderMap();
    act(() => store.select({ kind: "index", id: "g4" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "Zoom to" }));
    expect(store.snapshot().view).toMatchObject({ level: 2, scale: 0.8, touched: true });
    expect(store.snapshot().zoom).toBeNull();

    act(() => store.select({ kind: "spec", code: "B1" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "Zoom to" }));
    expect(store.snapshot().view).toMatchObject({ level: 3, scale: 0.85 });
    expect(nodeButton("B1 b1 label")).toBeInTheDocument();
  });

  it("SMI-FR-RPCO: a node outside the focused subtree is reached by ending the focus", async () => {
    const { store } = await renderMap();
    act(() => store.setFocus("d1"));
    act(() => store.zoomTo({ kind: "index", id: "f3" }));
    expect(store.snapshot().focusId).toBeNull();
    expect(store.snapshot().view).toMatchObject({ level: 1 });
  });

  it("SMI-FR-FJDW: the canvas and the inspector set one selection, and the inspector shows the latest", async () => {
    const { store, container } = await renderMap();
    setView(store, { level: 1 });
    fireEvent.click(nodeButton("d1 label, domain"));
    expect(container.querySelector(".smap-inspector__name")).toHaveTextContent("d1 label");
    fireEvent.click(within(inspector()).getByRole("button", { name: /f2 label/ }));
    expect(store.snapshot().selection).toEqual({ kind: "index", id: "f2" });
    expect(nodeButton("f2 label, feature")).toHaveAttribute("data-selected", "true");
    expect(nodeButton("d1 label, domain")).not.toHaveAttribute("data-selected");
    expect(container.querySelector(".smap-inspector__name")).toHaveTextContent("f2 label");
  });

  it("SMI-FR-FJDW, SMI-FR-PRSL: a project path selects the spec, shows the last level, and the inspector follows", async () => {
    const { store, container } = await renderMap();
    setView(store, { level: 1 });
    fireEvent.click(nodeButton("f1 label, feature"));
    expect(container.querySelector(".smap-inspector__name")).toHaveTextContent("f1 label");
    act(() => store.selectByPath("specifications/ui/C1-c1.md"));
    expect(store.snapshot().view).toMatchObject({ level: 3, scale: 0.85, touched: true });
    expect(container.querySelector(".smap-inspector__name")).toHaveTextContent("c1 label");
    expect(nodeButton("C1 c1 label")).toHaveAttribute("data-selected", "true");
  });

  it("SMI-FR-WDAB: a subject removed from the tree returns the inspector to its empty state", async () => {
    const { store } = await renderMap();
    act(() => {
      store.dispatch({ kind: "attachDraft", nodeId: "g1", draft: { draftId: "x", name: "Plan" } });
      store.select({ kind: "planned", draftId: "x" });
    });
    expect(inspector()).toHaveTextContent("Planned under g1 label");
    act(() => {
      store.dispatch({ kind: "detachDraft", draftId: "x" });
    });
    expect(inspector()).toHaveTextContent("Select a node to inspect it.");
  });
});

describe("ordering and limits (SMI)", () => {
  const dependsOnLabels = (container: HTMLElement) =>
    [...container.querySelectorAll(".smap-rows")[1].querySelectorAll(".smap-row__label")].map((l) => l.textContent);

  it("SMI-FR-YHNT: dependencies list most citations first, for a spec and for an index node at depth 1", async () => {
    const { store, container } = await renderMap({
      index: smallIndex([
        { from: "A1", to: "C1", citations: 2 },
        { from: "A1", to: "B1", citations: 9 },
        { from: "A1", to: "C2", citations: 5 },
      ]),
    });
    act(() => store.select({ kind: "spec", code: "A1" }));
    expect(dependsOnLabels(container)).toEqual(["b1 label", "c2 label", "c1 label"]);
    // f1 → f2 holds 9 citations and f1 → f3 holds 2 + 5.
    act(() => store.select({ kind: "index", id: "f1" }));
    expect(dependsOnLabels(container)).toEqual(["f2 label", "f3 label"]);
  });

  it("SMI-FR-ZLOA: an index node lists at most four gap specs", async () => {
    const index = smallIndex();
    index.roots[0].children = [node("fx", [node("gx", ["G1", "G2", "G3", "G4", "G5"].map((c) => spec(c, "gap")))])];
    const { store } = await renderMap({ index });
    act(() => store.select({ kind: "index", id: "d1" }));
    const note = within(inspector()).getByRole("note");
    expect(note.querySelectorAll(".smap-attention__line")).toHaveLength(4);
    expect(note).toHaveTextContent("G1 — g1 label: no coverage yet");
    expect(note).not.toHaveTextContent("G5");
  });
});
