/**
 * The status filter of the Delivery lens
 * (`../../../specifications/ui/MDL-map-delivery-lens.md` MDL-FR-MIMF ..
 * MDL-FR-XHNZ, MDL-FR-JIKK; `DBR-delivery-board.md` DBR-FR-SKSO,
 * DBR-FR-POOT; `FLO-flow.md` FLO-FR-OHZB).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));

import { resetLogBufferForTest } from "../../logging";
import { type FlowDocument } from "../../state/flowDocument";
import { resetToasts } from "../../state/toasts";
import { FlowCanvas, type FlowPlanBinding } from "../Flow";
import { callsOf, clearFlowStores, flowBackend, renderFlow } from "../../test/flowCanvasHarness";
import { resetListeners } from "../../test/flowMocks";
import { blocks, item, plan } from "../../test/flowPlanFixtures";
import {
  clearLensStores,
  deliveryBackend,
  expectCentred,
  fileSource,
  githubAnswer,
  githubSource,
  issueFacts,
  leafState,
  milestoneFacts,
  renderLens,
  settle,
  type DeliveryBackend,
  type LensRender,
} from "../../test/deliveryLensHarness";

/** m1 holds the feature f1 of the tasks t1 and t2; x1 is a top-level defect that blocks t1. */
const DOC = plan(
  [
    item("m1", "milestone"),
    item("f1", "feature", { parentId: "m1" }),
    item("t1", "task", { parentId: "f1" }),
    item("t2", "task", { parentId: "f1" }),
    item("x1", "defect"),
  ],
  [blocks("e1", "x1", "t1")],
);

const TRACKER = [
  milestoneFacts("m1", 3),
  issueFacts("f1", 42, { status: "In Progress" }),
  issueFacts("t1", 51, { status: "Ready" }),
  issueFacts("t2", 52, { status: "Done" }),
  issueFacts("x1", 47, { status: null }),
];
const STATES = [
  leafState("f1", "in_progress"),
  leafState("t1", "drafting", { blocked: true, blockedBy: ["x1"] }),
  leafState("t2", "done"),
  leafState("x1", "not_started"),
];

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
  clearFlowStores();
});

async function openPlan(): Promise<LensRender> {
  const r = await renderLens({ source: githubSource() });
  await screen.findByTestId("flow-item-t1");
  await settle();
  await screen.findByTestId("delivery-status-filter");
  return r;
}

/** The last entry of a list. */
const last = <T,>(list: readonly T[]): T | undefined => list[list.length - 1];

/** The Status button of the toolbar. */
const statusButton = () =>
  within(screen.getByTestId("delivery-status-filter")).getByRole("button", { name: /^Status/ });
/** MDL-FR-MIMF: the menu of the status filter, opened where it is closed. */
const filterGroup = () => {
  if (statusButton().getAttribute("aria-expanded") !== "true") fireEvent.click(statusButton());
  return screen.getByRole("menu", { name: "Status filter" });
};
const toggle = (name: string) => within(filterGroup()).getByRole("menuitemcheckbox", { name });
const card = (id: string) => screen.queryByTestId(`flow-item-${id}`);

describe("the status filter of the toolbar (MDL-FR-MIMF)", () => {
  it("MDL-FR-MIMF: the toolbar holds one pressed toggle with its dot for each board status, and Blocked only", async () => {
    await openPlan();
    const statuses = ["backlog", "ready", "in progress", "in review", "done"];
    for (const status of statuses) {
      const button = toggle(status);
      expect(button).toHaveAttribute("aria-checked", "true");
      expect(button.querySelector(".dm-dot")).toHaveAttribute("data-s", status);
    }
    expect(toggle("Blocked only")).toHaveAttribute("aria-checked", "false");
    // It sits after the progress track.
    const rollup = screen.getByTestId("delivery-rollup-bar");
    expect(rollup.compareDocumentPosition(filterGroup()) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByTestId("delivery-hidden-count")).toBeNull();
    expect(callsOf(b, "load_delivery_view")).toEqual([{ scope: "github:PVT_1" }]);
  });
});

describe("what the filter hides (MDL-FR-WDMP, MDL-FR-BWEF, MDL-FR-ZAGM)", () => {
  it("MDL-FR-WDMP, MDL-FR-ZAGM, MDL-FR-BWEF: a released status hides its cards, the filter is stored, and the toolbar counts what it hides", async () => {
    await openPlan();
    const counts = screen.getByTestId("delivery-rollup-counts").textContent;
    fireEvent.click(toggle("done"));
    await waitFor(() => expect(card("t2")).toBeNull());
    expect(toggle("done")).toHaveAttribute("aria-checked", "false");
    expect(card("t1")).not.toBeNull();
    expect(card("f1")).not.toBeNull();
    expect(screen.getByTestId("delivery-hidden-count")).toHaveTextContent("1 hidden");
    await waitFor(() =>
      expect(b.viewWrites).toEqual([{ scope: "github:PVT_1", view: { hidden: ["done"], blockedOnly: false } }]),
    );
    // MDL-FR-BWEF: the progress track and its legend count the whole plan.
    expect(screen.getByTestId("delivery-rollup-counts").textContent).toBe(counts);
  });

  it("MDL-FR-WDMP, DBR-FR-SKSO, MDL-FR-BWEF: a group whose members and own status are hidden is hidden too, and counts as hidden", async () => {
    await openPlan();
    fireEvent.click(toggle("done"));
    fireEvent.click(toggle("ready"));
    fireEvent.click(toggle("in progress"));
    await waitFor(() => expect(card("f1")).toBeNull());
    expect(card("t1")).toBeNull();
    expect(card("t2")).toBeNull();
    // The milestone column always shows, and so does the backlog defect.
    expect(card("m1")).not.toBeNull();
    expect(card("x1")).not.toBeNull();
    expect(screen.getByTestId("delivery-hidden-count")).toHaveTextContent("3 hidden");
  });

  it("MDL-FR-WDMP, DBR-FR-POOT: Blocked only keeps the blocked items and their groups, and an edge with a hidden end is not drawn", async () => {
    await openPlan();
    expect(screen.getByTestId("flow-edge-path-e1")).toBeInTheDocument();
    fireEvent.click(toggle("Blocked only"));
    await waitFor(() => expect(card("x1")).toBeNull());
    expect(toggle("Blocked only")).toHaveAttribute("aria-checked", "true");
    expect(card("t1")).not.toBeNull();
    expect(card("f1")).not.toBeNull();
    expect(card("t2")).toBeNull();
    expect(screen.queryByTestId("flow-edge-path-e1")).toBeNull();
    // DBR-FR-POOT: the hidden blocker still names itself on the Blocked marker.
    const marker = within(card("t1")!).getByTestId("flow-marker-blocked");
    expect(marker).toHaveAttribute("title", "Blocked by X1");
    await waitFor(() =>
      expect(last(b.viewWrites)).toEqual({ scope: "github:PVT_1", view: { hidden: [], blockedOnly: true } }),
    );
  });

  it("MDL-FR-BWEF: Show all saves the empty view and shows every item", async () => {
    await openPlan();
    fireEvent.click(toggle("done"));
    await waitFor(() => expect(card("t2")).toBeNull());
    fireEvent.click(within(filterGroup()).getByRole("menuitem", { name: "Show all" }));
    await screen.findByTestId("flow-item-t2");
    expect(screen.queryByTestId("delivery-hidden-count")).toBeNull();
    // The menu closes and the focus goes back to the Status button.
    expect(screen.queryByRole("menu", { name: "Status filter" })).toBeNull();
    expect(statusButton()).toHaveFocus();
    expect(within(filterGroup()).queryByRole("menuitem", { name: "Show all" })).toBeNull();
    expect(toggle("done")).toHaveAttribute("aria-checked", "true");
    await waitFor(() =>
      expect(last(b.viewWrites)).toEqual({ scope: "github:PVT_1", view: { hidden: [], blockedOnly: false } }),
    );
  });

  it("MDL-FR-ZAGM: the stored view of the scope applies when the lens renders again", async () => {
    b.view = { hidden: ["done"], blockedOnly: false };
    await openPlan();
    await waitFor(() => expect(card("t2")).toBeNull());
    expect(toggle("done")).toHaveAttribute("aria-checked", "false");
    expect(screen.getByTestId("delivery-hidden-count")).toHaveTextContent("1 hidden");
    expect(b.viewWrites).toEqual([]);
  });

  it("MDL-FR-ZAGM: a failed read shows every item", async () => {
    b.on.load_delivery_view = () => Promise.reject({ code: "store_unavailable", message: "No store." });
    await openPlan();
    for (const id of ["f1", "t1", "t2", "x1"]) expect(card(id)).not.toBeNull();
    for (const status of ["backlog", "done"]) expect(toggle(status)).toHaveAttribute("aria-checked", "true");
  });

  it("MDL-FR-ZAGM: a failed write keeps the filter on screen", async () => {
    b.on.save_delivery_view = () => Promise.reject({ code: "store_unavailable", message: "No store." });
    await openPlan();
    fireEvent.click(toggle("done"));
    await waitFor(() => expect(callsOf(b, "save_delivery_view")).toHaveLength(1));
    await act(async () => {});
    expect(card("t2")).toBeNull();
    expect(toggle("done")).toHaveAttribute("aria-checked", "false");
  });
});

describe("the items that always show (MDL-FR-ZKXX, MDL-FR-JIKK)", () => {
  it("MDL-FR-ZKXX, MDL-FR-JIKK: a request to show a hidden item selects it, so it shows, and the filter stays", async () => {
    b.view = { hidden: ["done"], blockedOnly: false };
    const r = await openPlan();
    await waitFor(() => expect(card("t2")).toBeNull());
    act(() => r.store.showDeliveryItem("t2"));
    await screen.findByTestId("flow-item-t2");
    expect(card("t2")).toHaveAttribute("data-selected", "true");
    await waitFor(() => expectCentred("t2"));
    expect(toggle("done")).toHaveAttribute("aria-checked", "false");
    expect(b.viewWrites).toEqual([]);
    expect(screen.queryByTestId("delivery-hidden-count")).toBeNull();
  });

  it("MDL-FR-ZKXX: a selected item stays while its status is released", async () => {
    await openPlan();
    fireEvent.pointerDown(card("t2")!, { pointerId: 1, buttons: 1, clientX: 1, clientY: 1 });
    fireEvent.pointerUp(card("t2")!, { pointerId: 1 });
    fireEvent.click(toggle("done"));
    await waitFor(() => expect(b.viewWrites).toHaveLength(1));
    expect(card("t2")).not.toBeNull();
  });
});

describe("a file source (MDL-FR-ZAGM)", () => {
  it("MDL-FR-ZAGM: the filter of a Flow file is the view of its layout scope", async () => {
    const FLOW = "plans/release.flow";
    b = deliveryBackend(DOC);
    b.delivery = { items: [leafState("t1", "in_progress"), leafState("t2", "done"), leafState("x1", "not_started")] };
    await renderLens({ source: fileSource(FLOW) });
    await screen.findByTestId("flow-item-t2");
    await screen.findByTestId("delivery-status-filter");
    expect(callsOf(b, "load_delivery_view")).toEqual([{ scope: `flow:${FLOW}` }]);
    fireEvent.click(toggle("done"));
    await waitFor(() => expect(card("t2")).toBeNull());
    await waitFor(() =>
      expect(b.viewWrites).toEqual([{ scope: `flow:${FLOW}`, view: { hidden: ["done"], blockedOnly: false } }]),
    );
  });
});

describe("a Flow tab (MDL-FR-XHNZ, FLO-FR-OHZB)", () => {
  it("MDL-FR-XHNZ: a Flow tab has no status filter and hides no item", async () => {
    const fb = flowBackend(DOC);
    fb.delivery = { items: [leafState("t2", "done")] };
    await renderFlow(fb);
    await screen.findByTestId("flow-item-t2");
    expect(screen.queryByTestId("delivery-status-filter")).toBeNull();
    expect(callsOf(fb, "load_delivery_view")).toEqual([]);
  });
});

describe("the proposal marks on a filtered board (MDL-FR-ZKXX)", () => {
  it("MDL-FR-ZKXX: an item with a proposal mark and a ghost card show while the filter hides their status", async () => {
    // The canvas reads its layout and the project tree from this backend.
    flowBackend(DOC);
    const withGhost = plan([...DOC.items, item("g9", "task", { parentId: "m1" })], DOC.edges);
    const binding: FlowPlanBinding = {
      kind: "plan",
      scope: "github:PVT_1",
      document: withGhost,
      states: [...STATES, leafState("g9", "done")],
      overlaps: [],
      tracker: TRACKER,
      repository: "acme/payments",
      proposal: { marks: new Map([["t2", "proposed"]]), ghosts: new Map([["g9", { kindKnown: true }]]), reviewed: null },
      onEdit: () => Promise.resolve({ ok: true, document: withGhost }),
      onRemoveRequest: () => {},
    };
    const onHiddenCount = vi.fn();
    render(
      <FlowCanvas binding={binding} board statusFilter={{ hidden: ["done", "backlog"], blockedOnly: false }} onHiddenCount={onHiddenCount} />,
    );
    await screen.findByTestId("flow-item-t2");
    expect(card("g9")).not.toBeNull();
    expect(card("x1")).toBeNull();
    await waitFor(() => expect(onHiddenCount).toHaveBeenLastCalledWith(1));
  });
});

describe("the menu of the status filter (MDL-FR-MIMF)", () => {
  it("MDL-FR-MIMF: Escape closes the menu and gives the focus back to the Status button, and the arrows move between the items", async () => {
    await openPlan();
    const menu = filterGroup();
    expect(statusButton()).toHaveAttribute("aria-expanded", "true");
    expect(within(menu).getByRole("menuitemcheckbox", { name: "backlog" })).toHaveFocus();
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(within(menu).getByRole("menuitemcheckbox", { name: "ready" })).toHaveFocus();
    fireEvent.keyDown(menu, { key: "ArrowUp" });
    fireEvent.keyDown(menu, { key: "ArrowUp" });
    expect(within(menu).getByRole("menuitemcheckbox", { name: "Blocked only" })).toHaveFocus();
    fireEvent.keyDown(menu, { key: "Escape" });
    expect(screen.queryByRole("menu", { name: "Status filter" })).toBeNull();
    expect(statusButton()).toHaveFocus();
  });

  it("MDL-FR-MIMF: the menu stays open while the author toggles, and a click outside closes it", async () => {
    await openPlan();
    fireEvent.click(toggle("done"));
    fireEvent.click(toggle("ready"));
    expect(screen.getByRole("menu", { name: "Status filter" })).toBeInTheDocument();
    fireEvent.pointerDown(document.body);
    await waitFor(() => expect(screen.queryByRole("menu", { name: "Status filter" })).toBeNull());
    expect(statusButton()).toHaveTextContent("2 hidden");
  });
});
