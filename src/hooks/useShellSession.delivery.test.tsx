// The project's delivery through the shell: the routing of the delivery's
// Flow file to the Map tab (LIB-FR-JTXR, MDL-FR-YSMB), the requests to show a
// delivery item (MDL-FR-JIKK, DSH-FR-DFDW, NAW-FR-PHGO), the Map tab as the
// Flow tab of that file (MDL-FR-VLQJ, SMP-FR-SWUL, SNV-FR-GNQB, SNV-FR-31),
// the close of a Flow tab whose file becomes the delivery (MDL-FR-EFQF), and
// the session's reset (SMP-FR-MEZK).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
import { SPEC_MAP_TAB } from "./shell/tabRecords";
import { createLifecycleActions, type LifecycleDeps } from "./shell/lifecycleActions";
import { deliveryFlowId, useDeliveryShell } from "./shell/useDeliveryShell";
import { resetToasts } from "../state/toasts";
import { resetPanelReveals } from "../state/panelReveal";
import { SearchSessionStore } from "../state/searchSessions";
import { EditSessionStore } from "../state/editSessions";
import { FlowSessionStore } from "../state/flowSessions";
import { DraftSessionStore } from "../state/draftSessions";
import { makeStore } from "../test/specMapRender";
import { githubSource } from "../test/deliveryLensHarness";
import { handle } from "../test/shellSessionFixtures";
import type { DeliverySource } from "../types/delivery";
import type { SearchHit, Tab } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
type Handler = (ev: { payload: unknown }) => void;
const listeners = vi.hoisted(() => new Map<string, Handler[]>());
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: Handler) => {
    listeners.set(name, [...(listeners.get(name) ?? []), cb]);
    return () => listeners.set(name, (listeners.get(name) ?? []).filter((x) => x !== cb));
  }),
}));
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

const FLOW = "plans/release.flow";
const OTHER = "plans/other.flow";
const FILE: DeliverySource = { kind: "file", flowId: FLOW, path: FLOW };

let writes: Array<{ id: string; body: string }>;
let refuseWrites: boolean;

beforeEach(() => {
  resetToasts();
  resetPanelReveals();
  listeners.clear();
  writes = [];
  refuseWrites = false;
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "append_log_records":
      case "set_save_menu_state":
      case "set_find_menu_state":
        return Promise.resolve(null);
      case "load_artifact_contents_by_id":
        return Promise.resolve({ body: "", checksum: "ck" });
      case "validate_flow_document":
        return Promise.resolve({ valid: true, violations: [] });
      case "save_artifact_contents":
        if (refuseWrites) return Promise.reject({ code: "write_refused", message: "The file cannot be written." });
        writes.push({ id: String(args?.id ?? args?.artifactId ?? ""), body: String(args?.body) });
        return Promise.resolve({ checksum: `ck-${writes.length}` });
      default:
        return Promise.reject(new Error(`unexpected command ${cmd}`));
    }
  });
});

const stores: FlowSessionStore[] = [];
afterEach(() => {
  cleanup();
  for (const flows of stores.splice(0)) flows.clear();
});

async function shell(source: DeliverySource | null = FILE) {
  const { store } = makeStore();
  await store.load();
  store.setDeliverySource(source);
  const flows = new FlowSessionStore();
  stores.push(flows);
  const hook = renderHook(() => useShellSession({ specMap: store, flows }));
  return { store, flows, ...hook };
}

/** Load the delivery's Flow file into its session and give it an unsaved edit. */
async function dirtyFlow(flows: FlowSessionStore, flowId = FLOW) {
  await act(async () => {
    flows.openTab(flowId);
    await flows.get(flowId)?.pendingLoad;
  });
  act(() => {
    flows.applyEdit(flowId, (d) => ({ ...d, name: "Edited" }));
  });
}

const flowTabs = (tabs: Tab[]) => tabs.filter((t) => t.kind === "flow");

describe("the delivery's Flow file opens the Map tab (LIB-FR-JTXR, MDL-FR-YSMB)", () => {
  it("LIB-FR-JTXR: a Project panel click on the delivery's Flow opens the Map tab on its Delivery lens and no Flow tab", async () => {
    const { store, result } = await shell();
    act(() => result.current.activateLibraryFile({ id: FLOW, name: "release.flow", artifactType: "flow" }));
    expect(result.current.activeTab).toBe(SPEC_MAP_TAB.id);
    expect(store.snapshot().lens).toBe("delivery");
    expect(flowTabs(result.current.tabs)).toHaveLength(0);
  });

  it("MDL-FR-YSMB: a Search hit, a Dashboard row, or a plan tag of the delivery's Flow opens the Map tab and no Flow tab", async () => {
    const { result } = await shell();
    const hit = { id: FLOW, name: "release.flow", group: "flow", kind: "artifact", artifactType: "flow" } as unknown as SearchHit;
    act(() => result.current.activateSearchHit(hit));
    expect(result.current.activeTab).toBe(SPEC_MAP_TAB.id);
    act(() => result.current.activateTab("dashboard"));
    // DSH-FR-06 and NAW-FR-TXKN take the shell's open route.
    act(() => result.current.openArtifact({ id: FLOW, name: "release.flow", artifactType: "flow" }));
    expect(result.current.activeTab).toBe(SPEC_MAP_TAB.id);
    act(() => result.current.activateTab("dashboard"));
    act(() => result.current.openArtifact({ id: FLOW, name: "release.flow" }));
    expect(result.current.activeTab).toBe(SPEC_MAP_TAB.id);
    expect(flowTabs(result.current.tabs)).toHaveLength(0);
  });

  it("LIB-FR-03: a Flow that is not the delivery opens its Flow tab", async () => {
    const { result } = await shell();
    act(() => result.current.activateLibraryFile({ id: OTHER, name: "other.flow", artifactType: "flow" }));
    expect(result.current.activeTab).toBe(`art:${OTHER}`);
    expect(flowTabs(result.current.tabs)).toHaveLength(1);
  });

  it("LIB-FR-SJDC: a Spec file clicked while the Map tab shows the Delivery lens selects its node on the Specifications lens", async () => {
    const { store, result } = await shell();
    act(() => result.current.showDeliveryItem(null));
    act(() =>
      result.current.activateLibraryFile({ id: "specifications/ui/C1-c1.md", name: "C1-c1.md", artifactType: "spec" }),
    );
    expect(store.snapshot().lens).toBe("specifications");
    expect(store.snapshot().selection).toEqual({ kind: "spec", code: "C1" });
  });
});

describe("requests to show a delivery item (MDL-FR-JIKK, DSH-FR-DFDW, NAW-FR-PHGO)", () => {
  it("MDL-FR-JIKK, DSH-FR-DFDW: a milestone opens the Map tab on its Delivery lens and asks the lens to select it", async () => {
    const { store, result } = await shell();
    act(() => result.current.showDeliveryItem("m1"));
    expect(result.current.activeTab).toBe(SPEC_MAP_TAB.id);
    expect(store.snapshot().lens).toBe("delivery");
    expect(store.snapshot().deliveryReveal?.itemId).toBe("m1");
  });

  it("DSH-FR-DFDW: the widget header opens the Map tab on its Delivery lens with no item to select", async () => {
    const { store, result } = await shell();
    act(() => store.setLens("specifications"));
    act(() => result.current.showDeliveryItem(null));
    expect(result.current.activeTab).toBe(SPEC_MAP_TAB.id);
    expect(store.snapshot().lens).toBe("delivery");
    expect(store.snapshot().deliveryReveal).toBeNull();
  });
});

describe("the Map tab as the Flow tab of the delivery (MDL-FR-VLQJ, SMP-FR-SWUL, SNV-FR-GNQB, SNV-FR-31)", () => {
  it("SNV-FR-GNQB, SMP-FR-SWUL: Save is enabled while the Map tab is active and the delivery's Flow is dirty, and it writes the Flow", async () => {
    const { flows, result } = await shell();
    act(() => result.current.openSpecMap());
    expect(result.current.saveEnabled).toBe(false);
    await dirtyFlow(flows);
    expect(result.current.saveEnabled).toBe(true);
    expect(result.current.tabs.find((t) => t.kind === "map")?.dirty).toBe(true);
    await act(async () => {
      await result.current.requestSave();
    });
    expect(writes).toHaveLength(1);
    expect(writes[0].body).toContain("Edited");
    expect(flows.get(FLOW)?.dirty).toBe(false);
    expect(result.current.saveEnabled).toBe(false);
    expect(result.current.tabs.find((t) => t.kind === "map")?.dirty).toBeFalsy();
  });

  it("SNV-FR-GNQB: with another tab active, the dirty delivery Flow does not enable Save", async () => {
    const { flows, result } = await shell();
    act(() => result.current.openSpecMap());
    await dirtyFlow(flows);
    act(() => result.current.activateTab("dashboard"));
    expect(result.current.saveEnabled).toBe(false);
    expect(result.current.saveAllEnabled).toBe(true);
  });

  it("SNV-FR-GNQB: with a github source, the Map tab active, and nothing dirty, Save is disabled", async () => {
    const { result } = await shell(githubSource());
    act(() => result.current.openSpecMap());
    expect(result.current.activeTab).toBe(SPEC_MAP_TAB.id);
    expect(result.current.saveEnabled).toBe(false);
    expect(result.current.tabs.find((t) => t.kind === "map")?.dirty).toBeFalsy();
  });

  it("SNV-FR-31: Save All writes the delivery's Flow with everything else that is unsaved", async () => {
    const { flows, result } = await shell();
    await dirtyFlow(flows);
    await act(async () => {
      await result.current.requestSaveAll();
    });
    expect(writes).toHaveLength(1);
    expect(flows.get(FLOW)?.dirty).toBe(false);
  });

  it("MDL-FR-VLQJ, FLO-FR-28: closing the Map tab writes the pending changes of the delivery's Flow, and nothing of it is kept", async () => {
    const { flows, result } = await shell();
    act(() => result.current.openSpecMap());
    await dirtyFlow(flows);
    await act(async () => {
      expect(await result.current.closeTab(SPEC_MAP_TAB.id)).toBe(true);
    });
    expect(writes).toHaveLength(1);
    expect(flows.get(FLOW)).toBeUndefined();
    expect(result.current.tabs.some((t) => t.id === SPEC_MAP_TAB.id)).toBe(false);
  });

  it("MDL-FR-VLQJ, FLO-FR-28: a refused write keeps the Map tab open and active", async () => {
    const { flows, result } = await shell();
    act(() => result.current.openSpecMap());
    await dirtyFlow(flows);
    act(() => result.current.activateTab("dashboard"));
    refuseWrites = true;
    await act(async () => {
      expect(await result.current.closeTab(SPEC_MAP_TAB.id)).toBe(false);
    });
    expect(result.current.activeTab).toBe(SPEC_MAP_TAB.id);
    expect(flows.get(FLOW)?.dirty).toBe(true);
  });

  it("SMP-FR-BXAP, MDL-FR-VLQJ: a close that a refused write stops resets no view", async () => {
    const { store, flows, result } = await shell();
    act(() => result.current.openSpecMap());
    await dirtyFlow(flows);
    act(() => {
      store.setView({ level: 1, scale: 1.2, tx: 4, ty: 5, touched: true });
      store.keepDeliveryView({ x: 1, y: 2, scale: 1 });
    });
    refuseWrites = true;
    await act(async () => {
      await result.current.closeTab(SPEC_MAP_TAB.id);
    });
    expect(store.snapshot().view).toMatchObject({ tx: 4, ty: 5 });
    expect(store.snapshot().delivery.view).toEqual({ x: 1, y: 2, scale: 1 });
  });
});

describe("a Flow tab of the file that becomes the delivery (MDL-FR-EFQF)", () => {
  /**
   * An open project with no delivery, and a dirty Flow tab of FLOW. The shell
   * follows the delivery only while a project is open, so the project opens
   * first; the source it reads is `current`.
   */
  async function dirtyFlowTab() {
    let current: DeliverySource = { kind: "none", reason: "no_source" };
    const { store } = makeStore(undefined, { getDeliverySource: vi.fn(async () => current) });
    await store.load();
    const flows = new FlowSessionStore();
    stores.push(flows);
    const hook = renderHook(() => useShellSession({ specMap: store, flows }));
    await act(async () => hook.result.current.loadProject(handle("p", "/p")));
    await waitFor(() => expect(store.snapshot().deliverySource).toEqual(current));
    const s = {
      store,
      flows,
      result: hook.result,
      becomeDelivery: () => {
        current = FILE;
        store.setDeliverySource(FILE);
      },
    };
    act(() => s.result.current.activateLibraryFile({ id: FLOW, name: "release.flow", artifactType: "flow" }));
    expect(flowTabs(s.result.current.tabs)).toHaveLength(1);
    await dirtyFlow(s.flows);
    return s;
  }

  it("MDL-FR-EFQF: the open Flow tab of the new delivery file writes its changes and closes", async () => {
    const { flows, result, becomeDelivery } = await dirtyFlowTab();
    expect(writes).toHaveLength(0);
    act(() => becomeDelivery());
    await waitFor(() => expect(flowTabs(result.current.tabs)).toHaveLength(0));
    expect(writes).toHaveLength(1);
    expect(writes[0].body).toContain("Edited");
    expect(flows.get(FLOW)?.dirty ?? false).toBe(false);
  });

  it("MDL-FR-EFQF, FLO-FR-28: a refused write keeps the Flow tab open and dirty", async () => {
    const { flows, result, becomeDelivery } = await dirtyFlowTab();
    refuseWrites = true;
    act(() => becomeDelivery());
    const saves = () => invokeMock.mock.calls.filter((c) => c[0] === "save_artifact_contents");
    await waitFor(() => expect(saves()).toHaveLength(1));
    // Let the refusal reach the close, which then keeps the tab: a macrotask
    // runs only after every microtask of the refusal ran.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(flowTabs(result.current.tabs)).toHaveLength(1);
    expect(flows.get(FLOW)?.dirty).toBe(true);
    expect(writes).toHaveLength(0);
    expect(saves()).toHaveLength(1);
  });

  it("MDL-FR-EFQF: a source that names another file, or no file, closes no tab", async () => {
    const { store } = makeStore();
    const flows = new FlowSessionStore();
    stores.push(flows);
    const tab: Tab = { id: `art:${FLOW}`, label: "release.flow", kind: "flow", artifactId: FLOW };
    const closeTab = vi.fn(async () => true);
    renderHook(() =>
      useDeliveryShell({ active: true, projectKey: "/p", contentRootEpoch: 0, specMap: store, flows, tabs: [tab], closeTab }),
    );
    act(() => store.setDeliverySource({ kind: "file", flowId: OTHER, path: OTHER }));
    act(() => store.setDeliverySource({ kind: "none", reason: "no_source" }));
    expect(closeTab).not.toHaveBeenCalled();
  });

  it("LIB-FR-JTXR: an open project reads the source at once and again on delivery source changed", async () => {
    const getDeliverySource = vi.fn(async () => FILE);
    const { store } = makeStore(undefined, { getDeliverySource });
    const flows = new FlowSessionStore();
    stores.push(flows);
    renderHook(() =>
      useDeliveryShell({ active: true, projectKey: "/p", contentRootEpoch: 0, specMap: store, flows, tabs: [], closeTab: vi.fn() }),
    );
    await waitFor(() => expect(store.snapshot().deliverySource).toEqual(FILE));
    await waitFor(() => expect(listeners.get("delivery-source-changed")?.length).toBe(1));
    act(() => listeners.get("delivery-source-changed")!.forEach((cb) => cb({ payload: FILE })));
    await waitFor(() => expect(getDeliverySource).toHaveBeenCalledTimes(2));
    expect(deliveryFlowId(store.snapshot().deliverySource)).toBe(FLOW);
  });
});

describe("a project or worktree switch (SMP-FR-MEZK)", () => {
  it("SMP-FR-MEZK: the viewport reset clears the lens, the delivery source, and the Map tab's dirty indicator", async () => {
    const { store } = makeStore();
    await store.load();
    store.setLens("delivery");
    store.setDeliverySource(FILE);
    store.setDeliveryDirty(true);
    const real: Record<string, unknown> = {
      specMap: store,
      searches: new SearchSessionStore(),
      sessions: new EditSessionStore(),
      flows: new FlowSessionStore(),
      drafts: new DraftSessionStore(),
    };
    const deps = new Proxy(real, {
      get: (target, key: string) => (key in target ? target[key] : vi.fn()),
    }) as unknown as LifecycleDeps;
    createLifecycleActions(deps).resetViewport();
    expect(store.snapshot().lens).toBeNull();
    expect(store.snapshot().deliverySource).toBeNull();
    expect(store.deliveryDirty()).toBe(false);
  });
});
