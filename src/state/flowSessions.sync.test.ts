/**
 * The tracker sync of a Flow, as the store holds it
 * (`../../specifications/ui/FLO-flow.md` FLO-FR-KHBY, FLO-FR-WLHA, and the
 * memory-only review of `FSR-flow-sync-review.md`). It runs against the store
 * and not against a mounted canvas.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { startFlowSync } from "../components/Flow/useFlowSync";
import { resetLogBufferForTest } from "../logging";
import { renameItem, serializeFlowDocument } from "./flowDocument";
import { FlowSessionStore, IDLE_SYNC } from "./flowSessions";
import type { SyncConflict, SyncResult } from "../types";
import { issueBase, issueRef, samplePlan } from "../test/flowPlanFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const ID = "plans/payments.flow";

const synced = () => {
  const doc = samplePlan();
  doc.sync = { tracker: "github", links: [{ itemId: "t1", ref: issueRef(51), base: issueBase("T1") }] };
  return serializeFlowDocument(doc);
};

const CONFLICT: SyncConflict = {
  id: "c1",
  kind: "field",
  itemId: "t1",
  itemName: "T1",
  field: "name",
  base: "T1",
  flow: "T1",
  tracker: "Tokenize",
  choices: ["keep_flow", "take_tracker"],
  url: null,
};

const result = (extra: Partial<SyncResult> = {}): SyncResult => ({
  created: 0,
  updated: 0,
  added: 0,
  changed: 0,
  conflicts: [],
  skipped: [],
  stale: [],
  ...extra,
});

/** The sync answer of the current test, by default one that never comes. */
let syncAnswer: () => Promise<SyncResult>;
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetLogBufferForTest();
  syncAnswer = () => new Promise(() => {});
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "append_log_records":
        return Promise.resolve();
      case "load_artifact_contents_by_id":
        return Promise.resolve({ body: synced(), checksum: "ck1" });
      case "validate_flow_document":
        return Promise.resolve({ valid: true, violations: [] });
      case "save_artifact_contents":
        return Promise.resolve({ checksum: `ck-${String(args?.body).length}` });
      case "sync_flow_with_tracker":
        return syncAnswer();
      default:
        return Promise.reject(new Error(`unexpected command ${cmd}`));
    }
  });
});

/** A store with the Flow open on a tab and loaded. */
async function openStore() {
  const flows = new FlowSessionStore();
  flows.openTab(ID);
  await flows.get(ID)?.pendingLoad;
  return flows;
}

/** A sync answer the test settles, and a wait for the sync call to go out. */
function heldSync() {
  let settle!: (r: SyncResult) => void;
  syncAnswer = () => new Promise((res) => (settle = res));
  return {
    sent: () => vi.waitFor(() => expect(calls("sync_flow_with_tracker")).toHaveLength(1)),
    settle: (r: SyncResult) => settle(r),
  };
}

describe("an edit while a sync runs (FLO-FR-KHBY)", () => {
  it("FLO-FR-KHBY, FLO-FR-26: applyEdit while a sync runs is refused, changes nothing, and schedules no write", async () => {
    const flows = await openStore();
    const before = flows.get(ID)!.doc;
    flows.setSyncState(ID, { running: "sync" });

    expect(flows.applyEdit(ID, (d) => renameItem(d, "t1", "changed"))).toBe(false);
    expect(flows.get(ID)!.doc).toBe(before);
    expect(flows.get(ID)!.dirty).toBe(false);
    expect(flows.hasPendingWrite(ID)).toBe(false);

    flows.setSyncState(ID, { running: null });
    expect(flows.applyEdit(ID, (d) => renameItem(d, "t1", "changed"))).toBe(true);
    expect(flows.get(ID)!.dirty).toBe(true);
    flows.clear();
  });

  it("FLO-FR-26: applyEdit reports false for an edit that changes nothing and for a Flow with no document", async () => {
    const flows = await openStore();

    expect(flows.applyEdit(ID, (d) => d)).toBe(false);
    expect(flows.applyEdit("other.flow", (d) => renameItem(d, "t1", "x"))).toBe(false);
    expect(flows.get(ID)!.dirty).toBe(false);
  });
});

describe("the sync and the tab's lifecycle (FLO-FR-WLHA, FSR NFR)", () => {
  it("FLO-FR-KHBY, FLO-FR-28: a tab closed while its sync runs gets no reload and no sync state when the sync returns", async () => {
    const flows = await openStore();
    const sync = heldSync();
    const run = startFlowSync(flows, ID);
    await sync.sent();
    const loads = calls("load_artifact_contents_by_id").length;

    flows.closeTab(ID);
    sync.settle(result({ conflicts: [CONFLICT] }));
    await run;

    expect(calls("load_artifact_contents_by_id")).toHaveLength(loads);
    expect(flows.get(ID)).toBeUndefined();
    expect(flows.syncState(ID)).toBe(IDLE_SYNC);
  });

  it("FSR-FR-KGOY, FSR-FR-RDZI: a closed tab drops its review, and a reopened tab starts with none", async () => {
    const flows = await openStore();
    syncAnswer = () => Promise.resolve(result({ conflicts: [CONFLICT] }));
    await startFlowSync(flows, ID);
    expect(flows.syncState(ID)).toMatchObject({ running: null, reviewOpen: true, conflicts: [CONFLICT] });

    flows.closeTab(ID);
    expect(flows.syncState(ID)).toBe(IDLE_SYNC);
    flows.openTab(ID);
    await flows.get(ID)?.pendingLoad;
    expect(flows.syncState(ID).conflicts).toEqual([]);
    expect(flows.syncState(ID).reviewOpen).toBe(false);
  });

  it("FLO-FR-WLHA, FLO-FR-KHBY: a sync that no canvas shows still reloads the plan and keeps its result", async () => {
    const flows = await openStore();
    const sync = heldSync();
    const run = startFlowSync(flows, ID);
    await sync.sent();
    expect(flows.syncState(ID).running).toBe("sync");
    const loads = calls("load_artifact_contents_by_id").length;

    sync.settle(result({ conflicts: [CONFLICT] }));
    await run;

    expect(calls("load_artifact_contents_by_id")).toHaveLength(loads + 1);
    expect(flows.syncState(ID)).toMatchObject({
      running: null,
      reviewOpen: true,
      settled: true,
      conflicts: [CONFLICT],
    });
  });
});
