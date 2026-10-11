/**
 * A temporary stream in the selector: the marker on its row, the merge prompt,
 * the default merge message and the automatic deletion
 * (`../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-DNQM,
 * WSS-FR-PRVK, WSS-FR-GBTD, WSS-FR-YHCT, WSS-FR-LWZB, WSS-FR-NXCH,
 * WSS-FR-GQXN, WSS-FR-KFSA, WSS-FR-NZRB, WSS-FR-TCSB).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { WorkStreamSelector } from "./WorkStreamSelector";
import type { GraduationRun, GraduationRunState, WorkStreamSummary } from "../types";
import { makeQueue, makeRun } from "../test/graduationFixtures";
import { conflictedResult, mergedResult, mergeRunLink, summary } from "../test/streamFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

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

let listing: WorkStreamSummary[] = [];
let runs: GraduationRun[] = [];
let mergeAnswer: unknown = { kind: "nothing_to_merge" };
let deleteAnswer: () => Promise<unknown> = () => Promise.resolve(undefined);

/** A run of the temporary stream `w1`, with the draft name the test reads it by. */
function runOf(id: string, state: GraduationRunState, draftName = `Draft ${id}`) {
  const base = makeRun(id, state, { streamId: "w1" });
  return { ...base, input: { ...base.input, draftName } };
}

const temporary = (counts: Parameters<typeof summary>[1] = {}) =>
  summary({ temporary: true }, { aheadOfBase: 2, ...counts });

function backend() {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "list_work_streams") return listing;
    if (cmd === "list_graduation_queue") return makeQueue(runs);
    if (cmd === "merge_work_stream") return mergeAnswer;
    if (cmd === "delete_work_stream") return deleteAnswer();
    if (cmd === "get_work_stream_uncommitted_paths") return [];
    if (cmd === "append_log_records") return undefined;
    return undefined;
  });
}

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

/** Fire every handler of an event, as the backend does when something changed. */
async function emit(name: string) {
  await act(async () => {
    for (const [event, handler] of handlers) {
      if (event === name) handler({ payload: { projectKey: "p" } });
    }
  });
}

/** The backend moved: the listing and the runs are what the test says now. */
async function move(next: WorkStreamSummary[], nextRuns: GraduationRun[]) {
  listing = next;
  runs = nextRuns;
  await emit("work-streams-changed");
  await emit("graduation-queue-changed");
}

function renderSelector(onRequestMergeCommit = vi.fn(async () => null as string | null)) {
  render(
    <WorkStreamSelector
      inRepository
      activeBranch="main"
      onOpen={vi.fn()}
      onActivate={vi.fn(async () => ({ ok: true as const }))}
      onRequestMergeCommit={onRequestMergeCommit}
      onOpenChanges={vi.fn()}
      onOpenRun={vi.fn()}
    />,
  );
  return { onRequestMergeCommit };
}

const prompt = () => screen.queryByTestId("merge-prompt");

beforeEach(() => {
  invokeMock.mockReset();
  handlers = [];
  listing = [];
  runs = [];
  mergeAnswer = { kind: "nothing_to_merge" };
  deleteAnswer = () => Promise.resolve(undefined);
  backend();
});
afterEach(cleanup);

/** A temporary stream that is seen working, then finished, so the prompt is due. */
async function finishWork() {
  listing = [temporary({ queuedRunCount: 1 })];
  runs = [runOf("a", "working")];
  renderSelector();
  await screen.findByTestId("stream-selector");
  await waitFor(() => expect(calls("list_graduation_queue").length).toBeGreaterThan(0));
  await move([temporary()], [runOf("a", "completed")]);
}

describe("the row of a temporary stream", () => {
  it("WSS-FR-DNQM: the row says in words that the stream is temporary", async () => {
    listing = [temporary()];
    renderSelector();
    await userEvent.click(await screen.findByTestId("stream-selector"));
    const marker = await screen.findByTestId("stream-temporary-w1");
    expect(marker).toHaveTextContent(/temporary stream/i);
    expect(marker).toHaveTextContent(/deleted when its work is done/i);
  });

  it("WSS-FR-DNQM: a stream that is not temporary has no marker", async () => {
    listing = [summary({}, { aheadOfBase: 1 })];
    renderSelector();
    await userEvent.click(await screen.findByTestId("stream-selector"));
    await screen.findByTestId("stream-row-w1");
    expect(screen.queryByTestId("stream-temporary-w1")).toBeNull();
  });
});

describe("the merge prompt", () => {
  it("WSS-FR-PRVK, WSS-FR-LWZB: work seen and then done asks for the merge, and Yes opens the merge confirmation", async () => {
    await finishWork();
    const asked = await screen.findByTestId("merge-prompt");
    expect(asked).toHaveTextContent("editor work");
    expect(asked).toHaveTextContent("main");
    await userEvent.click(within(asked).getByRole("button", { name: "Yes" }));
    expect(prompt()).toBeNull();
    const row = await screen.findByTestId("stream-row-w1");
    expect(within(row).getByRole("button", { name: "Leave uncommitted" })).toBeInTheDocument();
  });

  it("WSS-FR-PRVK: a stream that stands finished at launch gets no prompt", async () => {
    listing = [temporary()];
    runs = [runOf("a", "completed")];
    renderSelector();
    await waitFor(() => expect(calls("list_graduation_queue").length).toBeGreaterThan(0));
    await move(listing, runs);
    expect(prompt()).toBeNull();
  });

  it.each(["failed", "awaiting_author"] as const)(
    "WSS-FR-GBTD: a %s run keeps the prompt away",
    async (state) => {
      listing = [temporary({ queuedRunCount: 1 })];
      runs = [runOf("a", "working")];
      renderSelector();
      await waitFor(() => expect(calls("list_graduation_queue").length).toBeGreaterThan(0));
      await move([temporary()], [runOf("a", "completed"), runOf("b", state)]);
      expect(prompt()).toBeNull();
    },
  );

  it("WSS-FR-GBTD: a discarded run keeps the prompt away", async () => {
    listing = [temporary({ queuedRunCount: 1 })];
    runs = [runOf("a", "working")];
    renderSelector();
    await waitFor(() => expect(calls("list_graduation_queue").length).toBeGreaterThan(0));
    await move([temporary()], [runOf("a", "discarded")]);
    expect(prompt()).toBeNull();
  });

  it("WSS-FR-LWZB, WSS-FR-NXCH: No keeps the stream and asks again only after later work", async () => {
    await finishWork();
    await userEvent.click(
      within(await screen.findByTestId("merge-prompt")).getByRole("button", { name: "No" }),
    );
    expect(prompt()).toBeNull();
    expect(calls("merge_work_stream")).toHaveLength(0);
    expect(calls("delete_work_stream")).toHaveLength(0);

    await move([temporary()], [runOf("a", "completed")]);
    expect(prompt()).toBeNull();

    await move([temporary({ queuedRunCount: 1 })], [runOf("a", "completed"), runOf("b", "working")]);
    await move([temporary()], [runOf("a", "completed"), runOf("b", "completed")]);
    expect(await screen.findByTestId("merge-prompt")).toBeInTheDocument();
  });

  it("WSS-FR-LWZB: Escape answers No", async () => {
    await finishWork();
    await screen.findByTestId("merge-prompt");
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(prompt()).toBeNull());
  });

  it("WSS-FR-YHCT: the prompt waits while another overlay is open, takes no focus from it, and shows when it closes", async () => {
    const other = document.createElement("div");
    other.className = "scrim";
    const field = document.createElement("input");
    other.appendChild(field);
    document.body.appendChild(other);
    field.focus();
    try {
      listing = [temporary({ queuedRunCount: 1 })];
      runs = [runOf("a", "working")];
      renderSelector();
      await screen.findByTestId("stream-selector");
      await waitFor(() => expect(calls("list_graduation_queue").length).toBeGreaterThan(0));
      await move([temporary()], [runOf("a", "completed")]);
      // The stream is due: the same listing shows the prompt once the overlay goes.
      expect(prompt()).toBeNull();
      expect(document.activeElement).toBe(field);
      expect(other.isConnected).toBe(true);
    } finally {
      other.remove();
    }
    expect(await screen.findByTestId("merge-prompt")).toBeInTheDocument();
  });

  it("WSS-FR-PRVK, WSS-FR-NXCH: a completed merge run left by a refused deletion does not keep the prompt away from later work", async () => {
    listing = [temporary({ queuedRunCount: 1 })];
    runs = [runOf("a", "working")];
    deleteAnswer = () => Promise.reject(new Error("stream_dirty"));
    renderSelector();
    await waitFor(() => expect(calls("list_graduation_queue").length).toBeGreaterThan(0));
    await move([temporary({ mergeRun: mergeRunLink("working") })], [runOf("a", "completed")]);
    await move([temporary({ mergeRun: mergeRunLink("completed") })], [runOf("a", "completed")]);
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
    expect(prompt()).toBeNull();

    await move(
      [temporary({ queuedRunCount: 1, mergeRun: mergeRunLink("completed") })],
      [runOf("a", "completed"), runOf("b", "working")],
    );
    await move(
      [temporary({ mergeRun: mergeRunLink("completed") })],
      [runOf("a", "completed"), runOf("b", "completed")],
    );
    expect(await screen.findByTestId("merge-prompt")).toBeInTheDocument();
  });
});

describe("the default merge message", () => {
  it("WSS-FR-GQXN: the commit message window opens with one dash line for each run, in run order", async () => {
    listing = [temporary()];
    runs = [runOf("a", "completed", "Fix tab order"), runOf("b", "completed", "Add run logs")];
    const { onRequestMergeCommit } = renderSelector();
    await userEvent.click(await screen.findByTestId("stream-selector"));
    const row = await screen.findByTestId("stream-row-w1");
    await userEvent.click(within(row).getByRole("button", { name: "Merge stream" }));
    await userEvent.click(within(row).getByRole("button", { name: "Commit…" }));
    await waitFor(() => expect(onRequestMergeCommit).toHaveBeenCalled());
    expect(onRequestMergeCommit).toHaveBeenCalledWith(
      "w1",
      "editor work",
      "- Fix tab order\n- Add run logs",
    );
  });

  it("WSS-FR-GQXN: a stream that is not temporary hands no default message", async () => {
    listing = [summary({}, { aheadOfBase: 2 })];
    const { onRequestMergeCommit } = renderSelector();
    await userEvent.click(await screen.findByTestId("stream-selector"));
    const row = await screen.findByTestId("stream-row-w1");
    await userEvent.click(within(row).getByRole("button", { name: "Merge stream" }));
    await userEvent.click(within(row).getByRole("button", { name: "Commit…" }));
    await waitFor(() => expect(onRequestMergeCommit).toHaveBeenCalled());
    expect(onRequestMergeCommit).toHaveBeenCalledWith("w1", "editor work", undefined);
  });
});

describe("the automatic deletion", () => {
  /** Merge the stream from the dropdown, leaving the result uncommitted. */
  const mergeUncommitted = async () => {
    await userEvent.click(await screen.findByTestId("stream-selector"));
    const row = await screen.findByTestId("stream-row-w1");
    await userEvent.click(within(row).getByRole("button", { name: "Merge stream" }));
    await userEvent.click(within(row).getByRole("button", { name: "Leave uncommitted" }));
    return row;
  };

  it("WSS-FR-KFSA: a merge left uncommitted deletes the stream with force", async () => {
    listing = [temporary()];
    mergeAnswer = mergedResult();
    renderSelector();
    await mergeUncommitted();
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
    expect(calls("delete_work_stream")[0][1]).toEqual({
      streamId: "w1",
      force: true,
      discardUncommitted: false,
    });
  });

  it("WSS-FR-KFSA: a merge that holds nothing deletes the stream without force", async () => {
    listing = [temporary()];
    mergeAnswer = { kind: "nothing_to_merge" };
    renderSelector();
    await mergeUncommitted();
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
    expect(calls("delete_work_stream")[0][1]).toMatchObject({ force: false, discardUncommitted: false });
  });

  it("WSS-FR-KFSA: a merge of a stream that is not temporary deletes nothing", async () => {
    listing = [summary({}, { aheadOfBase: 2 })];
    mergeAnswer = mergedResult();
    renderSelector();
    await mergeUncommitted();
    // The row shows the answer of the settled call before the check is made.
    await screen.findByText(/Merged editor work/);
    expect(calls("delete_work_stream")).toHaveLength(0);
  });

  it("WSS-FR-NZRB: a merge run deletes the stream when it completes, and not before", async () => {
    listing = [temporary()];
    mergeAnswer = conflictedResult();
    renderSelector();
    await mergeUncommitted();
    await move([temporary({ mergeRun: mergeRunLink("working") })], []);
    expect(calls("delete_work_stream")).toHaveLength(0);
    await move([temporary({ mergeRun: mergeRunLink("completed") })], []);
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
    expect(calls("delete_work_stream")[0][1]).toMatchObject({ streamId: "w1", force: false });
  });

  it("WSS-FR-NZRB: a merge run that fails deletes nothing", async () => {
    listing = [temporary()];
    mergeAnswer = conflictedResult();
    renderSelector();
    await mergeUncommitted();
    await move([temporary({ mergeRun: mergeRunLink("working") })], []);
    await move([temporary({ mergeRun: mergeRunLink("failed") })], []);
    await move([temporary({ mergeRun: mergeRunLink("failed") })], []);
    expect(calls("delete_work_stream")).toHaveLength(0);
  });

  it("WSS-FR-NZRB: a merge run found completed at launch deletes nothing", async () => {
    listing = [temporary({ mergeRun: mergeRunLink("completed") })];
    renderSelector();
    await screen.findByTestId("stream-selector");
    await waitFor(() => expect(calls("list_work_streams").length).toBeGreaterThan(0));
    await move(listing, []);
    expect(calls("delete_work_stream")).toHaveLength(0);
  });

  it("WSS-FR-NZRB: a merge run still working at launch is followed to its end", async () => {
    listing = [temporary({ mergeRun: mergeRunLink("working") })];
    renderSelector();
    await screen.findByTestId("stream-selector");
    await waitFor(() => expect(calls("list_work_streams").length).toBeGreaterThan(0));
    await move([temporary({ mergeRun: mergeRunLink("completed") })], []);
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
  });

  it("WSS-FR-NZRB: a merge run that completed before the merge call answered still deletes the stream", async () => {
    listing = [temporary()];
    let answer: (value: unknown) => void = () => {};
    mergeAnswer = new Promise((resolve) => (answer = resolve));
    renderSelector();
    await mergeUncommitted();
    await waitFor(() => expect(calls("merge_work_stream")).toHaveLength(1));
    await move([temporary({ mergeRun: mergeRunLink("completed") })], []);
    expect(calls("delete_work_stream")).toHaveLength(0);
    await act(async () => answer(conflictedResult()));
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
  });

  it("WSS-FR-TCSB: a refused deletion leaves the stream and shows the refusal on its row", async () => {
    listing = [temporary()];
    mergeAnswer = { kind: "nothing_to_merge" };
    deleteAnswer = () => Promise.reject(new Error("stream_active"));
    renderSelector();
    await mergeUncommitted();
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
    if (!screen.queryByTestId("stream-menu")) {
      await userEvent.click(await screen.findByTestId("stream-selector"));
    }
    const row = await screen.findByTestId("stream-row-w1");
    expect(row).toHaveTextContent(/active worktree/i);
    expect(calls("delete_work_stream")).toHaveLength(1);
  });
});
