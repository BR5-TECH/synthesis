import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { resetToasts } from "../state/toasts";
import type { SyncConflict, SyncResult } from "../types";
import {
  callsOf,
  clearFlowStores,
  emptyResult,
  flowBackend,
  renderFlow,
} from "../test/flowCanvasHarness";
import { ID, itemEl } from "../test/flowFixtures";
import { resetListeners } from "../test/flowMocks";
import { issueBase, issueRef, samplePlan } from "../test/flowPlanFixtures";

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

const synced = () => {
  const doc = samplePlan();
  doc.sync = {
    tracker: "github",
    links: [
      { itemId: "f1", ref: issueRef(42), base: issueBase("F1") },
      { itemId: "t1", ref: issueRef(51), base: issueBase("T1") },
      { itemId: "gone", ref: issueRef(47), base: issueBase("Old spike") },
      { itemId: "d1", ref: issueRef(40), base: issueBase("D1") },
    ],
  };
  return doc;
};

const nameConflict: SyncConflict = {
  id: "c-name",
  kind: "field",
  itemId: "f1",
  itemName: "F1",
  field: "name",
  base: "F1",
  flow: "F1",
  tracker: "Card capture and 3DS",
  choices: ["keep_flow", "take_tracker"],
  url: "https://github.com/acme/payments/issues/42",
};

const invalidConflict: SyncConflict = {
  id: "c-parent",
  kind: "invalid_on_tracker",
  itemId: "t1",
  itemName: "T1",
  field: "parent",
  base: issueRef(42),
  flow: issueRef(42),
  tracker: issueRef(40),
  choices: ["keep_flow", "unlink"],
  url: "https://github.com/acme/payments/issues/51",
};

const goneConflict: SyncConflict = {
  id: "c-gone",
  kind: "removed_in_flow",
  itemId: "gone",
  itemName: "Old spike",
  field: null,
  base: null,
  flow: null,
  tracker: null,
  choices: ["unlink", "restore"],
  url: "https://github.com/acme/payments/issues/47",
};

const removedConflict: SyncConflict = {
  id: "c-removed",
  kind: "removed_on_tracker",
  itemId: "d1",
  itemName: "D1",
  field: null,
  base: null,
  flow: null,
  tracker: null,
  choices: ["remove_item", "unlink"],
  url: null,
};

const RESULT = emptyResult({
  conflicts: [goneConflict, invalidConflict, nameConflict],
  skipped: [
    {
      number: 58,
      title: "Load test plan",
      reason: "no_kind_fits",
      url: "https://github.com/acme/payments/issues/58",
    } as unknown as SyncResult["skipped"][number],
  ],
});

/** Render the synced plan, sync, and wait for the review. */
async function openReview(result: SyncResult = RESULT) {
  const b = flowBackend(synced());
  b.on.sync_flow_with_tracker = () => result;
  b.on.open_tracker_link = () => undefined;
  const r = await renderFlow(b);
  await userEvent.click(screen.getByRole("button", { name: "Sync" }));
  const review = await screen.findByTestId("flow-sync-review");
  return { ...r, b, review };
}

describe("opening and closing the review (FSR-FR-KGOY, FSR-FR-ODJL)", () => {
  it("FSR-FR-KGOY: a sync with conflicts opens the review as a panel inside the Flow tab, over the canvas", async () => {
    const { review } = await openReview();

    expect(review).toHaveAccessibleName("Sync review");
    expect(review.closest(".flow")).not.toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(review).toHaveFocus();
  });

  it("FSR-FR-KGOY, FSR-FR-ODJL: the close button closes it, and the count on the Sync control opens it again", async () => {
    await openReview();

    await userEvent.click(screen.getByRole("button", { name: "Close sync review" }));
    expect(screen.queryByTestId("flow-sync-review")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Review 4 sync items" }));
    expect(screen.getByTestId("flow-sync-review")).toBeInTheDocument();
  });

  it("FSR-FR-ODJL: Escape inside the panel closes it, and the choices not applied are discarded", async () => {
    const { review } = await openReview();

    await userEvent.click(within(review).getAllByRole("radio", { name: "Keep Flow" })[0]);
    fireEvent.keyDown(within(review).getAllByRole("radio")[0], { key: "Escape" });
    expect(screen.queryByTestId("flow-sync-review")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Review 4 sync items" }));
    const again = screen.getByTestId("flow-sync-review");
    expect(within(again).getAllByRole("radio").every((r) => !(r as HTMLInputElement).checked)).toBe(true);
    expect(within(again).getByRole("button", { name: "Apply" })).toBeDisabled();
  });
});

describe("the rows (FSR-FR-RDZI, FSR-FR-IFQE, FSR-FR-BRIA, FSR-FR-QAKW)", () => {
  it("FSR-FR-RDZI: conflicts are grouped by item in document order, and an item gone from the Flow is last", async () => {
    const { review } = await openReview();

    const groups = [...review.querySelectorAll("[data-testid^='flow-review-group-']")].map((g) =>
      g.getAttribute("data-testid"),
    );
    expect(groups).toEqual([
      "flow-review-group-f1",
      "flow-review-group-t1",
      "flow-review-group-gone",
    ]);
    const f1 = within(review).getByTestId("flow-review-group-f1");
    expect(within(f1).getByRole("button", { name: "Show F1 on the canvas" })).toBeInTheDocument();
    expect(within(f1).getByRole("button", { name: "Open #42 of F1 on GitHub" })).toHaveTextContent("#42");
  });

  it("FSR-FR-IFQE: a field conflict names the field and shows both values side by side", async () => {
    const { review } = await openReview();

    const row = within(review).getByTestId("flow-conflict-c-name");
    expect(row).toHaveTextContent("Title");
    expect(row).toHaveTextContent("In this FlowF1");
    expect(row).toHaveTextContent("On GitHubCard capture and 3DS");
  });

  it("FSR-FR-IFQE: a long text shows its first lines and expands in place; a parent shows the item name and number", async () => {
    const long = "word ".repeat(60).trim();
    const { review } = await openReview(
      emptyResult({
        conflicts: [
          { ...nameConflict, id: "c-desc", field: "description", flow: "Short", tracker: long },
          { ...nameConflict, id: "c-par", field: "parent", flow: issueRef(40), tracker: null },
          { ...nameConflict, id: "c-type", field: "type", flow: "Feature", tracker: "Epic" },
        ],
      }),
    );

    const desc = within(review).getByTestId("flow-conflict-c-desc");
    expect(desc.querySelector("[data-clamped='true']")).toHaveTextContent(long);
    await userEvent.click(within(desc).getByRole("button", { name: "Show all" }));
    expect(desc.querySelector("[data-clamped='true']")).toBeNull();
    const parent = within(review).getByTestId("flow-conflict-c-par");
    expect(parent).toHaveTextContent("In this FlowD1 (#40)");
    expect(parent).toHaveTextContent("On GitHubNone");
    expect(within(review).getByTestId("flow-conflict-c-type")).toHaveTextContent("On GitHubEpic");
  });

  it("FSR-FR-BRIA: each kind of conflict reads in words, and an invalid value names the rule it breaks", async () => {
    const { review } = await openReview(
      emptyResult({ conflicts: [goneConflict, invalidConflict, removedConflict] }),
    );

    expect(within(review).getByTestId("flow-conflict-c-gone")).toHaveTextContent("Removed in this Flow");
    expect(within(review).getByTestId("flow-conflict-c-removed")).toHaveTextContent("Not found on GitHub");
    const invalid = within(review).getByTestId("flow-conflict-c-parent");
    expect(invalid).toHaveTextContent("Not valid in this plan");
    expect(invalid).toHaveTextContent("GitHub sets its parent to D1 (#40). A task can sit only in a feature.");
  });

  it("FSR-FR-QAKW: each row offers exactly its choices, as one group of options, none chosen", async () => {
    const { review } = await openReview(
      emptyResult({ conflicts: [nameConflict, invalidConflict, goneConflict, removedConflict] }),
    );

    const words = (id: string) =>
      within(within(review).getByTestId(`flow-conflict-${id}`))
        .getAllByRole("radio")
        .map((r) => ({ name: r.closest("label")!.textContent, checked: (r as HTMLInputElement).checked }));
    expect(words("c-name")).toEqual([
      { name: "Keep Flow", checked: false },
      { name: "Take GitHub", checked: false },
    ]);
    expect(words("c-parent").map((w) => w.name)).toEqual(["Keep Flow", "Unlink"]);
    expect(words("c-gone").map((w) => w.name)).toEqual(["Unlink", "Restore"]);
    expect(words("c-removed").map((w) => w.name)).toEqual(["Remove item", "Unlink"]);
    const row = within(review).getByTestId("flow-conflict-c-name");
    await userEvent.click(within(row).getByRole("radio", { name: "Take GitHub" }));
    await userEvent.click(within(row).getByRole("radio", { name: "Keep Flow" }));
    expect(within(row).getByRole("radio", { name: "Take GitHub" })).not.toBeChecked();
  });

  it("FSR-FR-MMVF: each choice group names the item and the field, and the rows are operable by keyboard", async () => {
    const { review } = await openReview();

    expect(within(review).getByRole("radiogroup", { name: "F1: Title" })).toBeInTheDocument();
    expect(within(review).getByRole("radiogroup", { name: "T1: Not valid in this plan" })).toBeInTheDocument();
    expect(within(review).getByRole("radiogroup", { name: "Old spike: Removed in this Flow" })).toBeInTheDocument();
    await userEvent.tab();
    await userEvent.tab();
    expect(document.activeElement?.closest("[data-testid='flow-sync-review']")).not.toBeNull();
    const first = within(within(review).getByRole("radiogroup", { name: "F1: Title" })).getAllByRole("radio")[0];
    first.focus();
    await userEvent.keyboard(" ");
    expect(first).toBeChecked();
  });
});

describe("skipped issues and links (FSR-FR-CSOT, FSR-FR-KDMM, FSR-FR-PGBX)", () => {
  it("FSR-FR-CSOT: a Skipped section lists each skipped issue with its number, title, and reason, and offers no choice", async () => {
    const { review } = await openReview(
      emptyResult({
        skipped: [
          { number: 58, title: "Load test plan", reason: "no_kind_fits" },
          { number: 61, title: "Vendor", reason: "foreign_repository" },
          { number: 62, title: "Spike", reason: "issue_type_unavailable" },
        ],
      }),
    );

    const skipped = within(review).getByRole("heading", { name: "Skipped" }).parentElement!;
    const rows = within(skipped).getAllByRole("listitem");
    expect(rows.map((r) => r.textContent)).toEqual([
      "#58 Load test planNo kind fits its Type",
      "#61 VendorIt is in another repository",
      "#62 SpikeIts Type is not available in the repository",
    ]);
    expect(within(skipped).queryByRole("radio")).toBeNull();
  });

  it("FSR-FR-KDMM: a group's link and a skipped issue's link invoke \"open tracker link\" with their URL", async () => {
    const { review, b } = await openReview();

    await userEvent.click(within(review).getByRole("button", { name: "Open #51 of T1 on GitHub" }));
    await userEvent.click(within(review).getByRole("button", { name: "Open #58 Load test plan on GitHub" }));
    await waitFor(() => expect(callsOf(b, "open_tracker_link")).toHaveLength(2));
    expect(callsOf(b, "open_tracker_link")).toEqual([
      { url: "https://github.com/acme/payments/issues/51" },
      { url: "https://github.com/acme/payments/issues/58" },
    ]);
  });

  it("FSR-FR-PGBX: an item's name selects it on the canvas; the name of an item that is gone selects nothing", async () => {
    const { review } = await openReview();

    await userEvent.click(within(review).getByRole("button", { name: "Show T1 on the canvas" }));
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
    const viewport = document.querySelector(".flow-viewport") as HTMLElement;
    // t1 is at (60, 180), 170 × 96: its middle (145, 228) goes to the middle of an 800 × 600 view.
    expect(viewport.style.transform).toContain("translate(255px, 72px)");
    const gone = within(review).getByTestId("flow-review-group-gone");
    expect(within(gone).queryByRole("button", { name: /on the canvas$/ })).toBeNull();
    expect(within(gone).getByText("Old spike")).toBeInTheDocument();
  });

  it("FSR-FR-PGBX: an item in a collapsed container is shown with its containers expanded", async () => {
    const { review } = await openReview();

    // Close the review to collapse, then reopen it.
    await userEvent.click(screen.getByRole("button", { name: "Close sync review" }));
    await userEvent.click(screen.getByRole("button", { name: "Collapse F1" }));
    expect(screen.queryByTestId("flow-item-t1")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Review 4 sync items" }));
    void review;
    await userEvent.click(screen.getByRole("button", { name: "Show T1 on the canvas" }));
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
  });
});

describe("Apply (FSR-FR-IDRR, FSR-FR-AUWG, FSR-FR-UHAM, FSR-FR-BXJJ)", () => {
  it("FSR-FR-IDRR: Apply is enabled once a row has a choice, writes pending changes first, and sends the chosen rows alone", async () => {
    const { review, b } = await openReview();
    b.on.resolve_flow_sync_conflicts = () => emptyResult({ conflicts: [invalidConflict, goneConflict] });

    const apply = within(review).getByRole("button", { name: "Apply" });
    expect(apply).toBeDisabled();
    await userEvent.type(screen.getByRole("textbox", { name: "Flow description" }), "Q3");
    await userEvent.click(within(within(review).getByTestId("flow-conflict-c-name")).getByRole("radio", { name: "Take GitHub" }));
    expect(apply).toBeEnabled();
    await userEvent.click(apply);
    await waitFor(() => expect(callsOf(b, "resolve_flow_sync_conflicts")).toHaveLength(1));
    expect(callsOf(b, "resolve_flow_sync_conflicts")[0]).toEqual({
      flowId: ID,
      resolutions: [{ conflictId: "c-name", choice: "take_tracker" }],
    });
    const order = b.calls.map((c) => c.cmd);
    expect(order.lastIndexOf("save_artifact_contents")).toBeLessThan(order.indexOf("resolve_flow_sync_conflicts"));
  });

  it("FSR-FR-IDRR: the tab is inert while the call runs", async () => {
    const { review, b } = await openReview();
    let finish!: (r: SyncResult) => void;
    b.on.resolve_flow_sync_conflicts = () => new Promise((res) => (finish = res));

    await userEvent.click(within(review).getAllByRole("radio", { name: "Keep Flow" })[0]);
    await userEvent.click(within(review).getByRole("button", { name: "Apply" }));
    expect(await within(review).findByRole("button", { name: "Applying…" })).toBeDisabled();
    expect(screen.getByRole("textbox", { name: "Flow name" })).toBeDisabled();
    await act(async () => finish(emptyResult()));
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Flow name" })).toBeEnabled());
  });

  it("FSR-FR-AUWG: the result replaces the list; a stale conflict stays, marked, with no choice, and Sync again runs a sync", async () => {
    const { review, b } = await openReview();
    b.on.resolve_flow_sync_conflicts = () =>
      emptyResult({ conflicts: [goneConflict], stale: ["c-name"] });

    await userEvent.click(within(within(review).getByTestId("flow-conflict-c-name")).getByRole("radio", { name: "Keep Flow" }));
    await userEvent.click(within(within(review).getByTestId("flow-conflict-c-parent")).getByRole("radio", { name: "Unlink" }));
    await userEvent.click(within(review).getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(within(review).queryByTestId("flow-conflict-c-parent")).toBeNull());
    const stale = within(review).getByTestId("flow-conflict-c-name");
    expect(stale).toHaveTextContent("Changed since the sync");
    expect(within(stale).queryByRole("radio")).toBeNull();
    expect(within(review).getByTestId("flow-conflict-c-gone")).toBeInTheDocument();
    await userEvent.click(within(review).getByRole("button", { name: "Sync again" }));
    await waitFor(() => expect(callsOf(b, "sync_flow_with_tracker")).toHaveLength(2));
  });

  it("FSR-FR-UHAM: a result with nothing left shows Everything is in sync, and the Sync control loses its count", async () => {
    const { review, b } = await openReview(emptyResult({ conflicts: [nameConflict] }));
    b.on.resolve_flow_sync_conflicts = () => emptyResult();

    await userEvent.click(within(review).getByRole("radio", { name: "Keep Flow" }));
    await userEvent.click(within(review).getByRole("button", { name: "Apply" }));
    expect(await within(review).findByText("Everything is in sync")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Review \d/ })).toBeNull();
  });

  it("FSR-FR-BXJJ: a refused Apply shows its typed text in the panel and keeps every row and every choice", async () => {
    const { review, b } = await openReview();
    b.on.resolve_flow_sync_conflicts = () =>
      Promise.reject({ code: "github_unreachable", message: "GitHub did not answer." });

    const take = within(within(review).getByTestId("flow-conflict-c-name")).getByRole("radio", { name: "Take GitHub" });
    await userEvent.click(take);
    await userEvent.click(within(review).getByRole("button", { name: "Apply" }));
    expect(await within(review).findByRole("alert")).toHaveTextContent("GitHub did not answer.");
    expect(within(within(review).getByTestId("flow-conflict-c-name")).getByRole("radio", { name: "Take GitHub" })).toBeChecked();
    expect(within(review).getByTestId("flow-conflict-c-parent")).toBeInTheDocument();
    expect(within(review).getByTestId("flow-conflict-c-gone")).toBeInTheDocument();
  });

  it("FSR-FR-BXJJ: a refused write before Apply shows its text and sends nothing", async () => {
    const { review, b } = await openReview();
    b.on.resolve_flow_sync_conflicts = () => emptyResult();

    await userEvent.type(screen.getByRole("textbox", { name: "Flow description" }), "x");
    b.validation = { valid: false, violations: [{ code: "dependency_cycle", message: "a cycle" }] };
    await userEvent.click(within(review).getAllByRole("radio", { name: "Keep Flow" })[0]);
    await userEvent.click(within(review).getByRole("button", { name: "Apply" }));
    expect(await within(review).findByRole("alert")).toHaveTextContent("a cycle");
    expect(callsOf(b, "resolve_flow_sync_conflicts")).toEqual([]);
  });
});
