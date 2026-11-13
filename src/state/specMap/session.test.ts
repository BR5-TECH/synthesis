import { beforeEach, describe, expect, it, vi } from "vitest";
import { INITIAL_VIEW, SpecMapSessionStore } from "./session";
import type { SpecMapOps } from "../../api/specMap";
import type { MapEdit } from "./types";
import { buildTree, specChildren } from "./tree";
import { openspecIndex, smallIndex } from "../../test/specMapFixtures";

const logWarn = vi.fn();
const logError = vi.fn();
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: (...args: unknown[]) => logWarn(...args),
  logError: (...args: unknown[]) => logError(...args),
}));

function makeOps(overrides: Partial<SpecMapOps> = {}) {
  const ops = {
    load: vi.fn(async () => smallIndex()),
    saveOrganization: vi.fn(async () => {}),
    attachDraft: vi.fn(async () => {}),
    ...overrides,
  };
  return ops;
}

async function loaded(ops = makeOps()) {
  const store = new SpecMapSessionStore(ops);
  await store.load();
  return { store, ops };
}

beforeEach(() => {
  logWarn.mockReset();
  logError.mockReset();
});

describe("loading the index", () => {
  it("SMP-FR-PMCX: the load runs once, and a session that holds an index loads nothing", async () => {
    const ops = makeOps();
    const store = new SpecMapSessionStore(ops);
    expect(store.snapshot().status).toBe("idle");
    const a = store.load();
    expect(store.snapshot().status).toBe("loading");
    const b = store.load();
    await Promise.all([a, b]);
    await store.load();
    expect(ops.load).toHaveBeenCalledTimes(1);
    expect(store.snapshot().status).toBe("ready");
  });

  it("SMP-FR-ONSD: a failed load enters the error state, is logged, and Retry loads again", async () => {
    const ops = makeOps({ load: vi.fn().mockRejectedValueOnce(new Error("boom")).mockResolvedValue(smallIndex()) });
    const store = new SpecMapSessionStore(ops);
    await store.load();
    expect(store.snapshot().status).toBe("error");
    expect(logError).toHaveBeenCalledWith(["frontend"], "specification map could not be loaded", { code: "boom", cause: "" });
    // A code that is not an OpenSpec failure gets no notice.
    expect(store.snapshot().errorFailure).toBeNull();
    await store.retry();
    expect(store.snapshot().status).toBe("ready");
    expect(ops.load).toHaveBeenCalledTimes(2);
  });

  it("SMP-FR-MEZK: a load that is still running when the session clears lands nowhere", async () => {
    let resolve!: (v: ReturnType<typeof smallIndex>) => void;
    const ops = makeOps({
      load: vi.fn(() => new Promise<ReturnType<typeof smallIndex>>((r) => (resolve = r))),
    });
    const store = new SpecMapSessionStore(ops);
    const pending = store.load();
    store.clear();
    resolve(smallIndex());
    await pending;
    expect(store.snapshot().status).toBe("idle");
    expect(store.snapshot().index).toBeNull();
  });
});

describe("the session lifetime", () => {
  it("SMP-FR-QNUH, SMP-FR-BXAP: resetting the view keeps the index, the edits, the selection and the toggles", async () => {
    const { store } = await loaded();
    store.dispatch({ kind: "edit", nodeId: "g1", label: "renamed", summary: "" });
    store.select({ kind: "spec", code: "A1" });
    store.toggleGaps();
    store.toggleDeps();
    store.setFocus("d1");
    store.setView({ level: 2, scale: 1.1, tx: 40, ty: -12, touched: true });

    store.resetView();
    const state = store.snapshot();
    expect(state.view).toEqual(INITIAL_VIEW);
    expect(state.focusId).toBeNull();
    expect(buildTree(state.index!).nodes.get("g1")!.label).toBe("renamed");
    expect(state.selection).toEqual({ kind: "spec", code: "A1" });
    expect(state.gapsOnly).toBe(true);
    expect(state.showDeps).toBe(false);
  });

  it("SMP-FR-MEZK, SMO-FR-MHEU, SMD-FR-BWJY: clearing discards the session, and the next load has no edits", async () => {
    const { store, ops } = await loaded();
    store.dispatch({ kind: "create", parentId: null, node: { id: "n1", label: "later", summary: "" } });
    store.dispatch({ kind: "attachDraft", nodeId: "g1", draft: { draftId: "x", name: "Untitled" } });
    store.clear();
    expect(store.snapshot().status).toBe("idle");
    expect(store.snapshot().index).toBeNull();
    expect(store.snapshot().selection).toBeNull();
    await store.load();
    const tree = store.tree()!;
    expect(tree.nodes.has("n1")).toBe(false);
    expect(tree.planned.size).toBe(0);
    expect(ops.load).toHaveBeenCalledTimes(2);
  });
});

describe("edits", () => {
  it("SMO-FR-CZLA: an edit applies at once and reaches the save operation as that one edit", async () => {
    const { store, ops } = await loaded();
    expect(
      store.dispatch({ kind: "move", ref: { kind: "spec", code: "A1" }, parentId: "g4", index: 0 }),
    ).toBe(true);
    expect(specChildren(store.tree()!.nodes.get("g4")!).map((s) => s.code)).toEqual(["A1", "C1", "C2"]);
    expect(ops.saveOrganization).toHaveBeenCalledWith({ kind: "move", nodeId: "A1", parentId: "g4", index: 0 });
  });

  it("SMO-FR-CZLA: a failed save is logged and changes nothing on the map", async () => {
    const { store } = await loaded(makeOps({ saveOrganization: vi.fn().mockRejectedValue(new Error("nope")) }));
    store.dispatch({ kind: "edit", nodeId: "g1", label: "renamed", summary: "" });
    await vi.waitFor(() =>
      expect(logWarn).toHaveBeenCalledWith(["frontend"], "specification map edit could not be saved", {
        kind: "edit",
        reason: "nope",
      }),
    );
    expect(store.tree()!.nodes.get("g1")!.label).toBe("renamed");
  });

  it("SMO-FR-CZLA: the edit is in the session before its save operation is invoked, and a delete saves as one delete", async () => {
    let seen: string | undefined;
    let store!: SpecMapSessionStore;
    const ops = makeOps({
      saveOrganization: vi.fn(async () => {
        seen = store.tree()!.nodes.get("g1")?.label;
      }),
    });
    store = new SpecMapSessionStore(ops);
    await store.load();
    store.dispatch({ kind: "edit", nodeId: "g1", label: "renamed", summary: "s" });
    expect(seen).toBe("renamed");
    store.dispatch({ kind: "delete", nodeId: "g2" });
    expect(ops.saveOrganization).toHaveBeenLastCalledWith({ kind: "delete", nodeId: "g2" });
  });

  it("SMO-FR-WGOF: a refused edit changes nothing, reaches no operation, and is logged", async () => {
    const { store, ops } = await loaded();
    const before = store.snapshot().index;
    expect(store.dispatch({ kind: "delete", nodeId: "g3" })).toBe(false);
    expect(store.snapshot().index).toBe(before);
    expect(ops.saveOrganization).not.toHaveBeenCalled();
    expect(logWarn).toHaveBeenCalledWith(["frontend"], "specification map edit refused", { kind: "delete" });
  });

  it("SMO-FR-NXDL: a created node is selected and asked to be shown", async () => {
    const { store } = await loaded();
    store.dispatch({ kind: "create", parentId: "f1", node: { id: "n1", label: "planning", summary: "" } });
    expect(store.snapshot().selection).toEqual({ kind: "index", id: "n1" });
    expect(store.snapshot().zoom?.ref).toEqual({ kind: "index", id: "n1" });
  });

  it("SMO-FR-OBRF: a delete moves the selection to the sibling that took the children", async () => {
    const { store } = await loaded();
    store.select({ kind: "index", id: "g2" });
    store.dispatch({ kind: "delete", nodeId: "g2" });
    expect(store.snapshot().selection).toEqual({ kind: "index", id: "g1" });
  });

  it("SMI-FR-WDAB: a subject removed from the tree leaves no selection", async () => {
    const { store } = await loaded();
    store.dispatch({ kind: "attachDraft", nodeId: "g1", draft: { draftId: "x", name: "Untitled" } });
    store.select({ kind: "planned", draftId: "x" });
    store.dispatch({ kind: "detachDraft", draftId: "x" });
    expect(store.snapshot().selection).toBeNull();
  });

  it("SMD-FR-OYLC, SMD-FR-QPAM: a placement and a planned chip's move both reach the placement operation", async () => {
    const { store, ops } = await loaded();
    store.dispatch({ kind: "attachDraft", nodeId: "g1", draft: { draftId: "x", name: "Untitled" } });
    expect(ops.attachDraft).toHaveBeenLastCalledWith({ draftId: "x", nodeId: "g1" });
    store.dispatch({ kind: "move", ref: { kind: "planned", draftId: "x" }, parentId: "d2", index: 0 });
    expect(ops.attachDraft).toHaveBeenLastCalledWith({ draftId: "x", nodeId: "d2" });
    expect(ops.saveOrganization).not.toHaveBeenCalled();
  });
});

describe("selection by project path (SMI-FR-PRSL)", () => {
  it("SMI-FR-PRSL: a spec path selects its spec node and asks to show it", async () => {
    const { store } = await loaded();
    store.selectByPath("specifications/ui/C1-c1.md");
    expect(store.snapshot().selection).toEqual({ kind: "spec", code: "C1" });
    expect(store.snapshot().zoom?.ref).toEqual({ kind: "spec", code: "C1" });
  });

  it("SMI-FR-PRSL: a path no spec node holds changes nothing", async () => {
    const { store } = await loaded();
    store.select({ kind: "index", id: "d1" });
    const version = store.getVersion();
    store.selectByPath("specifications/ui/ZZZ-none.md");
    expect(store.snapshot().selection).toEqual({ kind: "index", id: "d1" });
    expect(store.getVersion()).toBe(version);
  });

  it("SMI-FR-PRSL: a path that arrives before the index loads applies once it loads", async () => {
    const store = new SpecMapSessionStore(makeOps());
    store.selectByPath("specifications/core/A2-a2.md");
    expect(store.snapshot().selection).toBeNull();
    await store.load();
    expect(store.snapshot().selection).toEqual({ kind: "spec", code: "A2" });
    expect(store.snapshot().pendingPath).toBeNull();
  });
});

describe("an OpenSpec index (SMO-FR-RVAV, SMD-FR-YQSR)", () => {
  const refused: MapEdit[] = [
    { kind: "create", parentId: "group:/", node: { id: "n-1", label: "New", summary: "" } },
    { kind: "edit", nodeId: "capability:auth", label: "Renamed", summary: "" },
    { kind: "delete", nodeId: "capability:auth" },
    { kind: "move", ref: { kind: "index", id: "capability:auth" }, parentId: "group:billing", index: 0 },
    { kind: "move", ref: { kind: "spec", code: "auth#1" }, parentId: "capability:billing/pay", index: 0 },
  ];

  for (const edit of refused) {
    it(`SMO-FR-RVAV: a ${edit.kind} edit${edit.kind === "move" ? ` of a ${edit.ref.kind}` : ""} is refused, changes nothing, saves nothing, and is logged`, async () => {
      const { store, ops } = await loaded(makeOps({ load: vi.fn(async () => openspecIndex()) }));
      const before = store.snapshot();
      expect(store.dispatch(edit)).toBe(false);
      expect(store.snapshot()).toBe(before);
      expect(ops.saveOrganization).not.toHaveBeenCalled();
      expect(ops.attachDraft).not.toHaveBeenCalled();
      expect(logWarn).toHaveBeenCalledWith(
        ["frontend"],
        "specification map edit refused for an openspec index",
        { kind: edit.kind },
      );
    });
  }

  it("SMD-FR-YQSR: a draft edit is still applied to an OpenSpec index", async () => {
    const { store, ops } = await loaded(makeOps({ load: vi.fn(async () => openspecIndex()) }));
    expect(
      store.dispatch({ kind: "attachDraft", nodeId: "capability:auth", draft: { draftId: "dr", name: "Plan" } }),
    ).toBe(true);
    expect(ops.attachDraft).toHaveBeenCalledWith({ draftId: "dr", nodeId: "capability:auth" });
  });
});
