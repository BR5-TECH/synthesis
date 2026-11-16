/**
 * The popover of a selected item of the Delivery lens
 * (`../../../specifications/ui/MDL-map-delivery-lens.md` MDL-FR-XKCH,
 * MDL-FR-XCJN, MDL-FR-QEXS) and the status names it reads
 * (`DBR-delivery-board.md` DBR-FR-TCPE).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));

import { resetLogBufferForTest } from "../../logging";
import { resetToasts } from "../../state/toasts";
import { itemEl, panel, selectItem } from "../../test/flowFixtures";
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

describe("the popover of the board (MDL-FR-XKCH, MDL-FR-XCJN)", () => {
  it("MDL-FR-XKCH: the header shows the kind chip, the address, the name, Rename, and Remove; each section has its label", async () => {
    await openPlan();
    selectItem("t2");
    const pop = panel();
    expect(pop).toHaveClass("dm-pop");
    const head = pop.querySelector(".dm-pop__head") as HTMLElement;
    expect(within(head).getByTestId("flow-kind-chip")).toHaveTextContent("TSK");
    expect(within(head).getByTestId("flow-tracker-tag")).toHaveTextContent("#53");
    expect(head.querySelector(".dm-pop__title")).toHaveTextContent("T2");
    expect(within(head).getByRole("button", { name: "Rename" })).toHaveClass("btn--ghost");
    expect(within(head).getByRole("button", { name: "Remove" })).toHaveClass("btn--danger");
    const labels = [...pop.querySelectorAll(".dm-pop__body .flow-panel__label")].map((l) => l.textContent);
    expect(labels.slice(0, 3)).toEqual(["Status", "Description", "Kind"]);
    expect(within(screen.getByTestId("flow-panel-status")).getByTestId("flow-card-status")).toHaveTextContent("doing");
  });

  it("MDL-FR-XKCH: the popover of a milestone has a flag in place of a kind chip and no status section", async () => {
    await openPlan();
    selectItem("m1");
    const head = panel().querySelector(".dm-pop__head") as HTMLElement;
    expect(within(head).queryByTestId("flow-kind-chip")).toBeNull();
    expect(within(head).getByLabelText("Milestone")).toBeInTheDocument();
    expect(screen.queryByTestId("flow-panel-status")).toBeNull();
  });

  it("MDL-FR-XCJN: Discuss in the footer hands the item to the Discussion column", async () => {
    const r = await openPlan();
    selectItem("t2");
    const foot = screen.getByTestId("flow-panel-foot");
    await userEvent.click(within(foot).getByRole("button", { name: "Discuss" }));
    const { discussion } = r.store.snapshot();
    expect(discussion.visible).toBe(true);
    expect(discussion.node).toEqual({ lens: "delivery", kind: "item", id: "t2", label: "T2" });
  });

  it("MDL-FR-XCJN: Discuss in the footer of a milestone hands the milestone to the Discussion column", async () => {
    const r = await openPlan();
    selectItem("m1");
    await userEvent.click(within(screen.getByTestId("flow-panel-foot")).getByRole("button", { name: "Discuss" }));
    expect(r.store.snapshot().discussion.node).toEqual({ lens: "delivery", kind: "milestone", id: "m1", label: "M1" });
  });

  it("MDL-FR-XKCH, MDL-FR-QEXS: the claim action is the primary button of the footer, after Discuss, and only where it is offered", async () => {
    const claims = claimsOf();
    await openPlan({ claims });
    selectItem("t1");
    const foot = screen.getByTestId("flow-panel-foot");
    const buttons = within(foot).getAllByRole("button").map((x) => x.textContent);
    expect(buttons).toEqual(["Discuss", "Claim and graduate"]);
    const claim = within(foot).getByRole("button", { name: "Claim and graduate" });
    expect(claim).toHaveClass("btn--primary");
    await userEvent.click(claim);
    expect(claims.claim).toHaveBeenCalledWith(51);
    selectItem("t2");
    expect(within(screen.getByTestId("flow-panel-foot")).queryByRole("button", { name: "Claim and graduate" })).toBeNull();
  });
});

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
