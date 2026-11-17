/**
 * The shared review state of the plan proposals of the Map tab
 * (`MPV-map-proposal-review.md` MPV-FR-CZDO, `DBR-delivery-board.md`). The
 * backend is mocked at `invoke`; each answer is released by hand, never by a
 * timer.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
const unlistens: string[] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (e: { payload: unknown }) => void) => {
    const set = listeners.get(event) ?? new Set();
    set.add(cb);
    listeners.set(event, set);
    return () => {
      unlistens.push(event);
      set.delete(cb);
    };
  }),
}));
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

import { logInfo, logWarn } from "../../logging";
import { createRow, draftRow, modifyRow, proposal, withRow } from "../../test/mapPlanFixtures";
import { otherCommand } from "../../test/invokeMocks";
import type { MapPlanProposal } from "../../types";
import { resetToasts, visibleToasts } from "../toasts";
import { ghostItemId, rowId } from "./derive";
import { MapPlanReviewStore } from "./reviewStore";

const EVENT = "map-plan-proposals-changed";

/** A deferred answer, released by hand. */
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** Settle every microtask the store chained on an answer. */
const settle = () => new Promise<void>((r) => setTimeout(r, 0));

let routes: Record<string, (args: Record<string, unknown>) => Promise<unknown>>;
const calls: { cmd: string; args: Record<string, unknown> }[] = [];
let store: MapPlanReviewStore;

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  unlistens.length = 0;
  calls.length = 0;
  resetToasts();
  vi.mocked(logInfo).mockClear();
  vi.mocked(logWarn).mockClear();
  routes = {};
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    calls.push({ cmd, args });
    const route = routes[cmd];
    return route ? route(args) : otherCommand(cmd);
  });
  store = new MapPlanReviewStore();
});

afterEach(() => store.dispose());

const callsOf = (cmd: string) => calls.filter((c) => c.cmd === cmd).map((c) => c.args);

async function held(p: MapPlanProposal) {
  routes.list_map_plan_proposals = async () => [p];
  store.want(p.discussionId);
  await settle();
}

describe("reads (MPV-FR-IBSF, MPV-FR-CZDO)", () => {
  it("MPV-FR-IBSF: want holds null, then the list, and a second want reads nothing", async () => {
    const answer = deferred<MapPlanProposal[]>();
    routes.list_map_plan_proposals = () => answer.promise;
    store.want("t1");
    expect(store.snapshot().proposals.get("t1")).toBeNull();
    store.want("t1");
    answer.resolve([proposal([createRow("a", "A")])]);
    await settle();
    expect(store.snapshot().proposals.get("t1")).toHaveLength(1);
    expect(callsOf("list_map_plan_proposals")).toHaveLength(1);
  });

  it("MPV-FR-IBSF: a failed first read marks the discussion unavailable", async () => {
    routes.list_map_plan_proposals = () => Promise.reject(new Error("down"));
    store.want("t1");
    await settle();
    expect(store.snapshot().unavailable.has("t1")).toBe(true);
  });

  it("MPV-FR-CZDO: a failed read keeps the proposals it holds", async () => {
    await held(proposal([createRow("a", "A")]));
    routes.list_map_plan_proposals = () => Promise.reject(new Error("down"));
    store.read("t1");
    await settle();
    expect(store.snapshot().proposals.get("t1")).toHaveLength(1);
    expect(store.snapshot().unavailable.has("t1")).toBe(false);
  });

  it("MPV-FR-CZDO: an older answer never replaces a newer one", async () => {
    await held(proposal([createRow("a", "A")]));
    const older = deferred<MapPlanProposal[]>();
    const newer = deferred<MapPlanProposal[]>();
    const answers = [older, newer];
    routes.list_map_plan_proposals = () => answers.shift()!.promise;
    store.read("t1");
    store.read("t1");
    newer.resolve([proposal([createRow("a", "A", { state: "accepted" })])]);
    await settle();
    older.resolve([proposal([createRow("a", "A", { state: "rejected" })])]);
    await settle();
    expect(store.snapshot().proposals.get("t1")![0].rows[0].state).toBe("accepted");
  });

  it("MPV-FR-CZDO: the event re-reads a held discussion and ignores another", async () => {
    await held(proposal([createRow("a", "A")]));
    for (const cb of listeners.get(EVENT) ?? []) cb({ payload: { discussionId: "other" } });
    for (const cb of listeners.get(EVENT) ?? []) cb({ payload: { discussionId: "t1" } });
    await settle();
    expect(callsOf("list_map_plan_proposals")).toEqual([{ discussionId: "t1" }, { discussionId: "t1" }]);
    expect(listeners.get(EVENT)?.size).toBe(1);
  });

  it("MPV-FR-XAOG: the first read announces nothing; a later change of a state is announced", async () => {
    await held(proposal([createRow("a", "Alpha"), createRow("b", "Beta")]));
    expect(store.snapshot().announcement).toBe("");
    routes.list_map_plan_proposals = async () => [
      proposal([createRow("a", "Alpha", { state: "accepted" }), createRow("b", "Beta", { state: "rejected" })]),
    ];
    store.read("t1");
    await settle();
    expect(store.snapshot().announcement).toBe("Alpha: applied. Beta: rejected");
  });
});

describe("retain (MPV-FR-CZDO)", () => {
  it("MPV-FR-CZDO: the event is followed while a holder holds the store, and once only", async () => {
    const first = store.retain();
    const second = store.retain();
    await settle();
    expect(listeners.get(EVENT)?.size).toBe(1);
    first();
    first();
    expect(unlistens).toEqual([]);
    second();
    expect(unlistens).toEqual([EVENT]);
    store.retain();
    await settle();
    expect(listeners.get(EVENT)?.size).toBe(1);
  });

  it("MPV-FR-CZDO: a release before the subscription answers removes it when it answers", async () => {
    const release = store.retain();
    release();
    await settle();
    expect(unlistens).toEqual([EVENT]);
    expect(listeners.get(EVENT)?.size).toBe(0);
  });
});

describe("decisions (MPV-FR-XYDR, MPV-FR-QOPR, MPV-FR-WKVL)", () => {
  it("MPV-FR-QOPR, MPV-FR-XYDR: the row is busy while its call runs, and a second decision sends nothing", async () => {
    const p = proposal([createRow("a", "A")]);
    await held(p);
    const answer = deferred<MapPlanProposal>();
    routes.accept_map_plan_row = () => answer.promise;
    const run = store.decide("t1", "p1", "a", "accept");
    expect(store.snapshot().busy.has(rowId("p1", "a"))).toBe(true);
    expect(await store.decide("t1", "p1", "a", "accept")).toBe(false);
    answer.resolve(withRow(p, "a", { state: "accepted", result: { issueNumber: 40 } }));
    expect(await run).toBe(true);
    expect(store.snapshot().busy.size).toBe(0);
    expect(callsOf("accept_map_plan_row")).toHaveLength(1);
    expect(store.proposalOf("t1", "p1")!.rows[0].state).toBe("accepted");
    expect(visibleToasts().find((t) => t.title === "Plan change applied")?.body).toBe("Issue #40 was created.");
  });

  it("MPV-FR-WKVL: an accept that fails shows an Error toast with the error text", async () => {
    const p = proposal([modifyRow("m", 18, "M", [])]);
    await held(p);
    routes.accept_map_plan_row = async () =>
      withRow(p, "m", { state: "failed", error: { code: "changed_on_tracker", message: "Changed." } });
    await store.decide("t1", "p1", "m", "accept");
    expect(visibleToasts().find((t) => t.level === "Error")?.body).toBe("Changed.");
  });

  it("MPV-FR-WKVL: a refusal holds its text by row, reads again, and a later decision clears it", async () => {
    const p = proposal([createRow("a", "A")]);
    await held(p);
    routes.reject_map_plan_row = () => Promise.reject({ code: "row_decided" });
    expect(await store.decide("t1", "p1", "a", "reject")).toBe(false);
    expect(store.snapshot().refusals.get(rowId("p1", "a"))).toBe("row_decided");
    expect(visibleToasts().some((t) => t.level === "Error" && t.title === "Plan change not rejected")).toBe(true);
    expect(callsOf("list_map_plan_proposals")).toHaveLength(2);
    expect(store.snapshot().busy.size).toBe(0);
    routes.reject_map_plan_row = async () => withRow(p, "a", { state: "rejected" });
    await store.decide("t1", "p1", "a", "reject");
    expect(store.snapshot().refusals.size).toBe(0);
  });

  it("MPV-FR-XYDR: a decision on an unknown row sends nothing", async () => {
    await held(proposal([createRow("a", "A")]));
    expect(await store.decide("t1", "p1", "missing", "accept")).toBe(false);
    expect(callsOf("accept_map_plan_row")).toHaveLength(0);
  });

  it("MPV-FR-KMJX: no log record carries a title", async () => {
    const p = proposal([createRow("a", "Secret title")]);
    await held(p);
    routes.accept_map_plan_row = () => Promise.reject({ code: "x", message: "Secret title is bad" });
    await store.decide("t1", "p1", "a", "accept");
    const logged = JSON.stringify([vi.mocked(logInfo).mock.calls, vi.mocked(logWarn).mock.calls]);
    expect(logged).not.toContain("Secret title");
  });
});

describe("Accept all and Reject all (MPV-FR-TWSY)", () => {
  it("MPV-FR-TWSY: accepts in order, skips decided rows, and accepts a child its parent's answer unblocked", async () => {
    let p = proposal([
      createRow("parent", "Parent"),
      createRow("done", "Done", { state: "rejected" }),
      createRow("child", "Child", { blockedBy: { rowKey: "parent", reason: "waiting" } }, { parent: { row: "parent" } }),
      createRow("last", "Last"),
    ]);
    await held(p);
    routes.accept_map_plan_row = async (args) => {
      const key = args.rowKey as string;
      if (key === "last") throw { code: "not_github_source", message: "No." };
      p = withRow(p, key, { state: "accepted" });
      if (key === "parent") p = withRow(p, "child", { blockedBy: null });
      return p;
    };
    await store.decideAll("t1", "p1", "accept");
    expect(callsOf("accept_map_plan_row").map((c) => c.rowKey)).toEqual(["parent", "child", "last"]);
  });

  it("MPV-FR-TWSY: a child that stays blocked is not accepted, and a refusal does not stop the run", async () => {
    const p = proposal([
      createRow("parent", "Parent"),
      createRow("child", "Child", { blockedBy: { rowKey: "parent", reason: "waiting" } }, { parent: { row: "parent" } }),
      createRow("last", "Last"),
    ]);
    await held(p);
    routes.accept_map_plan_row = async (args) => {
      if (args.rowKey === "parent") throw { code: "not_github_source" };
      return withRow(p, args.rowKey as string, { state: "accepted" });
    };
    await store.decideAll("t1", "p1", "accept");
    expect(callsOf("accept_map_plan_row").map((c) => c.rowKey)).toEqual(["parent", "last"]);
  });

  it("MPV-FR-TWSY: Reject all rejects a blocked row too", async () => {
    let p = proposal([
      createRow("parent", "Parent"),
      createRow("child", "Child", { blockedBy: { rowKey: "parent", reason: "waiting" } }),
    ]);
    await held(p);
    routes.reject_map_plan_row = async (args) => (p = withRow(p, args.rowKey as string, { state: "rejected" }));
    await store.decideAll("t1", "p1", "reject");
    expect(callsOf("reject_map_plan_row").map((c) => c.rowKey)).toEqual(["parent", "child"]);
  });
});

describe("Accept all with a child before its parent (MPV-FR-TWSY)", () => {
  it("MPV-FR-TWSY: a child listed before its parent is accepted in a second pass, once", async () => {
    let p = proposal([
      createRow("child", "Child", { blockedBy: { rowKey: "parent", reason: "waiting" } }, { parent: { row: "parent" } }),
      createRow("parent", "Parent"),
    ]);
    await held(p);
    routes.accept_map_plan_row = async (args) => {
      const key = args.rowKey as string;
      p = withRow(p, key, { state: "accepted" });
      if (key === "parent") p = withRow(p, "child", { blockedBy: null });
      return p;
    };
    await store.decideAll("t1", "p1", "accept");
    expect(callsOf("accept_map_plan_row").map((c) => c.rowKey)).toEqual(["parent", "child"]);
  });
});

describe("re-prepare (MPV-FR-AAFF)", () => {
  it("MPV-FR-AAFF: sends one request at a time and holds the answer", async () => {
    const p = proposal([modifyRow("m", 18, "M", [], { state: "failed", error: { code: "changed_on_tracker", message: "" } })]);
    await held(p);
    const answer = deferred<MapPlanProposal>();
    routes.request_map_plan_reprepare = () => answer.promise;
    const run = store.requestReprepare("t1", "p1");
    await store.requestReprepare("t1", "p1");
    answer.resolve({ ...p, reprepareRequestedAt: "2026-01-01T00:00:00Z" });
    await run;
    expect(callsOf("request_map_plan_reprepare")).toEqual([{ discussionId: "t1", proposalId: "p1" }]);
    expect(store.proposalOf("t1", "p1")!.reprepareRequestedAt).toBe("2026-01-01T00:00:00Z");
  });

  it("MPV-FR-AAFF: a refused request holds its text and shows an Error toast", async () => {
    await held(proposal([createRow("a", "A")]));
    routes.request_map_plan_reprepare = () => Promise.reject({ code: "nothing_failed", message: "Nothing failed." });
    await store.requestReprepare("t1", "p1");
    expect(store.snapshot().refusals.get(rowId("p1", ""))).toBe("Nothing failed.");
    expect(visibleToasts().some((t) => t.title === "Re-prepare not requested")).toBe(true);
  });
});

describe("the open review (MPV-FR-CAQL, MPV-FR-PBAD)", () => {
  it("MPV-FR-CAQL: opens on the first pending row and moves only from the selected row", async () => {
    await held(
      proposal([createRow("a", "A", { state: "accepted" }), createRow("b", "B"), createRow("c", "C")]),
    );
    store.openReview("t1", "p1");
    expect(store.snapshot().review?.selectedKey).toBe("b");
    store.advanceFrom("c");
    expect(store.snapshot().review?.selectedKey).toBe("b");
    store.advanceFrom("b");
    expect(store.snapshot().review?.selectedKey).toBe("c");
  });

  it("MPV-FR-PBAD: closing gives the focus back to the registered control only when asked", async () => {
    await held(proposal([createRow("a", "A")]));
    const button = document.createElement("button");
    document.body.append(button);
    store.registerReturn("p1", button);
    store.openReview("t1", "p1");
    store.closeReview(false);
    expect(document.activeElement).not.toBe(button);
    store.openReview("t1", "p1");
    store.closeReview(true);
    expect(document.activeElement).toBe(button);
    expect(store.snapshot().review).toBeNull();
    store.closeReview(true);
    button.remove();
  });
});

describe("the board (DBR-FR-AGIN, DBR-FR-PMGS)", () => {
  it("DBR-FR-AGIN: the board reads the active thread and the reviewed proposal, once each", async () => {
    routes.list_map_plan_proposals = async (args) =>
      args.discussionId === "t1"
        ? [proposal([createRow("a", "A")])]
        : [proposal([modifyRow("m", 18, "M", [])], { id: "p2", discussionId: "t2" })];
    store.setActiveDiscussion("t1");
    store.want("t2");
    await settle();
    expect(store.boardProposals().map((p) => p.id)).toEqual(["p1"]);
    store.openReview("t2", "p2");
    expect(store.boardProposals().map((p) => p.id)).toEqual(["p1", "p2"]);
    store.openReview("t1", "p1");
    expect(store.boardProposals().map((p) => p.id)).toEqual(["p1"]);
  });

  it("DBR-FR-AGIN: no active thread reads nothing", () => {
    store.setActiveDiscussion(null);
    expect(callsOf("list_map_plan_proposals")).toHaveLength(0);
    expect(store.boardProposals()).toEqual([]);
  });

  it("DBR-FR-PMGS: the reviewed item is the issue, the ghost card, or nothing for a draft", async () => {
    await held(proposal([modifyRow("m", 18, "M", []), createRow("a", "A"), draftRow("d", "D", "")]));
    expect(store.reviewedItemId()).toBeNull();
    store.openReview("t1", "p1");
    expect(store.reviewedItemId()).toBe("issue-18");
    store.select("a");
    expect(store.reviewedItemId()).toBe(ghostItemId("p1", "a"));
    store.select("d");
    expect(store.reviewedItemId()).toBeNull();
  });
});
