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
    expect(itemEl("t1")).toHaveClass("dm-card");
  });

  it("DBR-FR-XOOU: the address of an issue opens the issue on GitHub", async () => {
    await openPlan();
    const address = within(itemEl("t1")).getByTestId("flow-tracker-tag");
    expect(address).toHaveTextContent("#51");
    expect(address).toHaveAccessibleName("Open acme/payments#51 on GitHub");
    expect(address).toHaveAttribute("title", "Open acme/payments#51 on GitHub");
    fireEvent.click(address);
    await waitFor(() => expect(callsOf(b, "open_tracker_link")).toHaveLength(1));
    expect(callsOf(b, "open_tracker_link")[0]).toEqual({ url: "https://github.com/acme/payments/issues/51" });
  });

  it("DBR-FR-SBTR, DBR-FR-XOOU, DBR-FR-HWHO, DBR-FR-TCPE: a group face shows the kind chip and the address, the name, and the Status in lowercase with its dot", async () => {
    await openPlan();
    const face = itemEl("f1").querySelector(".dm-group__head") as HTMLElement;
    const lines = [...face.children].filter((el) => !el.classList.contains("flow-port"));
    expect(lines.map((el) => el.className.split(" ")[0])).toEqual(["dm-card__id", "dm-card__title", "dm-card__meta"]);
    expect(within(lines[0] as HTMLElement).getByTestId("flow-kind-chip")).toHaveTextContent("FTR");
    expect(within(lines[0] as HTMLElement).getByTestId("flow-tracker-tag")).toHaveTextContent(/^#\d+$/);
    const status = within(face).getByTestId("flow-card-status");
    expect(status).toHaveTextContent("in progress");
    expect(within(status).getByTestId("flow-status-dot")).toHaveAttribute("data-s", "in progress");
  });
});

describe("the assignee of a GitHub plan (DBR-FR-WQAN, DBR-FR-GFGG)", () => {
  const ASSIGNED = plan(DOC.items.map((i) => (i.id === "t1" ? { ...i, draftId: "d-1" } : i)));

  async function openAssigned() {
    b = deliveryBackend(
      "",
      githubAnswer(ASSIGNED, {
        tracker: [
          { ...milestoneFacts("m1", 3), assignees: ["ola"] },
          issueFacts("f1", 42, { status: "In Progress", assignees: ["kai"] }),
          issueFacts("t1", 51, { status: "Ready", assignees: ["mia", "jo", "ana"] }),
          issueFacts("x1", 47, { status: null, assignees: [] }),
        ],
        states: STATES,
      }),
    );
    await openPlan();
  }

  it("DBR-FR-WQAN, GDP-FR-QGFO: a card shows the first assignee and the count of the others, with every login in its name and tooltip", async () => {
    await openAssigned();
    const assignee = within(itemEl("t1")).getByTestId("flow-card-assignee");
    expect(assignee).toHaveTextContent(/^@mia\+2$/);
    expect(assignee).toHaveAccessibleName("Assigned to @mia, @jo, @ana");
    expect(assignee).toHaveAttribute("title", "Assigned to @mia, @jo, @ana");
  });

  it("DBR-FR-WQAN, DBR-FR-SBTR: a group face shows its one assignee with no count", async () => {
    await openAssigned();
    const face = itemEl("f1").querySelector(".dm-group__head") as HTMLElement;
    const assignee = within(face).getByTestId("flow-card-assignee");
    expect(assignee).toHaveTextContent(/^@kai$/);
    expect(assignee.closest(".dm-card__meta")).not.toBeNull();
  });

  it("DBR-FR-WQAN: an issue with no assignee and a milestone show no assignee", async () => {
    await openAssigned();
    expect(within(itemEl("t1")).getByTestId("flow-card-assignee")).toBeInTheDocument();
    expect(within(itemEl("x1")).getByTestId("flow-card-status")).toHaveTextContent("no status");
    expect(within(itemEl("x1")).queryByTestId("flow-card-assignee")).toBeNull();
    expect(within(itemEl("m1").querySelector(".dm-lane__head") as HTMLElement).queryByTestId("flow-card-assignee")).toBeNull();
  });

  it("DBR-FR-WQAN: tracker facts with no assignee list show no assignee", async () => {
    await openPlan();
    expect(within(itemEl("t1")).getByTestId("flow-card-status")).toHaveTextContent("ready");
    expect(screen.queryByTestId("flow-card-assignee")).toBeNull();
  });

  it("DBR-FR-GFGG, DBR-FR-WQAN: the bottom line starts with the status and ends with the markers and then the assignee", async () => {
    await openAssigned();
    const meta = itemEl("t1").querySelector(".dm-card__meta") as HTMLElement;
    expect([...meta.children].map((el) => el.className.split(" ")[0])).toEqual([
      "dm-card__status",
      "dm-card__markers",
      "dm-card__assignee",
    ]);
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

  it("DBR-FR-CVJY, DBR-FR-CJSG, MIV-FR-XMMS: a refused move puts the item back in its stack, restores its slot, and shows the error on it and in its issue view", async () => {
    b.apply = () => Promise.reject({ code: "request_failed", message: "GitHub refused the move." });
    await openPlan();
    const before = { top: itemEl("x1").style.top, left: itemEl("x1").style.left };
    dragDefectIntoFeature();
    const error = await screen.findByTestId("flow-item-error-x1");
    expect(error).toHaveAttribute("title", "GitHub refused the move.");
    expect(itemEl("x1")).toHaveAttribute("data-error", "true");
    await waitFor(() => expect(itemEl("x1").style.top).toBe(before.top));
    expect(itemEl("x1").style.left).toBe(before.left);
    // DBR-FR-CJSG: the error strip is below the bottom line of the card and holds the text.
    expect(itemEl("x1").querySelector(".dm-card__face")!.lastElementChild).toBe(error);
    expect(error).toHaveTextContent("GitHub refused the move.");
    // The stored slot of the item is the one it had before the drop.
    await waitFor(() => expect(b.layout.items.x1).toBeUndefined());
    selectItem("x1");
    expect(within(screen.getByTestId("map-issue-view")).getByTestId("flow-panel-error")).toHaveTextContent(
      "GitHub refused the move.",
    );
  });
});
