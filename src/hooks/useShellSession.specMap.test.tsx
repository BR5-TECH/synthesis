// The Map tab through the shell: its identity in the strip, its dirty
// indicator, its session's lifetime, and the Project panel's routing into it.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
import { DASHBOARD_TAB, SPEC_MAP_TAB } from "./shell/tabRecords";
import { createTabClosures } from "./shell/tabClosures";
import { resetToasts } from "../state/toasts";
import { createLifecycleActions, type LifecycleDeps } from "./shell/lifecycleActions";
import { INITIAL_VIEW } from "../state/specMap/session";
import { SearchSessionStore } from "../state/searchSessions";
import { EditSessionStore } from "../state/editSessions";
import { FlowSessionStore } from "../state/flowSessions";
import { DraftSessionStore } from "../state/draftSessions";
import { followTargetForTab } from "../state/selectionFollowsTab";
import { resetPanelReveals } from "../state/panelReveal";
import { makeStore } from "../test/specMapRender";
import { currentMainBranchWrite, requestMainBranchWrite } from "../state/mainBranchWrite";
import { mainGuard } from "../test/mainBranchWriteFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

beforeEach(() => {
  resetToasts();
  resetPanelReveals();
  invokeMock.mockReset();
  // The shell pushes the menu state on each render, and the log flushes in
  // the background. Every other command is not expected here.
  invokeMock.mockImplementation((cmd: string) =>
    cmd === "append_log_records" || cmd === "set_save_menu_state" || cmd === "set_find_menu_state"
      ? Promise.resolve(null)
      : Promise.reject(new Error(`unexpected command ${cmd}`)),
  );
});
const stores: FlowSessionStore[] = [];
afterEach(() => {
  cleanup();
  // A scheduled write of a store this file made must not outlive its test.
  for (const flows of stores.splice(0)) flows.clear();
});

async function shellWithLoadedMap() {
  const { store, ops } = makeStore();
  await store.load();
  const hook = renderHook(() => useShellSession({ specMap: store }));
  return { store, ops, ...hook };
}

const mapTabs = (tabs: { kind?: string }[]) => tabs.filter((t) => t.kind === "map");

describe("the Map tab's identity (TAB-FR-KXMW, SMP-FR-WBNL)", () => {
  it("TAB-FR-KXMW, SMP-FR-WBNL, SMP-FR-HZRA: opening the map adds one Map tab and focuses it; a second request only focuses it", async () => {
    const { result } = await shellWithLoadedMap();
    act(() => result.current.openSpecMap());
    expect(mapTabs(result.current.tabs)).toHaveLength(1);
    expect(result.current.activeTab).toBe(SPEC_MAP_TAB.id);

    act(() => result.current.activateTab("dashboard"));
    act(() => {
      result.current.openSpecMap();
      result.current.openSpecMap();
    });
    expect(mapTabs(result.current.tabs)).toHaveLength(1);
    expect(result.current.activeTab).toBe(SPEC_MAP_TAB.id);
    expect(result.current.tabs.find((t) => t.kind === "map")!.label).toBe("Map");
  });

  it("SMP-FR-SWUL: without unsaved Delivery lens changes the Map tab is not dirty and Save is unavailable", async () => {
    const { result } = await shellWithLoadedMap();
    act(() => result.current.openSpecMap());
    act(() => result.current.specMap.select({ kind: "spec", code: "A1" }));
    expect(result.current.tabs.find((t) => t.kind === "map")?.dirty).toBeFalsy();
    expect(result.current.saveEnabled).toBe(false);
    expect(result.current.findEnabled).toBe(false);
  });

  it("SMP-FR-SWUL: the Map tab is dirty while the delivery's Flow file holds unsaved changes, and clean once they are written", async () => {
    const { store } = makeStore();
    await store.load();
    const flows = new FlowSessionStore();
    stores.push(flows);
    const flowId = "plans/release.flow";
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "load_artifact_contents_by_id") return Promise.resolve({ body: "", checksum: "ck" });
      if (cmd === "validate_flow_document") return Promise.resolve({ valid: true, violations: [] });
      if (cmd === "append_log_records" || cmd === "set_save_menu_state" || cmd === "set_find_menu_state") {
        return Promise.resolve(null);
      }
      return Promise.reject(new Error(`unexpected command ${cmd}`));
    });
    const { result } = renderHook(() => useShellSession({ specMap: store, flows }));
    act(() => result.current.openSpecMap());
    act(() => store.setDeliverySource({ kind: "file", flowId, path: flowId }));
    await act(async () => {
      flows.openTab(flowId);
      await flows.get(flowId)?.pendingLoad;
    });
    act(() => {
      flows.applyEdit(flowId, (d) => ({ ...d, name: "Edited" }));
    });
    expect(result.current.tabs.find((t) => t.kind === "map")?.dirty).toBe(true);
    // The flag leaves every other tab as it was.
    expect(result.current.tabs.find((t) => t.id === "dashboard")?.dirty).toBeFalsy();
    act(() => store.setDeliverySource({ kind: "none", reason: "no_source" }));
    expect(result.current.tabs.find((t) => t.kind === "map")?.dirty).toBeFalsy();
  });

  it("TAB-FR-KXMW: the closures of a removed path and of a commit never close the Map tab", () => {
    const editor = {
      id: "art:specifications/ui/C1-c1.md",
      label: "C1-c1.md",
      kind: "editor" as const,
      artifactId: "specifications/ui/C1-c1.md",
    };
    const tabs = [DASHBOARD_TAB, SPEC_MAP_TAB, editor];
    let current = tabs;
    const setTabs = vi.fn((next: typeof tabs | ((ts: typeof tabs) => typeof tabs)) => {
      current = typeof next === "function" ? next(current) : next;
    });
    const closures = createTabClosures({
      tabs,
      setTabs,
      setActiveTab: vi.fn(),
      sessions: new EditSessionStore(),
      flows: new FlowSessionStore(),
    } as unknown as Parameters<typeof createTabClosures>[0]);
    closures.closeTabsForRemovedPaths(["specifications"]);
    // The closure ran: the Editor tab under the removed folder is gone.
    expect(current.some((t) => t.id === editor.id)).toBe(false);
    expect(current.some((t) => t.id === SPEC_MAP_TAB.id)).toBe(true);
    closures.closeDiffTabsForCommittedPaths(["specifications/ui/C1-c1.md"]);
    expect(current.some((t) => t.id === SPEC_MAP_TAB.id)).toBe(true);
  });

  it("SNV-FR-66: the Map tab selects nowhere in a vertical panel", () => {
    expect(followTargetForTab(SPEC_MAP_TAB)).toBeNull();
  });
});

describe("the map session's lifetime (SMP)", () => {
  it("SMP-FR-QNUH, SMP-FR-BXAP: closing the Map tab resets the views and keeps the lens, the index and the selection", async () => {
    const { store, result } = await shellWithLoadedMap();
    act(() => result.current.openSpecMap());
    const index = store.snapshot().index;
    act(() => {
      store.setLens("specifications");
      store.setDeliveryState({ data: { plan: 1 }, view: { zoom: 2 } });
      store.select({ kind: "spec", code: "A1" });
      store.setView({ level: 2, scale: 1.2, tx: 10, ty: 20, touched: true });
      store.setFocus("d1");
      store.setInspectorOpen(false);
    });
    await act(async () => {
      await result.current.closeTab(SPEC_MAP_TAB.id);
    });
    expect(store.snapshot().inspectorOpen).toBe(false);
    expect(mapTabs(result.current.tabs)).toHaveLength(0);
    expect(store.snapshot().view).toEqual(INITIAL_VIEW);
    expect(store.snapshot().focusId).toBeNull();
    expect(store.snapshot().index).toBe(index);
    expect(store.snapshot().lens).toBe("specifications");
    expect(store.snapshot().delivery).toEqual({ data: { plan: 1 }, view: null });
    expect(store.snapshot().selection).toEqual({ kind: "spec", code: "A1" });

    // Reopened, the tab reads the same session.
    act(() => result.current.openSpecMap());
    expect(store.snapshot().status).toBe("ready");
  });

  it("SMP-FR-QNUH: switching away from the Map tab and back keeps the whole session", async () => {
    const { store, ops, result } = await shellWithLoadedMap();
    act(() => result.current.openSpecMap());
    act(() => {
      store.setLens("specifications");
      store.select({ kind: "spec", code: "A1" });
      store.setFocus("d1");
      store.setView({ level: 1, scale: 1.1, tx: 3, ty: 4, touched: true });
      store.toggleDeps();
      store.toggleGaps();
      store.setInspectorOpen(false);
    });
    const before = store.snapshot();
    act(() => result.current.activateTab("dashboard"));
    act(() => result.current.activateTab(SPEC_MAP_TAB.id));
    expect(store.snapshot()).toBe(before);
    expect(ops.load).toHaveBeenCalledTimes(1);
  });

  it("SMP-FR-MEZK: the viewport reset of a project or worktree switch discards the whole map session", async () => {
    const { store } = makeStore();
    await store.load();
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
    expect(store.snapshot().status).toBe("idle");
    expect(store.snapshot().index).toBeNull();
  });
});

describe("a confirmation that waits over a switch (MBW-FR-GYVD)", () => {
  it("MBW-FR-GYVD, MBW-FR-XBJS: the viewport reset of a project or worktree switch answers a waiting confirmation as cancelled", async () => {
    const answer = requestMainBranchWrite({
      write: "commit",
      branch: null,
      guard: mainGuard(),
      readError: null,
    });
    const real: Record<string, unknown> = {
      specMap: makeStore().store,
      searches: new SearchSessionStore(),
      sessions: new EditSessionStore(),
      flows: new FlowSessionStore(),
      drafts: new DraftSessionStore(),
    };
    const deps = new Proxy(real, {
      get: (target, key: string) => (key in target ? target[key] : vi.fn()),
    }) as unknown as LifecycleDeps;
    createLifecycleActions(deps).resetViewport();
    await expect(answer).resolves.toBe(false);
    expect(currentMainBranchWrite()).toBeNull();
  });
});

describe("Project panel file clicks (LIB-FR-SJDC)", () => {
  const specRow = { id: "specifications/ui/C1-c1.md", name: "C1-c1.md", artifactType: "spec" as const };

  it("LIB-FR-SJDC, SMI-FR-PRSL: while the Map tab is active, a Spec file selects its node and opens no tab", async () => {
    const { store, result } = await shellWithLoadedMap();
    act(() => result.current.openSpecMap());
    const before = result.current.tabs.length;
    act(() => result.current.activateLibraryFile(specRow));
    expect(store.snapshot().selection).toEqual({ kind: "spec", code: "C1" });
    expect(result.current.tabs).toHaveLength(before);
    expect(result.current.activeTab).toBe(SPEC_MAP_TAB.id);
  });

  it("LIB-FR-SJDC, LIB-FR-03: a file of another type opens even while the Map tab is active", async () => {
    const { store, result } = await shellWithLoadedMap();
    act(() => result.current.openSpecMap());
    act(() =>
      result.current.activateLibraryFile({ id: ".claude/skills/x/SKILL.md", name: "SKILL.md", artifactType: "skill" }),
    );
    expect(result.current.activeTab).toBe("art:.claude/skills/x/SKILL.md");
    expect(store.snapshot().selection).toBeNull();
  });

  it("LIB-FR-03: with any other tab active, a Spec file opens in its Editor tab", async () => {
    const { store, result } = await shellWithLoadedMap();
    act(() => result.current.activateLibraryFile(specRow));
    expect(result.current.activeTab).toBe(`art:${specRow.id}`);
    expect(store.snapshot().selection).toBeNull();
  });
});
