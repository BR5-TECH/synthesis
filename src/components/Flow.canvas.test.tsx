import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { resetToasts } from "../state/toasts";
import { absolutePosition, findItem } from "../state/flowDocument";
import { KIND_GLYPHS } from "./Flow/kindGlyphs";
import { clearFlowStores, flowBackend, itemState, planOf, renderFlow } from "../test/flowCanvasHarness";
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
import { blocks, item, plan, samplePlan } from "../test/flowPlanFixtures";

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
  it("FLO-FR-07: the add affordance opens a menu of the six kinds, each with its glyph", async () => {
    const b = flowBackend("");
    await renderFlow(b);

    await userEvent.click(screen.getByRole("button", { name: "Add to plan" }));
    const menu = screen.getByTestId("flow-add-menu");
    const entries = within(menu).getAllByRole("menuitem");
    expect(entries.map((e) => e.textContent)).toEqual([
      `${KIND_GLYPHS.deliverable}Deliverable`,
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

  it("FLO-FR-07, FLO-FR-42: a deliverable added to an empty plan is a top-level item, and it is selected", async () => {
    const b = flowBackend("");
    const { flows } = await renderFlow(b);

    await addFromMenu("deliverable");
    const added = planOf(flows).items;
    expect(added).toHaveLength(1);
    expect(added[0]).toMatchObject({ kind: "deliverable", name: "New deliverable" });
    expect(added[0].parentId).toBeUndefined();
    expect(itemEl(added[0].id)).toHaveAttribute("data-selected", "true");
  });

  it("FLO-FR-42: an item the add affordance places joins the container it lands in", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    // The middle of the default view is (376, 276), inside f1.
    await addFromMenu("task");
    const added = planOf(flows).items[planOf(flows).items.length - 1];
    expect(added.kind).toBe("task");
    expect(added.parentId).toBe("f1");
  });

  it("FLO-FR-OAOZ: a placement into a container the kind does not permit is refused and names where it can go", async () => {
    const b = flowBackend("");
    const { flows } = await renderFlow(b);

    await addFromMenu("task");
    expect(planOf(flows).items).toEqual([]);
    expect(refusal()).toHaveTextContent("Not added.");
    expect(refusal()).toHaveTextContent("A task can sit only in a feature.");
  });
});

describe("moving items (FLO-FR-21, FLO-FR-42, FLO-FR-OAOZ, FLO-FR-GZCH)", () => {
  it("FLO-FR-21: a drag moves an item, stores its position from its container's origin, and is one edit", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    dragItem("t1", { x: 70, y: 190 }, { x: 80, y: 220 }, 4);
    const t1 = findItem(planOf(flows), "t1")!;
    expect(t1.parentId).toBe("f1");
    expect(t1.position).toEqual({ x: 30, y: 90 });
    expect(flows.get(ID)!.dirty).toBe(true);
  });

  it("FLO-FR-21, FLO-FR-44: moving a container moves everything in it and changes one item's position", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    dragItem("f2", { x: 520, y: 190 }, { x: 540, y: 200 });
    const doc = planOf(flows);
    expect(findItem(doc, "f2")!.position).toEqual({ x: 40, y: 70 });
    expect(findItem(doc, "t3")!.position).toEqual({ x: 20, y: 60 });
    expect(absolutePosition(doc, "t3")).toEqual({ x: 540, y: 250 });
  });

  it("FLO-FR-42: a drop inside another container makes the item its member and changes no edge", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t3", "x1")]));
    const { flows } = await renderFlow(b);

    // From inside f1 to a point inside f2.
    dragItem("t1", { x: 70, y: 190 }, { x: 620, y: 300 });
    const doc = planOf(flows);
    expect(findItem(doc, "t1")!.parentId).toBe("f2");
    expect(doc.edges).toEqual([blocks("e1", "t3", "x1")]);
  });

  it("FLO-FR-OAOZ: a task dropped on open canvas is refused, returns to where it was, and the canvas names where it can go", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    dragItem("t1", { x: 70, y: 190 }, { x: 1300, y: 1000 });
    expect(findItem(planOf(flows), "t1")!.parentId).toBe("f1");
    expect(findItem(planOf(flows), "t1")!.position).toEqual({ x: 20, y: 60 });
    expect(itemEl("t1").style.left).toBe("60px");
    expect(refusal()).toHaveTextContent("Not moved.");
    expect(refusal()).toHaveTextContent("A task can sit only in a feature.");
    expect(flows.get(ID)!.dirty).toBe(false);
  });

  it("FLO-FR-OAOZ: a drop that would put an edge between an item and its container is refused", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "f2")]));
    const { flows } = await renderFlow(b);

    dragItem("t1", { x: 70, y: 190 }, { x: 620, y: 300 });
    expect(findItem(planOf(flows), "t1")!.parentId).toBe("f1");
    expect(refusal()).toHaveTextContent("An edge would join an item to an item that holds it.");
  });

  it("FLO-FR-GZCH: a drop into another container that would close a cycle is refused and names the items", async () => {
    // f2 blocks x1 and x1 blocks t1: in f2, t1 would wait for itself.
    const b = flowBackend(samplePlan([blocks("e1", "f2", "x1"), blocks("e2", "x1", "t1")]));
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
    const b = flowBackend(plan([item("d1", "deliverable"), item("m1", "milestone", { parentId: "d1", due: "2099-01-31", position: { x: 20, y: 60 } })]));
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
    const b = flowBackend(samplePlan());
    await renderFlow(b);

    expect(itemEl("t1")).toHaveClass("flow-card");
    expect(itemEl("x1")).toHaveClass("flow-card");
    expect(itemEl("x1")).toHaveTextContent(KIND_GLYPHS.defect);
    expect(itemEl("f1")).toHaveClass("flow-ctr");
  });

  it("FLO-FR-44: a container resized from its corner cannot be made smaller than its members", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    fireEvent.pointerDown(screen.getByTestId("flow-resize-f1-se"), { pointerId: 1, buttons: 1, clientX: 500, clientY: 500 });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, clientX: 100, clientY: 100 });
    fireEvent.pointerUp(canvas(), { pointerId: 1 });
    // t2 is at (220, 60) in f1 and is 170 × 96: the floor is 406 × 172.
    expect(findItem(planOf(flows), "f1")!.size).toEqual({ width: 406, height: 172 });
  });

  it("FLO-FR-44: a container resized from its leading edge keeps every member where it was", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    fireEvent.pointerDown(screen.getByTestId("flow-resize-f1-w"), { pointerId: 1, buttons: 1, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, clientX: 90, clientY: 100 });
    fireEvent.pointerUp(canvas(), { pointerId: 1 });
    const doc = planOf(flows);
    expect(absolutePosition(doc, "f1")).toEqual({ x: 30, y: 120 });
    expect(findItem(doc, "f1")!.size).toEqual({ width: 430, height: 300 });
    expect(absolutePosition(doc, "t1")).toEqual({ x: 60, y: 180 });
    expect(absolutePosition(doc, "t2")).toEqual({ x: 260, y: 180 });
  });

  it("FLO-FR-45: a container that holds nothing is removed with its edges, with no question", async () => {
    const b = flowBackend(plan([item("d1", "deliverable"), item("d2", "deliverable", { position: { x: 900, y: 0 } })], [blocks("e1", "d1", "d2")]));
    const { flows } = await renderFlow(b);

    selectItem("d1");
    await userEvent.click(within(panel()).getByRole("button", { name: "Remove" }));
    expect(planOf(flows).items.map((i) => i.id)).toEqual(["d2"]);
    expect(planOf(flows).edges).toEqual([]);
  });

  it("FLO-FR-45: removing a container that holds items asks first, naming how many go; cancel removes nothing", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "x1")]));
    const { flows } = await renderFlow(b);

    selectItem("m1");
    await userEvent.click(within(panel()).getByRole("button", { name: "Remove" }));
    const ask = screen.getByTestId("flow-remove-confirm");
    expect(ask).toHaveTextContent("Remove M1 and the 7 items under it?");
    await userEvent.click(within(ask).getByRole("button", { name: "Cancel" }));
    expect(planOf(flows).items).toHaveLength(9);

    await userEvent.click(within(panel()).getByRole("button", { name: "Remove" }));
    await userEvent.click(within(screen.getByTestId("flow-remove-confirm")).getByRole("button", { name: "Remove" }));
    expect(planOf(flows).items.map((i) => i.id)).toEqual(["d1"]);
    expect(planOf(flows).edges).toEqual([]);
  });

  it("FLO-FR-08: removing a task removes every edge it is an end of", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t2"), blocks("e2", "t3", "t1"), blocks("e3", "t2", "x1")]));
    const { flows } = await renderFlow(b);

    selectItem("t1");
    await userEvent.click(within(panel()).getByRole("button", { name: "Remove" }));
    expect(planOf(flows).edges).toEqual([blocks("e3", "t2", "x1")]);
    expect(queryItem("t1")).toBeNull();
  });

  it("FLO-FR-ZASL: a collapsed container shows its header and its leaves by state, and hides its members", async () => {
    const b = flowBackend(samplePlan());
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

    await userEvent.click(screen.getByRole("button", { name: "Expand F1" }));
    expect(itemEl("t1")).toBeInTheDocument();
  });

  it("FLO-FR-ZASL: before the delivery state answers, a collapsed container counts its leaves", async () => {
    const b = flowBackend(samplePlan());
    b.on.load_flow_delivery_state = () => new Promise(() => {});
    await renderFlow(b);

    await userEvent.click(screen.getByRole("button", { name: "Collapse M1" }));
    expect(screen.getByTestId("flow-leaf-counts-m1")).toHaveTextContent("4 items");
  });

  it("FLO-FR-ZASL, FLO-FR-39: an edge to a hidden member lands on the border of its collapsed container", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t3")]));
    await renderFlow(b);

    await userEvent.click(screen.getByRole("button", { name: "Collapse F1" }));
    // f1 is at (40, 120), 420 wide and 66 tall when collapsed; t3 is at (520, 240).
    const d = screen.getByTestId("flow-edge-path-e1").getAttribute("d")!;
    expect(d.startsWith("M 460 144 ")).toBe(true);
    expect(d.endsWith(", 520 264")).toBe(true);
  });

  it("FLO-FR-42: a drop on a collapsed container's header joins it", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    await userEvent.click(screen.getByRole("button", { name: "Collapse F2" }));
    // f2's header is at (500, 180); its full bounds would reach y = 400.
    dragItem("t1", { x: 70, y: 190 }, { x: 650, y: 200 });
    expect(findItem(planOf(flows), "t1")!.parentId).toBe("f2");
  });
});

describe("kind changes (FLO-FR-HGIY)", () => {
  it("FLO-FR-HGIY: the kind choice offers only the kinds the container and the members permit", async () => {
    const b = flowBackend(samplePlan());
    await renderFlow(b);

    selectItem("x1");
    const kinds = within(within(panel()).getByRole("combobox", { name: "Kind" }))
      .getAllByRole("option")
      .map((o) => (o as HTMLOptionElement).value);
    // x1 sits in a milestone, which holds epics, features, and defects.
    expect(kinds).toEqual(["epic", "feature", "defect"]);
  });

  it("FLO-FR-HGIY: a task with a draft and a footprint asks before it becomes a feature; confirm removes them", async () => {
    const b = flowBackend(plan([
      item("m1", "milestone"),
      item("x1", "defect", { parentId: "m1", draftId: "d-1", footprint: ["src/"], position: { x: 20, y: 60 } }),
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
    expect(x1.size).toBeDefined();
  });

  it("FLO-FR-HGIY: a kind change with nothing to remove is applied at once", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    selectItem("t1");
    await userEvent.selectOptions(within(panel()).getByRole("combobox", { name: "Kind" }), "defect");
    expect(findItem(planOf(flows), "t1")!.kind).toBe("defect");
    expect(screen.queryByTestId("flow-kind-confirm")).toBeNull();
  });
});

describe("a pointer that leaves the canvas", () => {
  it("FLO-FR-21: a drag whose release the canvas never sees moves nothing", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    fireEvent.pointerDown(itemEl("t1"), { pointerId: 1, buttons: 1, ...client({ x: 70, y: 190 }) });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, ...client({ x: 90, y: 200 }) });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 0, ...client({ x: 95, y: 205 }) });
    expect(findItem(planOf(flows), "t1")!.position).toEqual({ x: 20, y: 60 });
  });
});
