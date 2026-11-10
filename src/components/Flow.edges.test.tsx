import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { resetToasts } from "../state/toasts";
import { clearFlowStores, flowBackend, planOf, renderFlow } from "../test/flowCanvasHarness";
import { canvas, client, connectItems, ID, itemEl } from "../test/flowFixtures";
import { resetListeners } from "../test/flowMocks";
import { blocks, item, plan, relates, samplePlan } from "../test/flowPlanFixtures";

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
const edgeHandle = (id: string) => screen.getByTestId(`flow-edge-${id}`);
const edgePanel = () => screen.getByRole("group", { name: /^Edge / });

describe("drawing a connection (FLO-FR-15, FLO-FR-CEVI, FLO-FR-32, FLO-FR-33)", () => {
  it("FLO-FR-15, FLO-FR-CEVI: a completed connection creates a blocks edge from the item the drag started on", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    connectItems("t1", "t3");
    expect(planOf(flows).edges).toEqual([{ id: "e1", from: "t1", to: "t3", type: "blocks" }]);
    expect(flows.get(ID)!.dirty).toBe(true);
  });

  it("FLO-FR-15, FLO-FR-39: an edge joins items in different containers, and a container takes edges too", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    connectItems("f1", "f2");
    connectItems("x1", "t3");
    expect(planOf(flows).edges.map((e) => [e.from, e.to])).toEqual([
      ["f1", "f2"],
      ["x1", "t3"],
    ]);
  });

  it("FLO-FR-32: from the start of the drag, a dashed provisional edge runs to the pointer with an arrowhead", async () => {
    const b = flowBackend(samplePlan());
    await renderFlow(b);

    const port = within(itemEl("t1")).getByLabelText("Connect from T1");
    fireEvent.pointerDown(port, { pointerId: 1, buttons: 1, ...client({ x: 240, y: 200 }) });
    const preview = screen.getByTestId("flow-connection");
    expect(preview).toHaveClass("flow-edge--preview");
    expect(preview.getAttribute("marker-end")).toMatch(/^url\(#flow-arrow-active-/);
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, ...client({ x: 1100, y: 700 }) });
    expect(screen.getByTestId("flow-connection").getAttribute("d")).toMatch(/, 1100 700$/);
  });

  it("FLO-FR-32: an abandoned drag leaves the plan untouched", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    const port = within(itemEl("t1")).getByLabelText("Connect from T1");
    fireEvent.pointerDown(port, { pointerId: 1, buttons: 1, ...client({ x: 240, y: 200 }) });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, ...client({ x: 1100, y: 1000 }) });
    fireEvent.pointerUp(canvas(), { pointerId: 1 });
    expect(screen.queryByTestId("flow-connection")).toBeNull();
    expect(planOf(flows).edges).toEqual([]);
    expect(flows.get(ID)!.dirty).toBe(false);
  });

  it("FLO-FR-33: the nearest item in reach is highlighted, and a release completes the connection onto it", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    const port = within(itemEl("t1")).getByLabelText("Connect from T1");
    fireEvent.pointerDown(port, { pointerId: 1, buttons: 1, ...client({ x: 240, y: 200 }) });
    // Inside t3, inside f2: the deepest item in reach wins.
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, ...client({ x: 530, y: 250 }) });
    expect(itemEl("t3")).toHaveAttribute("data-magnet", "true");
    expect(screen.getByTestId("flow-connection")).toHaveAttribute("data-magnet", "true");
    fireEvent.pointerUp(canvas(), { pointerId: 1 });
    expect(planOf(flows).edges).toEqual([blocks("e1", "t1", "t3")]);
  });

  it("FLO-FR-33: an item the rules would refuse is never highlighted", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t2")]));
    await renderFlow(b);

    const port = within(itemEl("t1")).getByLabelText("Connect from T1");
    fireEvent.pointerDown(port, { pointerId: 1, buttons: 1, ...client({ x: 240, y: 200 }) });
    // Inside t2, which an edge already joins to t1, and inside f1, which holds t1.
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, ...client({ x: 300, y: 230 }) });
    expect(itemEl("t2")).toHaveAttribute("data-magnet", "false");
    expect(itemEl("f1")).toHaveAttribute("data-magnet", "false");
    expect(screen.getByTestId("flow-connection")).toHaveAttribute("data-magnet", "false");
  });

  it("FLO-FR-33: a release the canvas never sees abandons the drag", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    const port = within(itemEl("t1")).getByLabelText("Connect from T1");
    fireEvent.pointerDown(port, { pointerId: 1, buttons: 1, ...client({ x: 240, y: 200 }) });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, ...client({ x: 510, y: 280 }) });
    fireEvent.pointerMove(window, { pointerId: 1, buttons: 0, ...client({ x: 510, y: 280 }) });
    expect(screen.queryByTestId("flow-connection")).toBeNull();
    fireEvent.pointerUp(itemEl("x1"), { pointerId: 1 });
    expect(planOf(flows).edges).toEqual([]);
  });
});

describe("refused connections (FLO-FR-16, FLO-FR-17, FLO-FR-ZEPI, FLO-FR-GZCH)", () => {
  it("FLO-FR-16: a connection from an item to itself is refused with feedback", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    connectItems("t1", "t1");
    expect(planOf(flows).edges).toEqual([]);
    expect(refusal()).toHaveTextContent("Not connected.");
    expect(refusal()).toHaveTextContent("An item cannot connect to itself.");
  });

  it("FLO-FR-17: a second edge between two items is refused in either direction, and the first stays", async () => {
    const b = flowBackend(samplePlan([relates("e1", "t1", "t3")]));
    const { flows } = await renderFlow(b);

    connectItems("t3", "t1");
    connectItems("t1", "t3");
    expect(planOf(flows).edges).toEqual([relates("e1", "t1", "t3")]);
    expect(refusal()).toHaveTextContent("An edge already joins these two items.");
  });

  it("FLO-FR-ZEPI: a connection between an item and its ancestor is refused", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    connectItems("t1", "m1");
    expect(planOf(flows).edges).toEqual([]);
    expect(refusal()).toHaveTextContent("An item cannot connect to an item that holds it or that it holds.");
  });

  it("FLO-FR-GZCH: a connection that closes a cycle is refused, and the feedback names the cycle in blocking order", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t2")]));
    const { flows } = await renderFlow(b);

    connectItems("t2", "t1");
    expect(planOf(flows).edges).toHaveLength(1);
    // An edge already joins them, so try one step further away.
    connectItems("t2", "t3");
    connectItems("t3", "t1");
    expect(planOf(flows).edges.map((e) => e.id)).toEqual(["e1", "e2"]);
    expect(refusal()).toHaveTextContent("This edge makes a cycle: T1 → T2 → T3 → T1.");
    expect(screen.getByTestId("flow-edge-refused")).toBeInTheDocument();
  });

  it("FLO-FR-GZCH: a blocks edge into a container closes a cycle through its members", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "x1")]));
    const { flows } = await renderFlow(b);

    connectItems("x1", "f1");
    expect(planOf(flows).edges).toHaveLength(1);
    expect(refusal()).toHaveTextContent("This edge makes a cycle:");
    expect(refusal()).toHaveTextContent("F1");
  });

  it("FLO-FR-46: a refusal costs no backend call", async () => {
    const b = flowBackend(samplePlan());
    await renderFlow(b);
    const before = b.calls.length;

    connectItems("t1", "t1");
    connectItems("t1", "f1");
    expect(b.calls.slice(before).filter((c) => c.cmd !== "append_log_records")).toEqual([]);
  });
});

describe("a selected edge (FLO-FR-CEVI, FLO-FR-20, FLO-FR-GZCH)", () => {
  it("FLO-FR-CEVI: a selected edge offers its type as Blocks or Relates, and Reverse on a blocks edge", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t3")]));
    const { flows } = await renderFlow(b);

    await userEvent.click(edgeHandle("e1"));
    const type = within(edgePanel()).getByRole("radiogroup", { name: "Edge type" });
    expect(within(type).getByRole("radio", { name: "Blocks" })).toHaveAttribute("aria-checked", "true");
    await userEvent.click(within(edgePanel()).getByRole("button", { name: "Reverse" }));
    expect(planOf(flows).edges).toEqual([blocks("e1", "t3", "t1")]);
    await userEvent.click(within(type).getByRole("radio", { name: "Relates" }));
    expect(planOf(flows).edges).toEqual([relates("e1", "t3", "t1")]);
    expect(within(edgePanel()).queryByRole("button", { name: "Reverse" })).toBeNull();
  });

  it("FLO-FR-GZCH, FLO-FR-CEVI: a reversal that closes a cycle is refused and leaves the edge as it was", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t2"), blocks("e2", "t2", "t3"), blocks("e3", "t1", "t3")]));
    const { flows } = await renderFlow(b);

    await userEvent.click(edgeHandle("e3"));
    await userEvent.click(within(edgePanel()).getByRole("button", { name: "Reverse" }));
    expect(planOf(flows).edges[2]).toEqual(blocks("e3", "t1", "t3"));
    expect(refusal()).toHaveTextContent("Not reversed.");
    expect(refusal()).toHaveTextContent("makes a cycle");
  });

  it("FLO-FR-GZCH: a change to blocks that closes a cycle is refused; a relates edge never forms one", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t2"), blocks("e2", "t2", "t3"), relates("e3", "t3", "t1")]));
    const { flows } = await renderFlow(b);

    await userEvent.click(edgeHandle("e3"));
    await userEvent.click(within(edgePanel()).getByRole("radio", { name: "Blocks" }));
    expect(planOf(flows).edges[2]).toEqual(relates("e3", "t3", "t1"));
    expect(refusal()).toHaveTextContent("Not changed.");
  });

  it("FLO-FR-20: removing an edge removes only that edge", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t3"), blocks("e2", "t1", "x1")]));
    const { flows } = await renderFlow(b);

    await userEvent.click(edgeHandle("e1"));
    await userEvent.click(within(edgePanel()).getByRole("button", { name: "Remove edge" }));
    expect(planOf(flows).edges).toEqual([blocks("e2", "t1", "x1")]);
    expect(planOf(flows).items).toHaveLength(9);
  });

  it("FLO-FR-CEVI: a press on an edge's path selects it", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t3")]));
    await renderFlow(b);

    const hit = document.querySelector('.flow-edge-hit[data-edge-id="e1"]') as Element;
    fireEvent.pointerDown(hit, { pointerId: 1, buttons: 1 });
    expect(screen.getByTestId("flow-edge-path-e1")).toHaveAttribute("data-selected", "true");
    expect(edgePanel()).toHaveAccessibleName("Edge T1 blocks T3");
  });
});

describe("how an edge is drawn (FLO-FR-48, FLO-FR-39)", () => {
  it("FLO-FR-48: a blocks edge is solid with an arrowhead at the blocked end; a relates edge is dashed with none", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t3"), relates("e2", "t2", "x1")]));
    await renderFlow(b);

    const solid = screen.getByTestId("flow-edge-path-e1");
    expect(solid).toHaveClass("flow-edge--blocks");
    expect(solid.getAttribute("marker-end")).toMatch(/^url\(#flow-arrow-/);
    expect(solid).not.toHaveAttribute("marker-start");
    const dashed = screen.getByTestId("flow-edge-path-e2");
    expect(dashed).toHaveClass("flow-edge--relates");
    expect(dashed).not.toHaveAttribute("marker-end");
    expect(edgeHandle("e1")).toHaveAccessibleName("Edge: T1 blocks T3");
    expect(edgeHandle("e2")).toHaveAccessibleName("Edge: T2 relates to X1");
  });

  it("FLO-FR-48, FLO-FR-39: a blocks edge ends on the border of the blocked item, pointing into it", async () => {
    // t1 is at (60, 180) and t3 at (520, 240), both 170 × 96.
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t3")]));
    await renderFlow(b);

    const d = screen.getByTestId("flow-edge-path-e1").getAttribute("d")!;
    // They share the band y 240..276, so the edge runs straight across at 258.
    expect(d.startsWith("M 230 258 ")).toBe(true);
    expect(d.endsWith(", 520 258")).toBe(true);
  });

  it("FLO-FR-39: an edge to a container lands on the container's border", async () => {
    const b = flowBackend(plan([
      item("d1", "deliverable", { size: { width: 300, height: 200 } }),
      item("d2", "deliverable", { position: { x: 500, y: 0 }, size: { width: 300, height: 200 } }),
    ], [blocks("e1", "d1", "d2")]));
    await renderFlow(b);

    expect(screen.getByTestId("flow-edge-path-e1").getAttribute("d")).toBe(
      "M 300 100 C 390 100, 410 100, 500 100",
    );
  });

  it("FLO-FR-48: a selected edge takes the selection emphasis and its arrowhead with it", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t3")]));
    await renderFlow(b);

    await userEvent.click(edgeHandle("e1"));
    const path = screen.getByTestId("flow-edge-path-e1");
    expect(path).toHaveAttribute("data-selected", "true");
    expect(path.getAttribute("marker-end")).toMatch(/^url\(#flow-arrow-active-/);
  });
});
