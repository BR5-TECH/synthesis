/**
 * The Delivery lens over a GitHub plan
 * (`../../../specifications/ui/MDL-map-delivery-lens.md` MDL-FR-XJBV,
 * MDL-FR-LZIE, MDL-FR-NRJM, MDL-FR-LSKL, MDL-FR-KKYH, MDL-FR-PUCM,
 * MDL-FR-XWGP, MDL-FR-HNLP, MDL-FR-IEWL, MDL-FR-XUJY, MDL-FR-MWMO,
 * MDL-FR-JPYU, MDL-FR-ECPZ, MDL-FR-QAJN, MDL-FR-NTVL, MDL-FR-GDUP;
 * `ACT-action-control.md` ACT-FR-UNJB).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));

import { resetLogBufferForTest } from "../../logging";
import { resetToasts, visibleToasts } from "../../state/toasts";
import type { DeliverySource } from "../../types/delivery";
import { renameItem } from "../../state/flowDocument";
import { callsOf } from "../../test/flowCanvasHarness";
import { itemEl, selectItem } from "../../test/flowFixtures";
import { emitEvent, resetListeners } from "../../test/flowMocks";
import { blocks, item, plan } from "../../test/flowPlanFixtures";
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
import { CHANGED_ON_GITHUB } from "../../state/delivery/githubPlan";
import type { GithubDeliveryPlan } from "../../types/delivery";

const DOC = plan(
  [
    item("m1", "milestone", { due: "2027-03-31" }),
    item("f1", "feature", { parentId: "m1" }),
    item("t1", "task", { parentId: "f1" }),
    item("t2", "task", { parentId: "f1" }),
    item("x1", "defect"),
  ],
  [blocks("e1", "t1", "t2")],
);

const TRACKER = [
  milestoneFacts("m1", 3),
  issueFacts("f1", 42, { status: "In Progress" }),
  issueFacts("t1", 51, { status: "In Progress" }),
  issueFacts("t2", 53, { status: "Ready", externalBlockers: [{ number: 90, title: "Upstream SDK", url: "u" }] }),
  issueFacts("x1", 47, { status: null }),
];

const STATES = [
  leafState("m1", "in_progress"),
  leafState("f1", "in_progress"),
  leafState("t1", "done"),
  leafState("t2", "not_started", { blocked: true, blockedBy: [] }),
  leafState("x1", "drafting", { blocked: true, blockedBy: ["t1"] }),
];

const answerOf = (extra: Partial<GithubDeliveryPlan> = {}) =>
  githubAnswer(DOC, { tracker: TRACKER, states: STATES, ...extra });

/** The last entry of a list. */
const last = <T,>(list: readonly T[]): T | undefined => list[list.length - 1];

let b: DeliveryBackend;

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
  b = deliveryBackend("", answerOf());
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
  clearLensStores();
});

async function openPlan() {
  const r = await renderLens({ source: githubSource() });
  await screen.findByTestId("flow-item-t1");
  await settle();
  return r;
}

describe("the plan of a GitHub source (MDL-FR-XJBV, MDL-FR-LZIE, MDL-FR-NRJM)", () => {
  it("MDL-FR-XJBV, MDL-FR-LZIE: the lens loads with refresh false, then with refresh true, and hosts the canvas over the answer", async () => {
    await openPlan();
    expect(callsOf(b, "load_github_delivery_plan").map((a) => a.refresh)).toEqual([false, true]);
    expect(itemEl("m1")).toBeInTheDocument();
    expect(itemEl("x1")).toBeInTheDocument();
    // MDL-FR-XJBV: no write of a file and no state request of its own.
    expect(callsOf(b, "save_artifact_contents")).toHaveLength(0);
    expect(callsOf(b, "load_flow_delivery_state")).toHaveLength(0);
  });

  it("MDL-FR-LZIE: github polling changed loads with refresh true; drafts changed and graduation run changed with refresh false", async () => {
    await openPlan();
    const before = callsOf(b, "load_github_delivery_plan").length;
    act(() => emitEvent("github-polling-changed", { newIssues: [] }));
    await waitFor(() => expect(callsOf(b, "load_github_delivery_plan").length).toBe(before + 1));
    expect(last(callsOf(b, "load_github_delivery_plan"))).toEqual({ refresh: true });
    act(() => emitEvent("drafts-changed"));
    await waitFor(() => expect(callsOf(b, "load_github_delivery_plan").length).toBe(before + 2));
    expect(last(callsOf(b, "load_github_delivery_plan"))).toEqual({ refresh: false });
    act(() => emitEvent("graduation-run-changed", { runId: "r1" }));
    await waitFor(() => expect(callsOf(b, "load_github_delivery_plan").length).toBe(before + 3));
    expect(last(callsOf(b, "load_github_delivery_plan"))).toEqual({ refresh: false });
  });

  it("MDL-FR-LZIE: after a first load that refused, github polling changed loads with refresh true and shows the plan", async () => {
    b.plan = () => Promise.reject({ code: "project_unavailable", message: "The GitHub Project cannot be read." });
    await renderLens({ source: githubSource() });
    expect(await screen.findByText("The GitHub Project cannot be read.")).toBeInTheDocument();
    const before = callsOf(b, "load_github_delivery_plan").length;
    b.plan = () => answerOf();
    act(() => emitEvent("github-polling-changed", { newIssues: [] }));
    await waitFor(() => expect(callsOf(b, "load_github_delivery_plan").length).toBe(before + 1));
    expect(last(callsOf(b, "load_github_delivery_plan"))).toEqual({ refresh: true });
    expect(await screen.findByTestId("flow-item-t1")).toBeInTheDocument();
  });

  it("MDL-FR-YTAP, MDL-FR-LZIE: once the Map tab unmounts, the five events read no source and load no plan", async () => {
    const r = await openPlan();
    const reads = r.ops.getDeliverySource.mock.calls.length;
    const loads = callsOf(b, "load_github_delivery_plan").length;
    r.unmount();
    act(() => {
      emitEvent("delivery-source-changed", githubSource());
      emitEvent("project-tree-changed", { paths: [] });
      emitEvent("github-polling-changed", { newIssues: [] });
      emitEvent("drafts-changed");
      emitEvent("graduation-run-changed", { runId: "r1" });
    });
    await act(async () => {});
    expect(r.ops.getDeliverySource).toHaveBeenCalledTimes(reads);
    expect(callsOf(b, "load_github_delivery_plan")).toHaveLength(loads);
  });

  it("MDL-FR-NRJM: the lens shows a loading state until the first answer", async () => {
    let answer!: (a: GithubDeliveryPlan) => void;
    b.plan = () => new Promise((r) => (answer = r));
    await renderLens({ source: githubSource() });
    expect(await screen.findByText("Loading the delivery plan…")).toBeInTheDocument();
    await act(async () => answer(answerOf()));
    expect(await screen.findByTestId("flow-item-t1")).toBeInTheDocument();
  });

  it("MDL-FR-NRJM: a load that refuses with no plan shows the error text and Retry, which loads again", async () => {
    b.plan = () => Promise.reject({ code: "project_unavailable", message: "The GitHub Project cannot be read." });
    await renderLens({ source: githubSource() });
    expect(await screen.findByText("The GitHub Project cannot be read.")).toBeInTheDocument();
    b.plan = () => answerOf();
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByTestId("flow-item-t1")).toBeInTheDocument();
    expect(last(callsOf(b, "load_github_delivery_plan"))).toEqual({ refresh: true });
  });
});

describe("the link to the GitHub Project (MDL-FR-BZHY)", () => {
  it("MDL-FR-BZHY: owner/name is a link with the tooltip Open the GitHub Project, and activating it opens the Project's URL", async () => {
    await openPlan();
    const link = screen.getByRole("button", { name: "acme/payments: Open the GitHub Project" });
    expect(link).toHaveAttribute("title", "Open the GitHub Project");
    expect(link.querySelector('[data-icon="external-link"]')).not.toBeNull();
    expect(link).toBeEnabled();
    link.focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() =>
      expect(callsOf(b, "open_tracker_link")).toEqual([{ url: "https://github.com/orgs/acme/projects/7" }]),
    );
    expect(visibleToasts()).toEqual([]);
  });

  it("MDL-FR-BZHY: a refused link shows its error text in an Error toast", async () => {
    b.on.open_tracker_link = () => Promise.reject({ code: "tracker_url_refused", message: "The link cannot open." });
    await openPlan();
    await userEvent.click(screen.getByTestId("delivery-band-repo"));
    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    expect(visibleToasts()[0]).toMatchObject({ level: "Error", body: "The link cannot open." });
  });

  it("MDL-FR-BZHY: with no URL yet the link is disabled and keeps its tooltip, and a source answer with a URL enables it", async () => {
    const base = githubSource() as Extract<DeliverySource, { kind: "github" }>;
    const r = await renderLens({ source: { ...base, project: { ...base.project, url: "" } } });
    await screen.findByTestId("flow-item-t1");
    const link = () => screen.getByTestId("delivery-band-repo");
    expect(link()).toBeDisabled();
    expect(link()).toHaveAttribute("title", "Open the GitHub Project");
    await userEvent.click(link());
    expect(callsOf(b, "open_tracker_link")).toEqual([]);
    act(() => r.store.setDeliverySource(base));
    await waitFor(() => expect(link()).toBeEnabled());
  });
});

describe("a lens switch (SMP-FR-RNLH)", () => {
  it("SMP-FR-RNLH: a switch to the Specifications lens and back keeps the plan and loads nothing again", async () => {
    await openPlan();
    const loads = callsOf(b, "load_github_delivery_plan").length;
    const lens = () => within(screen.getByRole("radiogroup", { name: "Lens" }));
    fireEvent.click(lens().getByRole("radio", { name: "Specifications" }));
    await screen.findByTestId("spec-map-canvas");
    expect(screen.queryByTestId("flow-item-t1")).toBeNull();
    fireEvent.click(lens().getByRole("radio", { name: "Delivery" }));
    // The plan shows at once, from the session.
    expect(screen.getByTestId("flow-item-t1")).toBeInTheDocument();
    await settle();
    expect(callsOf(b, "load_github_delivery_plan")).toHaveLength(loads);
  });
});

describe("the band and the notices (MDL-FR-KKYH, MDL-FR-LSKL, MDL-FR-IEWL, MDL-FR-XUJY, MDL-FR-MWMO)", () => {
  it("MDL-FR-KKYH: the band names the Project's title, read-only, the repository, the time of the read, and Refresh loads with refresh true", async () => {
    // The answer of the harness was read at 10:00; the clock says 10:05.
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-11-14T10:05:00Z"));
    await openPlan();
    const name = screen.getByLabelText("Flow name");
    expect(name).toHaveValue("Payments board");
    expect(name).toHaveAttribute("readonly");
    expect(screen.getByLabelText("Flow description")).toHaveAttribute("readonly");
    expect(screen.getByTestId("delivery-band-repo")).toHaveTextContent("acme/payments");
    expect(screen.getByTestId("delivery-band-read")).toHaveTextContent(/^read 5 min ago$/);
    const before = callsOf(b, "load_github_delivery_plan").length;
    await userEvent.click(screen.getByRole("button", { name: /Refresh/ }));
    await waitFor(() => expect(callsOf(b, "load_github_delivery_plan").length).toBe(before + 1));
    expect(last(callsOf(b, "load_github_delivery_plan"))).toEqual({ refresh: true });
  });

  it("MDL-FR-KKYH: a failed Refresh keeps the plan and reports the failure in an Error toast", async () => {
    await openPlan();
    b.plan = () => Promise.reject({ code: "request_failed", message: "offline" });
    await userEvent.click(screen.getByRole("button", { name: /Refresh/ }));
    await waitFor(() => expect(visibleToasts()[0]).toMatchObject({ level: "Error", title: "Plan not refreshed" }));
    expect(itemEl("t1")).toBeInTheDocument();
  });

  it("MDL-FR-LSKL: a stale answer renders its plan with one notice that names the read and the error, and offers Retry", async () => {
    b.plan = () =>
      answerOf({ stale: true, readAt: "2026-11-14T09:00:00Z", error: { code: "request_failed", message: "GitHub did not answer" } });
    await openPlan();
    const notice = screen.getByTestId("delivery-stale-notice");
    // MDL layout notes: the notice sits below the band and above the canvas.
    expect(screen.getByTestId("flow-band").compareDocumentPosition(notice) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(notice.compareDocumentPosition(document.querySelector(".flow")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(notice).toHaveTextContent("GitHub did not answer");
    expect(notice).toHaveTextContent(/read /);
    expect(itemEl("t1")).toBeInTheDocument();
    const before = callsOf(b, "load_github_delivery_plan").length;
    await userEvent.click(within(notice).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(callsOf(b, "load_github_delivery_plan").length).toBe(before + 1));
  });

  it("MDL-FR-IEWL: unmapped Types show one notice with each Type and its count, No Type for the issues with none, and Map Types", async () => {
    b.plan = () =>
      answerOf({
        notices: { unmappedTypes: [{ type: "Spike", count: 3 }, { type: null, count: 2 }], misplaced: [], omittedRelations: [], skipped: 0 },
      });
    await openPlan();
    const notice = screen.getByTestId("delivery-unmapped-notice");
    expect(notice).toHaveTextContent("5 issues have no kind: Spike (3), No Type (2)");
    expect(within(notice).getByRole("button", { name: "Map Types" })).toBeInTheDocument();
  });

  it("MDL-FR-XUJY: Map Types offers a row for each Type, Confirm waits for a choice, writes the settings, and loads with refresh false", async () => {
    b.plan = () =>
      answerOf({
        notices: { unmappedTypes: [{ type: "Spike", count: 3 }, { type: null, count: 2 }], misplaced: [], omittedRelations: [], skipped: 0 },
      });
    const r = await openPlan();
    await userEvent.click(screen.getByRole("button", { name: "Map Types" }));
    expect(r.onOverlayOpening).toHaveBeenCalled();
    const dialog = screen.getByTestId("delivery-map-types-dialog");
    const confirm = within(dialog).getByRole("button", { name: "Confirm" });
    expect(confirm).toBeDisabled();
    const spike = within(dialog).getByLabelText("Kind of Spike");
    expect([...spike.querySelectorAll("option")].map((o) => o.textContent)).toEqual([
      "Choose…",
      "Epic",
      "Feature",
      "Task",
      "Defect",
      "Leave out",
    ]);
    await userEvent.selectOptions(spike, "Feature");
    // A choice in one row is enough.
    expect(confirm).toBeEnabled();
    await userEvent.selectOptions(within(dialog).getByLabelText("Kind of No Type"), "Leave out");
    expect(confirm).toBeEnabled();
    const before = callsOf(b, "load_github_delivery_plan").length;
    await userEvent.click(confirm);
    await waitFor(() => expect(screen.queryByTestId("delivery-map-types-dialog")).toBeNull());
    expect(b.settings.readEntries).toEqual([{ type: "Spike", kind: "feature" }]);
    expect(b.settings.untyped).toBe("exclude");
    await waitFor(() => expect(callsOf(b, "load_github_delivery_plan").length).toBe(before + 1));
    expect(last(callsOf(b, "load_github_delivery_plan"))).toEqual({ refresh: false });
  });

  it("MDL-FR-XUJY: a refused write shows its error in the dialog, and Escape closes it with nothing written", async () => {
    b.plan = () =>
      answerOf({ notices: { unmappedTypes: [{ type: "Spike", count: 1 }], misplaced: [], omittedRelations: [], skipped: 0 } });
    b.on.set_delivery_settings = () => Promise.reject({ code: "invalid_delivery_settings", message: "One Type can map to one kind only." });
    await openPlan();
    await userEvent.click(screen.getByRole("button", { name: "Map Types" }));
    const dialog = screen.getByTestId("delivery-map-types-dialog");
    await userEvent.selectOptions(within(dialog).getByLabelText("Kind of Spike"), "Task");
    const settings = structuredClone(b.settings);
    const loads = callsOf(b, "load_github_delivery_plan").length;
    expect(callsOf(b, "set_delivery_settings")).toHaveLength(0);
    await userEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("One Type can map to one kind only.");
    expect(callsOf(b, "set_delivery_settings")).toHaveLength(1);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByTestId("delivery-map-types-dialog")).toBeNull();
    await act(async () => {});
    expect(b.settings).toEqual(settings);
    expect(callsOf(b, "set_delivery_settings")).toHaveLength(1);
    expect(callsOf(b, "load_github_delivery_plan")).toHaveLength(loads);
  });

  it("MDL-FR-MWMO: misplaced items and omitted relations show one line of counts, and activating it lists each with its numbers and reason", async () => {
    b.plan = () =>
      answerOf({
        notices: {
          unmappedTypes: [],
          misplaced: [{ itemId: "t1", parentItemId: "x1" }],
          omittedRelations: [
            { blockedItemId: "t2", blockingItemId: "f1", reason: "lineage" },
            { blockedItemId: "t1", blockingItemId: "t2", reason: "cycle" },
          ],
          skipped: 0,
        },
      });
    await openPlan();
    const notice = screen.getByTestId("delivery-structure-notice");
    expect(notice).toHaveTextContent("1 item is misplaced · 2 relations are left out");
    expect(screen.queryByTestId("delivery-structure-list")).toBeNull();
    await userEvent.click(within(notice).getByRole("button"));
    const list = screen.getByTestId("delivery-structure-list");
    expect(list).toHaveTextContent("#51 is under #47, which cannot hold it");
    expect(list).toHaveTextContent("#42 blocks #53, but one holds the other");
    expect(list).toHaveTextContent("#53 blocks #51, but the relation closes a cycle");
  });
});

describe("the items of a GitHub plan (MDL-FR-JPYU, MDL-FR-ECPZ, MDL-FR-NTVL, MDL-FR-GDUP)", () => {
  it("MDL-FR-JPYU: an open external blocker gives the Blocked marker, and its tooltip names it by number and title", async () => {
    await openPlan();
    const marker = within(itemEl("t2")).getByRole("button", { name: /Blocked by/ });
    expect(marker).toHaveAttribute("title", "Blocked by #90 Upstream SDK");
    // An item blocked by a plan item and by nothing outside names the plan item.
    expect(within(itemEl("x1")).getByRole("button", { name: /Blocked by/ })).toHaveAttribute("title", "Blocked by T1");
  });

  it("MDL-FR-ECPZ: each issue shows its Status after its state chip, and every item carries its tracker tag", async () => {
    await openPlan();
    const card = itemEl("t2");
    const row = within(card).getByTestId("flow-status-chip").parentElement!;
    const chips = [...row.querySelectorAll(".flow-chip")].map((c) => c.textContent);
    expect(chips).toEqual([expect.stringContaining("Not started"), "Ready"]);
    expect(within(itemEl("f1")).getByTestId("flow-tracker-status")).toHaveTextContent("In Progress");
    // A milestone and an issue with no Status show no Status chip.
    expect(within(itemEl("m1")).queryByTestId("flow-tracker-status")).toBeNull();
    expect(within(itemEl("x1")).queryByTestId("flow-tracker-status")).toBeNull();
    for (const id of ["m1", "f1", "t1", "t2", "x1"]) {
      expect(within(itemEl(id)).getAllByTestId("flow-tracker-tag").length).toBeGreaterThan(0);
    }
    expect(within(itemEl("m1")).getAllByTestId("flow-tracker-tag")[0]).toHaveTextContent("milestone 3");
  });

  it("MDL-FR-NTVL, ACT-FR-UNJB: a GitHub plan carries no action control and no discussions control", async () => {
    await openPlan();
    expect(document.querySelector(".flow-actionhost")).toBeNull();
    expect(screen.queryByRole("button", { name: "Actions" })).toBeNull();
    expect(screen.queryByRole("button", { name: /^Discuss$/ })).toBeNull();
    expect(callsOf(b, "list_discussions")).toHaveLength(0);
  });

  it("MDL-FR-GDUP: the toolbar holds, after the lens switch, the bar of the leaf states and the counts line", async () => {
    await openPlan();
    const toolbar = screen.getByRole("toolbar", { name: "Map controls" });
    expect(toolbar.firstElementChild).toHaveAttribute("aria-label", "Lens");
    const bar = within(toolbar).getByTestId("delivery-rollup-bar");
    expect([...bar.children].map((c) => c.getAttribute("data-state"))).toEqual([
      "done",
      "in_progress",
      "drafting",
      "not_started",
      "dropped",
    ]);
    expect(within(toolbar).getByTestId("delivery-rollup-counts")).toHaveTextContent("1 done · 0 in progress · 2 blocked");
    expect((bar.children[0] as HTMLElement).style.width).toBe(`${(1 / 3) * 100}%`);
  });
});

describe("edits of a GitHub plan (MDL-FR-PUCM, MDL-FR-XWGP, MDL-FR-HNLP)", () => {
  it("MDL-FR-PUCM: a rename renders at once with a pending mark, and the answer replaces the plan", async () => {
    let answer!: (a: GithubDeliveryPlan) => void;
    b.apply = () => new Promise((r) => (answer = r));
    await openPlan();
    fireEvent.doubleClick(screen.getByTestId("flow-item-name-x1"));
    const field = screen.getByLabelText("Item name");
    fireEvent.change(field, { target: { value: "Decline text" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(screen.getByTestId("flow-item-name-x1")).toHaveTextContent("Decline text");
    expect(within(itemEl("x1")).getByTestId("flow-pending")).toBeInTheDocument();
    await waitFor(() => expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(1));
    expect(callsOf(b, "apply_github_delivery_edit")[0].edit).toMatchObject({
      kind: "rename",
      itemId: "x1",
      name: "Decline text",
      updatedAt: TRACKER[4].updatedAt,
    });
    await act(async () => answer(answerOf({ document: JSON.parse(JSON.stringify(renameItem(DOC, "x1", "Decline message"))) })));
    expect(screen.getByTestId("flow-item-name-x1")).toHaveTextContent("Decline message");
    expect(within(itemEl("x1")).queryByTestId("flow-pending")).toBeNull();
  });

  it("MDL-FR-PUCM, MDL-FR-HNLP: a refusal with changed_on_tracker restores the plan, states it on the item, and loads with refresh true", async () => {
    b.apply = () => Promise.reject({ code: "changed_on_tracker", message: "changed" });
    await openPlan();
    const before = callsOf(b, "load_github_delivery_plan").length;
    fireEvent.doubleClick(screen.getByTestId("flow-item-name-x1"));
    const field = screen.getByLabelText("Item name");
    fireEvent.change(field, { target: { value: "Other" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(await screen.findByTestId("flow-item-error-x1")).toHaveTextContent(CHANGED_ON_GITHUB);
    expect(screen.getByTestId("flow-item-name-x1")).toHaveTextContent("X1");
    await waitFor(() => expect(callsOf(b, "load_github_delivery_plan").length).toBe(before + 1));
    expect(last(callsOf(b, "load_github_delivery_plan"))).toEqual({ refresh: true });
    expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(1);
  });

  it("MDL-FR-XWGP: while an edit is pending the canvas takes another, and the second goes out after the first answers", async () => {
    const answers: Array<(a: GithubDeliveryPlan) => void> = [];
    b.apply = () => new Promise((r) => answers.push(r));
    await openPlan();
    for (const [id, name] of [["x1", "First"], ["t1", "Second"]] as const) {
      fireEvent.doubleClick(screen.getByTestId(`flow-item-name-${id}`));
      const field = screen.getByLabelText("Item name");
      fireEvent.change(field, { target: { value: name } });
      fireEvent.keyDown(field, { key: "Enter" });
    }
    await waitFor(() => expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(1));
    expect(screen.getByTestId("flow-item-name-t1")).toHaveTextContent("Second");
    await act(async () => answers[0](answerOf()));
    await waitFor(() => expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(2));
    expect(callsOf(b, "apply_github_delivery_edit").map((a) => (a.edit as { name: string }).name)).toEqual(["First", "Second"]);
    await act(async () => answers[1](answerOf()));
  });

  it("MDL-FR-QAJN: Reset layout stores the empty layout of the plan's scope, and the canvas lays the plan out again", async () => {
    await openPlan();
    await userEvent.click(screen.getByRole("button", { name: "Collapse F1" }));
    expect(itemEl("f1")).toHaveAttribute("data-collapsed", "true");
    await userEvent.click(screen.getByRole("button", { name: "Plan actions" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Reset layout" }));
    await waitFor(() => expect(last(b.layoutWrites)).toEqual({ scope: "github:PVT_1", layout: { items: {} } }));
    await waitFor(() => expect(itemEl("f1")).toHaveAttribute("data-collapsed", "false"));
    expect(itemEl("t2")).toBeInTheDocument();
  });

  it("MDL-FR-XJBV: a selected item shows its panel, and the plan's name and description are read-only", async () => {
    await openPlan();
    selectItem("t1");
    expect(await screen.findByTestId("flow-selection-panel")).toBeInTheDocument();
    const name = screen.getByLabelText("Flow name");
    const description = screen.getByLabelText("Flow description");
    expect(name).toHaveAttribute("readonly");
    expect(description).toHaveAttribute("readonly");
    const nameBefore = (name as HTMLInputElement).value;
    const descriptionBefore = (description as HTMLTextAreaElement).value;
    await userEvent.type(name, "x");
    await userEvent.type(description, "y");
    expect(name).toHaveValue(nameBefore);
    expect(description).toHaveValue(descriptionBefore);
    expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(0);
  });
});
