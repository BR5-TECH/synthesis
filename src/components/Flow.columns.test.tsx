/**
 * The milestone columns of the canvas
 * (`../../specifications/ui/FLO-flow.md` FLO-FR-SQUS, FLO-FR-IAOQ,
 * FLO-FR-MCFR, FLO-FR-OGPF, FLO-FR-PLUZ, FLO-FR-42).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";

import { resetLogBufferForTest } from "../logging";
import { COLUMN_GAP, CONTAINER_PADDING, findItem } from "../state/flowDocument";
import { resetToasts } from "../state/toasts";
import { currentMainBranchWrite, rememberConfirmation, settleMainBranchWrite } from "../state/mainBranchWrite";
import { mainGuard, scriptedReads } from "../test/mainBranchWriteFixtures";
import { callsOf, clearFlowStores, flowBackend, planOf, renderFlow } from "../test/flowCanvasHarness";
import { dragItem, ID, itemEl } from "../test/flowFixtures";
import { resetListeners } from "../test/flowMocks";
import { item, plan } from "../test/flowPlanFixtures";

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

/** m2 is dated before m1, so the milestone order is m2, m1 (FLO-FR-MCFR). */
const DOC = plan([
  item("m1", "milestone", { due: "2027-06-30" }),
  item("f1", "feature", { parentId: "m1" }),
  item("t1", "task", { parentId: "f1" }),
  item("t2", "task", { parentId: "m1" }),
  item("m2", "milestone", { due: "2027-03-31" }),
  item("x1", "defect"),
]);

const px = (v: string) => Number.parseFloat(v);
const box = (el: HTMLElement) => ({
  x: px(el.style.left),
  y: px(el.style.top),
  width: px(el.style.width),
  height: px(el.style.height),
});

async function open(doc = DOC) {
  const b = flowBackend(doc, { items: {} });
  const r = await renderFlow(b);
  return { b, ...r };
}

describe("the milestone columns (FLO-FR-SQUS, FLO-FR-MCFR)", () => {
  it("FLO-FR-SQUS, FLO-FR-MCFR: the columns stand left to right in the milestone order, one width, tops aligned, each with its place", async () => {
    await open();
    const m2 = box(itemEl("m2"));
    const m1 = box(itemEl("m1"));
    expect(m2.x).toBe(0);
    expect(m1.x).toBe(m2.width + COLUMN_GAP);
    expect(m1.width).toBe(m2.width);
    expect(m1.y).toBe(m2.y);
    expect(screen.getByTestId("flow-column-order-m2")).toHaveTextContent("1");
    expect(screen.getByTestId("flow-column-order-m1")).toHaveTextContent("2");
    expect(screen.queryByTestId("flow-column-order-f1")).toBeNull();
  });

  it("FLO-FR-SQUS: a sequence arrow joins each column to the next, and none goes to the No milestone column", async () => {
    await open();
    expect(screen.getByTestId("flow-sequence-m2-m1")).toBeInTheDocument();
    expect(screen.getByTestId("flow-sequence").querySelectorAll("g")).toHaveLength(1);
  });

  it("FLO-FR-SQUS: a column stacks its members top to bottom", async () => {
    await open();
    const f1 = box(itemEl("f1"));
    const t2 = box(itemEl("t2"));
    expect(t2.x).toBe(f1.x);
    expect(t2.y).toBeGreaterThanOrEqual(f1.y + f1.height);
  });

  it("FLO-FR-OGPF: a milestone column has no resize handles; a container in it has them", async () => {
    await open();
    expect(screen.queryByTestId("flow-resize-m1-e")).toBeNull();
    expect(screen.getByTestId("flow-resize-f1-e")).toBeInTheDocument();
  });
});

describe("the No milestone column (FLO-FR-IAOQ)", () => {
  it("FLO-FR-IAOQ: the last column holds the top-level items that are not milestones; it has no actions and no state", async () => {
    await open();
    const none = screen.getByTestId("flow-no-milestone");
    expect(none).toHaveTextContent("No milestone");
    expect(within(none).queryAllByRole("button")).toEqual([]);
    expect(none.querySelector(".flow-chip")).toBeNull();
    const m1 = box(itemEl("m1"));
    const column = box(none);
    expect(column.x).toBe(m1.x + m1.width + COLUMN_GAP);
    expect(column.width).toBe(m1.width);
    expect(column.y).toBe(0);
    expect(box(itemEl("x1")).x).toBe(column.x + CONTAINER_PADDING);
  });

  it("FLO-FR-IAOQ, FLO-FR-42: an item dropped into the No milestone column becomes a top-level item", async () => {
    const { flows } = await open();
    const column = box(screen.getByTestId("flow-no-milestone"));
    const t2 = box(itemEl("t2"));
    const m1 = box(itemEl("m1"));
    dragItem("t2", { x: m1.x + t2.x + 10, y: m1.y + t2.y + 10 }, { x: column.x + 30, y: column.y + 300 });
    expect(findItem(planOf(flows), "t2")!.parentId).toBeUndefined();
    expect(flows.get(ID)!.dirty).toBe(true);
  });
});

describe("moving a milestone column (FLO-FR-OGPF)", () => {
  /** Two undated milestones, so a move is not given back by due date. */
  const UNDATED = plan([item("m1", "milestone"), item("x1", "defect"), item("m2", "milestone")]);

  it("FLO-FR-OGPF, FLO-FR-26: a column dragged before another moves its milestone item in the document, which is an edit, and stores no layout", async () => {
    const { b, flows } = await open(UNDATED);
    const m2 = box(itemEl("m2"));
    dragItem("m2", { x: m2.x + 30, y: 10 }, { x: 5, y: 10 });
    expect(planOf(flows).items.map((i) => i.id)).toEqual(["m2", "x1", "m1"]);
    expect(flows.get(ID)!.dirty).toBe(true);
    expect(callsOf(b, "save_delivery_layout")).toEqual([]);
    // The column takes its new place: first.
    expect(box(itemEl("m2")).x).toBe(0);
    expect(screen.getByTestId("flow-column-order-m2")).toHaveTextContent("1");
  });

  it("FLO-FR-OGPF: a column dropped back on its own place changes nothing", async () => {
    const { b, flows } = await open(UNDATED);
    const m2 = box(itemEl("m2"));
    dragItem("m2", { x: m2.x + 30, y: 10 }, { x: m2.x + 60, y: 40 });
    expect(planOf(flows).items.map((i) => i.id)).toEqual(["m1", "x1", "m2"]);
    expect(flows.get(ID)!.dirty).toBe(false);
    expect(box(itemEl("m2")).x).toBe(m2.x);
    expect(callsOf(b, "save_delivery_layout")).toEqual([]);
  });
});

describe("a column drag while the edit gate holds (FLO-FR-OGPF, EDT-FR-UPNU)", () => {
  const UNDATED = plan([item("m1", "milestone"), item("x1", "defect"), item("m2", "milestone")]);

  /** Drag m2 before m1 on a main branch: the gate holds the first change. */
  async function dragWhileHeld() {
    scriptedReads(mainGuard());
    const r = await open(UNDATED);
    const m2 = box(itemEl("m2"));
    act(() => dragItem("m2", { x: m2.x + 30, y: 10 }, { x: 5, y: 10 }));
    await waitFor(() => expect(currentMainBranchWrite()).not.toBeNull());
    expect(planOf(r.flows).items.map((i) => i.id)).toEqual(["m1", "x1", "m2"]);
    return r;
  }

  it("FLO-FR-OGPF, EDT-FR-UPNU: the new order goes in once the author confirms the write to the main branch", async () => {
    const { flows } = await dragWhileHeld();
    await act(async () => {
      rememberConfirmation(mainGuard({ confirmed: true }));
      settleMainBranchWrite(true);
    });
    await waitFor(() => expect(planOf(flows).items.map((i) => i.id)).toEqual(["m2", "x1", "m1"]));
    expect(flows.get(ID)!.dirty).toBe(true);
  });

  it("FLO-FR-OGPF, EDT-FR-UPNU: a cancel of the confirmation keeps the order", async () => {
    const { flows } = await dragWhileHeld();
    await act(async () => settleMainBranchWrite(false));
    await act(async () => {});
    expect(planOf(flows).items.map((i) => i.id)).toEqual(["m1", "x1", "m2"]);
    expect(flows.get(ID)!.dirty).toBe(false);
  });
});
