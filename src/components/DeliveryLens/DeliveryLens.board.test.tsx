/**
 * The board in the Delivery lens over a GitHub plan
 * (`../../../specifications/ui/DBR-delivery-board.md`; MDL-FR-XJBV, MDL-FR-PUCM).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));

import { resetLogBufferForTest } from "../../logging";
import { absolutePosition, resolveBoardLayout, subtreeOf, type FlowDocument } from "../../state/flowDocument";
import { resetToasts } from "../../state/toasts";
import { callsOf } from "../../test/flowCanvasHarness";
import { canvas, client, itemEl, selectItem } from "../../test/flowFixtures";
import { resetListeners } from "../../test/flowMocks";
import { item, plan } from "../../test/flowPlanFixtures";
import {
  clearLensStores,
  deliveryBackend,
  githubAnswer,
  githubSource,
  issueFacts,
  leafState,
  milestoneFacts,
  renderLens,
  settle,
  type DeliveryBackend,
} from "../../test/deliveryLensHarness";

const DOC = plan([
  item("m1", "milestone", { due: "2027-03-31" }),
  item("f1", "feature", { parentId: "m1" }),
  item("t1", "task", { parentId: "f1" }),
  item("x1", "defect"),
]);

const TRACKER = [
  milestoneFacts("m1", 3),
  issueFacts("f1", 42, { status: "In Progress" }),
  issueFacts("t1", 51, { status: "Ready" }),
  issueFacts("x1", 47, { status: null }),
];
const STATES = [leafState("f1", "in_progress"), leafState("t1", "drafting"), leafState("x1", "not_started")];

const answerOf = (doc: FlowDocument = DOC) => githubAnswer(doc, { tracker: TRACKER, states: STATES });

let b: DeliveryBackend;

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
  b = deliveryBackend("", answerOf());
});

afterEach(() => {
  cleanup();
  clearLensStores();
});

async function openPlan() {
  const r = await renderLens({ source: githubSource() });
  await screen.findByTestId("flow-item-t1");
  await settle();
  return r;
}

/** Drag x1 from the No milestone column to the end of the stack of f1. */
function dragDefectIntoFeature() {
  const lay = resolveBoardLayout(DOC, { items: {} });
  const at = absolutePosition(DOC, lay.layout, "x1");
  const lifted = resolveBoardLayout(DOC, { items: {} }, { lifted: subtreeOf(DOC, "x1") });
  const t1 = absolutePosition(DOC, lifted.layout, "t1");
  const start = { x: at.x + 40, y: at.y + 20 };
  fireEvent.pointerDown(itemEl("x1"), { pointerId: 1, buttons: 1, ...client(start) });
  fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, ...client({ x: start.x - 30, y: start.y }) });
  fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, ...client({ x: t1.x + 40, y: t1.y + 70 }) });
  expect(screen.getByTestId("flow-board-placeholder")).toBeInTheDocument();
  expect(itemEl("f1")).toHaveAttribute("data-drop-target", "true");
  fireEvent.pointerUp(canvas(), { pointerId: 1 });
}

const moved = () =>
  plan(DOC.items.map((i) => (i.id === "x1" ? { ...i, parentId: "f1" } : i)));

describe("the board of a GitHub plan (DBR-FR-ERHW, DBR-FR-XOOU, DBR-FR-SBTR)", () => {
  it("DBR-FR-ERHW, MDL-FR-XJBV: the lens lays a GitHub plan out as the board", async () => {
    await openPlan();
    expect(canvas()).toHaveAttribute("data-board", "true");
    expect(itemEl("t1")).toHaveClass("flow-bcard");
  });

  it("DBR-FR-XOOU: the address of an issue opens the issue on GitHub", async () => {
    await openPlan();
    const address = within(itemEl("t1")).getByTestId("flow-tracker-tag");
    expect(address).toHaveTextContent("acme/payments#51");
    fireEvent.click(address);
    await waitFor(() => expect(callsOf(b, "open_tracker_link")).toHaveLength(1));
    expect(callsOf(b, "open_tracker_link")[0]).toEqual({ url: "https://github.com/acme/payments/issues/51" });
  });

  it("DBR-FR-SBTR: a group header shows the state glyph, the marker glyphs, the Status, and the address in that order", async () => {
    await openPlan();
    const head = itemEl("f1").querySelector(".flow-ctr__head")!;
    const order = [...head.querySelectorAll(".flow-sglyph, .flow-bmarks, .flow-ctr__status, .flow-baddr")].map(
      (el) => el.className.split(" ")[0],
    );
    expect(order).toEqual(["flow-sglyph", "flow-bmarks", "flow-ctr__status", "flow-baddr"]);
    expect(within(head as HTMLElement).getByTestId("flow-group-status")).toHaveTextContent("In Progress");
  });
});

describe("a drop of a GitHub plan (DBR-FR-KTJS, DBR-FR-CVJY, DBR-FR-CJSG)", () => {
  it("DBR-FR-KTJS: a drop into another container sends one move edit and writes the layout once", async () => {
    b.apply = async () => answerOf(moved());
    await openPlan();
    const writes = b.layoutWrites.length;
    dragDefectIntoFeature();
    await waitFor(() => expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(1));
    const edit = callsOf(b, "apply_github_delivery_edit")[0].edit as Record<string, unknown>;
    expect(edit).toMatchObject({ kind: "move", itemId: "x1" });
    expect(JSON.stringify(edit)).toContain('"f1"');
    expect(b.layoutWrites[b.layoutWrites.length - 1].layout.items.x1?.position).toBeDefined();
    await waitFor(() => expect(itemEl("x1")).toHaveAttribute("data-pending", "false"));
    // The answer of the edit writes no second layout.
    expect(b.layoutWrites.length - writes).toBe(1);
  });

  it("DBR-FR-CVJY, DBR-FR-CJSG: a refused move puts the item back in its stack, restores its slot, and shows the error on it and in its panel", async () => {
    b.apply = () => Promise.reject({ code: "request_failed", message: "GitHub refused the move." });
    await openPlan();
    const before = { top: itemEl("x1").style.top, left: itemEl("x1").style.left, height: itemEl("x1").style.height };
    dragDefectIntoFeature();
    const error = await screen.findByTestId("flow-item-error-x1");
    expect(error).toHaveAttribute("title", "GitHub refused the move.");
    expect(itemEl("x1")).toHaveAttribute("data-error", "true");
    await waitFor(() => expect(itemEl("x1").style.top).toBe(before.top));
    expect(itemEl("x1").style.left).toBe(before.left);
    // DBR-FR-CJSG: the card keeps its height, and the error glyph comes before the other marker glyphs.
    expect(itemEl("x1").style.height).toBe(before.height);
    expect(itemEl("x1").querySelector(".flow-bmarks")!.firstElementChild).toBe(error);
    // The stored slot of the item is the one it had before the drop.
    await waitFor(() => expect(b.layout.items.x1).toBeUndefined());
    selectItem("x1");
    expect(within(screen.getByTestId("flow-selection-panel")).getByTestId("flow-panel-error")).toHaveTextContent(
      "GitHub refused the move.",
    );
  });
});
