/**
 * The GitHub plan of the Delivery lens
 * (`../../../specifications/ui/MDL-map-delivery-lens.md` MDL-FR-LZIE,
 * MDL-FR-NRJM, MDL-FR-PUCM, MDL-FR-XWGP, MDL-FR-HNLP, MDL-FR-XQGD,
 * MDL-FR-PUKL).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { githubAnswer, issueFacts } from "../../test/deliveryLensHarness";
import { item, plan } from "../../test/flowPlanFixtures";
import { renameItem } from "../flowDocument";
import { resetToasts, visibleToasts } from "../toasts";
import type { GithubDeliveryEdit, GithubDeliveryPlan } from "../../types/delivery";
import { CHANGED_ON_GITHUB, GithubPlanSession, type GithubPlanOps } from "./githubPlan";

vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

const DOC = plan([item("m1", "milestone"), item("t1", "task", { parentId: "m1" }), item("t2", "task")]);

/** A deferred answer a test settles by hand. */
function deferred<T>() {
  let ok!: (v: T) => void;
  let fail!: (e: unknown) => void;
  const promise = new Promise<T>((a, b) => {
    ok = a;
    fail = b;
  });
  return { promise, ok, fail };
}

function session(ops: Partial<GithubPlanOps> = {}) {
  const all: GithubPlanOps = {
    load: vi.fn(async () => githubAnswer(DOC)),
    apply: vi.fn(async () => githubAnswer(DOC)),
    ...ops,
  };
  return { s: new GithubPlanSession("PVT_1", all), ops: all };
}

beforeEach(() => {
  resetToasts();
});

describe("loading (MDL-FR-LZIE, MDL-FR-NRJM)", () => {
  it("MDL-FR-LZIE: the start loads with refresh false and then with refresh true, once", async () => {
    const { s, ops } = session();
    s.start();
    s.start();
    await vi.waitFor(() => expect(ops.load).toHaveBeenCalledTimes(2));
    expect(vi.mocked(ops.load).mock.calls.map((c) => c[0])).toEqual([false, true]);
    await vi.waitFor(() => expect(s.snapshot().loading).toBe(false));
    expect(s.snapshot().shown?.items.map((i) => i.id)).toEqual(["m1", "t1", "t2"]);
  });

  it("MDL-FR-NRJM: the loading state holds through both loads, so a missing cache shows no error before the refresh answers", async () => {
    const refresh = deferred<GithubDeliveryPlan>();
    const load = vi.fn((r: boolean) => (r ? refresh.promise : Promise.reject({ code: "no_cache", message: "No cache." })));
    const { s } = session({ load });
    s.start();
    await vi.waitFor(() => expect(load).toHaveBeenCalledTimes(2));
    expect(s.snapshot().loading).toBe(true);
    refresh.ok(githubAnswer(DOC));
    await vi.waitFor(() => expect(s.snapshot().loading).toBe(false));
    expect(s.snapshot().loadError).toBeNull();
    expect(s.snapshot().shown).not.toBeNull();
  });

  it("MDL-FR-NRJM: a load that refuses with no plan to show gives its error text, and Retry loads with refresh true", async () => {
    const load = vi
      .fn<GithubPlanOps["load"]>()
      .mockRejectedValueOnce({ code: "project_unavailable", message: "The Project cannot be read." })
      .mockResolvedValueOnce(githubAnswer(DOC));
    const { s } = session({ load });
    await s.load(false);
    expect(s.snapshot().loadError).toBe("The Project cannot be read.");
    await s.retry();
    expect(load).toHaveBeenLastCalledWith(true);
    expect(s.snapshot().loadError).toBeNull();
  });

  it("MDL-FR-LZIE: a newer answer replaces an older one, and an older answer that lands later changes nothing", async () => {
    const old = deferred<GithubDeliveryPlan>();
    const load = vi.fn<GithubPlanOps["load"]>().mockReturnValueOnce(old.promise).mockResolvedValueOnce(
      githubAnswer(renameItem(DOC, "t2", "Newer")),
    );
    const { s } = session({ load });
    const first = s.load(false);
    await s.load(true);
    old.ok(githubAnswer(renameItem(DOC, "t2", "Older")));
    await first;
    expect(s.snapshot().shown?.items.find((i) => i.id === "t2")?.name).toBe("Newer");
  });

  it("MDL-FR-NRJM: a failed refresh with a plan to show keeps the plan", async () => {
    const load = vi.fn<GithubPlanOps["load"]>().mockResolvedValueOnce(githubAnswer(DOC)).mockRejectedValueOnce({ code: "x" });
    const { s } = session({ load });
    await s.load(false);
    expect(await s.load(true)).toBe(false);
    expect(s.snapshot().loadError).toBeNull();
    expect(s.snapshot().shown).not.toBeNull();
  });
});

describe("edits (MDL-FR-PUCM, MDL-FR-XWGP, MDL-FR-HNLP)", () => {
  async function ready(apply: GithubPlanOps["apply"], tracker = [issueFacts("t1", 11), issueFacts("t2", 12)]) {
    const { s, ops } = session({ apply, load: vi.fn(async () => githubAnswer(DOC, { tracker })) });
    await s.load(false);
    return { s, ops };
  }

  it("MDL-FR-PUCM: an edit renders at once with a pending mark, and its answer replaces the plan", async () => {
    const answer = deferred<GithubDeliveryPlan>();
    const { s } = await ready(() => answer.promise);
    const next = renameItem(DOC, "t2", "Renamed");
    const done = s.change({ edits: [{ kind: "rename", itemId: "t2", name: "Renamed", updatedAt: "" }], next, itemIds: ["t2"] });
    expect(s.snapshot().shown).toBe(next);
    expect([...s.snapshot().pending]).toEqual(["t2"]);
    answer.ok(githubAnswer(renameItem(DOC, "t2", "From GitHub")));
    expect(await done).toMatchObject({ ok: true });
    expect(s.snapshot().pending.size).toBe(0);
    expect(s.snapshot().shown?.items.find((i) => i.id === "t2")?.name).toBe("From GitHub");
  });

  it("MDL-FR-PUCM: a refusal restores the plan of the last answer and shows the error text on the item", async () => {
    const { s } = await ready(() => Promise.reject({ code: "issue_type_unavailable", message: "The Type is not available." }));
    const answer = await s.change({
      edits: [{ kind: "rename", itemId: "t2", name: "X", updatedAt: "" }],
      next: renameItem(DOC, "t2", "X"),
      itemIds: ["t2"],
    });
    expect(answer).toEqual({ ok: false, error: "The Type is not available." });
    expect(s.snapshot().shown?.items.find((i) => i.id === "t2")?.name).toBe("T2");
    expect(s.snapshot().itemErrors).toEqual({ t2: "The Type is not available." });
    expect(s.snapshot().pending.size).toBe(0);
  });

  it("MDL-FR-PUCM: a refusal of an item that is not in the restored plan goes to an Error toast", async () => {
    const { s } = await ready(() => Promise.reject({ code: "type_unmapped", message: "No Type maps to task." }));
    const added = plan([...DOC.items, item("new1", "task")]);
    await s.change({
      edits: [{ kind: "create_item", itemKind: "task", name: "NEW1", containerId: null }],
      next: added,
      itemIds: ["new1"],
    });
    expect(visibleToasts()[0]).toMatchObject({ level: "Error", title: "Change not made", body: "No Type maps to task." });
  });

  it("MDL-FR-XWGP: edits made while one is pending go to the backend in the order the author made them, one at a time", async () => {
    const answers = [deferred<GithubDeliveryPlan>(), deferred<GithubDeliveryPlan>()];
    const sent: GithubDeliveryEdit[] = [];
    const { s } = await ready((edit) => {
      sent.push(edit);
      return answers[sent.length - 1].promise;
    });
    const a = s.change({ edits: [{ kind: "rename", itemId: "t1", name: "A", updatedAt: "" }], next: renameItem(DOC, "t1", "A"), itemIds: ["t1"] });
    const b = s.change({ edits: [{ kind: "rename", itemId: "t2", name: "B", updatedAt: "" }], next: renameItem(renameItem(DOC, "t1", "A"), "t2", "B"), itemIds: ["t2"] });
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    // B waits for the answer of A.
    expect(sent.map((e) => (e as { name: string }).name)).toEqual(["A"]);
    expect([...s.snapshot().pending].sort()).toEqual(["t1", "t2"]);
    answers[0].ok(githubAnswer(renameItem(DOC, "t1", "A")));
    await a;
    // The plan of the pending edit stays shown until it answers.
    expect(s.snapshot().shown?.items.find((i) => i.id === "t2")?.name).toBe("B");
    await vi.waitFor(() => expect(sent).toHaveLength(2));
    answers[1].ok(githubAnswer(renameItem(renameItem(DOC, "t1", "A"), "t2", "B")));
    await b;
    expect(s.snapshot().pending.size).toBe(0);
  });

  it("MDL-FR-PUCM, MDL-FR-XWGP: a refusal while a later edit waits keeps the plan of the later edit shown until it answers", async () => {
    const second = deferred<GithubDeliveryPlan>();
    const apply = vi
      .fn<GithubPlanOps["apply"]>()
      .mockRejectedValueOnce({ code: "x", message: "Refused." })
      .mockReturnValueOnce(second.promise);
    const { s } = await ready(apply);
    const later = renameItem(DOC, "t2", "Later");
    const a = s.change({ edits: [{ kind: "rename", itemId: "t1", name: "A", updatedAt: "" }], next: renameItem(DOC, "t1", "A"), itemIds: ["t1"] });
    const b = s.change({ edits: [{ kind: "rename", itemId: "t2", name: "Later", updatedAt: "" }], next: later, itemIds: ["t2"] });
    expect(await a).toMatchObject({ ok: false });
    expect(s.snapshot().shown).toBe(later);
    expect([...s.snapshot().pending]).toEqual(["t2"]);
    second.ok(githubAnswer(later));
    await b;
    expect(s.snapshot().shown?.items.find((i) => i.id === "t2")?.name).toBe("Later");
  });

  it("GDP-FR-GBDF, MDL-FR-XWGP: each edit names the updatedAt of the newest answer when it goes out", async () => {
    const sent: GithubDeliveryEdit[] = [];
    const { s } = await ready(async (edit) => {
      sent.push(edit);
      return githubAnswer(DOC, { tracker: [issueFacts("t1", 11, { updatedAt: `after-${sent.length}` })] });
    });
    const first = s.change({ edits: [{ kind: "rename", itemId: "t1", name: "A", updatedAt: "stale-at-diff" }], next: DOC, itemIds: ["t1"] });
    const second = s.change({ edits: [{ kind: "rename", itemId: "t1", name: "B", updatedAt: "stale-at-diff" }], next: DOC, itemIds: ["t1"] });
    await Promise.all([first, second]);
    expect(sent.map((e) => (e as { updatedAt: string }).updatedAt)).toEqual(["2026-11-01T00:00:01Z", "after-1"]);
  });

  it("MDL-FR-XWGP, GDP-FR-GBDF: a refused edit cancels no later edit; each goes out in order with the updatedAt of the newest answer", async () => {
    const sent: GithubDeliveryEdit[] = [];
    const { s } = await ready(async (edit) => {
      sent.push(edit);
      if (sent.length === 1) throw { code: "x", message: "Refused." };
      return githubAnswer(DOC, { tracker: [issueFacts("t1", 11, { updatedAt: `after-${sent.length}` })] });
    });
    const edit = (name: string): GithubDeliveryEdit => ({ kind: "rename", itemId: "t1", name, updatedAt: "stale-at-diff" });
    const a = s.change({ edits: [edit("A")], next: DOC, itemIds: ["t1"] });
    const b = s.change({ edits: [edit("B")], next: DOC, itemIds: ["t1"] });
    const c = s.change({ edits: [edit("C")], next: DOC, itemIds: ["t1"] });
    const answers = await Promise.all([a, b, c]);
    expect(answers.map((x) => x.ok)).toEqual([false, true, true]);
    expect(sent.map((e) => (e as { name: string }).name)).toEqual(["A", "B", "C"]);
    // The refusal gave no answer, so B names the updatedAt of the load.
    expect(sent.map((e) => (e as { updatedAt: string }).updatedAt)).toEqual([
      "2026-11-01T00:00:01Z",
      "2026-11-01T00:00:01Z",
      "after-2",
    ]);
    expect(s.snapshot().pending.size).toBe(0);
  });

  it("GDP-FR-GBDF: an edge edit names the updatedAt of its blocked end", async () => {
    const sent: GithubDeliveryEdit[] = [];
    const { s } = await ready(async (edit) => {
      sent.push(edit);
      return githubAnswer(DOC);
    });
    await s.change({ edits: [{ kind: "add_edge", from: "t1", to: "t2", type: "blocks", updatedAt: "" }], next: DOC, itemIds: ["t1", "t2"] });
    expect((sent[0] as { updatedAt: string }).updatedAt).toBe("2026-11-01T00:00:02Z");
  });

  it("MDL-FR-HNLP: a refusal with changed_on_tracker states that the item changed on GitHub, loads with refresh true, and does not send the edit again", async () => {
    const apply = vi.fn<GithubPlanOps["apply"]>(() => Promise.reject({ code: "changed_on_tracker", message: "x" }));
    const { s, ops } = await ready(apply);
    vi.mocked(ops.load).mockClear();
    await s.change({ edits: [{ kind: "rename", itemId: "t2", name: "X", updatedAt: "" }], next: DOC, itemIds: ["t2"] });
    expect(s.snapshot().itemErrors.t2).toBe(CHANGED_ON_GITHUB);
    await vi.waitFor(() => expect(ops.load).toHaveBeenCalledWith(true));
    expect(apply).toHaveBeenCalledTimes(1);
  });

  it("MDL-FR-LZIE, MDL-FR-PUCM: a load asked for while an edit is on its way goes out after the edit answers, so the plan from before the edit never replaces the answer of the edit", async () => {
    const edited = deferred<GithubDeliveryPlan>();
    const reload = deferred<GithubDeliveryPlan>();
    const { s, ops } = await ready(() => edited.promise);
    vi.mocked(ops.load).mockClear();
    vi.mocked(ops.load).mockReturnValueOnce(reload.promise);
    const next = renameItem(DOC, "t2", "Edited");
    const done = s.change({ edits: [{ kind: "rename", itemId: "t2", name: "Edited", updatedAt: "" }], next, itemIds: ["t2"] });
    const loaded = s.load(false);
    await Promise.resolve();
    await Promise.resolve();
    expect(ops.load).not.toHaveBeenCalled();
    edited.ok(githubAnswer(next));
    expect(await done).toMatchObject({ ok: true });
    await vi.waitFor(() => expect(ops.load).toHaveBeenCalledTimes(1));
    expect(s.snapshot().shown?.items.find((i) => i.id === "t2")?.name).toBe("Edited");
    reload.ok(githubAnswer(next));
    expect(await loaded).toBe(true);
    expect(s.snapshot().shown?.items.find((i) => i.id === "t2")?.name).toBe("Edited");
    expect(s.snapshot().loading).toBe(false);
  });

  it("MDL-FR-LZIE, MDL-FR-PUCM: a load that went out before an edit and answers after it changes nothing", async () => {
    const edited = deferred<GithubDeliveryPlan>();
    const early = deferred<GithubDeliveryPlan>();
    const { s, ops } = await ready(() => edited.promise);
    vi.mocked(ops.load).mockReturnValueOnce(early.promise);
    const loaded = s.load(true);
    await vi.waitFor(() => expect(ops.load).toHaveBeenCalledTimes(2));
    const next = renameItem(DOC, "t2", "Edited");
    const done = s.change({ edits: [{ kind: "rename", itemId: "t2", name: "Edited", updatedAt: "" }], next, itemIds: ["t2"] });
    edited.ok(githubAnswer(next));
    await done;
    early.ok(githubAnswer(DOC));
    await loaded;
    expect(s.snapshot().shown?.items.find((i) => i.id === "t2")?.name).toBe("Edited");
  });

  it("MDL-FR-PUCM: a later edit of an item clears the error it showed", async () => {
    const apply = vi
      .fn<GithubPlanOps["apply"]>()
      .mockRejectedValueOnce({ code: "x", message: "Refused." })
      .mockResolvedValueOnce(githubAnswer(DOC));
    const { s } = await ready(apply);
    await s.change({ edits: [{ kind: "rename", itemId: "t2", name: "X", updatedAt: "" }], next: DOC, itemIds: ["t2"] });
    expect(s.snapshot().itemErrors.t2).toBe("Refused.");
    const again = s.change({ edits: [{ kind: "rename", itemId: "t2", name: "Y", updatedAt: "" }], next: DOC, itemIds: ["t2"] });
    expect(s.snapshot().itemErrors.t2).toBeUndefined();
    await again;
  });
});

describe("removal and existing issues (MDL-FR-XQGD, MDL-FR-PUKL)", () => {
  it("MDL-FR-XQGD: a removal sends remove_item with the mode, and renders without the item and what is under it", async () => {
    const answer = deferred<GithubDeliveryPlan>();
    const apply = vi.fn<GithubPlanOps["apply"]>(() => answer.promise);
    const { s } = session({ apply, load: vi.fn(async () => githubAnswer(DOC, { tracker: [issueFacts("m1", 3)] })) });
    await s.load(false);
    const done = s.remove("m1", "close_not_planned");
    expect(s.snapshot().shown?.items.map((i) => i.id)).toEqual(["t2"]);
    await vi.waitFor(() => expect(apply).toHaveBeenCalledTimes(1));
    expect(apply).toHaveBeenCalledWith({ kind: "remove_item", itemId: "m1", mode: "close_not_planned", updatedAt: "2026-11-01T00:00:03Z" });
    answer.ok(githubAnswer(plan([item("t2", "task")])));
    await done;
  });

  it("MDL-FR-PUKL: Add existing issue sends add_existing_issue, and a refusal comes back to the caller and not onto an item", async () => {
    const apply = vi.fn<GithubPlanOps["apply"]>(() => Promise.reject({ code: "already_in_plan", message: "Issue #9 is in the plan." }));
    const { s } = session({ apply });
    await s.load(false);
    const answer = await s.addExistingIssue(9, "m1");
    expect(apply).toHaveBeenCalledWith({ kind: "add_existing_issue", number: 9, containerId: "m1" });
    expect(answer).toEqual({ ok: false, error: "Issue #9 is in the plan." });
    expect(s.snapshot().itemErrors).toEqual({});
    expect(visibleToasts()).toHaveLength(0);
  });

  it("MDL-FR-PUKL: nothing is sent before the plan has loaded", () => {
    const { s, ops } = session();
    expect(s.addExistingIssue(9, null)).toBeNull();
    expect(s.remove("t1", "remove_from_board")).toBeNull();
    expect(ops.apply).not.toHaveBeenCalled();
  });
});
