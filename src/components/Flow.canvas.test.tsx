import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { resetToasts } from "../state/toasts";
import { absolutePosition, findItem } from "../state/flowDocument";
import { KIND_GLYPHS } from "./Flow/kindGlyphs";
import {
  callsOf,
  clearFlowStores,
  flowBackend,
  itemState,
  placedOf,
  planOf,
  renderFlow,
  SCOPE,
  storedEntry,
} from "../test/flowCanvasHarness";
import {
  addFromMenu,
  canvas,
  client,
  dragItem,
  ID,
  itemEl,
  panel,
  queryItem,
  selectItem,
} from "../test/flowFixtures";
import { resetListeners } from "../test/flowMocks";
import {
  blocks,
  item,
  plan,
  sampleLayout,
  samplePlan,
  storedLayout,
} from "../test/flowPlanFixtures";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../test/flowMocks")).listen,
}));

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
});

afterEach(() => {
  cleanup();
  clearFlowStores();
});

const refusal = () => screen.getByTestId("flow-refusal");

describe("the add menu (FLO-FR-07, FLO-FR-42, FLO-FR-OAOZ)", () => {
  it("FLO-FR-07: the add affordance opens a menu of the five kinds, each with its glyph", async () => {
    const b = flowBackend("");
    await renderFlow(b);

    await userEvent.click(screen.getByRole("button", { name: "Add to plan" }));
    const menu = screen.getByTestId("flow-add-menu");
    const entries = within(menu).getAllByRole("menuitem");
    expect(entries.map((e) => e.textContent)).toEqual([
      `${KIND_GLYPHS.milestone}Milestone`,
      `${KIND_GLYPHS.epic}Epic`,
      `${KIND_GLYPHS.feature}Feature`,
      `${KIND_GLYPHS.task}Task`,
      `${KIND_GLYPHS.defect}Defect`,
    ]);
    expect(entries[0]).toHaveFocus();
    fireEvent.keyDown(entries[0], { key: "Escape" });
    expect(screen.queryByTestId("flow-add-menu")).toBeNull();
  });

  it("FLO-FR-07, FLO-FR-42: a milestone added to an empty plan is a top-level item, and it is selected", async () => {
    const b = flowBackend("");
    const { flows } = await renderFlow(b);

    await addFromMenu("milestone");
    const added = planOf(flows).items;
    expect(added).toHaveLength(1);
    expect(added[0]).toMatchObject({ kind: "milestone", name: "New milestone" });
    expect(added[0].parentId).toBeUndefined();
    expect(itemEl(added[0].id)).toHaveAttribute("data-selected", "true");
  });

  it("FLO-FR-42: an item the add affordance places joins the container it lands in", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    const { flows } = await renderFlow(b);

    // The middle of the default view is (376, 276), inside f1.
    await addFromMenu("task");
    const added = planOf(flows).items[planOf(flows).items.length - 1];
    expect(added.kind).toBe("task");
    expect(added.parentId).toBe("f1");
  });

  it("FLO-FR-OAOZ: a placement into a container the kind does not permit is refused and names where it can go", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    const { flows } = await renderFlow(b);

    // The middle of the default view is (376, 276), inside f1.
    await addFromMenu("epic");
    expect(planOf(flows).items).toHaveLength(8);
    expect(refusal()).toHaveTextContent("Not added.");
    expect(refusal()).toHaveTextContent("An epic can sit only at the top level or in a milestone.");
  });

  it("FGV-FR-YHFX, FLO-FR-42: a task added on open canvas is a top-level item", async () => {
    const b = flowBackend("");
    const { flows } = await renderFlow(b);

    await addFromMenu("task");
    expect(planOf(flows).items).toHaveLength(1);
    expect(planOf(flows).items[0]).toMatchObject({ kind: "task" });
    expect(planOf(flows).items[0].parentId).toBeUndefined();
  });

  it("FLO-FR-21: an added item is stored in the layout where the author sees it", async () => {
    const b = flowBackend("");
    const { flows } = await renderFlow(b);

    await addFromMenu("task");
    const id = planOf(flows).items[0].id;
    // The middle of the default view, less half a card: (376 - 85, 276 - 48).
    expect(storedEntry(b, id)?.position).toEqual({ x: 291, y: 228 });
    expect(callsOf(b, "save_delivery_layout").map((a) => a.scope)).toEqual([SCOPE]);
  });
});

describe("moving items (FLO-FR-21, FLO-FR-42, FLO-FR-OAOZ, FLO-FR-GZCH)", () => {
  it("FLO-FR-21, FLO-FR-TDHR: a drag stores the position from the container's origin once, and is no edit", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    const { flows } = await renderFlow(b);

    // The milestone column m1 is at the origin (FLO-FR-SQUS), so t1 is at (40, 120).
    dragItem("t1", { x: 50, y: 130 }, { x: 60, y: 160 }, 4);
    expect(findItem(planOf(flows), "t1")!.parentId).toBe("f1");
    expect(callsOf(b, "save_delivery_layout")).toHaveLength(1);
    expect(callsOf(b, "save_delivery_layout")[0].scope).toBe(SCOPE);
    expect(storedEntry(b, "t1")?.position).toEqual({ x: 30, y: 90 });
    expect(itemEl("t1").style.left).toBe("50px");
    expect(flows.get(ID)!.dirty).toBe(false);
    expect(flows.hasPendingWrite(ID)).toBe(false);
  });

  it("FLO-FR-21, FLO-FR-44: moving a container moves everything in it and changes one item's position", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    const { flows } = await renderFlow(b);

    dragItem("f2", { x: 500, y: 130 }, { x: 520, y: 140 });
    expect(storedEntry(b, "f2")?.position).toEqual({ x: 40, y: 70 });
    expect(storedEntry(b, "t3")?.position).toEqual({ x: 20, y: 60 });
    expect(absolutePosition(planOf(flows), placedOf(flows), "t3")).toEqual({ x: 520, y: 190 });
  });

  it("FLO-FR-PLUZ: the canvas reads the layout of its scope, and an item with no stored position takes an automatic one", async () => {
    const doc = plan([item("m2", "milestone", { due: "2027-06-30" }), item("m1", "milestone", { due: "2027-03-31" })]);
    const b = flowBackend(doc);
    await renderFlow(b);

    expect(callsOf(b, "load_delivery_layout")).toEqual([{ scope: SCOPE }]);
    // m1 is due first, so it sits left of m2.
    expect(parseFloat(itemEl("m1").style.left)).toBeLessThan(parseFloat(itemEl("m2").style.left));
    expect(callsOf(b, "save_delivery_layout")).toEqual([]);
  });

  it("FLO-FR-PLUZ: a stored position is where the item renders", async () => {
    const b = flowBackend(plan([item("t1", "task")]), storedLayout({ t1: { x: 300, y: 140 } }));
    await renderFlow(b);

    expect(itemEl("t1").style.left).toBe("300px");
    expect(itemEl("t1").style.top).toBe("140px");
  });

  it("FLO-FR-PLUZ: the canvas waits for the layout before it draws the plan", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    b.on.load_delivery_layout = () => new Promise(() => {});
    await renderFlow(b);

    expect(screen.getByText("Loading…")).toBeInTheDocument();
    expect(queryItem("t1")).toBeNull();
  });

  it("FLO-FR-PLUZ: a layout that cannot be read gives the automatic layout", async () => {
    const b = flowBackend(plan([item("t1", "task")]));
    b.on.load_delivery_layout = () => Promise.reject(new Error("store_unavailable"));
    await renderFlow(b);

    expect(await screen.findByTestId("flow-item-t1")).toBeInTheDocument();
  });

  it("FLO-FR-42: a drop inside another container makes the item its member and changes no edge", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t3", "x1")]), sampleLayout());
    const { flows } = await renderFlow(b);

    // From inside f1 to a point inside f2.
    dragItem("t1", { x: 70, y: 190 }, { x: 620, y: 300 });
    const doc = planOf(flows);
    expect(findItem(doc, "t1")!.parentId).toBe("f2");
    expect(doc.edges).toEqual([blocks("e1", "t3", "x1")]);
  });

  it("FLO-FR-OAOZ: an epic dropped into a feature is refused, returns to where it was, and the canvas names where it can go", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    const { flows } = await renderFlow(b);

    // ep1's header is at (460, 60); a point inside f1 is (80, 240).
    dragItem("ep1", { x: 470, y: 70 }, { x: 80, y: 240 });
    expect(findItem(planOf(flows), "ep1")!.parentId).toBe("m1");
    expect(itemEl("ep1").style.left).toBe("460px");
    expect(refusal()).toHaveTextContent("Not moved.");
    expect(refusal()).toHaveTextContent("An epic can sit only at the top level or in a milestone.");
    expect(flows.get(ID)!.dirty).toBe(false);
    expect(callsOf(b, "save_delivery_layout")).toEqual([]);
  });

  it("FLO-FR-42, FGV-FR-YHFX: a task dropped on open canvas becomes a top-level item, which is an edit", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    const { flows } = await renderFlow(b);

    dragItem("t1", { x: 50, y: 130 }, { x: 1280, y: 940 });
    expect(findItem(planOf(flows), "t1")!.parentId).toBeUndefined();
    expect(flows.get(ID)!.dirty).toBe(true);
    expect(storedEntry(b, "t1")?.position).toEqual({ x: 1270, y: 930 });
  });

  it("FLO-FR-OAOZ: a drop that would put an edge between an item and its container is refused", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "f2")]), sampleLayout());
    const { flows } = await renderFlow(b);

    dragItem("t1", { x: 70, y: 190 }, { x: 620, y: 300 });
    expect(findItem(planOf(flows), "t1")!.parentId).toBe("f1");
    expect(refusal()).toHaveTextContent("An edge would join an item to an item that holds it.");
  });

  it("FLO-FR-GZCH: a drop into another container that would close a cycle is refused and names the items", async () => {
    // f2 blocks x1 and x1 blocks t1: in f2, t1 would wait for itself.
    const b = flowBackend(samplePlan([blocks("e1", "f2", "x1"), blocks("e2", "x1", "t1")]), sampleLayout());
    const { flows } = await renderFlow(b);

    dragItem("t1", { x: 70, y: 190 }, { x: 620, y: 300 });
    expect(findItem(planOf(flows), "t1")!.parentId).toBe("f1");
    expect(refusal()).toHaveTextContent("Not moved.");
    expect(refusal()).toHaveTextContent("This move makes a cycle among");
    for (const name of ["T1", "F2", "X1"]) expect(refusal()).toHaveTextContent(name);
    expect(flows.get(ID)!.dirty).toBe(false);
  });
});

describe("containers (FLO-FR-38, FLO-FR-44, FLO-FR-45, FLO-FR-ZASL)", () => {
  it("FLO-FR-38: a container renders its header with glyph, name, due date, status chip, and its members' region", async () => {
    const b = flowBackend(plan([item("m1", "milestone", { due: "2099-01-31" })]), storedLayout({ m1: { x: 20, y: 60 } }));
    b.delivery = { items: [itemState("m1", "in_progress")] };
    await renderFlow(b);

    const head = itemEl("m1").querySelector(".flow-ctr__head") as HTMLElement;
    expect(head).toHaveTextContent(KIND_GLYPHS.milestone);
    expect(head).toHaveTextContent("M1");
    expect(within(head).getByTestId("flow-due")).toHaveTextContent("due 2099-01-31");
    expect(await within(head).findByTestId("flow-status-chip")).toHaveTextContent("In progress");
    // An empty container keeps its size.
    expect(itemEl("m1").style.width).toBe("640px");
    expect(itemEl("m1").style.height).toBe("380px");
  });

  it("FLO-FR-38: a task or a defect renders as a card", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    await renderFlow(b);

    expect(itemEl("t1")).toHaveClass("flow-card");
    expect(itemEl("x1")).toHaveClass("flow-card");
    expect(itemEl("x1")).toHaveTextContent(KIND_GLYPHS.defect);
    expect(itemEl("f1")).toHaveClass("flow-ctr");
  });

  it("FLO-FR-44: a container resized from its corner cannot be made smaller than its members", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    const { flows } = await renderFlow(b);

    fireEvent.pointerDown(screen.getByTestId("flow-resize-f1-se"), { pointerId: 1, buttons: 1, clientX: 500, clientY: 500 });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, clientX: 100, clientY: 100 });
    fireEvent.pointerUp(canvas(), { pointerId: 1 });
    // t2 is at (220, 60) in f1 and is 170 × 96: the floor is 406 × 172.
    expect(storedEntry(b, "f1")?.size).toEqual({ width: 406, height: 172 });
    expect(callsOf(b, "save_delivery_layout")).toHaveLength(1);
    expect(flows.get(ID)!.dirty).toBe(false);
  });

  it("FLO-FR-44: a container resized from its leading edge keeps every member where it was", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    const { flows } = await renderFlow(b);

    fireEvent.pointerDown(screen.getByTestId("flow-resize-f1-w"), { pointerId: 1, buttons: 1, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, clientX: 90, clientY: 100 });
    fireEvent.pointerUp(canvas(), { pointerId: 1 });
    const doc = planOf(flows);
    const layout = placedOf(flows);
    expect(absolutePosition(doc, layout, "f1")).toEqual({ x: 10, y: 60 });
    expect(layout.f1.size).toEqual({ width: 430, height: 300 });
    expect(absolutePosition(doc, layout, "t1")).toEqual({ x: 40, y: 120 });
    expect(absolutePosition(doc, layout, "t2")).toEqual({ x: 240, y: 120 });
  });

  it("FLO-FR-45: a container that holds nothing is removed with its edges, with no question", async () => {
    const b = flowBackend(plan([item("m1", "milestone"), item("m2", "milestone")], [blocks("e1", "m1", "m2")]));
    const { flows } = await renderFlow(b);

    selectItem("m1");
    await userEvent.click(within(panel()).getByRole("button", { name: "Remove" }));
    expect(screen.queryByTestId("flow-remove-confirm")).toBeNull();
    expect(planOf(flows).items.map((i) => i.id)).toEqual(["m2"]);
    expect(planOf(flows).edges).toEqual([]);
  });

  it("FLO-FR-45: removing a container that holds items asks first, naming how many go; cancel removes nothing", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "x1")]), sampleLayout());
    const { flows } = await renderFlow(b);

    selectItem("m1");
    await userEvent.click(within(panel()).getByRole("button", { name: "Remove" }));
    const ask = screen.getByTestId("flow-remove-confirm");
    expect(ask).toHaveTextContent("Remove M1 and the 7 items under it?");
    await userEvent.click(within(ask).getByRole("button", { name: "Cancel" }));
    expect(planOf(flows).items).toHaveLength(8);
    expect(planOf(flows).edges).toHaveLength(1);

    await userEvent.click(within(panel()).getByRole("button", { name: "Remove" }));
    await userEvent.click(within(screen.getByTestId("flow-remove-confirm")).getByRole("button", { name: "Remove" }));
    expect(planOf(flows).items).toEqual([]);
    expect(planOf(flows).edges).toEqual([]);
  });

  it("FLO-FR-45: removing a container that holds one item says \"1 item\", and an edge from outside it to that item goes", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t3"), blocks("e2", "t1", "x1")]), sampleLayout());
    const { flows } = await renderFlow(b);

    selectItem("f2");
    await userEvent.click(within(panel()).getByRole("button", { name: "Remove" }));
    const ask = screen.getByTestId("flow-remove-confirm");
    expect(ask).toHaveTextContent("Remove F2 and the 1 item under it?");
    await userEvent.click(within(ask).getByRole("button", { name: "Remove" }));
    const doc = planOf(flows);
    expect(doc.items.map((i) => i.id)).toEqual(["m1", "f1", "t1", "t2", "ep1", "x1"]);
    // t1 is outside f2, and its edge to t3 went with t3; its other edge stays.
    expect(doc.edges).toEqual([blocks("e2", "t1", "x1")]);
  });

  it("FLO-FR-08: removing a task removes every edge it is an end of", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t2"), blocks("e2", "t3", "t1"), blocks("e3", "t2", "x1")]), sampleLayout());
    const { flows } = await renderFlow(b);

    selectItem("t1");
    await userEvent.click(within(panel()).getByRole("button", { name: "Remove" }));
    expect(planOf(flows).edges).toEqual([blocks("e3", "t2", "x1")]);
    expect(queryItem("t1")).toBeNull();
  });

  it("FLO-FR-ZASL: a collapsed container shows its header and its leaves by state, and hides its members", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    b.delivery = {
      items: [
        itemState("f1", "in_progress", { leafCounts: { notStarted: 1, drafting: 0, inProgress: 1, done: 0, dropped: 0 } }),
      ],
    };
    const { flows } = await renderFlow(b);
    await screen.findAllByTestId("flow-status-chip");

    await userEvent.click(screen.getByRole("button", { name: "Collapse F1" }));
    expect(screen.getByRole("button", { name: "Expand F1" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByTestId("flow-leaf-counts-f1")).toHaveTextContent("1 not started · 1 in progress");
    expect(queryItem("t1")).toBeNull();
    expect(queryItem("t2")).toBeNull();
    expect(itemEl("f1").style.height).toBe("66px");
    expect(flows.get(ID)!.dirty).toBe(false);
    // FLO-FR-TDHR: the collapse is kept in the layout, and is no edit.
    expect(storedEntry(b, "f1")?.collapsed).toBe(true);
    expect(flows.hasPendingWrite(ID)).toBe(false);

    await userEvent.click(screen.getByRole("button", { name: "Expand F1" }));
    expect(itemEl("t1")).toBeInTheDocument();
    expect(storedEntry(b, "f1")?.collapsed).toBeUndefined();
  });

  it("FLO-FR-ZASL: a container the stored layout keeps collapsed opens collapsed", async () => {
    const stored = sampleLayout();
    stored.items.f1 = { ...stored.items.f1, collapsed: true };
    const b = flowBackend(samplePlan(), stored);
    await renderFlow(b);

    expect(screen.getByRole("button", { name: "Expand F1" })).toBeInTheDocument();
    expect(queryItem("t1")).toBeNull();
  });

  it("FLO-FR-ZASL: before the delivery state answers, a collapsed container counts its leaves", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    b.on.load_flow_delivery_state = () => new Promise(() => {});
    await renderFlow(b);

    await userEvent.click(screen.getByRole("button", { name: "Collapse M1" }));
    expect(screen.getByTestId("flow-leaf-counts-m1")).toHaveTextContent("4 items");
  });

  it("FLO-FR-ZASL, FLO-FR-39: an edge to a hidden member lands on the border of its collapsed container", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t3")]), sampleLayout());
    await renderFlow(b);

    await userEvent.click(screen.getByRole("button", { name: "Collapse F1" }));
    // f1 is at (20, 60), 420 wide and 66 tall when collapsed; t3 is at (500, 180).
    const d = screen.getByTestId("flow-edge-path-e1").getAttribute("d")!;
    expect(d.startsWith("M 440 84 ")).toBe(true);
    expect(d.endsWith(", 500 204")).toBe(true);
  });

  it("FLO-FR-42: a drop on a collapsed container's header joins it", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    const { flows } = await renderFlow(b);

    await userEvent.click(screen.getByRole("button", { name: "Collapse F2" }));
    // f2's header is at (480, 120); its full bounds would reach y = 340.
    dragItem("t1", { x: 50, y: 130 }, { x: 630, y: 140 });
    expect(findItem(planOf(flows), "t1")!.parentId).toBe("f2");
  });
});

describe("kind changes (FLO-FR-HGIY)", () => {
  it("FLO-FR-HGIY: the kind choice offers only the kinds the container and the members permit", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    await renderFlow(b);

    selectItem("x1");
    const kinds = within(within(panel()).getByRole("combobox", { name: "Kind" }))
      .getAllByRole("option")
      .map((o) => (o as HTMLOptionElement).value);
    // x1 sits in a milestone, which holds epics, features, tasks, and defects.
    expect(kinds).toEqual(["epic", "feature", "task", "defect"]);
  });

  it("FLO-FR-HGIY: the kind choice of a container offers only the kinds its members can sit in", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    await renderFlow(b);
    const kindsOf = (id: string) => {
      selectItem(id);
      return within(within(panel()).getByRole("combobox", { name: "Kind" }))
        .getAllByRole("option")
        .map((o) => (o as HTMLOptionElement).value);
    };

    // f1 holds tasks, which an epic or a feature holds.
    expect(kindsOf("f1")).toEqual(["epic", "feature"]);
    // ep1 holds a feature, which a feature cannot hold; an epic is all that is left.
    expect(kindsOf("ep1")).toEqual(["epic"]);
  });

  it("FLO-FR-HGIY: Cancel on the question keeps the kind, the draft link, and the footprint", async () => {
    const b = flowBackend(plan([
      item("m1", "milestone"),
      item("x1", "defect", { parentId: "m1", draftId: "d-1", footprint: ["src/"] }),
    ]));
    const { flows } = await renderFlow(b);
    const before = planOf(flows);

    selectItem("x1");
    await userEvent.selectOptions(within(panel()).getByRole("combobox", { name: "Kind" }), "epic");
    const ask = screen.getByTestId("flow-kind-confirm");
    expect(ask).toHaveTextContent("Change X1 to epic? This removes its draft link and its footprint.");
    await userEvent.click(within(ask).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("flow-kind-confirm")).toBeNull();
    expect(planOf(flows)).toBe(before);
    expect(findItem(planOf(flows), "x1")).toMatchObject({ kind: "defect", draftId: "d-1", footprint: ["src/"] });
    expect(within(panel()).getByRole("combobox", { name: "Kind" })).toHaveValue("defect");
    expect(flows.get(ID)!.dirty).toBe(false);
  });

  it("FLO-FR-HGIY: a defect with a draft and a footprint asks before it becomes a feature; confirm removes them", async () => {
    const b = flowBackend(plan([
      item("m1", "milestone"),
      item("x1", "defect", { parentId: "m1", draftId: "d-1", footprint: ["src/"] }),
    ]));
    const { flows } = await renderFlow(b);

    selectItem("x1");
    await userEvent.selectOptions(within(panel()).getByRole("combobox", { name: "Kind" }), "feature");
    const ask = screen.getByTestId("flow-kind-confirm");
    expect(ask).toHaveTextContent("This removes its draft link and its footprint.");
    expect(findItem(planOf(flows), "x1")!.kind).toBe("defect");
    await userEvent.click(within(ask).getByRole("button", { name: "Change kind" }));
    const x1 = findItem(planOf(flows), "x1")!;
    expect(x1.kind).toBe("feature");
    expect(x1.draftId).toBeUndefined();
    expect(x1.footprint).toBeUndefined();
  });

  it("FLO-FR-HGIY: a kind change with nothing to remove is applied at once", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    const { flows } = await renderFlow(b);

    selectItem("t1");
    await userEvent.selectOptions(within(panel()).getByRole("combobox", { name: "Kind" }), "defect");
    expect(findItem(planOf(flows), "t1")!.kind).toBe("defect");
    expect(screen.queryByTestId("flow-kind-confirm")).toBeNull();
  });
});

describe("a pointer that leaves the canvas", () => {
  it("FLO-FR-21: a drag whose release the canvas never sees moves nothing", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    const { flows } = await renderFlow(b);

    fireEvent.pointerDown(itemEl("t1"), { pointerId: 1, buttons: 1, ...client({ x: 50, y: 130 }) });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, ...client({ x: 70, y: 140 }) });
    // The drag is in flight: the card follows the pointer.
    expect(itemEl("t1").style.left).toBe("60px");
    // A move with no button held means the release happened outside the canvas.
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 0, ...client({ x: 95, y: 205 }) });
    expect(itemEl("t1").style.left).toBe("40px");
    // A later release lands nothing: the drag was abandoned, not held.
    fireEvent.pointerUp(canvas(), { pointerId: 1, ...client({ x: 95, y: 205 }) });
    expect(itemEl("t1").style.left).toBe("40px");
    expect(callsOf(b, "save_delivery_layout")).toEqual([]);
    expect(flows.get(ID)!.dirty).toBe(false);
  });
});
