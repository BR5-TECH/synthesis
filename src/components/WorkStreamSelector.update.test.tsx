/**
 * Bringing one stream up to its base branch, from the row and the window it
 * opens (`../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-TKMB,
 * WSS-FR-HZVQ, WSS-FR-NLXD, WSS-FR-WPGR, WSS-FR-CJYE, WSS-FR-QSAF,
 * WSS-FR-BDMU, WSS-FR-XRHT, WSS-FR-FVKO, WSS-FR-GTQL, WSS-FR-PMYA,
 * WSS-FR-ZWCB).
 *
 * Apart from the merge files because Update is the opposite direction: what it
 * writes is the stream, and a test that shared a fixture with the merge suite
 * could pass while naming the wrong branch.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { WorkStreamSelector, refusalText } from "./WorkStreamSelector";
import { STREAM_ERRORS, type WorkStreamSummary } from "../types";
import {
  mergeRunLink,
  summary,
  updateAskedOnce,
  updateRecord,
} from "../test/streamFixtures";

// The selector reaches the backend only through `invoke` (`../api`).
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

function backend(
  streams: WorkStreamSummary[],
  overrides: Record<string, () => unknown> = {},
) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (overrides[cmd]) return overrides[cmd]();
    if (cmd === "list_work_streams") return streams;
    return undefined;
  });
}

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function renderSelector() {
  render(
    <WorkStreamSelector
      inRepository
      activeBranch="some-other-branch"
      onOpen={vi.fn()}
      onActivate={vi.fn(async () => ({ ok: true as const }))}
      onRequestMergeCommit={vi.fn(async () => null as string | null)}
      onOpenChanges={vi.fn()}
      onOpenRun={vi.fn()}
    />,
  );
}

const openMenu = async () => {
  await userEvent.click(await screen.findByTestId("stream-selector"));
  return screen.findByTestId("stream-menu");
};

const openRow = async (
  streams: WorkStreamSummary[],
  overrides: Record<string, () => unknown> = {},
) => {
  backend(streams, overrides);
  renderSelector();
  const menu = await openMenu();
  return within(menu).getByTestId("stream-row-w1");
};

/** A stream three commits behind its base and holding nothing of its own. */
const behind = (over: Parameters<typeof summary>[1] = {}) =>
  summary({}, { behindBase: 3, ...over });

beforeEach(() => {
  invokeMock.mockReset();
  handlers = [];
});
afterEach(cleanup);

describe("the Update action on a stream row (WSS-FR-TKMB, WSS-FR-HZVQ)", () => {
  it("WSS-FR-TKMB: the row offers Update between Merge and Delete", async () => {
    const row = await openRow([behind({ aheadOfBase: 2 })]);
    const labels = within(row)
      .getAllByRole("button")
      .map((button) => button.textContent?.trim())
      .filter((label) => label && /^(Merge…|Update…|Delete…)$/.test(label));

    expect(labels).toEqual(["Merge…", "Update…", "Delete…"]);
  });

  it("WSS-FR-HZVQ: Update is enabled only where the stream is behind its base", async () => {
    const row = await openRow([behind()]);
    expect(within(row).getByTestId("stream-update-w1")).toBeEnabled();

    cleanup();
    const upToDate = await openRow([summary({}, { behindBase: 0 })]);
    const disabled = within(upToDate).getByTestId("stream-update-w1");
    expect(disabled).toBeDisabled();
    // WSS-FR-RWLB: and it says why in words rather than by the greyed control.
    expect(disabled).toHaveAttribute("title", expect.stringMatching(/up to date/i));
  });

  it("WSS-FR-HZVQ: Update is disabled while runs are queued on the stream", async () => {
    const row = await openRow([behind({ queuedRunCount: 2 })]);
    const disabled = within(row).getByTestId("stream-update-w1");

    expect(disabled).toBeDisabled();
    expect(disabled).toHaveAttribute("title", expect.stringMatching(/queued/i));
  });

  it("WSS-FR-YPDA: Merge is disabled where the stream is ahead of its base by nothing", async () => {
    const row = await openRow([behind({ aheadOfBase: 0 })]);

    expect(within(row).getByTestId("stream-merge-w1")).toBeDisabled();
    expect(within(row).getByTestId("stream-update-w1")).toBeEnabled();
  });
});

describe("the stream update window (WSS-FR-NLXD, WSS-FR-WPGR)", () => {
  const open = async (streams: WorkStreamSummary[] = [behind()]) => {
    const row = await openRow(streams);
    await userEvent.click(within(row).getByTestId("stream-update-w1"));
    return within(row).getByTestId("stream-update-window-w1");
  };

  it("WSS-FR-NLXD: the window names the recorded base branch, its revision and the missing commits", async () => {
    const window_ = await open();

    // WSS-FR-TKMB: the recorded base branch, not the active worktree's branch.
    expect(window_.textContent).toMatch(/from main at/);
    expect(window_.textContent).not.toMatch(/some-other-branch/);
    const revision = within(window_).getByTestId("stream-update-revision");
    expect(revision).toHaveTextContent("9f2c1ab");
    expect(revision).toHaveAttribute(
      "title",
      "9f2c1ab0000000000000000000000000000000ba",
    );
    expect(
      within(window_).getByTestId("stream-update-commits").textContent,
    ).toMatch(/base commit 1/);
    // The direction is stated, so nobody reads Update as the merge.
    expect(window_.textContent).toMatch(/main does not move/);
  });

  it("WSS-FR-WPGR: each strategy submits the displayed revision, and Cancel starts nothing", async () => {
    const window_ = await open();
    await userEvent.click(
      within(window_).getByTestId("stream-update-cancel-window-w1"),
    );
    expect(calls("update_work_stream")).toHaveLength(0);

    cleanup();
    const reopened = await open();
    await userEvent.click(
      within(reopened).getByTestId("stream-update-rebase-source-w1"),
    );
    await waitFor(() => expect(calls("update_work_stream")).toHaveLength(1));
    expect(calls("update_work_stream")[0][1]).toEqual({
      streamId: "w1",
      strategy: "rebase_source",
      baseRevision: "9f2c1ab0000000000000000000000000000000ba",
    });
  });

  it("WSS-FR-NLXD, WSS-FR-JMWA: a long commit list is cut and the rest counted", async () => {
    const window_ = await open([
      summary(
        {},
        {
          behindBase: 120,
          missingCommits: Array.from({ length: 50 }, (_, index) => ({
            revision: `${index}`.padStart(40, "d"),
            summary: `base commit ${index + 1}`,
            author: "Test Author",
            committedAt: "2026-05-01T00:00:00Z",
          })),
        },
      ),
    ]);

    const commits = within(window_).getByTestId("stream-update-commits");
    expect(commits.textContent).toMatch(/base commit 8/);
    expect(commits.textContent).not.toMatch(/base commit 9\b/);
    expect(commits.textContent).toMatch(/and 42 more/);
    // The whole count is what the author reads, not the length of the listing.
    expect(window_.textContent).toMatch(/120 commits on main/);
  });

  it("WSS-FR-WPGR: the merge-source strategy submits the same pinned revision", async () => {
    const window_ = await open();
    await userEvent.click(
      within(window_).getByTestId("stream-update-merge-source-w1"),
    );

    await waitFor(() => expect(calls("update_work_stream")).toHaveLength(1));
    expect(calls("update_work_stream")[0][1]).toMatchObject({
      strategy: "merge_source",
      baseRevision: "9f2c1ab0000000000000000000000000000000ba",
    });
  });
});

describe("what an update refuses (WSS-FR-CJYE, WSS-FR-QSAF, WSS-FR-ZWCB)", () => {
  const startWith = async (refusal: string) => {
    const row = await openRow([behind()], {
      update_work_stream: () => {
        throw new Error(refusal);
      },
    });
    await userEvent.click(within(row).getByTestId("stream-update-w1"));
    await userEvent.click(
      within(
        within(row).getByTestId("stream-update-window-w1"),
      ).getByTestId("stream-update-merge-source-w1"),
    );
    return row;
  };

  it("WSS-FR-CJYE: a base branch that moved is told, and the author reopens Update", async () => {
    const row = await startWith(
      `${STREAM_ERRORS.staleBaseRevision}: 41b0d7e`,
    );

    const stale = await within(row).findByTestId("stream-update-stale");
    expect(stale.textContent).toMatch(/moved since this window read it/i);
    expect(stale.textContent).toMatch(/Nothing was written/i);
    // Nothing is re-run on the author's behalf: the commit list they read is
    // stale too, so the window offers only a way out.
    expect(calls("update_work_stream")).toHaveLength(1);
    expect(
      within(row).queryByTestId("stream-update-merge-source-w1"),
    ).toBeNull();
  });

  it("WSS-FR-QSAF: a dirty base worktree is named as such, with the complete path set", async () => {
    const row = await startWith(
      `${STREAM_ERRORS.baseDirty}: docs/a.md, docs/b.md, src/c.ts`,
    );

    const dirty = await within(row).findByTestId("stream-update-dirty");
    expect(dirty.textContent).toMatch(/docs\/a\.md/);
    expect(dirty.textContent).toMatch(/docs\/b\.md/);
    expect(dirty.textContent).toMatch(/src\/c\.ts/);
    // The refusal names WHICH worktree: the two are settled in different
    // checkouts, so "not committed" alone leaves the author nowhere to go.
    expect(within(row).getByRole("alert").textContent).toMatch(
      /worktree holding main/,
    );
    expect(
      within(row).getByRole("button", { name: "Open Changes" }),
    ).toBeInTheDocument();
  });

  it("WSS-FR-QSAF: a dirty stream working copy names the stream instead", async () => {
    const row = await startWith(`${STREAM_ERRORS.dirty}: notes/draft.md`);

    expect(
      (await within(row).findByTestId("stream-update-dirty")).textContent,
    ).toMatch(/notes\/draft\.md/);
    expect(within(row).getByRole("alert").textContent).toMatch(
      /working copy of editor work/,
    );
  });

  it("WSS-FR-QSAF: a base branch with no checkout says so and offers no Changes route", async () => {
    const row = await startWith(STREAM_ERRORS.baseNotCheckedOut);

    expect(within(row).getByRole("alert").textContent).toMatch(
      /no worktree holds main/i,
    );
    // There is no uncommitted work to settle, so the Changes route would lead
    // the author nowhere.
    expect(within(row).queryByRole("button", { name: "Open Changes" })).toBeNull();
  });

  it("WSS-FR-QSAF: Open Changes routes to the Changes panel", async () => {
    const onOpenChanges = vi.fn();
    backend([behind()], {
      update_work_stream: () => {
        throw new Error(`${STREAM_ERRORS.baseDirty}: docs/a.md`);
      },
    });
    render(
      <WorkStreamSelector
        inRepository
        activeBranch="some-other-branch"
        onOpen={vi.fn()}
        onActivate={vi.fn(async () => ({ ok: true as const }))}
        onRequestMergeCommit={vi.fn(async () => null as string | null)}
        onOpenChanges={onOpenChanges}
        onOpenRun={vi.fn()}
      />,
    );
    const row = within(await openMenu()).getByTestId("stream-row-w1");
    await userEvent.click(within(row).getByTestId("stream-update-w1"));
    await userEvent.click(
      within(
        within(row).getByTestId("stream-update-window-w1"),
      ).getByTestId("stream-update-merge-source-w1"),
    );
    await userEvent.click(
      await within(row).findByRole("button", { name: "Open Changes" }),
    );

    expect(onOpenChanges).toHaveBeenCalled();
  });

  it("WSS-FR-ZWCB: each typed refusal of an update reads as its own sentence", async () => {
    const codes = [
      STREAM_ERRORS.busy,
      STREAM_ERRORS.missing,
      STREAM_ERRORS.staleBaseRevision,
      STREAM_ERRORS.updateInProgress,
      STREAM_ERRORS.mergeInProgress,
      STREAM_ERRORS.updateAttemptsExhausted,
      STREAM_ERRORS.updateInterrupted,
      STREAM_ERRORS.updateStateNotPermitted,
    ];
    const said = codes.map((code) => refusalText(code));

    // Every code reads as words rather than as itself, and no two of them say
    // the same thing — a row must tell the author which refusal they hit.
    for (const [index, sentence] of said.entries()) {
      expect(sentence).not.toBe(codes[index]);
      expect(sentence).toMatch(/[a-z] [a-z]/);
    }
    expect(new Set(said).size).toBe(said.length);
  });

  it("WSS-FR-ZWCB: every other typed refusal renders on the stream's own row", async () => {
    const row = await startWith(STREAM_ERRORS.updateInProgress);

    const alert = await within(row).findByRole("alert");
    expect(alert.textContent).toMatch(/already updating a stream/i);
  });
});

describe("an update that is running (WSS-FR-BDMU, WSS-FR-XRHT)", () => {
  it("WSS-FR-BDMU: the row says it is updating and offers Cancel update", async () => {
    const row = await openRow([
      behind({ update: updateRecord("running", { strategy: "rebase_source" }) }),
    ]);

    expect(within(row).getByTestId("stream-update-running-w1").textContent).toMatch(
      /Updating by rebase/,
    );
    await userEvent.click(within(row).getByTestId("stream-update-cancel-w1"));
    await waitFor(() =>
      expect(calls("cancel_work_stream_update")).toHaveLength(1),
    );
    expect(calls("cancel_work_stream_update")[0][1]).toEqual({ streamId: "w1" });
    // WSS-FR-BDMU: and never an operation on a graduation run.
    expect(calls("pause_graduation_run")).toHaveLength(0);
    expect(calls("discard_graduation_run")).toHaveLength(0);
  });

  it("WSS-FR-XRHT: Merge and Update are both unavailable while either is running", async () => {
    const updatingRow = await openRow([
      behind({ aheadOfBase: 2, update: updateRecord("running") }),
    ]);
    expect(within(updatingRow).queryByTestId("stream-merge-w1")).toBeNull();
    expect(within(updatingRow).queryByTestId("stream-update-w1")).toBeNull();

    // WSS-FR-XRHT: a merge run that is not completed and not failed holds the
    // stream's one reconciliation, so both actions are disabled.
    cleanup();
    const mergingRow = await openRow([
      behind({ aheadOfBase: 2, mergeRun: mergeRunLink("queued") }),
    ]);
    expect(within(mergingRow).getByTestId("stream-merge-w1")).toBeDisabled();
    expect(within(mergingRow).getByTestId("stream-update-w1")).toBeDisabled();
  });

  it("WSS-FR-BDMU: a settled report leaves no turn on the row, and another stream's is ignored", async () => {
    const row = await openRow([behind({ update: updateRecord("running") })]);
    const emit = handlers.find(
      ([name]) => name === "work-stream-update-progress",
    )?.[1];
    const report = (over: Record<string, unknown>) =>
      emit?.({
        payload: {
          projectKey: "p",
          streamId: "w1",
          attemptId: "u1",
          strategy: "merge_source",
          turn: 1,
          turnsMax: 3,
          reconcilingPaths: ["only-this-one.md"],
          ...over,
        },
      });

    // A settled report carries turn 0 and must not make the row claim a turn.
    report({ turn: 0 });
    expect(
      within(row).getByTestId("stream-update-running-w1").textContent,
    ).not.toMatch(/turn 0/);
    // And a report about another stream never renders on this one.
    report({ streamId: "w2", reconcilingPaths: ["some-other-stream.md"] });
    expect(row.textContent).not.toMatch(/some-other-stream\.md/);
  });

  it("WSS-FR-XRHT: a second confirmation while one update is in flight starts nothing", async () => {
    const row = await openRow([behind({ update: updateRecord("running") })]);
    // The row offers no way to start a second one at all.
    expect(within(row).queryByTestId("stream-update-w1")).toBeNull();
    expect(calls("update_work_stream")).toHaveLength(0);
  });

  it("WSS-FR-BDMU: the progress event names the turn and the paths it reconciles", async () => {
    const row = await openRow([behind({ update: updateRecord("running") })]);
    const emit = handlers.find(
      ([name]) => name === "work-stream-update-progress",
    );
    expect(emit).toBeDefined();
    emit?.[1]({
      payload: {
        projectKey: "p",
        streamId: "w1",
        attemptId: "u1",
        strategy: "rebase_source",
        turn: 2,
        turnsMax: 3,
        reconcilingPaths: ["README.md"],
      },
    });

    await waitFor(() =>
      expect(
        within(row).getByTestId("stream-update-running-w1").textContent,
      ).toMatch(/turn 2 of 3/),
    );
    expect(row.textContent).toMatch(/README\.md/);
  });
});

describe("an update that stopped (WSS-FR-FVKO, WSS-FR-GTQL, WSS-FR-PMYA)", () => {
  it("WSS-FR-FVKO, WSS-FR-GTQL: the row renders the durable record and offers to review it", async () => {
    const row = await openRow([
      behind({
        update: updateRecord("conflicted", {
          conflicts: [
            { path: "README.md", baseChange: "updated", streamChange: "updated" },
          ],
        }),
      }),
    ]);

    expect(within(row).getByTestId("stream-update-state-w1")).toHaveTextContent(
      "Update stopped",
    );
    expect(row.textContent).toMatch(/could not settle 1 path/);
    expect(row.textContent).toMatch(/Nothing was written/);
    expect(
      within(row).getByTestId("stream-update-open-w1"),
    ).toHaveTextContent("Review update…");
  });

  it("WSS-FR-GTQL: a failed and a cancelled update each say what stopped them", async () => {
    const failedRow = await openRow([
      behind({
        update: updateRecord("failed", {
          failure: STREAM_ERRORS.updateInterrupted,
        }),
      }),
    ]);
    expect(failedRow.textContent).toMatch(/application stopped/i);
    expect(
      within(failedRow).getByTestId("stream-update-open-w1"),
    ).toHaveTextContent("Review update…");

    cleanup();
    const cancelledRow = await openRow([
      behind({ update: updateRecord("cancelled") }),
    ]);
    expect(cancelledRow.textContent).toMatch(/You cancelled this update/);
    expect(cancelledRow.textContent).toMatch(/Neither branch was changed/);
  });

  it("WSS-FR-FVKO: the row re-renders from the record a reload brings, not from what it attempted", async () => {
    const running = behind({ update: updateRecord("running") });
    const settled = behind({
      update: updateRecord("updated", { updatedPaths: ["a.ts"] }),
    });
    let listing = [running];
    backend([], { list_work_streams: () => listing });
    renderSelector();
    const row = within(await openMenu()).getByTestId("stream-row-w1");
    expect(within(row).getByTestId("stream-update-running-w1")).toBeInTheDocument();

    // The update settles in another window, and the event reloads this listing.
    listing = [settled];
    handlers
      .filter(([name]) => name === "work-streams-changed")
      .forEach(([, handler]) => handler({ payload: { projectKey: "p" } }));

    await waitFor(() =>
      expect(within(row).queryByTestId("stream-update-running-w1")).toBeNull(),
    );
    expect(within(row).getByTestId("stream-update-w1")).toBeInTheDocument();
  });

  it("WSS-FR-GTQL: an escalated update offers Answer… and names the source direction", async () => {
    const row = await openRow([behind({ update: updateAskedOnce() })]);
    await userEvent.click(within(row).getByTestId("stream-update-open-w1"));

    const dialog = await screen.findByTestId("update-resolution");
    // WSS-FR-ZMPC: the direction an update reconciles in, base into stream.
    expect(dialog.textContent).toMatch(/main into synthesis\/stream\/editor-work/);
    expect(dialog.textContent).toMatch(/Which design stands\?/);
    expect(
      within(dialog).getByTestId("update-resolution-paths").textContent,
    ).toMatch(/README\.md/);
  });

  it("WSS-FR-PMYA: the window's actions reach the update operations and never merge or run operations", async () => {
    const row = await openRow([
      behind({
        mergeRun: mergeRunLink("failed"),
        update: updateRecord("conflicted", {
          conflicts: [
            { path: "README.md", baseChange: "updated", streamChange: "updated" },
          ],
        }),
      }),
    ]);
    await userEvent.click(within(row).getByTestId("stream-update-open-w1"));
    const dialog = await screen.findByTestId("update-resolution");

    expect(
      within(dialog).getByTestId("update-resolution-retry"),
    ).toHaveTextContent("Retry update");
    await userEvent.click(within(dialog).getByTestId("update-resolution-retry"));
    await waitFor(() =>
      expect(calls("retry_work_stream_update")).toHaveLength(1),
    );

    await userEvent.click(within(dialog).getByTestId("update-resolution-dismiss"));
    await waitFor(() =>
      expect(calls("clear_work_stream_update")).toHaveLength(1),
    );

    // Not one merge or graduation run operation was invoked.
    expect(calls("merge_work_stream")).toHaveLength(0);
    expect(calls("continue_graduation_run")).toHaveLength(0);
    expect(calls("discard_graduation_run")).toHaveLength(0);
  });

  it("WSS-FR-PMYA: answering an escalated update sends the set through the update operation", async () => {
    const row = await openRow([behind({ update: updateAskedOnce() })]);
    await userEvent.click(within(row).getByTestId("stream-update-open-w1"));
    const dialog = await screen.findByTestId("update-resolution");

    await userEvent.click(
      within(dialog).getByRole("radio", { name: /Keep the base branch's/ }),
    );
    await userEvent.click(
      within(dialog).getByTestId("graduation-send-answers"),
    );

    await waitFor(() =>
      expect(calls("answer_work_stream_update_escalation")).toHaveLength(1),
    );
    expect(calls("answer_graduation_escalation")).toHaveLength(0);
    expect(
      calls("answer_work_stream_update_escalation")[0][1],
    ).toMatchObject({ streamId: "w1" });
  });
});
