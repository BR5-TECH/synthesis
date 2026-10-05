import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import { buildTree, indexChildren, specChildren } from "../../state/specMap/tree";
import { layoutMap, scopeOf } from "./layout";
import { mockCanvasRect, nodeButton, renderMap, setView } from "../../test/specMapRender";
import type { SpecMapSessionStore } from "../../state/specMap/session";

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
const dialog = () => screen.getByRole("dialog");
const childIds = (store: SpecMapSessionStore, id: string) =>
  indexChildren(store.tree()!.nodes.get(id)!).map((n) => n.id);
const specCodes = (store: SpecMapSessionStore, id: string) =>
  specChildren(store.tree()!.nodes.get(id)!).map((s) => s.code);

describe("creating a node (SMO-FR-UAKE, SMO-FR-HCQN, SMO-FR-NXDL)", () => {
  it("SMO-FR-UAKE: New is offered above the leaf depth and at the root, and never on a leaf group or a spec", async () => {
    const { store } = await renderMap();
    expect(within(inspector()).getByRole("button", { name: "New domain" })).toBeInTheDocument();
    act(() => store.select({ kind: "index", id: "f1" }));
    expect(within(inspector()).getByRole("button", { name: "New group" })).toBeInTheDocument();
    act(() => store.select({ kind: "index", id: "g1" }));
    expect(within(inspector()).queryByRole("button", { name: /^New (group|feature|domain|spec)$/ })).toBeNull();
    act(() => store.select({ kind: "spec", code: "A1" }));
    expect(within(inspector()).queryByRole("button", { name: /^New / })).toBeNull();
  });

  it("SMO-FR-HCQN, SNV-FR-56: the dialog needs a name, caps both fields, and Escape changes nothing", async () => {
    const { store, onOverlayOpening, ops } = await renderMap();
    act(() => store.select({ kind: "index", id: "f1" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "New group" }));
    expect(onOverlayOpening).toHaveBeenCalledTimes(1);
    const name = within(dialog()).getByLabelText("Name");
    expect(name).toHaveAttribute("maxlength", "60");
    expect(within(dialog()).getByLabelText("Summary")).toHaveAttribute("maxlength", "400");
    const create = within(dialog()).getByRole("button", { name: "Create" });
    expect(create).toBeDisabled();
    fireEvent.change(name, { target: { value: "   " } });
    expect(create).toBeDisabled();
    fireEvent.change(name, { target: { value: "planning" } });
    expect(create).toBeEnabled();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(childIds(store, "f1")).toEqual(["g1", "g2"]);
    expect(ops.saveOrganization).not.toHaveBeenCalled();
  });

  it("SMO-FR-NXDL, SMO-FR-CZLA: a created node is the last child, selected, shown at its level, and saved", async () => {
    const { store, ops } = await renderMap();
    act(() => store.select({ kind: "index", id: "f1" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "New group" }));
    fireEvent.change(within(dialog()).getByLabelText("Name"), { target: { value: " planning " } });
    fireEvent.change(within(dialog()).getByLabelText("Summary"), { target: { value: "Work ahead." } });
    fireEvent.click(within(dialog()).getByRole("button", { name: "Create" }));

    expect(screen.queryByRole("dialog")).toBeNull();
    const ids = childIds(store, "f1");
    expect(ids).toHaveLength(3);
    const created = store.tree()!.nodes.get(ids[2])!;
    expect([created.label, created.summary]).toEqual(["planning", "Work ahead."]);
    expect(store.snapshot().selection).toEqual({ kind: "index", id: created.id });
    expect(store.snapshot().view).toMatchObject({ level: 2, touched: true });
    expect(nodeButton("planning, group")).toHaveAttribute("data-selected", "true");
    expect(ops.saveOrganization).toHaveBeenCalledWith({
      kind: "create",
      parentId: "f1",
      node: { id: created.id, label: "planning", summary: "Work ahead." },
    });
  });
});

describe("editing and moving a node (SMO-FR-PZEI, SMO-FR-SLNC)", () => {
  it("SMO-FR-PZEI: Rename opens the dialog filled in, and Save changes the label everywhere", async () => {
    const { store, container } = await renderMap();
    setView(store, { level: 2 });
    act(() => store.select({ kind: "index", id: "g1" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "Rename" }));
    expect(dialog()).toHaveAccessibleName("Rename group");
    const name = within(dialog()).getByLabelText("Name") as HTMLInputElement;
    expect(name.value).toBe("g1 label");
    expect((within(dialog()).getByLabelText("Summary") as HTMLTextAreaElement).value).toBe("g1 summary.");
    fireEvent.change(name, { target: { value: "zones" } });
    fireEvent.click(within(dialog()).getByRole("button", { name: "Save" }));
    expect(nodeButton("zones, group")).toBeInTheDocument();
    expect(container.querySelector(".smap-inspector__name")).toHaveTextContent("zones");
  });

  it("SMO-FR-SLNC: Move to lists every valid new parent, and a confirmed move makes the node the last child", async () => {
    const { store, ops } = await renderMap();
    act(() => store.select({ kind: "index", id: "g1" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "Rename" }));
    const select = within(dialog()).getByLabelText("Move to") as HTMLSelectElement;
    expect([...select.options].map((o) => o.textContent)).toEqual([
      "Keep the current parent",
      "f2 label",
      "f3 label",
    ]);
    fireEvent.change(select, { target: { value: "f3" } });
    fireEvent.click(within(dialog()).getByRole("button", { name: "Save" }));
    expect(childIds(store, "f3")).toEqual(["g4", "g1"]);
    expect(childIds(store, "f1")).toEqual(["g2"]);
    // An unchanged name and summary are not saved as an edit.
    expect(ops.saveOrganization).toHaveBeenCalledTimes(1);
    expect(ops.saveOrganization).toHaveBeenCalledWith({ kind: "move", nodeId: "g1", parentId: "f3", index: 1 });
  });
});

describe("deleting a node (SMO-FR-IMXW, SMO-FR-OBRF, SMO-FR-GQTS)", () => {
  it("SMO-FR-IMXW: Delete asks first, naming the node and its direct children; Cancel changes nothing", async () => {
    const { store } = await renderMap();
    act(() => store.select({ kind: "index", id: "f1" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "Delete" }));
    expect(within(dialog()).getByText("Delete feature")).toBeInTheDocument();
    expect(dialog()).toHaveTextContent("Delete f1 label? It holds 2 direct children.");
    fireEvent.click(within(dialog()).getByRole("button", { name: "Cancel" }));
    expect(store.tree()!.nodes.has("f1")).toBe(true);
  });

  it("SMO-FR-OBRF: a confirmed delete gives the children to the sibling and selects it", async () => {
    const { store } = await renderMap();
    setView(store, { level: 3 });
    act(() => store.select({ kind: "index", id: "g2" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "Delete" }));
    expect(dialog()).toHaveTextContent("It holds 1 direct child.");
    fireEvent.click(within(dialog()).getByRole("button", { name: "Delete" }));
    expect(screen.queryByRole("button", { name: "g2 label, group" })).toBeNull();
    expect(specCodes(store, "g1")).toEqual(["A1", "A2", "A3"]);
    expect(store.snapshot().selection).toEqual({ kind: "index", id: "g1" });
  });

  it("SMO-FR-GQTS: a node with children and no sibling cannot be deleted, and says why", async () => {
    const { store } = await renderMap();
    act(() => store.select({ kind: "index", id: "g3" }));
    const remove = within(inspector()).getByRole("button", { name: "Delete" });
    expect(remove).toBeDisabled();
    expect(remove).toHaveAttribute("title", "Move the children out first");
  });
});

describe("drag and drop (SMO-FR-DKTM, SMO-FR-ARQV, SMO-FR-JYVB)", () => {
  async function dragSetup() {
    mockCanvasRect({ width: 1200, height: 800 });
    const rendered = await renderMap();
    setView(rendered.store, { level: 3 });
    const layout = layoutMap(scopeOf(buildTree(rendered.store.snapshot().index!), null), 3, 1200);
    const at = (x: number, y: number) => ({ clientX: x + 600, clientY: y + 400, pointerId: 1 });
    return { ...rendered, layout, at };
  }

  it("SMO-FR-DKTM, SMO-FR-ARQV, SMO-FR-WGOF, SMZ-FR-CUTY: a drag past 4px moves a chip onto the group under the pointer", async () => {
    const { store, layout, at, container } = await dragSetup();
    const a1 = layout.chip.get("A1")!;
    const g4 = layout.box.get("g4")!;
    fireEvent.pointerDown(nodeButton("A1 a1 label"), { button: 0, ...at(a1.x + 5, a1.y + 5) });
    fireEvent.pointerMove(window, at(a1.x + 7, a1.y + 5));
    expect(container.querySelector(".smap-ghost")).toBeNull();

    fireEvent.pointerMove(window, at(g4.x + 20, g4.y + 70));
    expect(container.querySelector(".smap-ghost")).toHaveTextContent("A1");
    expect(nodeButton("g4 label, group")).toHaveAttribute("data-drop", "true");
    expect(nodeButton("A1 a1 label")).toHaveAttribute("data-dim", "drag");
    expect(container.querySelector(".smap-drop-bar")).not.toBeNull();
    expect(screen.getByRole("button", { name: "Zoom in" })).toBeDisabled();
    const view = store.snapshot().view;
    fireEvent.wheel(screen.getByTestId("spec-map-canvas"), { deltaY: -120 });
    expect(store.snapshot().view).toEqual(view);

    fireEvent.pointerUp(window, at(g4.x + 20, g4.y + 70));
    expect(specCodes(store, "g4")).toEqual(["A1", "C1", "C2"]);
    expect(container.querySelector(".smap-ghost")).toBeNull();

    // The click that ends a drag does not also select.
    fireEvent.click(nodeButton("A1 a1 label"));
    expect(store.snapshot().selection).toBeNull();
    fireEvent.click(nodeButton("A1 a1 label"));
    expect(store.snapshot().selection).toEqual({ kind: "spec", code: "A1" });
  });

  it("SMO-FR-JYVB: Escape ends the drag and changes nothing", async () => {
    const { store, layout, at, container } = await dragSetup();
    const a1 = layout.chip.get("A1")!;
    const g4 = layout.box.get("g4")!;
    fireEvent.pointerDown(nodeButton("A1 a1 label"), { button: 0, ...at(a1.x + 5, a1.y + 5) });
    fireEvent.pointerMove(window, at(g4.x + 20, g4.y + 70));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(container.querySelector(".smap-ghost")).toBeNull();
    fireEvent.pointerUp(window, at(g4.x + 20, g4.y + 70));
    expect(specCodes(store, "g1")).toEqual(["A1", "A2"]);
  });

  it("SMO-FR-JYVB: a release outside a valid target changes nothing, and so does a pointer cancel", async () => {
    const { store, layout, at, ops } = await dragSetup();
    const a1 = layout.chip.get("A1")!;
    const f3 = layout.box.get("f3")!;
    fireEvent.pointerDown(nodeButton("A1 a1 label"), { button: 0, ...at(a1.x + 5, a1.y + 5) });
    fireEvent.pointerMove(window, at(f3.x + 4, f3.y + 20));
    fireEvent.pointerUp(window, at(f3.x + 4, f3.y + 20));
    expect(specCodes(store, "g1")).toEqual(["A1", "A2"]);

    const g4 = layout.box.get("g4")!;
    fireEvent.pointerDown(nodeButton("A1 a1 label"), { button: 0, ...at(a1.x + 5, a1.y + 5) });
    fireEvent.pointerMove(window, at(g4.x + 20, g4.y + 70));
    fireEvent.pointerCancel(window, at(g4.x + 20, g4.y + 70));
    expect(specCodes(store, "g1")).toEqual(["A1", "A2"]);
    expect(ops.saveOrganization).not.toHaveBeenCalled();
  });

  it("SMD-FR-QPAM: a planned chip dragged onto another index node moves its placement", async () => {
    const { store, layout, at, ops } = await dragSetup();
    act(() => {
      store.dispatch({ kind: "attachDraft", nodeId: "g1", draft: { draftId: "dr", name: "Untitled" } });
    });
    const fresh = layoutMap(scopeOf(store.tree()!, null), 3, 1200);
    const chip = fresh.chip.get("dr")!;
    const f3 = layout.box.get("f3")!;
    fireEvent.pointerDown(nodeButton("draft Untitled"), { button: 0, ...at(chip.x + 5, chip.y + 5) });
    fireEvent.pointerMove(window, at(f3.x + 4, f3.y + 20));
    fireEvent.pointerUp(window, at(f3.x + 4, f3.y + 20));
    expect(store.tree()!.plannedParent.get("dr")).toBe("f3");
    expect(ops.attachDraft).toHaveBeenLastCalledWith({ draftId: "dr", nodeId: "f3" });
  });
});

describe("drafts from the map (SMD)", () => {
  it("SMD-FR-RTMQ, SMD-FR-HVBE: every index node offers New draft, and a spec does not", async () => {
    const { store, onNewDraft } = await renderMap();
    act(() => store.select({ kind: "index", id: "d2" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "New draft" }));
    expect(onNewDraft).toHaveBeenCalledWith("d2");
    act(() => store.select({ kind: "spec", code: "C1" }));
    expect(within(inspector()).queryByRole("button", { name: "New draft" })).toBeNull();
  });

  it("SMD-FR-XEPS: a planned chip selects, and its inspector opens the draft", async () => {
    const { store, onOpenDraft } = await renderMap();
    act(() => {
      store.dispatch({ kind: "attachDraft", nodeId: "g4", draft: { draftId: "dr", name: "Planning" } });
    });
    setView(store, { level: 3 });
    fireEvent.click(nodeButton("draft Planning"));
    expect(store.snapshot().selection).toEqual({ kind: "planned", draftId: "dr" });
    expect(inspector()).toHaveTextContent("Planned under g4 label");
    expect(within(inspector()).queryByRole("button", { name: "New draft" })).toBeNull();
    fireEvent.click(within(inspector()).getByRole("button", { name: "Open draft" }));
    expect(onOpenDraft).toHaveBeenCalledWith({ id: "dr", name: "Planning" });
  });
});

describe("after an edit, and dialog focus", () => {
  it("SMO-FR-PZEI: a renamed node reads its new name in the hover card and in a child's breadcrumb", async () => {
    const { store, container } = await renderMap();
    setView(store, { level: 1 });
    act(() => store.select({ kind: "index", id: "f1" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "Rename" }));
    fireEvent.change(within(dialog()).getByLabelText("Name"), { target: { value: "zones" } });
    fireEvent.click(within(dialog()).getByRole("button", { name: "Save" }));
    fireEvent.mouseOver(nodeButton("zones, feature"));
    expect(screen.getByRole("tooltip")).toHaveTextContent("zones");
    act(() => store.select({ kind: "index", id: "g1" }));
    expect(container.querySelector(".smap-inspector__crumbs")).toHaveTextContent("d1 label → zones");
  });

  it("a map dialog takes focus, keeps Tab inside it, and returns focus to its opener on Escape", async () => {
    const { store } = await renderMap();
    act(() => store.select({ kind: "index", id: "g1" }));
    const rename = within(inspector()).getByRole("button", { name: "Rename" });
    rename.focus();
    fireEvent.click(rename);
    const name = within(dialog()).getByLabelText("Name");
    expect(document.activeElement).toBe(name);
    const save = within(dialog()).getByRole("button", { name: "Save" });
    save.focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement).toBe(name);
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(save);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(within(inspector()).getByRole("button", { name: "Rename" }));
  });
});
