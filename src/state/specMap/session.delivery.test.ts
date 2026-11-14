/**
 * The parts of the map session that the Delivery lens and the delivery links
 * use (`SMP-specification-map.md`, `MDL-map-delivery-lens.md`,
 * `SDL-specification-delivery-links.md`, `SMI-specification-map-inspector.md`).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SpecMapSessionStore } from "./session";
import type { SpecMapOps } from "../../api/specMap";
import type { DeliverySource } from "../../types/delivery";
import { smallIndex } from "../../test/specMapFixtures";

const logWarn = vi.fn();
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: (...args: unknown[]) => logWarn(...args),
  logError: vi.fn(),
}));

const NONE: DeliverySource = { kind: "none", reason: "no_source" };
const FILE: DeliverySource = { kind: "file", flowId: "flow-1", path: "plan.flow" };
const OTHER: DeliverySource = { kind: "file", flowId: "flow-2", path: "other.flow" };

function makeOps(overrides: Partial<SpecMapOps> = {}) {
  return {
    load: vi.fn<SpecMapOps["load"]>(async () => smallIndex()),
    getDeliverySource: vi.fn<SpecMapOps["getDeliverySource"]>(async () => NONE),
    ...overrides,
  };
}

beforeEach(() => {
  logWarn.mockReset();
});

describe("a failed load (SMP-FR-RNLH, SMP-FR-ONSD)", () => {
  it("SMP-FR-RNLH, SMP-FR-ONSD: a failed load stays failed when the lens renders again; only Retry loads again", async () => {
    const ops = makeOps({ load: vi.fn().mockRejectedValueOnce(new Error("boom")).mockResolvedValue(smallIndex()) });
    const store = new SpecMapSessionStore(ops);
    await store.load();
    expect(store.snapshot().status).toBe("error");
    // A lens switch or a remount calls load again; the error stays.
    await store.load();
    expect(store.snapshot().status).toBe("error");
    expect(ops.load).toHaveBeenCalledTimes(1);
    await store.retry();
    expect(store.snapshot().status).toBe("ready");
    expect(ops.load).toHaveBeenCalledTimes(2);
  });
});

describe("the selection of a new index (SMI-FR-WDAB)", () => {
  it("SMI-FR-WDAB: an index that holds no node of the selection clears the selection", async () => {
    const ops = makeOps({ load: vi.fn().mockRejectedValueOnce(new Error("boom")).mockResolvedValue(smallIndex()) });
    const store = new SpecMapSessionStore(ops);
    await store.load();
    store.select({ kind: "spec", code: "GONE" });
    await store.retry();
    expect(store.snapshot().selection).toBeNull();
  });

  it("SMI-FR-WDAB: an index that holds the node of the selection keeps the selection", async () => {
    const ops = makeOps({ load: vi.fn().mockRejectedValueOnce(new Error("boom")).mockResolvedValue(smallIndex()) });
    const store = new SpecMapSessionStore(ops);
    await store.load();
    store.select({ kind: "spec", code: "A1" });
    await store.retry();
    expect(store.snapshot().selection).toEqual({ kind: "spec", code: "A1" });
  });
});

describe("the delivery source (MDL-FR-YTAP, SMP-FR-MEZK)", () => {
  it("MDL-FR-YTAP: a read records the source and changes no lens", async () => {
    const store = new SpecMapSessionStore(makeOps({ getDeliverySource: vi.fn(async () => FILE) }));
    await store.resolveDeliverySource();
    expect(store.snapshot().deliverySource).toEqual(FILE);
    expect(store.snapshot().lens).toBeNull();
  });

  it("MDL-FR-YTAP: a read asked for while one is in flight makes one more read, and the newest answer stays", async () => {
    const answers: Array<(s: DeliverySource) => void> = [];
    const getDeliverySource = vi.fn(() => new Promise<DeliverySource>((r) => answers.push(r)));
    const store = new SpecMapSessionStore(makeOps({ getDeliverySource }));
    const first = store.resolveDeliverySource();
    const second = store.resolveDeliverySource();
    const third = store.resolveDeliverySource();
    answers[0](FILE);
    await vi.waitFor(() => expect(answers).toHaveLength(2));
    answers[1](OTHER);
    await Promise.all([first, second, third]);
    expect(getDeliverySource).toHaveBeenCalledTimes(2);
    expect(store.snapshot().deliverySource).toEqual(OTHER);
  });

  it("MDL-FR-YTAP: a failed read keeps the last source and is logged", async () => {
    const getDeliverySource = vi.fn<SpecMapOps["getDeliverySource"]>().mockResolvedValueOnce(FILE).mockRejectedValueOnce(new TypeError("x"));
    const store = new SpecMapSessionStore(makeOps({ getDeliverySource }));
    await store.resolveDeliverySource();
    await store.resolveDeliverySource();
    expect(store.snapshot().deliverySource).toEqual(FILE);
    expect(logWarn).toHaveBeenCalledWith(["frontend"], "the delivery source could not be read again", { reason: "TypeError" });
  });

  it("MDL-FR-YTAP: an answer equal to the source held notifies nobody", async () => {
    const store = new SpecMapSessionStore(makeOps({ getDeliverySource: vi.fn(async () => ({ ...FILE })) }));
    await store.resolveDeliverySource();
    const version = store.getVersion();
    await store.resolveDeliverySource();
    expect(store.getVersion()).toBe(version);
  });

  it("SMP-FR-MEZK: a read that answers after the session cleared lands nowhere", async () => {
    let answer!: (s: DeliverySource) => void;
    const store = new SpecMapSessionStore(
      makeOps({ getDeliverySource: vi.fn(() => new Promise<DeliverySource>((r) => (answer = r))) }),
    );
    const pending = store.resolveDeliverySource();
    store.clear();
    answer(FILE);
    await pending;
    expect(store.snapshot().deliverySource).toBeNull();
  });

  it("SMP-FR-MEZK: a clear while the default lens is asked for lets the next session ask again, and the old answer or refusal lands nowhere", async () => {
    const pending: Array<{ ok: (s: DeliverySource) => void; fail: (e: unknown) => void }> = [];
    const getDeliverySource = vi.fn(
      () => new Promise<DeliverySource>((ok, fail) => pending.push({ ok, fail })),
    );
    const store = new SpecMapSessionStore(makeOps({ getDeliverySource }));
    const old = store.decideLens();
    store.clear();
    const next = store.decideLens();
    expect(getDeliverySource).toHaveBeenCalledTimes(2);
    pending[0].fail(new TypeError("late"));
    await old;
    expect(store.snapshot().lens).toBeNull();
    expect(logWarn).not.toHaveBeenCalled();
    pending[1].ok(FILE);
    await next;
    expect(store.snapshot().lens).toBe("delivery");

    expect(store.snapshot().lens).toBe("delivery");
  });
});

describe("requests to the Delivery lens (MDL-FR-JIKK, SDL-FR-CFHH, SDL-FR-MCGI)", () => {
  it("MDL-FR-JIKK, SDL-FR-MCGI: a request to show an item shows the Delivery lens and leaves the item for the lens", () => {
    const store = new SpecMapSessionStore(makeOps());
    store.setLens("specifications");
    store.showDeliveryItem("t1");
    const { lens, deliveryReveal } = store.snapshot();
    expect(lens).toBe("delivery");
    expect(deliveryReveal?.itemId).toBe("t1");
    store.consumeDeliveryReveal(deliveryReveal!.nonce);
    expect(store.snapshot().deliveryReveal).toBeNull();
  });

  it("DSH-FR-DFDW: a request with no item shows the Delivery lens and asks for no item", () => {
    const store = new SpecMapSessionStore(makeOps());
    store.showDeliveryItem(null);
    expect(store.snapshot().lens).toBe("delivery");
    expect(store.snapshot().deliveryReveal).toBeNull();
  });

  it("SDL-FR-CFHH: Plan work shows the Delivery lens with the task to add, which the lens consumes once", () => {
    const store = new SpecMapSessionStore(makeOps());
    store.setLens("specifications");
    store.requestPlanWork("a1 label", ["specifications/ui/A1-a1.md"]);
    const request = store.snapshot().planWork!;
    expect(store.snapshot().lens).toBe("delivery");
    expect(request).toMatchObject({ name: "a1 label", references: ["specifications/ui/A1-a1.md"] });
    store.consumePlanWork(request.nonce - 1);
    expect(store.snapshot().planWork).not.toBeNull();
    store.consumePlanWork(request.nonce);
    expect(store.snapshot().planWork).toBeNull();
  });
});

describe("the unplanned toggle (SDL-FR-HZLB, SDL-FR-VUQH)", () => {
  it("SDL-FR-HZLB: the toggle is off by default", () => {
    expect(new SpecMapSessionStore(makeOps()).snapshot().unplanned).toBe(false);
  });

  it("SDL-FR-VUQH: turning unplanned on turns gaps only off, and turning gaps only on turns unplanned off", () => {
    const store = new SpecMapSessionStore(makeOps());
    store.toggleGaps();
    expect(store.snapshot()).toMatchObject({ gapsOnly: true, unplanned: false });
    store.toggleUnplanned();
    expect(store.snapshot()).toMatchObject({ gapsOnly: false, unplanned: true });
    store.toggleGaps();
    expect(store.snapshot()).toMatchObject({ gapsOnly: true, unplanned: false });
    // Turning a toggle off changes the other one not at all.
    store.toggleGaps();
    expect(store.snapshot()).toMatchObject({ gapsOnly: false, unplanned: false });
  });
});

describe("the data and the view of the Delivery lens (SMP-FR-RNLH, SMP-FR-BXAP)", () => {
  it("SMP-FR-RNLH: the data is made once, notifies nobody, and a clear drops it", () => {
    const store = new SpecMapSessionStore(makeOps());
    const isBox = (d: unknown): d is { box: number } => typeof d === "object" && d !== null && "box" in d;
    const version = store.getVersion();
    const made = store.ensureDeliveryData(isBox, () => ({ box: 1 }));
    expect(store.ensureDeliveryData(isBox, () => ({ box: 2 }))).toBe(made);
    expect(store.getVersion()).toBe(version);
    store.clear();
    expect(store.ensureDeliveryData(isBox, () => ({ box: 3 }))).toEqual({ box: 3 });
  });

  it("SMP-FR-RNLH, SMP-FR-BXAP: the kept view notifies nobody, and closing the tab resets it", () => {
    const store = new SpecMapSessionStore(makeOps());
    const version = store.getVersion();
    store.keepDeliveryView({ x: 1, y: 2, scale: 1.5 });
    expect(store.getVersion()).toBe(version);
    expect(store.snapshot().delivery.view).toEqual({ x: 1, y: 2, scale: 1.5 });
    store.resetView();
    expect(store.snapshot().delivery.view).toBeNull();
  });
});
