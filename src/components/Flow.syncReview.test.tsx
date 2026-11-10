import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { resetToasts } from "../state/toasts";
import type { Discussion, SyncConflict, SyncResult } from "../types";
import {
  callsOf,
  clearFlowStores,
  emptyResult,
  endFakeTimers,
  flowBackend,
  renderFlow,
} from "../test/flowCanvasHarness";
import { ID, itemEl } from "../test/flowFixtures";
import { resetListeners } from "../test/flowMocks";
import { issueBase, issueRef, milestoneRef, samplePlan } from "../test/flowPlanFixtures";

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

afterEach(async () => {
  await endFakeTimers();
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
      { itemId: "ep1", ref: issueRef(41), base: issueBase("EP1") },
      { itemId: "x1", ref: issueRef(45), base: issueBase("X1") },
      { itemId: "m1", ref: milestoneRef(3), base: { name: "M1", description: "", state: "open", due: null } },
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
  tracker: issueRef(41),
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
  itemId: "x1",
  itemName: "X1",
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
      url: "https://github.com/acme/payments/issues/58",
      reason: "no_kind_fits",
    },
  ],
});

const band = () => screen.getByTestId("flow-band");

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
    await waitFor(() => expect(review).toHaveFocus());
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

  it("FSR-FR-ODJL: Escape while focus is outside the panel keeps it open, with its choices", async () => {
    const { review } = await openReview();

    const keep = within(review).getAllByRole("radio", { name: "Keep Flow" })[0];
    await userEvent.click(keep);
    const name = screen.getByRole("textbox", { name: "Flow name" });
    name.focus();
    fireEvent.keyDown(name, { key: "Escape" });
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByTestId("flow-sync-review")).toBeInTheDocument();
    expect(keep).toBeChecked();
  });

  it("FSR-FR-KGOY: opening the review closes no floating overlay", async () => {
    const opened: Discussion = {
      id: "dsc1",
      target: { kind: "artifact", artifactId: ID },
      fragmentTarget: null,
      comments: [
        {
          id: "c1",
          author: { kind: "human", login: "raver119" },
          body: "Check the refunds.",
          quotes: [],
          attachments: [],
          createdAt: "2026-01-01T00:00:00Z",
        },
      ],
      resolved: false,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    };
    const b = flowBackend(synced());
    b.on.sync_flow_with_tracker = () => RESULT;
    b.on.list_discussions = () => [opened];
    b.on.list_project_agents = () => [];
    b.on.list_agent_turns = () => [];
    await renderFlow(b);

    await userEvent.click(await screen.findByRole("button", { name: "Show discussion (1)" }));
    expect(screen.getByRole("dialog", { name: "Discussion" })).toBeInTheDocument();
    // A sync opens the review with its result.
    await userEvent.click(screen.getByRole("button", { name: "Sync" }));
    await screen.findByTestId("flow-sync-review");
    expect(screen.getByRole("dialog", { name: "Discussion" })).toBeInTheDocument();
    // The count on the Sync control opens it again.
    await userEvent.click(screen.getByRole("button", { name: "Close sync review" }));
    await userEvent.click(screen.getByRole("button", { name: "Review 4 sync items" }));
    expect(screen.getByTestId("flow-sync-review")).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Discussion" })).toBeInTheDocument();
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

  it("FSR-FR-RDZI: the conflicts of one item form one group, in the order of the result", async () => {
    const { review } = await openReview(
      emptyResult({
        conflicts: [
          nameConflict,
          { ...nameConflict, id: "c-desc", field: "description", flow: "A", tracker: "B" },
          invalidConflict,
        ],
      }),
    );

    const groups = [...review.querySelectorAll("[data-testid^='flow-review-group-']")];
    expect(groups.map((g) => g.getAttribute("data-testid"))).toEqual([
      "flow-review-group-f1",
      "flow-review-group-t1",
    ]);
    const rows = [...groups[0].querySelectorAll("[data-testid^='flow-conflict-']")];
    expect(rows.map((r) => r.getAttribute("data-testid"))).toEqual([
      "flow-conflict-c-name",
      "flow-conflict-c-desc",
    ]);
  });

  it("FSR-FR-RDZI: the group of an item gone from the Flow is headed by its name and its tracker tag", async () => {
    const { review } = await openReview(emptyResult({ conflicts: [goneConflict] }));

    const gone = within(review).getByTestId("flow-review-group-gone");
    expect(within(gone).getByText("Old spike")).toBeInTheDocument();
    expect(within(gone).getByRole("button", { name: "Open #47 of Old spike on GitHub" })).toHaveTextContent("#47");
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
          { ...nameConflict, id: "c-par", field: "parent", flow: issueRef(41), tracker: null },
          { ...nameConflict, id: "c-type", field: "type", flow: "Feature", tracker: "Epic" },
        ],
      }),
    );

    const desc = within(review).getByTestId("flow-conflict-c-desc");
    expect(desc.querySelector("[data-clamped='true']")).toHaveTextContent(long);
    await userEvent.click(within(desc).getByRole("button", { name: "Show all" }));
    expect(desc.querySelector("[data-clamped='true']")).toBeNull();
    const parent = within(review).getByTestId("flow-conflict-c-par");
    expect(parent).toHaveTextContent("In this FlowEP1 (#41)");
    expect(parent).toHaveTextContent("On GitHubNone");
    expect(within(review).getByTestId("flow-conflict-c-type")).toHaveTextContent("On GitHubEpic");
  });

  it("FSR-FR-IFQE: a milestone shows the milestone item's name and number, or the number alone when no item links it", async () => {
    const { review } = await openReview(
      emptyResult({
        conflicts: [
          { ...nameConflict, id: "c-ms", field: "milestone", base: 3, flow: 3, tracker: 4 },
          { ...nameConflict, id: "c-ms2", itemId: "t1", itemName: "T1", field: "milestone", base: null, flow: null, tracker: 3 },
        ],
      }),
    );

    const row = within(review).getByTestId("flow-conflict-c-ms");
    expect(row).toHaveTextContent("Milestone");
    expect(row).toHaveTextContent("In this FlowM1 (milestone 3)");
    expect(row).toHaveTextContent("On GitHubmilestone 4");
    const other = within(review).getByTestId("flow-conflict-c-ms2");
    expect(other).toHaveTextContent("In this FlowNone");
    expect(other).toHaveTextContent("On GitHubM1 (milestone 3)");
    expect(within(review).getByRole("radiogroup", { name: "F1: Milestone" })).toBeInTheDocument();
  });

  it("FSR-FR-BRIA: each kind of conflict reads in words, and an invalid value names the rule it breaks", async () => {
    const { review } = await openReview(
      emptyResult({ conflicts: [goneConflict, invalidConflict, removedConflict] }),
    );

    expect(within(review).getByTestId("flow-conflict-c-gone")).toHaveTextContent("Removed in this Flow");
    expect(within(review).getByTestId("flow-conflict-c-removed")).toHaveTextContent("Not found on GitHub");
    const invalid = within(review).getByTestId("flow-conflict-c-parent");
    expect(invalid).toHaveTextContent("Not valid in this plan");
    expect(invalid).toHaveTextContent("GitHub sets its parent to EP1 (#41). A task can sit only in a feature.");
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

describe("the review by keyboard alone (FSR-FR-MMVF)", () => {
  it("FSR-FR-MMVF: from the Sync control, the keyboard alone opens the review, makes a choice, and applies it", async () => {
    const b = flowBackend(synced());
    b.on.sync_flow_with_tracker = () => RESULT;
    b.on.resolve_flow_sync_conflicts = () => emptyResult({ conflicts: [goneConflict, invalidConflict] });
    await renderFlow(b);

    within(screen.getByTestId("flow-band")).getByRole("button", { name: "Sync" }).focus();
    await userEvent.keyboard("{Enter}");
    const review = await screen.findByTestId("flow-sync-review");
    await waitFor(() => expect(review).toHaveFocus());

    const group = within(review).getByRole("radiogroup", { name: "F1: Title" });
    for (let i = 0; i < 30 && !group.contains(document.activeElement); i += 1) await userEvent.tab();
    expect(group).toContainElement(document.activeElement as HTMLElement);
    await userEvent.keyboard(" ");
    expect(within(group).getByRole("radio", { name: "Keep Flow" })).toBeChecked();
    await userEvent.keyboard("{ArrowDown}");
    expect(within(group).getByRole("radio", { name: "Take GitHub" })).toBeChecked();

    const apply = within(review).getByRole("button", { name: "Apply" });
    for (let i = 0; i < 30 && document.activeElement !== apply; i += 1) await userEvent.tab();
    expect(apply).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(callsOf(b, "resolve_flow_sync_conflicts")).toHaveLength(1));
    expect(callsOf(b, "resolve_flow_sync_conflicts")[0]).toEqual({
      flowId: ID,
      resolutions: [{ conflictId: "c-name", choice: "take_tracker" }],
    });
  });
});

describe("skipped issues and links (FSR-FR-CSOT, FSR-FR-KDMM, FSR-FR-PGBX)", () => {
  it("FSR-FR-CSOT: a Skipped section lists each skipped issue with its number, title, and reason, and offers no choice", async () => {
    const { review } = await openReview(
      emptyResult({
        skipped: [
          { number: 58, title: "Load test plan", url: "https://github.com/acme/payments/issues/58", reason: "no_kind_fits" },
          { number: 61, title: "Vendor", url: "https://github.com/acme/vendor/issues/61", reason: "foreign_repository" },
          { number: null, title: "Spike", url: null, itemId: "t2", reason: "issue_type_unavailable" },
        ],
      }),
    );

    const skipped = within(review).getByRole("heading", { name: "Skipped" }).parentElement!;
    const rows = within(skipped).getAllByRole("listitem");
    expect(rows.map((r) => r.textContent)).toEqual([
      "#58 Load test plan ↗No kind fits its Type",
      "#61 Vendor ↗It is in another repository",
      "SpikeIts Type is not available in the repository",
    ]);
    expect(within(skipped).queryByRole("radio")).toBeNull();
    // FSR-FR-KDMM: a skipped issue links through its URL; an item whose issue
    // was not created has no URL and no link.
    expect(within(rows[0]).getByRole("button", { name: "Open #58 Load test plan on GitHub" })).toBeInTheDocument();
    expect(within(rows[1]).getByRole("button", { name: "Open #61 Vendor on GitHub" })).toBeInTheDocument();
    expect(within(rows[2]).queryByRole("button")).toBeNull();
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

  it("FSR-FR-KDMM: a row's link opens its conflict's URL, also when the item links another URL", async () => {
    const elsewhere = "https://github.com/acme/payments/issues/42#issuecomment-7";
    const { review, b } = await openReview(emptyResult({ conflicts: [{ ...nameConflict, url: elsewhere }] }));

    const row = within(review).getByTestId("flow-conflict-c-name");
    await userEvent.click(within(row).getByRole("button", { name: "Open F1: Title on GitHub" }));
    await waitFor(() => expect(callsOf(b, "open_tracker_link")).toEqual([{ url: elsewhere }]));
  });

  it("FSR-FR-KDMM: two conflicts of one item with two URLs each have their own link; a conflict with no URL has none", async () => {
    const first = "https://github.com/acme/payments/issues/42";
    const second = "https://github.com/acme/payments/milestone/3";
    const { review, b } = await openReview(
      emptyResult({
        conflicts: [
          { ...nameConflict, url: first },
          { ...nameConflict, id: "c-ms", field: "milestone", base: 3, flow: 3, tracker: 4, url: second },
          { ...nameConflict, id: "c-desc", field: "description", flow: "A", tracker: "B", url: null },
        ],
      }),
    );

    const f1 = within(review).getByTestId("flow-review-group-f1");
    await userEvent.click(within(within(f1).getByTestId("flow-conflict-c-name")).getByRole("button", { name: /on GitHub$/ }));
    await userEvent.click(within(within(f1).getByTestId("flow-conflict-c-ms")).getByRole("button", { name: /on GitHub$/ }));
    expect(within(within(f1).getByTestId("flow-conflict-c-desc")).queryByRole("button", { name: /on GitHub$/ })).toBeNull();
    await waitFor(() => expect(callsOf(b, "open_tracker_link")).toHaveLength(2));
    expect(callsOf(b, "open_tracker_link")).toEqual([{ url: first }, { url: second }]);
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
    await openReview();

    // Close the review to collapse, then reopen it.
    await userEvent.click(screen.getByRole("button", { name: "Close sync review" }));
    await userEvent.click(screen.getByRole("button", { name: "Collapse F1" }));
    expect(screen.queryByTestId("flow-item-t1")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Review 4 sync items" }));
    await userEvent.click(screen.getByRole("button", { name: "Show T1 on the canvas" }));
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
  });
});

describe("Apply (FSR-FR-IDRR, FSR-FR-AUWG, FSR-FR-UHAM, FSR-FR-BXJJ)", () => {
  it("FSR-FR-IDRR: Apply is enabled once a row has a choice, writes pending changes first, and sends the chosen rows alone", async () => {
    const { review, b, flows } = await openReview();
    /** What was on disk when the resolve call came. */
    let onDisk: string[] | null = null;
    b.on.resolve_flow_sync_conflicts = () => {
      onDisk = [...b.writes];
      return emptyResult({ conflicts: [invalidConflict, goneConflict] });
    };
    const before = b.writes.length;
    // The clock is held below the rest, so only Apply can write the edit.
    vi.useFakeTimers();

    const apply = within(review).getByRole("button", { name: "Apply" });
    expect(apply).toBeDisabled();
    fireEvent.change(screen.getByRole("textbox", { name: "Flow description" }), { target: { value: "Q3" } });
    expect(flows.hasPendingWrite(ID)).toBe(true);
    fireEvent.click(within(within(review).getByTestId("flow-conflict-c-name")).getByRole("radio", { name: "Take GitHub" }));
    expect(apply).toBeEnabled();
    fireEvent.click(apply);
    await vi.waitFor(() => expect(callsOf(b, "resolve_flow_sync_conflicts")).toHaveLength(1));
    expect(callsOf(b, "resolve_flow_sync_conflicts")[0]).toEqual({
      flowId: ID,
      resolutions: [{ conflictId: "c-name", choice: "take_tracker" }],
    });
    expect(onDisk).toHaveLength(before + 1);
    expect(JSON.parse(onDisk![before]).description).toBe("Q3");
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

  it("FSR-FR-AUWG: a stale conflict that the result also lists renders once, marked stale", async () => {
    const { review, b } = await openReview();
    b.on.resolve_flow_sync_conflicts = () =>
      emptyResult({ conflicts: [nameConflict, goneConflict], stale: ["c-name"] });

    await userEvent.click(within(within(review).getByTestId("flow-conflict-c-name")).getByRole("radio", { name: "Keep Flow" }));
    await userEvent.click(within(review).getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(within(review).queryByTestId("flow-conflict-c-parent")).toBeNull());
    const rows = within(review).getAllByTestId("flow-conflict-c-name");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveAttribute("data-stale", "true");
    expect(rows[0]).toHaveTextContent("Changed since the sync");
    expect(within(rows[0]).queryByRole("radio")).toBeNull();
  });

  it("FSR-FR-AUWG, FLO-FR-AKMV: Sync again first writes the pending changes", async () => {
    const { review, b, flows } = await openReview();
    b.on.resolve_flow_sync_conflicts = () => emptyResult({ conflicts: [goneConflict], stale: ["c-name"] });
    await userEvent.click(within(within(review).getByTestId("flow-conflict-c-name")).getByRole("radio", { name: "Keep Flow" }));
    await userEvent.click(within(review).getByRole("button", { name: "Apply" }));
    const again = await within(review).findByRole("button", { name: "Sync again" });
    let onDisk: string[] | null = null;
    b.on.sync_flow_with_tracker = () => {
      onDisk = [...b.writes];
      return emptyResult({ conflicts: [goneConflict] });
    };
    const before = b.writes.length;
    // The clock is held below the rest, so only Sync again can write the edit.
    vi.useFakeTimers();

    fireEvent.change(screen.getByRole("textbox", { name: "Flow description" }), { target: { value: "Q4" } });
    expect(flows.hasPendingWrite(ID)).toBe(true);
    fireEvent.click(again);
    await vi.waitFor(() => expect(callsOf(b, "sync_flow_with_tracker")).toHaveLength(2));
    expect(onDisk).toHaveLength(before + 1);
    expect(JSON.parse(onDisk![before]).description).toBe("Q4");
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
    // The clock is held below the rest, so only Apply writes the edit.
    vi.useFakeTimers();

    fireEvent.change(screen.getByRole("textbox", { name: "Flow description" }), { target: { value: "x" } });
    b.validation = { valid: false, violations: [{ code: "dependency_cycle", message: "a cycle" }] };
    fireEvent.click(within(review).getAllByRole("radio", { name: "Keep Flow" })[0]);
    fireEvent.click(within(review).getByRole("button", { name: "Apply" }));
    await vi.waitFor(() => expect(within(band()).getByRole("button", { name: "Sync" })).toBeEnabled());
    expect(within(review).getByRole("alert")).toHaveTextContent("a cycle");
    expect(callsOf(b, "save_artifact_contents")).toEqual([]);
    expect(callsOf(b, "resolve_flow_sync_conflicts")).toEqual([]);
  });
});
