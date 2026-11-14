/**
 * The per-machine layout of each scope (FLO-FR-21, FLO-FR-PLUZ, FLO-FR-TDHR,
 * MDL-FR-QAJN).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { resetLogBufferForTest } from "../logging";
import { EMPTY_LAYOUT, FlowLayoutStore } from "./flowLayouts";
import { FlowSessionStore } from "./flowSessions";
import type { StoredLayout } from "./flowDocument";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const LAYOUT: StoredLayout = { items: { t1: { position: { x: 4, y: 8 } } } };

/** A Flow on disk with the item `t1` the layout places. */
const FLOW_BODY = JSON.stringify({ version: 3, name: "", items: [{ id: "t1", kind: "task", name: "T1" }], edges: [] });

/** The answers of the layout commands; a test replaces one to hold or fail it. */
let load: (scope: string) => Promise<StoredLayout>;
let save: (scope: string, layout: StoredLayout) => Promise<void>;

beforeEach(() => {
  resetLogBufferForTest();
  load = async () => LAYOUT;
  save = async () => undefined;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "load_delivery_layout") return load(String(args.scope));
    if (cmd === "save_delivery_layout") return save(String(args.scope), args.layout as StoredLayout);
    if (cmd === "load_artifact_contents_by_id") return { body: FLOW_BODY, checksum: "ck-0" };
    if (cmd === "validate_flow_document") return { valid: true, violations: [] };
    throw new Error(`unexpected invoke ${cmd}`);
  });
});

afterEach(() => {
  vi.useRealTimers();
});

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);

/** Let the promises the store started settle. */
const settle = async () => {
  for (let i = 0; i < 5; i += 1) await Promise.resolve();
};

describe("FlowLayoutStore (FLO-FR-21, FLO-FR-PLUZ)", () => {
  it("FLO-FR-PLUZ: reads a scope's layout once, and holds nothing until the read lands", async () => {
    const store = new FlowLayoutStore();
    store.ensure("flow:plans/a.flow");
    store.ensure("flow:plans/a.flow");
    expect(store.get("flow:plans/a.flow")).toBeNull();
    await settle();
    expect(store.get("flow:plans/a.flow")).toEqual(LAYOUT);
    expect(calls("load_delivery_layout")).toEqual([{ scope: "flow:plans/a.flow" }]);
  });

  it("FLO-FR-PLUZ: a read that fails gives the empty layout, so the plan lays itself out", async () => {
    load = async () => {
      throw new Error("store_unavailable");
    };
    const store = new FlowLayoutStore();
    store.ensure("github:PVT_1");
    await settle();
    expect(store.get("github:PVT_1")).toEqual(EMPTY_LAYOUT);
  });

  it("FLO-FR-PLUZ: a read that gives nothing is the empty layout, and two scopes stay apart", async () => {
    load = async (scope) => (scope === "flow:a" ? (null as unknown as StoredLayout) : LAYOUT);
    const store = new FlowLayoutStore();
    store.ensure("flow:a");
    store.ensure("flow:b");
    await vi.waitFor(() => expect(store.get("flow:b")).toEqual(LAYOUT));
    expect(store.get("flow:a")).toEqual(EMPTY_LAYOUT);
  });

  it("FLO-FR-29: a read that lands after a clear fills nothing, and a later ensure reads again", async () => {
    let answer!: (l: StoredLayout) => void;
    load = () => new Promise((resolve) => (answer = resolve));
    const store = new FlowLayoutStore();
    store.ensure("flow:a");
    await vi.waitFor(() => expect(calls("load_delivery_layout")).toHaveLength(1));
    store.clear();
    answer(LAYOUT);
    await settle();
    expect(store.get("flow:a")).toBeNull();
    load = async () => LAYOUT;
    store.ensure("flow:a");
    await vi.waitFor(() => expect(store.get("flow:a")).toEqual(LAYOUT));
    expect(calls("load_delivery_layout")).toHaveLength(2);
  });

  it("FLO-FR-21: a save is held at once and stored once, with its scope", async () => {
    const store = new FlowLayoutStore();
    const seen = vi.fn();
    store.subscribe(seen);
    void store.save("flow:plans/a.flow", LAYOUT);
    expect(store.get("flow:plans/a.flow")).toEqual(LAYOUT);
    expect(seen).toHaveBeenCalledTimes(1);
    await settle();
    expect(calls("save_delivery_layout")).toEqual([{ scope: "flow:plans/a.flow", layout: LAYOUT }]);
  });

  it("FLO-FR-21: a read that lands after a save does not replace the saved layout", async () => {
    let answer!: (l: StoredLayout) => void;
    load = () => new Promise((resolve) => (answer = resolve));
    const store = new FlowLayoutStore();
    store.ensure("flow:a");
    await settle();
    const mine: StoredLayout = { items: { m1: { position: { x: 1, y: 1 } } } };
    await store.save("flow:a", mine);
    answer(LAYOUT);
    await settle();
    expect(store.get("flow:a")).toEqual(mine);
  });

  it("FLO-FR-21: a save that fails keeps the layout for this session", async () => {
    save = async () => {
      throw new Error("invalid_layout");
    };
    const store = new FlowLayoutStore();
    await store.save("flow:a", LAYOUT);
    expect(store.get("flow:a")).toEqual(LAYOUT);
  });

  it("MDL-FR-QAJN: a reset stores the empty layout", async () => {
    const store = new FlowLayoutStore();
    await store.save("github:PVT_1", LAYOUT);
    await store.reset("github:PVT_1");
    expect(store.get("github:PVT_1")).toEqual(EMPTY_LAYOUT);
    expect(calls("save_delivery_layout").slice(-1)[0]).toEqual({
      scope: "github:PVT_1",
      layout: { items: {} },
    });
  });
});

describe("the layouts of the Flow session store (FLO-FR-TDHR)", () => {
  it("FLO-FR-TDHR: a layout save of an open Flow marks it not dirty and schedules no write", async () => {
    vi.useFakeTimers();
    const flows = new FlowSessionStore();
    flows.openTab("plans/a.flow");
    await flows.get("plans/a.flow")?.pendingLoad;
    expect(flows.get("plans/a.flow")?.doc?.items.map((i) => i.id)).toEqual(["t1"]);
    await flows.layouts.save("flow:plans/a.flow", LAYOUT);
    expect(flows.layouts.get("flow:plans/a.flow")).toEqual(LAYOUT);
    expect(flows.get("plans/a.flow")?.dirty).toBe(false);
    expect(flows.dirtyIds()).toEqual([]);
    expect(flows.hasPendingWrite("plans/a.flow")).toBe(false);
    await vi.runAllTimersAsync();
    expect(calls("save_artifact_contents")).toEqual([]);
  });

  it("FLO-FR-29: a clear of the store forgets the layouts of the project", async () => {
    const flows = new FlowSessionStore();
    await flows.layouts.save("flow:plans/a.flow", LAYOUT);
    flows.clear();
    expect(flows.layouts.get("flow:plans/a.flow")).toBeNull();
  });
});
