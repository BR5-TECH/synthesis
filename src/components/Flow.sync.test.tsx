import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { findItem, parseFlowDocument } from "../state/flowDocument";
import { resetToasts, visibleToasts } from "../state/toasts";
import type { SyncResult } from "../types";
import {
  callsOf,
  clearFlowStores,
  emptyResult,
  endFakeTimers,
  flowBackend,
  planOf,
  renderFlow,
} from "../test/flowCanvasHarness";
import { canvas, client, dragItem, ID, itemEl, panel, selectItem } from "../test/flowFixtures";
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

afterEach(async () => {
  await endFakeTimers();
  cleanup();
  clearFlowStores();
});

const synced = () => {
  const doc = samplePlan();
  doc.sync = { tracker: "github", links: [{ itemId: "f1", ref: issueRef(42), base: issueBase("F1") }] };
  return doc;
};

const band = () => screen.getByTestId("flow-band");

describe("the Sync control (FLO-FR-AKMV)", () => {
  it("FLO-FR-AKMV: a Flow with no sync section offers Connect to GitHub, which adds the section, writes it, and starts a sync", async () => {
    const b = flowBackend(samplePlan());
    b.on.sync_flow_with_tracker = () => emptyResult();
    await renderFlow(b);

    await userEvent.click(within(band()).getByRole("button", { name: "Connect to GitHub" }));
    await waitFor(() => expect(callsOf(b, "sync_flow_with_tracker")).toEqual([{ flowId: ID }]));
    expect(b.writes).toHaveLength(1);
    const written = parseFlowDocument(b.writes[0]);
    expect(written.ok && written.doc.sync).toEqual({ tracker: "github", links: [] });
    const order = b.calls.map((c) => c.cmd);
    expect(order.indexOf("save_artifact_contents")).toBeLessThan(order.indexOf("sync_flow_with_tracker"));
  });

  it("FLO-FR-AKMV: a synced Flow offers Sync, and a sync first writes the pending changes", async () => {
    const b = flowBackend(synced());
    /** What was on disk when the sync started. */
    let onDisk: string[] | null = null;
    b.on.sync_flow_with_tracker = () => {
      onDisk = [...b.writes];
      return emptyResult();
    };
    const { flows } = await renderFlow(b);
    expect(within(band()).getByRole("button", { name: "Sync" })).toBeInTheDocument();
    expect(within(band()).getByText("acme/payments")).toBeInTheDocument();
    // The clock is held below the rest, so only the sync can write the edit.
    vi.useFakeTimers();

    fireEvent.change(screen.getByRole("textbox", { name: "Flow description" }), { target: { value: "Q3" } });
    expect(flows.hasPendingWrite(ID)).toBe(true);
    expect(b.writes).toEqual([]);
    fireEvent.click(within(band()).getByRole("button", { name: "Sync" }));
    await vi.waitFor(() => expect(callsOf(b, "sync_flow_with_tracker")).toHaveLength(1));
    expect(onDisk).toHaveLength(1);
    expect(JSON.parse(onDisk![0]).description).toBe("Q3");
    expect(flows.hasPendingWrite(ID)).toBe(false);
  });

  it("FLO-FR-AKMV, FLO-FR-47: a write that is refused before the sync starts no sync", async () => {
    const b = flowBackend(synced());
    b.on.sync_flow_with_tracker = () => emptyResult();
    await renderFlow(b);
    b.validation = { valid: false, violations: [{ code: "dependency_cycle", message: "a cycle" }] };

    await userEvent.type(screen.getByRole("textbox", { name: "Flow description" }), "x");
    await userEvent.click(within(band()).getByRole("button", { name: "Sync" }));
    await screen.findByText("This Flow was not written.");
    expect(callsOf(b, "sync_flow_with_tracker")).toEqual([]);
    expect(within(band()).getByRole("button", { name: "Sync" })).toBeEnabled();
  });
});

describe("while a sync runs (FLO-FR-KHBY)", () => {
  it("FLO-FR-KHBY: the Sync control shows progress, every edit affordance is inert, and no write is scheduled", async () => {
    const b = flowBackend(synced());
    let finish!: (r: SyncResult) => void;
    b.on.sync_flow_with_tracker = () => new Promise((res) => (finish = res));
    const { flows } = await renderFlow(b);
    selectItem("t1");

    await userEvent.click(within(band()).getByRole("button", { name: "Sync" }));
    const progress = await within(band()).findByRole("button", { name: /Syncing/ });
    expect(progress).toHaveAttribute("aria-busy", "true");
    expect(progress).toBeDisabled();
    expect(screen.getByRole("textbox", { name: "Flow name" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Add to plan" })).toBeDisabled();
    expect(within(panel()).getByRole("textbox", { name: "Description" })).toBeDisabled();
    expect(within(itemEl("t2")).getByRole("button", { name: "Create draft" })).toBeDisabled();
    act(() => flows.applyEdit(ID, (d) => ({ ...d, name: "changed" })));
    expect(planOf(flows).name).toBe("Plan");
    expect(flows.hasPendingWrite(ID)).toBe(false);

    const loads = callsOf(b, "load_artifact_contents_by_id").length;
    await act(async () => finish(emptyResult()));
    await waitFor(() => expect(within(band()).getByRole("button", { name: "Sync" })).toBeEnabled());
    expect(callsOf(b, "load_artifact_contents_by_id").length).toBe(loads + 1);
    expect(screen.getByRole("textbox", { name: "Flow name" })).toBeEnabled();
  });

  it("FLO-FR-KHBY: while a sync runs, a drag, a connection and a resize do nothing", async () => {
    const b = flowBackend(synced());
    b.on.sync_flow_with_tracker = () => new Promise(() => {});
    const { flows } = await renderFlow(b);

    await userEvent.click(within(band()).getByRole("button", { name: "Sync" }));
    await within(band()).findByRole("button", { name: /Syncing/ });
    const before = planOf(flows);

    dragItem("t1", { x: 70, y: 190 }, { x: 620, y: 300 });
    fireEvent.pointerDown(within(itemEl("t1")).getByLabelText("Connect from T1"), { pointerId: 1, buttons: 1, ...client({ x: 240, y: 200 }) });
    expect(screen.queryByTestId("flow-connection")).toBeNull();
    fireEvent.pointerUp(itemEl("t3"), { pointerId: 1 });
    // A container offers no resize handle while the tab is inert.
    expect(screen.queryByTestId("flow-resize-f1-se")).toBeNull();
    fireEvent.pointerDown(itemEl("f1").querySelector(".flow-ctr__head") as HTMLElement, { pointerId: 1, buttons: 1, clientX: 500, clientY: 500 });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, clientX: 700, clientY: 700 });
    fireEvent.pointerUp(canvas(), { pointerId: 1 });
    expect(itemEl("f1").style.width).toBe("420px");

    expect(planOf(flows)).toBe(before);
    expect(itemEl("t1").style.left).toBe("60px");
    expect(flows.get(ID)!.dirty).toBe(false);
  });

  it("FLO-FR-KHBY: after a sync error, every edit affordance returns", async () => {
    const b = flowBackend(synced());
    let fail!: (e: unknown) => void;
    b.on.sync_flow_with_tracker = () => new Promise((_, rej) => (fail = rej));
    const { flows } = await renderFlow(b);
    selectItem("t1");

    await userEvent.click(within(band()).getByRole("button", { name: "Sync" }));
    await within(band()).findByRole("button", { name: /Syncing/ });
    expect(screen.getByRole("button", { name: "Add to plan" })).toBeDisabled();
    await act(async () => fail({ code: "github_unreachable", message: "GitHub did not answer." }));
    await screen.findByTestId("flow-sync-error");

    await waitFor(() => expect(within(band()).getByRole("button", { name: "Sync" })).toBeEnabled());
    expect(screen.getByRole("textbox", { name: "Flow name" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Add to plan" })).toBeEnabled();
    expect(within(panel()).getByRole("textbox", { name: "Description" })).toBeEnabled();
    expect(within(itemEl("t2")).getByRole("button", { name: "Create draft" })).toBeEnabled();
    expect(screen.getByTestId("flow-resize-f1-se")).toBeInTheDocument();
    // A drag lands again.
    dragItem("t1", { x: 70, y: 190 }, { x: 80, y: 220 }, 4);
    expect(findItem(planOf(flows), "t1")!.position).toEqual({ x: 30, y: 90 });
  });

  it("FLO-FR-KHBY, FLO-FR-31: when the sync returns, the tab reloads the plan the sync wrote", async () => {
    const b = flowBackend(synced());
    b.on.sync_flow_with_tracker = () => {
      const pulled = synced();
      pulled.items = pulled.items.map((i) => (i.id === "f1" ? { ...i, name: "Card capture and 3DS" } : i));
      b.body = JSON.stringify(pulled);
      return emptyResult({ changed: 1 });
    };
    const { flows } = await renderFlow(b);

    await userEvent.click(within(band()).getByRole("button", { name: "Sync" }));
    await waitFor(() => expect(findItem(planOf(flows), "f1")!.name).toBe("Card capture and 3DS"));
  });
});

describe("the result of a sync (FLO-FR-WLHA)", () => {
  it("FLO-FR-WLHA: a sync with conflicts or skipped issues marks the Sync control with their count and opens the review", async () => {
    const b = flowBackend(synced());
    b.on.sync_flow_with_tracker = () =>
      emptyResult({
        conflicts: [
          {
            id: "c1",
            kind: "field",
            itemId: "f1",
            itemName: "F1",
            field: "name",
            base: "F1",
            flow: "F1",
            tracker: "F1 and more",
            choices: ["keep_flow", "take_tracker"],
            url: null,
          },
        ],
        skipped: [
          {
            number: 58,
            title: "Load test plan",
            url: "https://github.com/acme/payments/issues/58",
            reason: "no_kind_fits",
          },
        ],
      });
    await renderFlow(b);

    await userEvent.click(within(band()).getByRole("button", { name: "Sync" }));
    const count = await within(band()).findByRole("button", { name: "Review 2 sync items" });
    expect(count).toHaveTextContent("2");
    expect(screen.getByTestId("flow-sync-review")).toBeInTheDocument();
  });

  it("FLO-FR-WLHA: a sync that returns nothing to decide shows a short confirmation with what it pushed and pulled", async () => {
    const b = flowBackend(synced());
    b.on.sync_flow_with_tracker = () => emptyResult({ created: 2, updated: 1, added: 1, changed: 0 });
    await renderFlow(b);

    await userEvent.click(within(band()).getByRole("button", { name: "Sync" }));
    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    expect(visibleToasts()[0]).toMatchObject({
      level: "Info",
      title: "Flow synced",
      body: "3 pushed to GitHub, 1 pulled from GitHub.",
    });
    expect(screen.queryByTestId("flow-sync-review")).toBeNull();
    expect(within(band()).queryByRole("button", { name: /^Review / })).toBeNull();
  });

  it("FLO-FR-WLHA: a sync error shows its typed text in the band", async () => {
    const b = flowBackend(synced());
    b.on.sync_flow_with_tracker = () =>
      Promise.reject({ code: "github_unreachable", message: "GitHub did not answer. Check the network." });
    await renderFlow(b);

    await userEvent.click(within(band()).getByRole("button", { name: "Sync" }));
    expect(await screen.findByTestId("flow-sync-error")).toHaveTextContent(
      "GitHub did not answer. Check the network.",
    );
    expect(within(band()).getByRole("button", { name: "Sync" })).toBeEnabled();
  });
});

describe("Link to existing (FLO-FR-TYEZ)", () => {
  it("FLO-FR-TYEZ: an item with no link in a synced Flow takes the tracker's name, description, and link", async () => {
    const b = flowBackend(synced());
    b.on.read_tracker_item = (args) => ({
      ref: issueRef(Number(args.number)),
      name: "Tokenize card numbers",
      description: "From GitHub",
      state: "open",
      type: "Task",
      parent: null,
      milestone: null,
      blockedBy: [],
    });
    const { flows } = await renderFlow(b);

    selectItem("t1");
    await userEvent.click(within(panel()).getByRole("button", { name: "Link to existing…" }));
    await userEvent.type(within(panel()).getByRole("spinbutton", { name: "Issue number" }), "51");
    await userEvent.click(within(panel()).getByRole("button", { name: "Link" }));
    await waitFor(() => expect(findItem(planOf(flows), "t1")!.name).toBe("Tokenize card numbers"));
    expect(callsOf(b, "read_tracker_item")).toEqual([{ flowId: ID, kind: "issue", number: 51 }]);
    const doc = planOf(flows);
    expect(findItem(doc, "t1")!.description).toBe("From GitHub");
    expect(doc.sync!.links[doc.sync!.links.length - 1]).toEqual({
      itemId: "t1",
      ref: issueRef(51),
      base: {
        name: "Tokenize card numbers",
        description: "From GitHub",
        state: "open",
        type: "Task",
        parent: null,
        milestone: null,
        blockedBy: [],
      },
    });
    expect(within(itemEl("t1")).getByRole("button", { name: "Open acme/payments#51 on GitHub" })).toBeInTheDocument();
  });

  it("FLO-FR-TYEZ: a milestone asks for a milestone number", async () => {
    const b = flowBackend(synced());
    b.on.read_tracker_item = () => Promise.reject({ code: "tracker_item_not_found", message: "No milestone 9." });
    await renderFlow(b);

    selectItem("m1");
    await userEvent.click(within(panel()).getByRole("button", { name: "Link to existing…" }));
    await userEvent.type(within(panel()).getByRole("spinbutton", { name: "Milestone number" }), "9");
    await userEvent.click(within(panel()).getByRole("button", { name: "Link" }));
    await waitFor(() =>
      expect(callsOf(b, "read_tracker_item")).toEqual([{ flowId: ID, kind: "milestone", number: 9 }]),
    );
  });

  it("FLO-FR-TYEZ: a refusal shows its typed text and stores nothing", async () => {
    const b = flowBackend(synced());
    b.on.read_tracker_item = () =>
      Promise.reject({ code: "tracker_item_not_found", message: "No issue #404 in acme/payments." });
    const { flows } = await renderFlow(b);

    selectItem("t1");
    await userEvent.click(within(panel()).getByRole("button", { name: "Link to existing…" }));
    await userEvent.type(within(panel()).getByRole("spinbutton", { name: "Issue number" }), "404");
    await userEvent.click(within(panel()).getByRole("button", { name: "Link" }));
    expect(await within(panel()).findByRole("alert")).toHaveTextContent("No issue #404 in acme/payments.");
    expect(planOf(flows).sync!.links).toHaveLength(1);
    expect(flows.get(ID)!.dirty).toBe(false);
  });

  it("FLO-FR-TYEZ: an unsynced Flow, or an item that has a link, offers no Link to existing", async () => {
    const b = flowBackend(synced());
    await renderFlow(b);

    selectItem("f1");
    expect(within(panel()).queryByRole("button", { name: "Link to existing…" })).toBeNull();
    cleanup();
    const b2 = flowBackend(samplePlan());
    await renderFlow(b2);
    selectItem("t1");
    expect(within(panel()).queryByRole("button", { name: "Link to existing…" })).toBeNull();
  });
});
