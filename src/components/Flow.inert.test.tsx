/**
 * The Flow tab while a sync runs (`../../specifications/ui/FLO-flow.md`
 * FLO-FR-KHBY), and the edits that a sync drops when it starts while a backend
 * call is still out (FLO-FR-NRBV, FLO-FR-TYEZ, FLO-FR-AKMV).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { findItem, type FlowDocument } from "../state/flowDocument";
import {
  NOT_MAIN_GUARD,
  setBranchWriteGuardReadsForTest,
} from "../state/mainBranchWrite";
import { resetToasts } from "../state/toasts";
import type { BranchWriteGuard, TrackerItemSnapshot } from "../types";
import {
  callsOf,
  clearFlowStores,
  emptyResult,
  endFakeTimers,
  flowBackend,
  itemState,
  planOf,
  renderFlow,
} from "../test/flowCanvasHarness";
import { HLD_ID, ID, itemEl, panel, selectItem } from "../test/flowFixtures";
import { resetListeners } from "../test/flowMocks";
import { blocks, issueBase, issueRef, samplePlan } from "../test/flowPlanFixtures";
import { LINK_DROPPED_TEXT, unlinkedDraftText } from "./Flow/useItemActions";
import { SYNC_NOT_CONNECTED } from "./Flow/useFlowSync";

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

const band = () => screen.getByTestId("flow-band");

/**
 * A synced plan where `f1` alone has a tracker link, `t1` blocks `t2`, and
 * `t1` links a draft, holds a footprint path, and references the HLD.
 */
function richPlan(): FlowDocument {
  const doc = samplePlan([blocks("e1", "t1", "t2")]);
  doc.items = doc.items.map((i) =>
    i.id === "t1"
      ? { ...i, draftId: "d-1", footprint: ["src/main.ts"], artifactIds: [HLD_ID] }
      : i,
  );
  doc.sync = { tracker: "github", links: [{ itemId: "f1", ref: issueRef(42), base: issueBase("F1") }] };
  return doc;
}

/** Click Sync with a sync that never returns, and wait for its progress. */
async function startEndlessSync(b: ReturnType<typeof flowBackend>) {
  b.on.sync_flow_with_tracker = () => new Promise(() => {});
  fireEvent.click(within(band()).getByRole("button", { name: "Sync" }));
  await within(band()).findByRole("button", { name: /Syncing/ });
}

describe("every edit affordance is inert while a sync runs (FLO-FR-KHBY)", () => {
  it("FLO-FR-KHBY, FLO-FR-YDWT: the band's Add HLD picker is disabled and adds nothing", async () => {
    const b = flowBackend(richPlan());
    const { flows } = await renderFlow(b);
    expect(within(band()).getByRole("combobox", { name: "Add HLD" })).toBeEnabled();

    await startEndlessSync(b);
    const picker = within(band()).getByRole("combobox", { name: "Add HLD" });
    expect(picker).toBeDisabled();
    fireEvent.change(picker, { target: { value: HLD_ID } });
    expect(planOf(flows).references).toEqual([]);
  });

  it("FLO-FR-KHBY, FLO-FR-YDWT: the band's Flow reference picker is disabled, and no reference offers Remove", async () => {
    const doc = richPlan();
    doc.references = [HLD_ID];
    const b = flowBackend(doc);
    await renderFlow(b);
    expect(within(band()).getByRole("button", { name: /^Remove reference/ })).toBeEnabled();

    await startEndlessSync(b);
    expect(within(band()).getByRole("combobox", { name: "Add Flow reference" })).toBeDisabled();
    expect(within(band()).queryByRole("button", { name: /^Remove reference/ })).toBeNull();
  });

  it("FLO-FR-KHBY: every control of the selection panel is disabled", async () => {
    const b = flowBackend(richPlan());
    b.drafts = [{ id: "d-2", name: "Other", status: "active", updatedAt: "2026-11-01T00:00:00Z" } as never];
    await renderFlow(b);
    selectItem("t1");
    await within(panel()).findByRole("option", { name: "Other" });

    await startEndlessSync(b);
    const p = within(panel());
    for (const name of ["Rename", "Remove", "Unlink draft", "Remove path src/main.ts", "Link to existing…"]) {
      expect(p.getByRole("button", { name })).toBeDisabled();
    }
    expect(p.getByRole("button", { name: /^Remove reference/ })).toBeDisabled();
    for (const name of ["Kind", "Link draft", "Add path", "Add reference"]) {
      expect(p.getByRole("combobox", { name })).toBeDisabled();
    }
    expect(p.getByRole("textbox", { name: "Description" })).toBeDisabled();
    expect(p.getByRole("switch", { name: "Local only" })).toBeDisabled();
  });

  it("FLO-FR-KHBY: a milestone's Due field is disabled", async () => {
    const b = flowBackend(richPlan());
    await renderFlow(b);
    selectItem("m1");

    await startEndlessSync(b);
    expect(within(panel()).getByLabelText("Due")).toBeDisabled();
  });

  it("FLO-FR-KHBY: the edge panel's type, Reverse, and Remove edge are disabled", async () => {
    const b = flowBackend(richPlan());
    await renderFlow(b);
    fireEvent.click(screen.getByTestId("flow-edge-e1"));
    const edge = () => screen.getByRole("group", { name: /^Edge / });
    expect(within(edge()).getByRole("button", { name: "Reverse" })).toBeEnabled();

    await startEndlessSync(b);
    expect(within(edge()).getByRole("radio", { name: "Blocks" })).toBeDisabled();
    expect(within(edge()).getByRole("radio", { name: "Relates" })).toBeDisabled();
    expect(within(edge()).getByRole("button", { name: "Reverse" })).toBeDisabled();
    expect(within(edge()).getByRole("button", { name: "Remove edge" })).toBeDisabled();
  });

  it("FLO-FR-KHBY: the cards' Create draft and a missing draft's Unlink are disabled, and a double click opens no rename field", async () => {
    const b = flowBackend(richPlan());
    b.delivery = { items: [itemState("t1", "not_started", { draftMissing: true })] };
    const { flows } = await renderFlow(b);
    const missing = await screen.findByTestId("flow-draft-missing");
    expect(within(missing).getByRole("button", { name: "Unlink" })).toBeEnabled();

    await startEndlessSync(b);
    expect(within(screen.getByTestId("flow-draft-missing")).getByRole("button", { name: "Unlink" })).toBeDisabled();
    expect(within(itemEl("t2")).getByRole("button", { name: "Create draft" })).toBeDisabled();
    fireEvent.doubleClick(screen.getByTestId("flow-item-name-t2"));
    fireEvent.doubleClick(screen.getByTestId("flow-item-name-f1"));
    expect(screen.queryByRole("textbox", { name: "Item name" })).toBeNull();
    expect(flows.get(ID)!.dirty).toBe(false);
  });
});

describe("an edit that a sync drops (FLO-FR-KHBY)", () => {
  it("FLO-FR-NRBV, FLO-FR-KHBY: a draft created while a sync started is not linked, does not open, and the item says so", async () => {
    const b = flowBackend(richPlan());
    let created!: (outcome: { ok: true; draft: { id: string; name: string } }) => void;
    const onCreateDraft = vi.fn(
      () => new Promise<{ ok: true; draft: { id: string; name: string } }>((res) => (created = res)),
    );
    const onOpenDraft = vi.fn();
    const { flows } = await renderFlow(b, { onCreateDraft, onOpenDraft });

    fireEvent.click(within(itemEl("t2")).getByRole("button", { name: "Create draft" }));
    await within(itemEl("t2")).findByRole("button", { name: "Creating draft…" });
    await startEndlessSync(b);
    await act(async () => created({ ok: true, draft: { id: "d-7", name: "T2" } }));

    expect(await within(itemEl("t2")).findByRole("alert")).toHaveTextContent(unlinkedDraftText("T2"));
    expect(findItem(planOf(flows), "t2")!.draftId).toBeUndefined();
    expect(onOpenDraft).not.toHaveBeenCalled();
  });

  it("FLO-FR-TYEZ, FLO-FR-KHBY: a tracker item read while a sync started is not linked, and the panel says so", async () => {
    const b = flowBackend(richPlan());
    let answer!: (snap: TrackerItemSnapshot) => void;
    b.on.read_tracker_item = () => new Promise((res) => (answer = res));
    const { flows } = await renderFlow(b);
    selectItem("t1");

    await userEvent.click(within(panel()).getByRole("button", { name: "Link to existing…" }));
    fireEvent.change(within(panel()).getByRole("spinbutton", { name: "Issue number" }), { target: { value: "51" } });
    fireEvent.click(within(panel()).getByRole("button", { name: "Link" }));
    await waitFor(() => expect(callsOf(b, "read_tracker_item")).toHaveLength(1));
    await startEndlessSync(b);
    await act(async () =>
      answer({
        ref: issueRef(51),
        name: "Tokenize card numbers",
        description: "",
        state: "open",
      } as TrackerItemSnapshot),
    );

    expect(await within(panel()).findByRole("alert")).toHaveTextContent(LINK_DROPPED_TEXT);
    expect(planOf(flows).sync!.links).toHaveLength(1);
    expect(findItem(planOf(flows), "t1")!.name).toBe("T1");
  });

  it("FLO-FR-AKMV, EDT-FR-UPNU: Connect to GitHub on a main branch whose write is not confirmed starts no sync and says why in the band", async () => {
    const main: BranchWriteGuard = { ...NOT_MAIN_GUARD, branch: "main", isMain: true, isDefault: true };
    setBranchWriteGuardReadsForTest({
      read: () => Promise.resolve(main),
      known: () => main,
      confirm: () => Promise.resolve({ ...main, confirmed: true }),
    });
    const b = flowBackend(samplePlan());
    b.on.sync_flow_with_tracker = () => emptyResult();
    const { flows } = await renderFlow(b);

    fireEvent.click(within(band()).getByRole("button", { name: "Connect to GitHub" }));
    expect(await screen.findByTestId("flow-sync-error")).toHaveTextContent(SYNC_NOT_CONNECTED);
    expect(within(band()).getByRole("button", { name: "Connect to GitHub" })).toBeEnabled();
    expect(callsOf(b, "sync_flow_with_tracker")).toEqual([]);
    expect(b.writes).toEqual([]);
    expect(planOf(flows).sync).toBeUndefined();
  });
});
