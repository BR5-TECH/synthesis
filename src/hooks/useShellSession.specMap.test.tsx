// The Map tab through the shell: its identity in the strip, its session's
// lifetime, the Project panel's routing into it, and drafts started from it.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
import { DASHBOARD_TAB, SPEC_MAP_TAB } from "./shell/tabRecords";
import { createTabClosures } from "./shell/tabClosures";
import { MAP_DRAFT_FAILED } from "./shell/draftActions";
import { createLifecycleActions, type LifecycleDeps } from "./shell/lifecycleActions";
import { INITIAL_VIEW } from "../state/specMap/session";
import { SearchSessionStore } from "../state/searchSessions";
import { EditSessionStore } from "../state/editSessions";
import { FlowSessionStore } from "../state/flowSessions";
import { DraftSessionStore } from "../state/draftSessions";
import { followTargetForTab } from "../state/selectionFollowsTab";
import { resetPanelReveals } from "../state/panelReveal";
import { makeStore } from "../test/specMapRender";

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

type Listing = { id: string; name: string; status: string }[];
let listing: Listing = [];
let failCreate = false;

beforeEach(() => {
  resetPanelReveals();
  listing = [];
  failCreate = false;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "create_draft":
        if (failCreate) throw new Error("store is malformed");
        return {
          draft: { id: "dr1", name: "Untitled", status: "active", folder: "", promptPath: "p.md" },
          file: "p.md",
        };
      case "list_drafts":
        return { folders: [], drafts: listing.map((d) => ({ ...d, folder: "" })) };
      default:
        return null;
    }
  });
});
afterEach(cleanup);

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
    expect(result.current.tabs.find((t) => t.kind === "map")!.label).toBe("Map — specifications");
  });

  it("SMP-FR-SWUL: the Map tab is never dirty and Save is unavailable while it is active", async () => {
    const { result } = await shellWithLoadedMap();
    act(() => result.current.openSpecMap());
    const { store } = { store: result.current.specMap };
    act(() => {
      store.dispatch({ kind: "edit", nodeId: "g1", label: "renamed", summary: "" });
    });
    expect(result.current.tabs.find((t) => t.kind === "map")?.dirty).toBeFalsy();
    expect(result.current.saveEnabled).toBe(false);
    expect(result.current.findEnabled).toBe(false);
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
  it("SMP-FR-QNUH, SMP-FR-BXAP: closing the Map tab resets the view and keeps the index, the edits and the selection", async () => {
    const { store, result } = await shellWithLoadedMap();
    act(() => result.current.openSpecMap());
    act(() => {
      store.dispatch({ kind: "edit", nodeId: "g1", label: "renamed", summary: "" });
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
    expect(store.tree()!.nodes.get("g1")!.label).toBe("renamed");
    expect(store.snapshot().selection).toEqual({ kind: "spec", code: "A1" });

    // Reopened, the tab reads the same session.
    act(() => result.current.openSpecMap());
    expect(store.snapshot().status).toBe("ready");
  });

  it("SMP-FR-QNUH: switching away from the Map tab and back keeps the whole session", async () => {
    const { store, ops, result } = await shellWithLoadedMap();
    act(() => result.current.openSpecMap());
    act(() => {
      store.dispatch({ kind: "attachDraft", nodeId: "g1", draft: { draftId: "p", name: "Plan" } });
      store.dispatch({ kind: "edit", nodeId: "g2", label: "renamed", summary: "" });
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

describe("drafts started from the map (SMD)", () => {
  it("SMD-FR-HVBE, SMD-FR-OYLC, NAW-FR-03: New draft creates at the drafts root, opens the New Artifact tab, and places the draft", async () => {
    const { store, ops, result } = await shellWithLoadedMap();
    listing = [{ id: "dr1", name: "Untitled", status: "active" }];
    act(() => result.current.openSpecMap());
    await act(async () => {
      await result.current.createDraftForMapNode("g1");
    });
    const create = invokeMock.mock.calls.find((c) => c[0] === "create_draft")!;
    expect(create[1]).toEqual(expect.objectContaining({ folder: null }));
    expect(result.current.activeTab).toBe("draft:dr1");
    expect(store.tree()!.plannedParent.get("dr1")).toBe("g1");
    expect(ops.attachDraft).toHaveBeenCalledWith({ draftId: "dr1", nodeId: "g1" });
  });

  it("SMD-FR-EKWN: a failed creation places nothing, opens no tab, and says the draft could not be created", async () => {
    const { store, result } = await shellWithLoadedMap();
    failCreate = true;
    act(() => result.current.openSpecMap());
    await act(async () => {
      await result.current.createDraftForMapNode("g1");
    });
    expect(result.current.toast).toBe(MAP_DRAFT_FAILED);
    expect(result.current.tabs.some((t) => t.kind === "draft")).toBe(false);
    expect(result.current.activeTab).toBe(SPEC_MAP_TAB.id);
    expect(store.tree()!.planned.size).toBe(0);
  });

  it("SMD-FR-LKVU: a published draft keeps its planned chip", async () => {
    const { store, result } = await shellWithLoadedMap();
    act(() => {
      store.dispatch({ kind: "attachDraft", nodeId: "g1", draft: { draftId: "dr1", name: "Plan" } });
    });
    listing = [{ id: "dr1", name: "Plan", status: "published" }];
    invokeMock.mockClear();
    act(() => result.current.bumpDrafts());
    await vi.waitFor(() => expect(invokeMock.mock.calls.some((c) => c[0] === "list_drafts")).toBe(true));
    await act(async () => {
      await Promise.resolve();
    });
    expect(store.tree()!.planned.has("dr1")).toBe(true);
  });

  it("SMD-FR-LKVU: a planned chip follows its draft's rename, and leaves the map when the draft is archived", async () => {
    const { store, result } = await shellWithLoadedMap();
    act(() => {
      store.dispatch({ kind: "attachDraft", nodeId: "g1", draft: { draftId: "dr1", name: "Untitled" } });
    });
    listing = [{ id: "dr1", name: "Planning", status: "active" }];
    act(() => result.current.bumpDrafts());
    await waitFor(() => expect(store.tree()!.planned.get("dr1")?.name).toBe("Planning"));

    listing = [{ id: "dr1", name: "Planning", status: "archived" }];
    act(() => result.current.bumpDrafts());
    await waitFor(() => expect(store.tree()!.planned.size).toBe(0));
  });

  it("SMD-FR-LKVU: a draft placed while a listing is in flight is not judged by that listing", async () => {
    const { store, result } = await shellWithLoadedMap();
    act(() => {
      store.dispatch({ kind: "attachDraft", nodeId: "g1", draft: { draftId: "old", name: "Old" } });
    });
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd !== "list_drafts") return null;
      await gate;
      // The listing predates the placement below: it holds only the old draft.
      return { folders: [], drafts: [{ id: "old", name: "Old", status: "active", folder: "" }] };
    });
    act(() => result.current.bumpDrafts());
    act(() => {
      store.dispatch({ kind: "attachDraft", nodeId: "g4", draft: { draftId: "new", name: "New" } });
    });
    await act(async () => {
      release();
      await gate;
    });
    await vi.waitFor(() => expect(invokeMock.mock.calls.some((c) => c[0] === "list_drafts")).toBe(true));
    await act(async () => {
      await Promise.resolve();
    });
    expect(store.tree()!.planned.has("new")).toBe(true);
    expect(store.tree()!.planned.has("old")).toBe(true);
  });

  it("reads no draft listing while the map holds no planned draft", async () => {
    const { result } = await shellWithLoadedMap();
    invokeMock.mockClear();
    act(() => result.current.bumpDrafts());
    await Promise.resolve();
    expect(invokeMock.mock.calls.filter((c) => c[0] === "list_drafts")).toHaveLength(0);
  });
});
