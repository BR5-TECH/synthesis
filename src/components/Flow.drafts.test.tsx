import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { findItem } from "../state/flowDocument";
import { resetToasts, visibleToasts } from "../state/toasts";
import type { DraftSummary } from "../types";
import {
  callsOf,
  clearFlowStores,
  flowBackend,
  itemState,
  planOf,
  renderFlow,
} from "../test/flowCanvasHarness";
import { itemEl, panel, selectItem } from "../test/flowFixtures";
import { resetListeners } from "../test/flowMocks";
import { item, plan } from "../test/flowPlanFixtures";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../test/flowMocks")).listen,
}));

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
});

afterEach(() => {
  cleanup();
  clearFlowStores();
});

const draft = (id: string, name: string): DraftSummary =>
  ({ id, name, status: "active", updatedAt: "2026-11-01T00:00:00Z" }) as DraftSummary;

/** A feature with two tasks; `t1` links `d-1` when `linked`. */
const twoTasks = (linked = false, extra: Record<string, unknown> = {}) =>
  plan([
    item("f1", "feature", { size: { width: 600, height: 300 } }),
    item("t1", "task", {
      parentId: "f1",
      name: "Tokenize",
      position: { x: 20, y: 60 },
      ...(linked ? { draftId: "d-1" } : {}),
      ...extra,
    }),
    item("t2", "task", { parentId: "f1", name: "Card form", position: { x: 220, y: 60 } }),
  ]);

describe("Create draft (FLO-FR-NRBV)", () => {
  it("FLO-FR-NRBV: Create draft invokes \"create draft (name)\" with the item's name, stores the id, and opens the draft", async () => {
    const b = flowBackend(twoTasks());
    b.on.create_draft = (args) => ({
      draft: { id: "d-9", name: String(args.name), status: "active" },
      file: "Tokenize.md",
    });
    const onOpenDraft = vi.fn();
    const { flows } = await renderFlow(b, { onOpenDraft });

    await userEvent.click(within(itemEl("t1")).getByRole("button", { name: "Create draft" }));
    await waitFor(() => expect(onOpenDraft).toHaveBeenCalledWith({ id: "d-9", name: "Tokenize" }));
    expect(callsOf(b, "create_draft")).toEqual([{ name: "Tokenize", folder: null }]);
    expect(findItem(planOf(flows), "t1")!.draftId).toBe("d-9");
  });

  it("FLO-FR-NRBV: the shell's creation route is used when the tab is given one", async () => {
    const b = flowBackend(twoTasks());
    const onCreateDraft = vi.fn(async (name: string) => ({ ok: true as const, draft: { id: "d-7", name } }));
    const onOpenDraft = vi.fn();
    const { flows } = await renderFlow(b, { onCreateDraft, onOpenDraft });

    await userEvent.click(within(itemEl("t2")).getByRole("button", { name: "Create draft" }));
    await waitFor(() => expect(onOpenDraft).toHaveBeenCalledWith({ id: "d-7", name: "Card form" }));
    expect(onCreateDraft).toHaveBeenCalledWith("Card form");
    expect(findItem(planOf(flows), "t2")!.draftId).toBe("d-7");
  });

  it("FLO-FR-NRBV: a refused creation stores nothing and shows the error on the item", async () => {
    const b = flowBackend(twoTasks());
    b.on.create_draft = () => Promise.reject({ code: "drafts_store_invalid", message: "The drafts store is not valid." });
    const onOpenDraft = vi.fn();
    const { flows } = await renderFlow(b, { onOpenDraft });

    await userEvent.click(within(itemEl("t1")).getByRole("button", { name: "Create draft" }));
    expect(await within(itemEl("t1")).findByRole("alert")).toHaveTextContent(
      "The draft was not created. The drafts store is not valid.",
    );
    expect(findItem(planOf(flows), "t1")!.draftId).toBeUndefined();
    expect(onOpenDraft).not.toHaveBeenCalled();
    expect(flows.get("plans/payments.flow")!.dirty).toBe(false);
  });

  it("FLO-FR-NRBV: a creation the shell's route refuses stores nothing and shows its error; a cancelled one shows nothing", async () => {
    const b = flowBackend(twoTasks());
    const onCreateDraft = vi
      .fn<(name: string) => Promise<{ ok: false; error: string | null }>>()
      .mockResolvedValueOnce({ ok: false, error: "The draft was not created. The name is in use." })
      .mockResolvedValueOnce({ ok: false, error: null });
    const onOpenDraft = vi.fn();
    const { flows } = await renderFlow(b, { onCreateDraft, onOpenDraft });

    await userEvent.click(within(itemEl("t1")).getByRole("button", { name: "Create draft" }));
    expect(await within(itemEl("t1")).findByRole("alert")).toHaveTextContent("The name is in use.");
    await userEvent.click(within(itemEl("t2")).getByRole("button", { name: "Create draft" }));
    await waitFor(() => expect(onCreateDraft).toHaveBeenCalledTimes(2));
    expect(within(itemEl("t2")).queryByRole("alert")).toBeNull();
    expect(findItem(planOf(flows), "t1")!.draftId).toBeUndefined();
    expect(findItem(planOf(flows), "t2")!.draftId).toBeUndefined();
    expect(onOpenDraft).not.toHaveBeenCalled();
    expect(callsOf(b, "create_draft")).toEqual([]);
  });

  it("FLO-FR-NRBV: a defect with no draft link offers Create draft, which stores the new draft on the defect", async () => {
    const b = flowBackend(
      plan([
        item("m1", "milestone"),
        item("x1", "defect", { parentId: "m1", name: "Decline message", position: { x: 20, y: 60 } }),
      ]),
    );
    b.on.create_draft = (args) => ({
      draft: { id: "d-5", name: String(args.name), status: "active" },
      file: "Decline message.md",
    });
    const onOpenDraft = vi.fn();
    const { flows } = await renderFlow(b, { onOpenDraft });

    await userEvent.click(within(itemEl("x1")).getByRole("button", { name: "Create draft" }));
    await waitFor(() => expect(onOpenDraft).toHaveBeenCalledWith({ id: "d-5", name: "Decline message" }));
    expect(callsOf(b, "create_draft")).toEqual([{ name: "Decline message", folder: null }]);
    expect(findItem(planOf(flows), "x1")!.draftId).toBe("d-5");
  });

  it("FLO-FR-NRBV: a task with a draft link offers no Create draft, and a container never does", async () => {
    const b = flowBackend(twoTasks(true));
    await renderFlow(b);

    expect(within(itemEl("t1")).queryByRole("button", { name: "Create draft" })).toBeNull();
    expect(within(itemEl("f1")).queryByRole("button", { name: "Create draft" })).toBeNull();
  });
});

describe("the draft row (FLO-FR-QZUR)", () => {
  it("FLO-FR-QZUR: a task with a draft shows the draft's name, and activating it opens the draft through \"open draft (id)\"", async () => {
    const b = flowBackend(twoTasks(true));
    b.delivery = {
      items: [itemState("t1", "drafting", { draft: { id: "d-1", name: "tokenize-cards", status: "active" } })],
    };
    b.on.open_draft = (args) => ({ id: args.id, name: "tokenize-cards" });
    await renderFlow(b);

    const row = await within(itemEl("t1")).findByRole("button", { name: "Open draft tokenize-cards" });
    await userEvent.click(row);
    await waitFor(() => expect(callsOf(b, "open_draft")).toEqual([{ id: "d-1" }]));
  });

  it("FLO-FR-QZUR: a defect with a draft renders its row; an open that is refused shows the error and changes nothing", async () => {
    const b = flowBackend(
      plan([
        item("m1", "milestone"),
        item("x1", "defect", { parentId: "m1", draftId: "d-4", position: { x: 20, y: 60 } }),
      ]),
    );
    b.delivery = {
      items: [itemState("x1", "drafting", { draft: { id: "d-4", name: "decline-message", status: "active" } })],
    };
    b.on.open_draft = () => Promise.reject({ code: "draft_not_found", message: "The draft d-4 is not in this worktree." });
    const { flows } = await renderFlow(b);

    await userEvent.click(await within(itemEl("x1")).findByRole("button", { name: "Open draft decline-message" }));
    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    expect(callsOf(b, "open_draft")).toEqual([{ id: "d-4" }]);
    expect(visibleToasts()[0]).toMatchObject({
      level: "Error",
      title: "Draft not opened",
      body: "The draft d-4 is not in this worktree.",
    });
    expect(findItem(planOf(flows), "x1")!.draftId).toBe("d-4");
    expect(flows.get("plans/payments.flow")!.dirty).toBe(false);
  });

  it("FLO-FR-QZUR: the shell's open route is used when the tab is given one", async () => {
    const b = flowBackend(twoTasks(true));
    const onOpenDraftById = vi.fn();
    await renderFlow(b, { onOpenDraftById });

    await userEvent.click(within(itemEl("t1")).getByRole("button", { name: "Open draft d-1" }));
    expect(onOpenDraftById).toHaveBeenCalledWith("d-1");
  });

  it("FLO-FR-QZUR: a draft the delivery state reports as missing shows a warning naming the id, with Unlink", async () => {
    const b = flowBackend(twoTasks(true));
    b.delivery = { items: [itemState("t1", "not_started", { draftMissing: true })] };
    const { flows } = await renderFlow(b);

    const warning = await within(itemEl("t1")).findByTestId("flow-draft-missing");
    expect(warning).toHaveTextContent("Draft d-1 is missing");
    expect(within(itemEl("t1")).queryByRole("button", { name: /^Open draft/ })).toBeNull();
    await userEvent.click(within(warning).getByRole("button", { name: "Unlink" }));
    expect(findItem(planOf(flows), "t1")!.draftId).toBeUndefined();
  });
});

describe("Link draft (FLO-FR-RMVR)", () => {
  it("FLO-FR-RMVR: Link draft lists the worktree's drafts that no other item of this Flow links, and stores the choice", async () => {
    const b = flowBackend(twoTasks(true));
    b.drafts = [draft("d-1", "Linked by t1"), draft("d-2", "Free draft"), draft("d-3", "Other free")];
    const { flows } = await renderFlow(b);

    selectItem("t2");
    const picker = within(panel()).getByRole("combobox", { name: "Link draft" });
    await waitFor(() => expect(within(picker).getAllByRole("option")).toHaveLength(3));
    expect(within(picker).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Link draft…",
      "Free draft",
      "Other free",
    ]);
    await userEvent.selectOptions(picker, "d-3");
    expect(findItem(planOf(flows), "t2")!.draftId).toBe("d-3");
  });

  it("FLO-FR-RMVR: Unlink removes the draft link and changes nothing in the draft", async () => {
    const b = flowBackend(twoTasks(true));
    const { flows } = await renderFlow(b);

    selectItem("t1");
    await userEvent.click(within(panel()).getByRole("button", { name: "Unlink draft" }));
    expect(findItem(planOf(flows), "t1")!.draftId).toBeUndefined();
    expect(b.calls.some((c) => /^(delete|rename|set)_draft/.test(c.cmd))).toBe(false);
  });

  it("FLO-FR-RMVR: a task with no draft offers no Unlink", async () => {
    const b = flowBackend(twoTasks());
    await renderFlow(b);

    selectItem("t1");
    expect(within(panel()).queryByRole("button", { name: "Unlink draft" })).toBeNull();
  });
});

describe("the footprint (FLO-FR-ZEJU)", () => {
  it("FLO-FR-ZEJU: a selected task shows its footprint; a path that names nothing on disk is marked as missing", async () => {
    const b = flowBackend(twoTasks(false, { footprint: ["src/main.ts", "src/gone.ts"] }));
    await renderFlow(b);

    selectItem("t1");
    const entries = [...panel().querySelectorAll(".flow-fp__path")];
    expect(entries.map((e) => e.textContent)).toEqual(["src/main.ts", "src/gone.ts"]);
    expect(entries[0].closest(".flow-fp")).toHaveAttribute("data-missing", "false");
    expect(entries[1].closest(".flow-fp")).toHaveAttribute("data-missing", "true");
    expect(within(entries[1].closest(".flow-fp") as HTMLElement).getByText("missing")).toBeInTheDocument();
  });

  it("FLO-FR-ZEJU: paths are added from a picker of the tree's files and folders, and removed one at a time", async () => {
    const b = flowBackend(twoTasks());
    const { flows } = await renderFlow(b);

    selectItem("t1");
    const picker = within(panel()).getByRole("combobox", { name: "Add path" });
    const paths = within(picker)
      .getAllByRole("option")
      .map((o) => (o as HTMLOptionElement).value);
    expect(paths).toContain("src/");
    expect(paths).toContain("src/main.ts");
    expect(paths).toContain("notes.txt");
    await userEvent.selectOptions(picker, "src/");
    await userEvent.selectOptions(within(panel()).getByRole("combobox", { name: "Add path" }), "notes.txt");
    expect(findItem(planOf(flows), "t1")!.footprint).toEqual(["src/", "notes.txt"]);
    await userEvent.click(within(panel()).getByRole("button", { name: "Remove path src/" }));
    expect(findItem(planOf(flows), "t1")!.footprint).toEqual(["notes.txt"]);
  });

  it("FLO-FR-ZEJU: a container shows no draft and no footprint controls", async () => {
    const b = flowBackend(twoTasks());
    await renderFlow(b);

    selectItem("f1");
    expect(within(panel()).queryByRole("combobox", { name: "Add path" })).toBeNull();
    expect(within(panel()).queryByRole("combobox", { name: "Link draft" })).toBeNull();
    expect(screen.queryByText("Footprint")).toBeNull();
  });
});
