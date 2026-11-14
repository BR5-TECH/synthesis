import { beforeEach, describe, expect, it, vi } from "vitest";
import { INITIAL_VIEW, SpecMapSessionStore } from "./session";
import type { SpecMapOps } from "../../api/specMap";
import type { DeliverySource } from "../../types/delivery";
import { smallIndex } from "../../test/specMapFixtures";

const logWarn = vi.fn();
const logError = vi.fn();
const logInfo = vi.fn();
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: (...args: unknown[]) => logInfo(...args),
  logWarn: (...args: unknown[]) => logWarn(...args),
  logError: (...args: unknown[]) => logError(...args),
}));

const NONE: DeliverySource = { kind: "none", reason: "no_source" };
const FILE: DeliverySource = { kind: "file", flowId: "flow-1", path: "plan.flow" };

function makeOps(overrides: Partial<SpecMapOps> = {}) {
  const ops = {
    load: vi.fn<SpecMapOps["load"]>(async () => smallIndex()),
    getDeliverySource: vi.fn<SpecMapOps["getDeliverySource"]>(async () => NONE),
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
  logInfo.mockReset();
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
  it("SMP-FR-QNUH, SMP-FR-BXAP: resetting the views keeps the lens, the index, the selection, the toggles and the delivery data", async () => {
    const { store } = await loaded();
    store.setLens("delivery");
    store.select({ kind: "spec", code: "A1" });
    store.toggleGaps();
    store.toggleDeps();
    store.setFocus("d1");
    store.setView({ level: 2, scale: 1.1, tx: 40, ty: -12, touched: true });
    store.setDeliveryState({ data: { plan: "p" }, view: { x: 3 } });
    const index = store.snapshot().index;

    store.resetView();
    const state = store.snapshot();
    expect(state.view).toEqual(INITIAL_VIEW);
    expect(state.focusId).toBeNull();
    expect(state.delivery).toEqual({ data: { plan: "p" }, view: null });
    expect(state.lens).toBe("delivery");
    expect(state.index).toBe(index);
    expect(state.selection).toEqual({ kind: "spec", code: "A1" });
    expect(state.gapsOnly).toBe(true);
    expect(state.showDeps).toBe(false);
  });

  it("SMP-FR-MEZK: clearing discards the whole session, and the next open asks for the lens and the index again", async () => {
    const { store, ops } = await loaded();
    await store.decideLens();
    store.select({ kind: "spec", code: "A1" });
    store.setDeliveryState({ data: { plan: "p" } });
    store.clear();
    const state = store.snapshot();
    expect(state.status).toBe("idle");
    expect(state.index).toBeNull();
    expect(state.selection).toBeNull();
    expect(state.lens).toBeNull();
    expect(state.deliverySource).toBeNull();
    expect(state.delivery).toEqual({ data: null, view: null });
    await store.decideLens();
    await store.load();
    expect(ops.getDeliverySource).toHaveBeenCalledTimes(2);
    expect(ops.load).toHaveBeenCalledTimes(2);
  });
});

describe("the lens (SMP-FR-TKUS, SMP-FR-RNLH)", () => {
  const sources: [string, DeliverySource, "delivery" | "specifications"][] = [
    ["github", {
      kind: "github",
      project: { nodeId: "P1", title: "Plan", ownerLogin: "acme", number: 1, url: "" },
      repository: { host: "github.com", owner: "acme", name: "app" },
      configuration: {} as never,
    }, "delivery"],
    ["file", FILE, "delivery"],
    ["none", NONE, "specifications"],
    ["none with a missing file", { kind: "none", reason: "delivery_file_missing", flowId: "f" }, "specifications"],
  ];
  for (const [name, source, lens] of sources) {
    it(`SMP-FR-TKUS: a ${name} source opens the ${lens} lens`, async () => {
      const store = new SpecMapSessionStore(makeOps({ getDeliverySource: vi.fn(async () => source) }));
      expect(store.snapshot().lens).toBeNull();
      await store.decideLens();
      expect(store.snapshot().lens).toBe(lens);
      expect(store.snapshot().deliverySource).toEqual(source);
      expect(logInfo).toHaveBeenCalledWith(["frontend"], "map tab chose its default lens", {
        sourceKind: source.kind,
        lens,
      });
    });
  }

  it("SMP-FR-TKUS: the source is asked for once per session, also by two renders at the same time", async () => {
    const ops = makeOps();
    const store = new SpecMapSessionStore(ops);
    await Promise.all([store.decideLens(), store.decideLens()]);
    await store.decideLens();
    expect(ops.getDeliverySource).toHaveBeenCalledTimes(1);
  });

  it("SMP-FR-TKUS: a source that cannot be read opens the Specifications lens, and is logged", async () => {
    const store = new SpecMapSessionStore(
      makeOps({ getDeliverySource: vi.fn(() => Promise.reject(new TypeError("no backend"))) }),
    );
    await store.decideLens();
    expect(store.snapshot().lens).toBe("specifications");
    expect(store.snapshot().deliverySource).toBeNull();
    expect(logWarn).toHaveBeenCalledWith(["frontend"], "the delivery source of the map could not be read", {
      reason: "TypeError",
    });
  });

  it("SMP-FR-TKUS: a lens the author chose while the source was asked for stays", async () => {
    let answer!: (s: DeliverySource) => void;
    const store = new SpecMapSessionStore(
      makeOps({ getDeliverySource: vi.fn(() => new Promise<DeliverySource>((r) => (answer = r))) }),
    );
    const pending = store.decideLens();
    store.setLens("specifications");
    answer(FILE);
    await pending;
    expect(store.snapshot().lens).toBe("specifications");
    expect(store.snapshot().deliverySource).toEqual(FILE);
  });

  it("SMP-FR-MEZK: a source answer that arrives after the session cleared lands nowhere", async () => {
    let answer!: (s: DeliverySource) => void;
    const store = new SpecMapSessionStore(
      makeOps({ getDeliverySource: vi.fn(() => new Promise<DeliverySource>((r) => (answer = r))) }),
    );
    const pending = store.decideLens();
    store.clear();
    answer(FILE);
    await pending;
    expect(store.snapshot().lens).toBeNull();
    expect(store.snapshot().deliverySource).toBeNull();
  });

  it("SMP-FR-RNLH: a lens switch keeps the selection, the view, the index and the delivery state", async () => {
    const { store } = await loaded();
    store.setLens("specifications");
    store.select({ kind: "index", id: "f1" });
    store.setView({ level: 1, scale: 1, tx: 5, ty: 6, touched: true });
    store.setDeliveryState({ data: { selected: "item-1" }, view: { zoom: 2 } });
    const before = store.snapshot();
    store.setLens("delivery");
    store.setLens("specifications");
    const after = store.snapshot();
    expect(after.selection).toBe(before.selection);
    expect(after.view).toBe(before.view);
    expect(after.index).toBe(before.index);
    expect(after.delivery).toEqual({ data: { selected: "item-1" }, view: { zoom: 2 } });
  });

  it("SMP-FR-TKUS: a switch to the lens that is already active changes nothing", async () => {
    const { store } = await loaded();
    store.setLens("specifications");
    const version = store.getVersion();
    store.setLens("specifications");
    expect(store.getVersion()).toBe(version);
  });
});

describe("the dirty indicator of the Delivery lens (SMP-FR-SWUL)", () => {
  it("SMP-FR-SWUL: the flag notifies its own listeners on a change only, and not the map listeners", () => {
    const store = new SpecMapSessionStore(makeOps());
    const dirty = vi.fn();
    const map = vi.fn();
    store.subscribeDeliveryDirty(dirty);
    store.subscribe(map);
    store.setDeliveryDirty(true);
    store.setDeliveryDirty(true);
    expect(store.deliveryDirty()).toBe(true);
    expect(dirty).toHaveBeenCalledTimes(1);
    expect(map).not.toHaveBeenCalled();
    store.setDeliveryDirty(false);
    expect(dirty).toHaveBeenCalledTimes(2);
  });

  it("SMP-FR-SWUL: Save writes through the handler of the lens only while it holds unsaved changes", async () => {
    const store = new SpecMapSessionStore(makeOps());
    const save = vi.fn(async () => {});
    store.setDeliverySave(save);
    await store.saveDelivery();
    expect(save).not.toHaveBeenCalled();
    store.setDeliveryDirty(true);
    await store.saveDelivery();
    expect(save).toHaveBeenCalledTimes(1);
  });

  it("SMP-FR-SWUL, SMP-FR-MEZK: clearing the session clears the flag and drops the handler", async () => {
    const store = new SpecMapSessionStore(makeOps());
    const dirty = vi.fn();
    const save = vi.fn(async () => {});
    store.subscribeDeliveryDirty(dirty);
    store.setDeliverySave(save);
    store.setDeliveryDirty(true);
    store.clear();
    expect(store.deliveryDirty()).toBe(false);
    expect(dirty).toHaveBeenCalledTimes(2);
    store.setDeliveryDirty(true);
    await store.saveDelivery();
    expect(save).not.toHaveBeenCalled();
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
