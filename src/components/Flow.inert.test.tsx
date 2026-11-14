/**
 * The Flow canvas while its host keeps it inert (`../../specifications/ui/MDL-map-delivery-lens.md`
 * MDL-FR-WMGU), and an edit that the plan cannot take when the backend call
 * that asked for it returns (FLO-FR-NRBV, EDT-FR-UPNU).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";

import { resetLogBufferForTest } from "../logging";
import { findItem, type FlowDocument } from "../state/flowDocument";
import {
  NOT_MAIN_GUARD,
  resetMainBranchWriteForTest,
  setBranchWriteGuardReadsForTest,
} from "../state/mainBranchWrite";
import { resetToasts } from "../state/toasts";
import type { BranchWriteGuard } from "../types";
import {
  callsOf,
  clearFlowStores,
  endFakeTimers,
  flowBackend,
  itemState,
  planOf,
  renderFlow,
} from "../test/flowCanvasHarness";
import { dragItem, HLD_ID, ID, itemEl, panel, selectItem } from "../test/flowFixtures";
import { resetListeners } from "../test/flowMocks";
import { blocks, sampleLayout, samplePlan } from "../test/flowPlanFixtures";
import { unlinkedDraftText } from "./Flow/useItemActions";

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
  resetMainBranchWriteForTest();
});

afterEach(async () => {
  await endFakeTimers();
  cleanup();
  clearFlowStores();
  resetMainBranchWriteForTest();
});

const band = () => screen.getByTestId("flow-band");

/** `t1` blocks `t2`, and `t1` links a draft, holds a footprint path, and references the HLD. */
function richPlan(): FlowDocument {
  const doc = samplePlan([blocks("e1", "t1", "t2")]);
  doc.items = doc.items.map((i) =>
    i.id === "t1"
      ? { ...i, draftId: "d-1", footprint: ["src/main.ts"], artifactIds: [HLD_ID] }
      : i,
  );
  return doc;
}

describe("every edit affordance is inert while the host keeps the canvas inert (MDL-FR-WMGU)", () => {
  it("MDL-FR-WMGU, FLO-FR-YDWT: the band's name, description, and Add HLD picker are disabled", async () => {
    const b = flowBackend(richPlan(), sampleLayout());
    const { flows } = await renderFlow(b, { inert: true });

    expect(within(band()).getByRole("textbox", { name: "Flow name" })).toBeDisabled();
    expect(within(band()).getByRole("textbox", { name: "Flow description" })).toBeDisabled();
    const picker = within(band()).getByRole("combobox", { name: "Add HLD" });
    expect(picker).toBeDisabled();
    fireEvent.change(picker, { target: { value: HLD_ID } });
    expect(planOf(flows).references).toEqual([]);
  });

  it("MDL-FR-WMGU: every control of the selection panel is disabled", async () => {
    const b = flowBackend(richPlan(), sampleLayout());
    b.drafts = [{ id: "d-2", name: "Other", status: "active", updatedAt: "2026-11-01T00:00:00Z" } as never];
    await renderFlow(b, { inert: true });
    selectItem("t1");
    await within(panel()).findByRole("option", { name: "Other" });

    const p = within(panel());
    for (const name of ["Rename", "Remove", "Unlink draft", "Remove path src/main.ts"]) {
      expect(p.getByRole("button", { name })).toBeDisabled();
    }
    expect(p.getByRole("button", { name: /^Remove reference/ })).toBeDisabled();
    for (const name of ["Kind", "Link draft", "Add path", "Add reference"]) {
      expect(p.getByRole("combobox", { name })).toBeDisabled();
    }
    expect(p.getByRole("textbox", { name: "Description" })).toBeDisabled();
  });

  it("MDL-FR-WMGU: the edge panel's type, Reverse, and Remove edge are disabled", async () => {
    const b = flowBackend(richPlan(), sampleLayout());
    await renderFlow(b, { inert: true });
    fireEvent.click(screen.getByTestId("flow-edge-e1"));
    const edge = within(screen.getByRole("group", { name: /^Edge / }));

    expect(edge.getByRole("radio", { name: "Blocks" })).toBeDisabled();
    expect(edge.getByRole("radio", { name: "Relates" })).toBeDisabled();
    expect(edge.getByRole("button", { name: "Reverse" })).toBeDisabled();
    expect(edge.getByRole("button", { name: "Remove edge" })).toBeDisabled();
  });

  it("MDL-FR-WMGU: Create draft, a missing draft's Unlink, and the add affordance are disabled, and a double click opens no rename field", async () => {
    const b = flowBackend(richPlan(), sampleLayout());
    b.delivery = { items: [itemState("t1", "not_started", { draftMissing: true })] };
    const { flows } = await renderFlow(b, { inert: true });
    const missing = await screen.findByTestId("flow-draft-missing");

    expect(within(missing).getByRole("button", { name: "Unlink" })).toBeDisabled();
    expect(within(itemEl("t2")).getByRole("button", { name: "Create draft" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Add to plan" })).toBeDisabled();
    fireEvent.doubleClick(screen.getByTestId("flow-item-name-t2"));
    fireEvent.doubleClick(screen.getByTestId("flow-item-name-f1"));
    expect(screen.queryByRole("textbox", { name: "Item name" })).toBeNull();
    expect(flows.get(ID)!.dirty).toBe(false);
  });

  it("MDL-FR-WMGU, FLO-FR-TDHR: a drag moves nothing and stores no layout", async () => {
    const b = flowBackend(richPlan(), sampleLayout());
    await renderFlow(b, { inert: true });

    expect(document.querySelector(".flow-surface")).toHaveAttribute("data-inert", "true");
    dragItem("t1", { x: 50, y: 130 }, { x: 100, y: 200 });
    expect(itemEl("t1").style.left).toBe("40px");
    expect(callsOf(b, "save_delivery_layout")).toEqual([]);
  });
});

describe("an edit the plan cannot take when its backend call returns (FLO-FR-NRBV)", () => {
  it("FLO-FR-NRBV, EDT-FR-UPNU: a draft created while the write is held is not linked, does not open, and the item says so", async () => {
    const main: BranchWriteGuard = { ...NOT_MAIN_GUARD, branch: "main", isMain: true, isDefault: true };
    setBranchWriteGuardReadsForTest({
      read: () => Promise.resolve(main),
      known: () => main,
      confirm: () => new Promise(() => {}),
    });
    const b = flowBackend(richPlan(), sampleLayout());
    const onCreateDraft = vi.fn(async () => ({ ok: true as const, draft: { id: "d-7", name: "T2" } }));
    const onOpenDraft = vi.fn();
    const { flows } = await renderFlow(b, { onCreateDraft, onOpenDraft });

    await act(async () => {
      fireEvent.click(within(itemEl("t2")).getByRole("button", { name: "Create draft" }));
    });

    expect(await within(itemEl("t2")).findByRole("alert")).toHaveTextContent(unlinkedDraftText("T2"));
    expect(findItem(planOf(flows), "t2")!.draftId).toBeUndefined();
    expect(onOpenDraft).not.toHaveBeenCalled();
  });
});
