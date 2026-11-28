import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { FLUSH_DELAY_MS, resetLogBufferForTest } from "../logging";
import { findItem } from "../state/flowDocument";
import { resetToasts, visibleToasts } from "../state/toasts";
import type { DraftSummary } from "../types";
import {
  advance,
  callsOf,
  clearFlowStores,
  endFakeTimers,
  flowBackend,
  itemState,
  planOf,
  renderFlow,
} from "../test/flowCanvasHarness";
import { itemEl, panel, selectItem } from "../test/flowFixtures";
import { emitEvent, resetListeners } from "../test/flowMocks";
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

afterEach(async () => {
  await endFakeTimers();
  cleanup();
  clearFlowStores();
});

/** The log records the frontend handed to the backend, as one text. */
const loggedText = (b: ReturnType<typeof flowBackend>) =>
  JSON.stringify(callsOf(b, "append_log_records"));

const draft = (id: string, name: string): DraftSummary =>
  ({ id, name, status: "active", updatedAt: "2026-11-01T00:00:00Z" }) as DraftSummary;

/** A feature with two tasks; `t1` links `d-1` when `linked`. */
const twoTasks = (linked = false, extra: Record<string, unknown> = {}) =>
  plan([
    item("f1", "feature", {}),
    item("t1", "task", {
      parentId: "f1",
      name: "Tokenize",
      ...(linked ? { draftId: "d-1" } : {}),
      ...extra,
    }),
    item("t2", "task", { parentId: "f1", name: "Card form" }),
  ]);

/** FLO-FR-NRBV: select the item, then take the Create draft action of its panel. */
const createDraftOf = (id: string) => {
  selectItem(id);
  return within(panel()).getByRole("button", { name: "Create change" });
};

describe("Create draft (FLO-FR-NRBV)", () => {
  it("FLO-FR-NRBV: Create draft invokes \"create draft (name)\" with the item's name, stores the id, and opens the draft", async () => {
    const b = flowBackend(twoTasks());
    b.on.create_draft = (args) => ({
      draft: { id: "d-9", name: String(args.name), status: "active" },
      file: "Tokenize.md",
    });
    const onOpenDraft = vi.fn();
    const { flows } = await renderFlow(b, { onOpenDraft });

    await userEvent.click(createDraftOf("t1"));
    await waitFor(() => expect(onOpenDraft).toHaveBeenCalledWith({ id: "d-9", name: "Tokenize" }));
    expect(callsOf(b, "create_draft")).toEqual([{ name: "Tokenize", folder: null }]);
    expect(findItem(planOf(flows), "t1")!.draftId).toBe("d-9");
  });

  it("FLO-FR-NRBV: the shell's creation route is used when the tab is given one", async () => {
    const b = flowBackend(twoTasks());
    const onCreateDraft = vi.fn(async (name: string) => ({ ok: true as const, draft: { id: "d-7", name } }));
    const onOpenDraft = vi.fn();
    const { flows } = await renderFlow(b, { onCreateDraft, onOpenDraft });

    await userEvent.click(createDraftOf("t2"));
    await waitFor(() => expect(onOpenDraft).toHaveBeenCalledWith({ id: "d-7", name: "Card form" }));
    expect(onCreateDraft).toHaveBeenCalledWith("Card form");
    expect(findItem(planOf(flows), "t2")!.draftId).toBe("d-7");
  });

  it("FLO-FR-NRBV: a refused creation stores nothing and shows the error on the item", async () => {
    const b = flowBackend(twoTasks());
    b.on.create_draft = () => Promise.reject({ code: "drafts_store_invalid", message: "The drafts store is not valid." });
    const onOpenDraft = vi.fn();
    const { flows } = await renderFlow(b, { onOpenDraft });

    await userEvent.click(createDraftOf("t1"));
    expect(await within(itemEl("t1")).findByRole("alert")).toHaveTextContent(
      "The change was not created. The drafts store is not valid.",
    );
    expect(findItem(planOf(flows), "t1")!.draftId).toBeUndefined();
    expect(onOpenDraft).not.toHaveBeenCalled();
    expect(flows.get("plans/payments.flow")!.dirty).toBe(false);
  });

  it("FLO-FR-NRBV: a creation the shell's route refuses stores nothing and shows its error; a cancelled one shows nothing", async () => {
    const b = flowBackend(twoTasks());
    const onCreateDraft = vi
      .fn<(name: string) => Promise<{ ok: false; error: string | null }>>()
      .mockResolvedValueOnce({ ok: false, error: "The name is in use." })
      .mockResolvedValueOnce({ ok: false, error: null });
    const onOpenDraft = vi.fn();
    const { flows } = await renderFlow(b, { onCreateDraft, onOpenDraft });

    await userEvent.click(createDraftOf("t1"));
    expect(await within(itemEl("t1")).findByRole("alert")).toHaveTextContent("The name is in use.");
    await userEvent.click(createDraftOf("t2"));
    await waitFor(() => expect(onCreateDraft).toHaveBeenCalledTimes(2));
    expect(within(itemEl("t2")).queryByRole("alert")).toBeNull();
    expect(findItem(planOf(flows), "t1")!.draftId).toBeUndefined();
    expect(findItem(planOf(flows), "t2")!.draftId).toBeUndefined();
    expect(onOpenDraft).not.toHaveBeenCalled();
    expect(callsOf(b, "create_draft")).toEqual([]);
  });

  it("FLO-FR-NRBV: a double press makes one \"create draft\" call", async () => {
    const b = flowBackend(twoTasks());
    let answer!: (v: unknown) => void;
    b.on.create_draft = () => new Promise((res) => (answer = res));
    const onOpenDraft = vi.fn();
    const { flows } = await renderFlow(b, { onOpenDraft });

    const button = createDraftOf("t1");
    fireEvent.click(button);
    fireEvent.click(button);
    fireEvent.click(within(panel()).getByRole("button", { name: "Creating change…" }));
    await waitFor(() => expect(callsOf(b, "create_draft")).toHaveLength(1));
    await act(async () =>
      answer({ draft: { id: "d-9", name: "Tokenize", status: "active" }, file: "Tokenize.md" }),
    );
    await waitFor(() => expect(onOpenDraft).toHaveBeenCalledTimes(1));
    expect(callsOf(b, "create_draft")).toHaveLength(1);
    expect(findItem(planOf(flows), "t1")!.draftId).toBe("d-9");
  });

  it("FLO-FR-NRBV: a defect with no draft link offers Create draft, which stores the new draft on the defect", async () => {
    const b = flowBackend(
      plan([
        item("m1", "milestone"),
        item("x1", "defect", { parentId: "m1", name: "Decline message" }),
      ]),
    );
    b.on.create_draft = (args) => ({
      draft: { id: "d-5", name: String(args.name), status: "active" },
      file: "Decline message.md",
    });
    const onOpenDraft = vi.fn();
    const { flows } = await renderFlow(b, { onOpenDraft });

    await userEvent.click(createDraftOf("x1"));
    await waitFor(() => expect(onOpenDraft).toHaveBeenCalledWith({ id: "d-5", name: "Decline message" }));
    expect(callsOf(b, "create_draft")).toEqual([{ name: "Decline message", folder: null }]);
    expect(findItem(planOf(flows), "x1")!.draftId).toBe("d-5");
  });

  it("FLO-FR-NRBV: a task with a draft link offers no Create draft, and a container never does", async () => {
    const b = flowBackend(twoTasks(true));
    await renderFlow(b);

    selectItem("t1");
    expect(within(panel()).queryByRole("button", { name: "Create change" })).toBeNull();
    selectItem("f1");
    expect(within(panel()).queryByRole("button", { name: "Create change" })).toBeNull();
  });

  it("FLO-FR-CMPK, FLO-FR-NRBV: a card never shows a draft action, also before or after a selection", async () => {
    const b = flowBackend(twoTasks());
    await renderFlow(b);

    expect(within(itemEl("t2")).queryByRole("button", { name: "Create change" })).toBeNull();
    selectItem("t2");
    expect(within(itemEl("t2")).queryByRole("button", { name: "Create change" })).toBeNull();
    expect(within(panel()).getByRole("button", { name: "Create change" })).toBeEnabled();
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

    const row = await within(itemEl("t1")).findByRole("button", { name: "Open change tokenize-cards" });
    await userEvent.click(row);
    await waitFor(() => expect(callsOf(b, "open_draft")).toEqual([{ id: "d-1" }]));
  });

  it("FLO-FR-QZUR: a defect with a draft renders its row; an open that is refused shows the error and changes nothing", async () => {
    const b = flowBackend(
      plan([
        item("m1", "milestone"),
        item("x1", "defect", { parentId: "m1", draftId: "d-4" }),
      ]),
    );
    b.delivery = {
      items: [itemState("x1", "drafting", { draft: { id: "d-4", name: "decline-message", status: "active" } })],
    };
    b.on.open_draft = () => Promise.reject({ code: "draft_not_found", message: "The draft d-4 is not in this worktree." });
    const { flows } = await renderFlow(b);

    await userEvent.click(await within(itemEl("x1")).findByRole("button", { name: "Open change decline-message" }));
    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    expect(callsOf(b, "open_draft")).toEqual([{ id: "d-4" }]);
    expect(visibleToasts()[0]).toMatchObject({
      level: "Error",
      title: "Change not opened",
      body: "The draft d-4 is not in this worktree.",
    });
    expect(findItem(planOf(flows), "x1")!.draftId).toBe("d-4");
    expect(flows.get("plans/payments.flow")!.dirty).toBe(false);
  });

  it("FLO-FR-QZUR: the shell's open route is used when the tab is given one", async () => {
    const b = flowBackend(twoTasks(true));
    const onOpenDraftById = vi.fn();
    await renderFlow(b, { onOpenDraftById });

    await userEvent.click(within(itemEl("t1")).getByRole("button", { name: "Open change d-1" }));
    expect(onOpenDraftById).toHaveBeenCalledWith("d-1");
  });

  it("FLO-FR-QZUR: a draft the delivery state reports as missing shows a warning naming the id, with Unlink", async () => {
    const b = flowBackend(twoTasks(true));
    b.delivery = { items: [itemState("t1", "not_started", { draftMissing: true })] };
    const { flows } = await renderFlow(b);

    const warning = await within(itemEl("t1")).findByTestId("flow-draft-missing");
    expect(warning).toHaveTextContent("Change d-1 is missing");
    expect(within(itemEl("t1")).queryByRole("button", { name: /^Open change/ })).toBeNull();
    await userEvent.click(within(warning).getByRole("button", { name: "Unlink" }));
    expect(findItem(planOf(flows), "t1")!.draftId).toBeUndefined();
  });

  it("FLO-FR-QZUR, PVC-FR-HMJD: the panel of an item with a missing linked change names a change, not a draft", async () => {
    const b = flowBackend(twoTasks(true));
    b.delivery = { items: [itemState("t1", "not_started", { draftMissing: true })] };
    await renderFlow(b);

    selectItem("t1");
    expect(await within(panel()).findByText("The linked change d-1 is missing.")).toBeInTheDocument();
    expect(within(panel()).queryByText(/linked draft/)).toBeNull();
  });
});

describe("Link draft (FLO-FR-RMVR)", () => {
  it("FLO-FR-RMVR: Link draft lists the worktree's drafts that no other item of this Flow links, and stores the choice", async () => {
    const b = flowBackend(twoTasks(true));
    b.drafts = [draft("d-1", "Linked by t1"), draft("d-2", "Free draft"), draft("d-3", "Other free")];
    const { flows } = await renderFlow(b);

    selectItem("t2");
    const picker = within(panel()).getByRole("combobox", { name: "Link change" });
    await waitFor(() => expect(within(picker).getAllByRole("option")).toHaveLength(3));
    expect(within(picker).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Link change…",
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
    await userEvent.click(within(panel()).getByRole("button", { name: "Unlink change" }));
    expect(findItem(planOf(flows), "t1")!.draftId).toBeUndefined();
    expect(b.calls.some((c) => /^(delete|rename|set)_draft/.test(c.cmd))).toBe(false);
  });

  it("FLO-FR-RMVR: a drafts list that cannot be read offers the placeholder alone, and the failure is logged by its code", async () => {
    const b = flowBackend(twoTasks());
    b.on.list_drafts = () =>
      Promise.reject({ code: "drafts_store_invalid", message: "The store at /home/me is not valid." });
    await renderFlow(b);
    vi.useFakeTimers();

    selectItem("t2");
    await advance(0);
    const picker = within(panel()).getByRole("combobox", { name: "Link change" });
    expect(within(picker).getAllByRole("option").map((o) => o.textContent)).toEqual(["Link change…"]);
    await advance(FLUSH_DELAY_MS);
    expect(loggedText(b)).toContain("drafts not listed for the Flow");
    expect(loggedText(b)).toContain("drafts_store_invalid");
    expect(loggedText(b)).not.toContain("/home/me");
    expect(panel()).toBeInTheDocument();
  });

  it("FLO-FR-RMVR: a task with no draft offers no Unlink", async () => {
    const b = flowBackend(twoTasks());
    await renderFlow(b);

    selectItem("t1");
    expect(within(panel()).queryByRole("button", { name: "Unlink change" })).toBeNull();
  });
});

describe("the footprint (FLO-FR-ZEJU)", () => {
  it("FLO-FR-ZEJU: a selected task shows its footprint; a path that names nothing on disk is marked as missing", async () => {
    const b = flowBackend(
      twoTasks(false, { footprint: ["src/main.ts", "src/gone.ts", "src/", "gone/"] }),
    );
    await renderFlow(b);

    selectItem("t1");
    const entries = [...panel().querySelectorAll(".flow-fp__path")];
    expect(entries.map((e) => e.textContent)).toEqual(["src/main.ts", "src/gone.ts", "src/", "gone/"]);
    const row = (i: number) => entries[i].closest(".flow-fp") as HTMLElement;
    await waitFor(() => expect(row(1)).toHaveAttribute("data-missing", "true"));
    expect(row(0)).toHaveAttribute("data-missing", "false");
    expect(within(row(1)).getByText("missing")).toBeInTheDocument();
    // A folder entry keeps its trailing `/`, and is looked up without it.
    expect(row(2)).toHaveAttribute("data-missing", "false");
    expect(row(3)).toHaveAttribute("data-missing", "true");
    // ASC-FR-QKXR: one question to the project index, and no folder walk.
    expect(callsOf(b, "existing_project_paths")).toEqual([
      { paths: ["src/main.ts", "src/gone.ts", "src", "gone"] },
    ]);
    expect(callsOf(b, "list_tree_children")).toEqual([]);
  });

  it("FLO-FR-ZEJU: an entry that appears on disk stops being missing on a tree change", async () => {
    const b = flowBackend(twoTasks(false, { footprint: ["src/new.ts"] }));
    let held: string[] = [];
    b.on.existing_project_paths = () => Promise.resolve(held);
    await renderFlow(b);

    selectItem("t1");
    const row = () => panel().querySelector(".flow-fp") as HTMLElement;
    await waitFor(() => expect(row()).toHaveAttribute("data-missing", "true"));

    held = ["src/new.ts"];
    act(() => emitEvent("project-tree-changed", { changeCount: 1, changedFolders: ["src"] }));
    await waitFor(() => expect(row()).toHaveAttribute("data-missing", "false"));
  });

  it("FLO-FR-ZEJU: when the index cannot answer, no footprint entry is marked missing", async () => {
    const b = flowBackend(twoTasks(false, { footprint: ["src/main.ts", "src/gone.ts"] }));
    b.on.existing_project_paths = () => Promise.reject("index_not_ready");
    await renderFlow(b);

    selectItem("t1");
    await waitFor(() => expect(callsOf(b, "existing_project_paths")).toHaveLength(1));
    await act(async () => {});
    for (const entry of panel().querySelectorAll(".flow-fp")) {
      expect(entry).toHaveAttribute("data-missing", "false");
    }
  });

  it("FLO-FR-ZEJU: the picker searches the project paths after a rest in typing, files and folders, a folder with a trailing slash", async () => {
    const b = flowBackend(twoTasks());
    const { flows } = await renderFlow(b);
    vi.useFakeTimers();

    selectItem("t1");
    const input = () => within(panel()).getByRole("combobox", { name: "Add path" });
    const offered = () =>
      [...panel().querySelectorAll("datalist option")].map((o) => (o as HTMLOptionElement).value);

    // Nothing is searched until the rest in typing has passed.
    expect(callsOf(b, "search_project_paths")).toEqual([]);
    await advance(150);
    expect(callsOf(b, "search_project_paths")).toEqual([
      { text: "", kind: "file", limit: 50 },
      { text: "", kind: "folder", limit: 50 },
    ]);
    expect(offered()).toContain("src/");
    expect(offered()).toContain("src/main.ts");
    expect(offered()).toContain("notes.txt");
    expect(offered()).not.toContain("src");

    // Typed text searches again, once, for that text.
    fireEvent.change(input(), { target: { value: "s" } });
    fireEvent.change(input(), { target: { value: "sr" } });
    await advance(100);
    expect(callsOf(b, "search_project_paths")).toHaveLength(2);
    fireEvent.change(input(), { target: { value: "src" } });
    await advance(150);
    expect(callsOf(b, "search_project_paths").slice(2)).toEqual([
      { text: "src", kind: "file", limit: 50 },
      { text: "src", kind: "folder", limit: 50 },
    ]);
    expect(offered()).toEqual(["src/", "src/main.ts"]);

    // A choice from the list is added at once, and leaves the list.
    fireEvent.change(input(), { target: { value: "src/" } });
    expect(findItem(planOf(flows), "t1")!.footprint).toEqual(["src/"]);
    expect(input()).toHaveValue("");
    expect(offered()).not.toContain("src/");
  });

  it("FLO-FR-ZEJU: typed text is added on Enter, and paths are removed one at a time", async () => {
    const b = flowBackend(twoTasks());
    const { flows } = await renderFlow(b);

    selectItem("t1");
    const input = () => within(panel()).getByRole("combobox", { name: "Add path" });
    fireEvent.change(input(), { target: { value: "src/" } });
    fireEvent.keyDown(input(), { key: "Enter" });
    fireEvent.change(input(), { target: { value: "docs/later.md" } });
    fireEvent.keyDown(input(), { key: "Enter" });
    expect(findItem(planOf(flows), "t1")!.footprint).toEqual(["src/", "docs/later.md"]);

    // An entry already in the footprint is not added twice.
    fireEvent.change(input(), { target: { value: "src/" } });
    fireEvent.keyDown(input(), { key: "Enter" });
    expect(findItem(planOf(flows), "t1")!.footprint).toEqual(["src/", "docs/later.md"]);

    await userEvent.click(within(panel()).getByRole("button", { name: "Remove path src/" }));
    expect(findItem(planOf(flows), "t1")!.footprint).toEqual(["docs/later.md"]);
  });

  it("FLO-FR-ZEJU: while the project tree is not read, no footprint entry is marked missing", async () => {
    const b = flowBackend(twoTasks(false, { footprint: ["src/main.ts", "src/gone.ts"] }));
    b.on.list_project_artifacts = () => new Promise(() => {});
    await renderFlow(b);

    selectItem("t1");
    const entries = [...panel().querySelectorAll(".flow-fp")];
    expect(entries).toHaveLength(2);
    for (const entry of entries) expect(entry).toHaveAttribute("data-missing", "false");
    expect(within(panel()).queryByText("missing")).toBeNull();
  });

  it("FLO-FR-ZEJU: a container shows no draft and no footprint controls", async () => {
    const b = flowBackend(twoTasks());
    await renderFlow(b);

    selectItem("f1");
    expect(within(panel()).queryByRole("combobox", { name: "Add path" })).toBeNull();
    expect(within(panel()).queryByRole("combobox", { name: "Link change" })).toBeNull();
    expect(screen.queryByText("Footprint")).toBeNull();
  });
});
