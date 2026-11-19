/**
 * The status filter of each layout scope (`MDL-map-delivery-lens.md`
 * MDL-FR-ZAGM, `DMS-delivery-machine-state.md` DMS-FR-TXMH, DMS-FR-ROMF).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { DeliveryView } from "../types";

const invokeMock = vi.fn();
const warnMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock("../logging", async (original) => ({
  ...(await original<typeof import("../logging")>()),
  logWarn: (...args: unknown[]) => warnMock(...args),
}));

const { DeliveryViewStore, EMPTY_VIEW } = await import("./deliveryViews");
const { resetLogBufferForTest } = await import("../logging");

const VIEW: DeliveryView = { hidden: ["done"], blockedOnly: true };

/** The answers of the view commands; a test replaces one to hold or fail it. */
let load: (scope: string) => Promise<DeliveryView>;
let save: (scope: string, view: DeliveryView) => Promise<void>;

beforeEach(() => {
  resetLogBufferForTest();
  warnMock.mockReset();
  load = async () => VIEW;
  save = async () => undefined;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "load_delivery_view") return load(String(args.scope));
    if (cmd === "save_delivery_view") return save(String(args.scope), args.view as DeliveryView);
    throw new Error(`unexpected invoke ${cmd}`);
  });
});

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);

/** Let the promises the store started settle. */
const settle = async () => {
  for (let i = 0; i < 5; i += 1) await Promise.resolve();
};

describe("DeliveryViewStore (MDL-FR-ZAGM)", () => {
  it("MDL-FR-ZAGM: reads a scope's view once, and holds nothing until the read lands", async () => {
    const store = new DeliveryViewStore();
    store.ensure("flow:plans/a.flow");
    store.ensure("flow:plans/a.flow");
    expect(store.get("flow:plans/a.flow")).toBeNull();
    await settle();
    expect(store.get("flow:plans/a.flow")).toEqual(VIEW);
    expect(calls("load_delivery_view")).toEqual([{ scope: "flow:plans/a.flow" }]);
  });

  it("MDL-FR-ZAGM: a read that fails shows every item, and logs its typed error without the scope", async () => {
    load = () => Promise.reject({ code: "store_unavailable", message: "The store is not available." });
    const store = new DeliveryViewStore();
    store.ensure("flow:secret/plan.flow");
    await settle();
    expect(store.get("flow:secret/plan.flow")).toEqual(EMPTY_VIEW);
    expect(warnMock).toHaveBeenCalledTimes(1);
    const [, message, fields] = warnMock.mock.calls[0]!;
    expect(message).toBe("delivery view not read");
    expect(fields).toMatchObject({ scope_kind: "flow", code: "store_unavailable" });
    expect(JSON.stringify(warnMock.mock.calls)).not.toContain("secret/plan.flow");
  });

  it("DMS-FR-TXMH: a read keeps each board status once, in board order, and drops a word it does not know", async () => {
    load = async () =>
      ({ hidden: ["done", "backlog", "done", "archived"], blockedOnly: "yes" }) as unknown as DeliveryView;
    const store = new DeliveryViewStore();
    store.ensure("github:PVT_1");
    await settle();
    expect(store.get("github:PVT_1")).toEqual({ hidden: ["backlog", "done"], blockedOnly: false });
  });

  it("MDL-FR-ZAGM: a save is held at once and stored once, with its scope", async () => {
    const store = new DeliveryViewStore();
    const seen = vi.fn();
    store.subscribe(seen);
    void store.save("flow:plans/a.flow", { hidden: ["in review", "ready"], blockedOnly: false });
    expect(store.get("flow:plans/a.flow")).toEqual({ hidden: ["ready", "in review"], blockedOnly: false });
    expect(seen).toHaveBeenCalledTimes(1);
    await settle();
    expect(calls("save_delivery_view")).toEqual([
      { scope: "flow:plans/a.flow", view: { hidden: ["ready", "in review"], blockedOnly: false } },
    ]);
  });

  it("MDL-FR-ZAGM: a read that lands after a save does not replace the saved view", async () => {
    let answer!: (v: DeliveryView) => void;
    load = () => new Promise((resolve) => (answer = resolve));
    const store = new DeliveryViewStore();
    store.ensure("flow:a");
    await settle();
    await store.save("flow:a", EMPTY_VIEW);
    answer(VIEW);
    await settle();
    expect(store.get("flow:a")).toEqual(EMPTY_VIEW);
  });

  it("MDL-FR-ZAGM: a save that fails keeps the filter on screen, and logs its typed error", async () => {
    save = () => Promise.reject({ code: "invalid_view", message: "The view is not valid." });
    const store = new DeliveryViewStore();
    await store.save("github:PVT_1", VIEW);
    expect(store.get("github:PVT_1")).toEqual(VIEW);
    expect(warnMock).toHaveBeenCalledTimes(1);
    expect(warnMock.mock.calls[0]![2]).toMatchObject({ scope_kind: "github", code: "invalid_view", hidden: 1 });
    expect(JSON.stringify(warnMock.mock.calls)).not.toContain("PVT_1");
  });

  it("MDL-FR-ZAGM: a clear forgets every scope, and a later ensure reads again", async () => {
    const store = new DeliveryViewStore();
    store.ensure("flow:a");
    await settle();
    store.clear();
    expect(store.get("flow:a")).toBeNull();
    store.ensure("flow:a");
    await settle();
    expect(calls("load_delivery_view")).toHaveLength(2);
  });
});
