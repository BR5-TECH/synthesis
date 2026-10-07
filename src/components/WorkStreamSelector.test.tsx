import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { WorkStreamSelector } from "./WorkStreamSelector";
import { STREAM_ERRORS, type WorkStreamSummary } from "../types";
import { stream, summary } from "../test/streamFixtures";

// The selector reaches the backend only through `invoke` (`../api`) and hands
// every checkout switch to `onActivate` — the shell owns that transition — so
// these tests assert what the selector ASKS FOR rather than what the shell then
// does with it.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

// WSS-FR-JBYF: the surface listens on `"work-streams-changed"` and reloads. The
// mock records the channel name with each handler, so a test fires exactly that
// one — a component listening on the wrong channel must not pass.
type EventHandler = (ev: { payload: unknown }) => void;
let handlers: [string, EventHandler][] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: EventHandler) => {
    handlers.push([name, handler]);
    return Promise.resolve(() => {
      handlers = handlers.filter(([, h]) => h !== handler);
    });
  },
  emit: () => Promise.resolve(),
}));

/** Answer `list_work_streams` with `streams` and everything else with nothing. */
function backend(
  streams: WorkStreamSummary[],
  draftNames: Record<string, string> = {},
) {
  invokeMock.mockImplementation(async (cmd: string, args?: { runId?: string }) => {
    if (cmd === "list_work_streams") return streams;
    // WSS-FR-PSXK: a busy row reads the run's record for its draft's name.
    if (cmd === "get_graduation_run") {
      const name = draftNames[args?.runId ?? ""];
      if (name === undefined) throw new Error("unknown run");
      return { id: args?.runId, input: { draftName: name } };
    }
    if (cmd === "list_worktrees_and_branches")
      return { worktrees: [], branches: [{ name: "main" }, { name: "dev" }] };
    // WSS-FR-BRMT: opening a delete reads the stream's uncommitted paths.
    if (cmd === "get_work_stream_uncommitted_paths") return [];
    return undefined;
  });
}

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function renderSelector(over: Partial<Parameters<typeof WorkStreamSelector>[0]> = {}) {
  const onActivate = vi.fn(async () => ({ ok: true as const }));
  const onRequestMergeCommit = vi.fn(async () => null as string | null);
  const onOpenChanges = vi.fn();
  const onOpenRun = vi.fn();
  const onOpen = vi.fn();
  render(
    <WorkStreamSelector
      inRepository
      activeBranch="main"
      onOpen={onOpen}
      onActivate={onActivate}
      onRequestMergeCommit={onRequestMergeCommit}
      onOpenChanges={onOpenChanges}
      onOpenRun={onOpenRun}
      {...over}
    />,
  );
  return { onActivate, onRequestMergeCommit, onOpenChanges, onOpen, onOpenRun };
}

const openMenu = async () => {
  await userEvent.click(await screen.findByTestId("stream-selector"));
  return screen.findByTestId("stream-menu");
};

beforeEach(() => {
  invokeMock.mockReset();
  handlers = [];
});
afterEach(cleanup);

describe("the resting control (WSS-FR-JVUF, WSS-FR-DGAG)", () => {
  it("WSS-FR-JVUF: nothing renders at all outside a Git repository", async () => {
    backend([summary()]);
    renderSelector({ inRepository: false });
    expect(screen.queryByTestId("stream-selector")).toBeNull();
    // Not merely hidden: the listing is not even asked for.
    expect(calls("list_work_streams")).toHaveLength(0);
  });

  it("WSS-FR-DGAG: the label names how many streams there are", async () => {
    backend([summary(), summary({ id: "w2", name: "docs" })]);
    renderSelector();
    expect(await screen.findByText("2 streams")).toBeInTheDocument();
  });

  it("WSS-FR-DGAG: one stream reads in the singular", async () => {
    backend([summary()]);
    renderSelector();
    expect(await screen.findByText("1 stream")).toBeInTheDocument();
  });

  it("WSS-FR-DGAG: the label says when a stream is busy", async () => {
    backend([summary(), summary({ id: "w2", name: "docs", busyRunId: "g7" })]);
    renderSelector();
    expect(await screen.findByText("2 streams · 1 busy")).toBeInTheDocument();
  });
});

describe("the listing (WSS-FR-SOAS, WSS-FR-XZRD, WSS-FR-JBYF)", () => {
  it("WSS-FR-SOAS: a row names the stream, its branch, its lead and its queue", async () => {
    backend([summary({}, { aheadOfBase: 4, queuedRunCount: 2 })]);
    renderSelector();
    const menu = await openMenu();
    expect(within(menu).getByText("editor work")).toBeInTheDocument();
    expect(within(menu).getByText("synthesis/stream/editor-work")).toBeInTheDocument();
    expect(
      within(menu).getByText("4 ahead of main · 0 behind · 2 queued"),
    ).toBeInTheDocument();
  });

  it("WSS-FR-XZRD: opening reports the opening, so every other overlay closes", async () => {
    backend([summary()]);
    const { onOpen } = renderSelector();
    await openMenu();
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("WSS-FR-XZRD: Escape closes the dropdown", async () => {
    backend([summary()]);
    renderSelector();
    await openMenu();
    await act(async () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    expect(screen.queryByTestId("stream-menu")).toBeNull();
  });

  it("WSS-FR-JBYF: a work-streams-changed event reloads the listing", async () => {
    backend([summary()]);
    renderSelector();
    await screen.findByText("1 stream");
    const before = calls("list_work_streams").length;

    backend([summary(), summary({ id: "w2", name: "docs" })]);
    const entry = handlers.find(([name]) => name === "work-streams-changed");
    expect(entry, "the surface listens on work-streams-changed").toBeTruthy();
    await act(async () => {
      entry![1]({ payload: { projectKey: "p" } });
    });

    await waitFor(() =>
      expect(calls("list_work_streams").length).toBeGreaterThan(before),
    );
    expect(await screen.findByText("2 streams")).toBeInTheDocument();
  });

  it("WSS-FR-SOAS: an empty project says so rather than rendering an empty region", async () => {
    backend([]);
    renderSelector();
    const menu = await openMenu();
    expect(
      within(menu).getByText(/holds no work stream yet/i),
    ).toBeInTheDocument();
  });
});

describe("a stream a run holds (WSS-FR-PSXK, WSS-FR-YCAL)", () => {
  it("WSS-FR-PSXK: a busy stream names the draft of the run and offers no action", async () => {
    backend([summary({ busyRunId: "g7" })], { g7: "Editor scroll fix" });
    renderSelector();
    const menu = await openMenu();
    expect(
      await within(menu).findByText(/Busy — “Editor scroll fix” is working in it/),
    ).toBeInTheDocument();
    expect(within(menu).queryByText(/g7/)).toBeNull();
    // The busy line REPLACES the actions, so nothing is offered that would be
    // refused rather than being offered disabled.
    expect(within(menu).queryByRole("button", { name: "Merge stream" })).toBeNull();
    expect(within(menu).queryByRole("button", { name: "Delete stream" })).toBeNull();
  });

  it("WSS-FR-PSXK: a busy stream whose draft name cannot be read names no run id", async () => {
    backend([summary({ busyRunId: "g7" })]);
    renderSelector();
    const menu = await openMenu();
    await waitFor(() => expect(calls("get_graduation_run")).toHaveLength(1));
    expect(
      within(menu).getByText(/Busy — a run is working in it/),
    ).toBeInTheDocument();
    expect(within(menu).queryByText(/g7/)).toBeNull();
  });

  it("WSS-FR-YCAL: a busy stream is not selectable, and nothing is invoked", async () => {
    backend([summary({ busyRunId: "g7" })]);
    const { onActivate } = renderSelector();
    const menu = await openMenu();
    const row = within(menu).getByRole("menuitem", {
      name: /Open the work stream editor work/,
    });
    expect(row).toBeDisabled();
    await userEvent.click(row);
    expect(onActivate).not.toHaveBeenCalled();
  });

  it("WSS-FR-YCAL: selecting a free stream opens its working copy", async () => {
    backend([summary()]);
    const { onActivate } = renderSelector();
    const menu = await openMenu();
    await userEvent.click(
      within(menu).getByRole("menuitem", { name: /Open the work stream/ }),
    );
    await waitFor(() => expect(onActivate).toHaveBeenCalledWith("/data/w/w1"));
  });

  it("WSS-FR-YCAL: a refused switch renders on the row rather than window-wide", async () => {
    backend([summary()]);
    renderSelector({
      onActivate: async () => ({ ok: false as const, error: STREAM_ERRORS.busy }),
    });
    const menu = await openMenu();
    await userEvent.click(
      within(menu).getByRole("menuitem", { name: /Open the work stream/ }),
    );
    const row = await screen.findByTestId("stream-row-w1");
    expect(
      within(row).getByRole("alert").textContent,
    ).toMatch(/A run holds this stream or waits in its queue/);
  });

  it("WSS-FR-OQYG: a missing stream says so and offers no action but removal", async () => {
    backend([summary({ isMissing: true })]);
    const { onActivate } = renderSelector();
    const menu = await openMenu();
    expect(within(menu).getByText(/Missing/)).toBeInTheDocument();
    await userEvent.click(
      within(menu).getByRole("menuitem", { name: /Open the work stream/ }),
    );
    expect(onActivate).not.toHaveBeenCalled();
  });
});

describe("creating a stream (WSS-FR-PDFX, WSS-FR-XZRO, WSS-FR-CRJD)", () => {
  it("WSS-FR-PDFX: New stream closes the dropdown and opens the dialog", async () => {
    backend([summary()]);
    renderSelector();
    await openMenu();
    await userEvent.click(screen.getByTestId("stream-new"));
    expect(screen.queryByTestId("stream-menu")).toBeNull();
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
  });

  it("WSS-FR-XZRO: exactly two inputs, and the branch is pre-filled", async () => {
    backend([summary()]);
    renderSelector();
    await openMenu();
    await userEvent.click(screen.getByTestId("stream-new"));
    const dialog = await screen.findByRole("dialog");

    expect(within(dialog).getByLabelText("Name")).toBeInTheDocument();
    const branch = within(dialog).getByLabelText("Created from");
    await waitFor(() => expect(branch).toHaveValue("main"));
    // Two and no more: no description, no path, no artifact type.
    expect(within(dialog).getAllByRole("textbox")).toHaveLength(1);
    expect(within(dialog).getAllByRole("combobox")).toHaveLength(1);
  });

  it("WSS-FR-CRJD: confirming invokes create with the name and the branch", async () => {
    backend([summary()]);
    renderSelector();
    await openMenu();
    await userEvent.click(screen.getByTestId("stream-new"));
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "docs pass");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create" }));

    await waitFor(() => expect(calls("create_work_stream")).toHaveLength(1));
    expect(calls("create_work_stream")[0][1]).toEqual({
      name: "docs pass",
      baseBranch: "main",
    });
  });

  it("WSS-FR-CRJD: an empty name is refused here, and invokes nothing", async () => {
    backend([summary()]);
    renderSelector();
    await openMenu();
    await userEvent.click(screen.getByTestId("stream-new"));
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create" }));

    expect(within(dialog).getByRole("alert").textContent).toMatch(/needs a name/i);
    expect(calls("create_work_stream")).toHaveLength(0);
  });

  it("WSS-FR-CRJD: a duplicate name renders against the field and the dialog stays open", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_work_streams") return [summary()];
      if (cmd === "list_worktrees_and_branches")
        return { worktrees: [], branches: [{ name: "main" }] };
      if (cmd === "create_work_stream") throw new Error(STREAM_ERRORS.nameTaken);
      return undefined;
    });
    renderSelector();
    await openMenu();
    await userEvent.click(screen.getByTestId("stream-new"));
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "editor work");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create" }));

    const alert = await within(dialog).findByRole("alert");
    expect(alert.textContent).toMatch(/already uses that name/i);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    // What the author typed is still there to correct.
    expect(within(dialog).getByLabelText("Name")).toHaveValue("editor work");
  });
});

describe("merging a stream (WSS-FR-YPDA, WSS-FR-VMNV, WSS-FR-OMAP)", () => {
  const openMerge = async () => {
    const menu = await openMenu();
    await userEvent.click(within(menu).getByRole("button", { name: "Merge stream" }));
    return screen.findByTestId("stream-row-w1");
  };

  it("WSS-FR-YPDA: the confirmation names the branch, the commits and both routes", async () => {
    backend([summary({}, { aheadOfBase: 4 })]);
    renderSelector();
    const row = await openMerge();
    expect(row.textContent).toMatch(/Merge 4 commits from editor work into main/);
    expect(within(row).getByRole("button", { name: "Leave uncommitted" })).toBeInTheDocument();
    expect(within(row).getByRole("button", { name: "Commit…" })).toBeInTheDocument();
  });

  it("WSS-FR-VMNV: it says Git merges first, where a conflict goes, and that no branch changes before approval", async () => {
    backend([summary({}, { aheadOfBase: 1 })]);
    renderSelector();
    const row = await openMerge();
    expect(row.textContent).toMatch(/Git merges first/);
    expect(row.textContent).toMatch(/completes at once and makes no run/);
    expect(row.textContent).toMatch(/handed to a merge run named “Merge editor work” in the Runs panel/);
    expect(row.textContent).toMatch(
      /Neither branch changes until a review of the reconciled result has judged it ready/,
    );
  });

  it("WSS-FR-YPDA: nothing is invoked until the confirmation is answered", async () => {
    backend([summary({}, { aheadOfBase: 1 })]);
    renderSelector();
    await openMerge();
    expect(calls("merge_work_stream")).toHaveLength(0);
  });

  it("WSS-FR-YPDA: the uncommitted route merges with that publication", async () => {
    backend([summary({}, { aheadOfBase: 1 })]);
    renderSelector();
    const row = await openMerge();
    await userEvent.click(within(row).getByRole("button", { name: "Leave uncommitted" }));
    await waitFor(() => expect(calls("merge_work_stream")).toHaveLength(1));
    expect(calls("merge_work_stream")[0][1]).toEqual({
      streamId: "w1",
      publication: { kind: "uncommitted" },
    });
  });

  it("WSS-FR-YPDA / CMW-FR-07: the commit route asks the shell for the message window", async () => {
    backend([summary({}, { aheadOfBase: 1 })]);
    const { onRequestMergeCommit } = renderSelector();
    const row = await openMerge();
    await userEvent.click(within(row).getByRole("button", { name: "Commit…" }));
    await waitFor(() =>
      expect(onRequestMergeCommit).toHaveBeenCalledWith("w1", "editor work"),
    );
    // The surface does not merge behind the window's back.
    expect(calls("merge_work_stream")).toHaveLength(0);
  });

  it("WSS-FR-OMAP: a dirty refusal names every path and routes to Changes", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_work_streams") return [summary({}, { aheadOfBase: 1 })];
      if (cmd === "merge_work_stream")
        throw new Error(`${STREAM_ERRORS.dirty}: a.ts, b/c.md`);
      return undefined;
    });
    const { onOpenChanges } = renderSelector();
    const row = await openMerge();
    await userEvent.click(within(row).getByRole("button", { name: "Leave uncommitted" }));

    const alert = await within(row).findByRole("alert");
    expect(alert.textContent).toMatch(/not committed/i);
    expect(within(row).getByText("a.ts")).toBeInTheDocument();
    expect(within(row).getByText("b/c.md")).toBeInTheDocument();
    // It neither commits nor discards that work here.
    await userEvent.click(within(row).getByRole("button", { name: "Open Changes" }));
    expect(onOpenChanges).toHaveBeenCalled();
    expect(calls("commit_paths")).toHaveLength(0);
  });
});

describe("deleting a stream (WSS-FR-UFZP)", () => {
  const openDelete = async () => {
    const menu = await openMenu();
    await userEvent.click(within(menu).getByRole("button", { name: "Delete stream" }));
    return screen.findByTestId("stream-row-w1");
  };

  it("WSS-FR-UFZP: nothing is removed before the confirmation is answered", async () => {
    backend([summary()]);
    renderSelector();
    await openDelete();
    expect(calls("delete_work_stream")).toHaveLength(0);
  });

  it("WSS-FR-UFZP: confirming deletes without a force the author did not ask for", async () => {
    backend([summary()]);
    renderSelector();
    const row = await openDelete();
    const confirm = within(row).getByRole("button", { name: "Delete" });
    await waitFor(() => expect(confirm).toBeEnabled());
    await userEvent.click(confirm);
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
    expect(calls("delete_work_stream")[0][1]).toEqual({
      streamId: "w1",
      force: false,
      discardUncommitted: false,
    });
  });

  it("WSS-FR-UFZP: an unmerged stream is warned about, says how many, and offers to merge", async () => {
    backend([summary({}, { aheadOfBase: 3 })]);
    renderSelector();
    const row = await openDelete();
    expect(within(row).getByRole("alert").textContent).toMatch(
      /holds 3 commits that main does not/,
    );
    expect(calls("delete_work_stream")).toHaveLength(0);

    await userEvent.click(within(row).getByRole("button", { name: "Merge instead" }));
    expect(row.textContent).toMatch(/Merge 3 commits/);
  });

  it("WSS-FR-UFZP: Delete anyway goes on to the path check and deletes with force", async () => {
    backend([summary({}, { aheadOfBase: 3 })]);
    renderSelector();
    const row = await openDelete();
    expect(within(row).getByRole("button", { name: "Delete anyway" })).toBeInTheDocument();
    await userEvent.click(within(row).getByRole("button", { name: "Delete anyway" }));
    // WSS-FR-BRMT: the uncommitted paths are still read before anything goes.
    expect(calls("delete_work_stream")).toHaveLength(0);
    await waitFor(() => expect(calls("get_work_stream_uncommitted_paths")).toHaveLength(1));
    expect(row.textContent).toMatch(/commits that main does not hold are lost/);
    const confirm = within(row).getByRole("button", { name: "Delete" });
    await waitFor(() => expect(confirm).toBeEnabled());
    await userEvent.click(confirm);
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
    expect(calls("delete_work_stream")[0][1]).toEqual({
      streamId: "w1",
      force: true,
      discardUncommitted: false,
    });
  });

  it("WSS-FR-UFZP, WSS-FR-NHCV: Delete anyway on a stream with uncommitted paths names the discard and passes both", async () => {
    backend([summary({}, { aheadOfBase: 1 })]);
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_work_streams") return [summary({}, { aheadOfBase: 1 })];
      if (cmd === "list_worktrees_and_branches")
        return { worktrees: [], branches: [{ name: "main" }] };
      if (cmd === "get_work_stream_uncommitted_paths") return ["src/a.ts"];
      return undefined;
    });
    renderSelector();
    const row = await openDelete();
    await userEvent.click(within(row).getByRole("button", { name: "Delete anyway" }));
    const confirm = await within(row).findByRole("button", { name: "Discard changes and delete" });
    await waitFor(() => expect(confirm).toBeEnabled());
    await userEvent.click(confirm);
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
    expect(calls("delete_work_stream")[0][1]).toEqual({
      streamId: "w1",
      force: true,
      discardUncommitted: true,
    });
  });

  it("WSS-FR-RWLB: each confirmation step takes focus when it opens", async () => {
    backend([summary({}, { aheadOfBase: 2 })]);
    renderSelector();
    const row = await openDelete();
    await waitFor(() =>
      expect(within(row).getByRole("button", { name: "Cancel" })).toHaveFocus(),
    );
    await userEvent.click(within(row).getByRole("button", { name: "Delete anyway" }));
    await waitFor(() =>
      expect(within(row).getByRole("button", { name: "Cancel" })).toHaveFocus(),
    );
  });

  it("WSS-FR-UFZP: Cancel on the unmerged warning removes nothing", async () => {
    backend([summary({}, { aheadOfBase: 2 })]);
    renderSelector();
    const row = await openDelete();
    await userEvent.click(within(row).getByRole("button", { name: "Cancel" }));
    expect(calls("delete_work_stream")).toHaveLength(0);
  });
});

describe("every state is carried in words (WSS-FR-RWLB)", () => {
  it("WSS-FR-RWLB: the control's accessible name carries the whole label", async () => {
    backend([summary({ busyRunId: "g7" })]);
    renderSelector();
    expect(
      await screen.findByRole("button", { name: "Work streams: 1 stream · 1 busy" }),
    ).toBeInTheDocument();
  });

  it("WSS-FR-RWLB: a typed refusal is rendered as a sentence, never as its code", async () => {
    backend([summary()]);
    renderSelector({
      onActivate: async () => ({
        ok: false as const,
        error: STREAM_ERRORS.missing,
      }),
    });
    const menu = await openMenu();
    await userEvent.click(
      within(menu).getByRole("menuitem", { name: /Open the work stream/ }),
    );
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toMatch(/working copy is gone/i);
    expect(alert.textContent).not.toMatch(/stream_missing/);
  });
});
