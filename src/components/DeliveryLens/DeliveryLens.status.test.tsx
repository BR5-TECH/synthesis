/**
 * The status names of the settings and the done count of a GitHub plan on the
 * board of the Delivery lens (`../../../specifications/ui/DBR-delivery-board.md`
 * DBR-FR-TCPE, DBR-FR-VNSJ).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, within } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));

import { resetLogBufferForTest } from "../../logging";
import { resetToasts } from "../../state/toasts";
import { itemEl } from "../../test/flowFixtures";
import { resetListeners } from "../../test/flowMocks";
import { item, plan } from "../../test/flowPlanFixtures";
import {
  claimsOf,
  clearLensStores,
  deliveryBackend,
  githubAnswer,
  githubSource,
  issueFacts,
  leafState,
  pollingView,
  renderLens,
  settle,
  type DeliveryBackend,
} from "../../test/deliveryLensHarness";

const DOC = plan([
  item("m1", "milestone"),
  item("f1", "feature", { parentId: "m1" }),
  item("t1", "task", { parentId: "f1" }),
  item("t2", "task", { parentId: "f1" }),
]);
const TRACKER = [
  issueFacts("f1", 42),
  issueFacts("t1", 51, { claimEligible: true, status: "Up next" }),
  issueFacts("t2", 53, { status: "Doing" }),
];

let b: DeliveryBackend;

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
  const answer = githubAnswer(DOC, { tracker: TRACKER, states: DOC.items.map((i) => leafState(i.id, "not_started")) });
  b = deliveryBackend("", answer);
  b.plan = () => answer;
});

afterEach(() => {
  cleanup();
  clearLensStores();
});

async function openPlan(host = {}) {
  const r = await renderLens({ source: githubSource(), host });
  await screen.findByTestId("flow-item-t1");
  await settle();
  return r;
}

describe("the status names of the settings (DBR-FR-TCPE)", () => {
  it("DBR-FR-TCPE: the Ready and In Progress status names of the GitHub Project settings give the ready and in progress dots", async () => {
    const view = pollingView();
    const claims = claimsOf({
      ...view,
      settings: { ...view.settings, readyStatus: "Up next", inProgressStatus: "Doing" },
    });
    await openPlan({ claims });
    expect(within(itemEl("t1")).getByTestId("flow-status-dot")).toHaveAttribute("data-s", "ready");
    expect(within(itemEl("t2")).getByTestId("flow-status-dot")).toHaveAttribute("data-s", "in progress");
  });

  it("DBR-FR-TCPE: with no such settings, those names give the backlog dot", async () => {
    await openPlan();
    expect(within(itemEl("t1")).getByTestId("flow-status-dot")).toHaveAttribute("data-s", "backlog");
    expect(within(itemEl("t2")).getByTestId("flow-status-dot")).toHaveAttribute("data-s", "backlog");
  });
});

describe("the done count of a GitHub plan (DBR-FR-VNSJ)", () => {
  it("DBR-FR-VNSJ: a closed issue and an issue with the Done status count as done; the delivery state does not", async () => {
    const doc = plan([
      item("m1", "milestone"),
      item("f1", "feature", { parentId: "m1" }),
      item("t1", "task", { parentId: "f1" }),
      item("t2", "task", { parentId: "f1" }),
      item("t3", "task", { parentId: "f1" }),
    ]);
    const tracker = [
      issueFacts("f1", 42),
      issueFacts("t1", 51, { state: "closed", status: "Todo" }),
      issueFacts("t2", 52, { status: "Done" }),
      issueFacts("t3", 53, { status: "Todo" }),
    ];
    const states = [leafState("t3", "done")];
    const answer = githubAnswer(doc, { tracker, states });
    b = deliveryBackend("", answer);
    b.plan = () => answer;
    await openPlan();
    const face = itemEl("f1").querySelector(".dm-group__head") as HTMLElement;
    const count = within(face).getByTestId("flow-leaf-counts-f1");
    expect(count.closest(".dm-card__id")).not.toBeNull();
    expect(count).toHaveTextContent("2/3");
    expect(within(itemEl("m1")).getByTestId("flow-leaf-counts-m1")).toHaveTextContent("2/3");
  });
});
