/**
 * A direct run in the history rail and the run region
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-UKNC,
 * GRU-FR-HKBD, GRU-FR-LORX, GRU-FR-PFBY and
 * `../../../specifications/ui/GRT-graduation-restart.md` GRT-FR-NPDC).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";

import { resetAppPreferencesCache } from "../../state/appPreferences";
import { forgetEveryDraft } from "../../state/escalationDrafts";
import { forgetEverything } from "../../state/graduationSelection";
import {
  aheadOf,
  canContinue,
  pauseAnnouncement,
  reorderAnnouncement,
  restartDirectStatement,
  runQueueKey,
  targetHoldStatement,
} from "../../state/graduation";
import { makeQueue, makeRun } from "../../test/graduationFixtures";
import type { GraduationQueue, GraduationRun } from "../../types";
import { GraduationRuns } from ".";
import { RestartRegionControl } from "./restart";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
const commands = () => invoked.mock.calls.map(([command]) => command);

/** A direct run on an ordinary worktree, which names no stream. */
const direct = (
  id: string,
  state: GraduationRun["state"] = "queued",
  over: Partial<GraduationRun> = {},
): GraduationRun =>
  makeRun(id, state, {
    streamId: "",
    streamName: "",
    directTarget: {
      worktreePath: "/Users/demo/dev/acme-main",
      worktreeName: "acme-main",
      branch: "feature/new-window",
    },
    ...over,
  });

let queue: GraduationQueue;

beforeEach(() => {
  queue = makeQueue([direct("r1", "working")]);
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    switch (command) {
      case "list_graduation_queue":
        return queue;
      case "load_app_preferences":
        return { graduationRailWidthFraction: 0.2 };
      case "list_work_streams":
        return [];
      case "continue_graduation_run":
        return queue.runs[0];
      default:
        return undefined;
    }
  });
  resetAppPreferencesCache();
  forgetEverything();
  forgetEveryDraft();
});
afterEach(cleanup);

async function open() {
  render(<GraduationRuns />);
  await screen.findByTestId("graduation-section");
  await waitFor(() => expect(commands()).toContain("list_graduation_queue"));
}

describe("what a direct run says about where it works", () => {
  it("GRU-FR-UKNC, GRU-FR-LORX: the provenance names the worktree and branch, and no stream", async () => {
    await open();
    const line = await screen.findByTestId("graduation-provenance");
    expect(line).toHaveTextContent("directly");
    expect(line).toHaveTextContent("acme-main");
    expect(line).toHaveTextContent("feature/new-window");
    expect(line).not.toHaveTextContent(/work stream/);
    expect(line).not.toHaveTextContent(/no longer exists/);
  });

  it("GRU-FR-UKNC: a direct run on a stream's working copy names the stream too", async () => {
    queue = makeQueue([direct("r1", "working", { streamId: "s-1", streamName: "editor-work" })]);
    await open();
    const line = await screen.findByTestId("graduation-provenance");
    expect(line).toHaveTextContent("acme-main");
    expect(line).toHaveTextContent("editor-work");
  });

  it("GRU-FR-LORX: the rail row names the worktree where a stream run names its stream", async () => {
    queue = makeQueue([direct("r1", "working"), makeRun("r2", "working")]);
    await open();
    const rows = await screen.findAllByTestId("graduation-row");
    expect(rows[0]).toHaveTextContent("acme-main");
    expect(rows[0]).not.toHaveTextContent("editor-work");
    expect(rows[1]).toHaveTextContent("editor-work");
  });

  it("GRU-FR-LORX: the rail filter finds a direct run by its worktree and its branch", async () => {
    queue = makeQueue([direct("r1", "working"), makeRun("r2", "working")]);
    await open();
    await screen.findAllByTestId("graduation-row");
    const filter = screen.getByRole("searchbox");
    await userEvent.type(filter, "new-window");
    await waitFor(() => expect(screen.getAllByTestId("graduation-row")).toHaveLength(1));
    expect(screen.getByTestId("graduation-row")).toHaveAttribute("aria-label", "Run r1");
  });
});

describe("a direct run in its queue", () => {
  it("GRU-FR-HKBD: a queued direct run counts its place in its worktree's queue alone", async () => {
    queue = makeQueue([
      makeRun("s1", "queued"),
      direct("r1", "queued"),
      makeRun("s2", "queued"),
      direct("r2", "queued"),
    ]);
    await open();
    await userEvent.click(await screen.findByRole("button", { name: "Run r2" }));
    const row = await screen.findByTestId("run-progress");
    expect(row).toHaveTextContent("1 ahead of it in the worktree's queue");
  });

  it("GRD-FR-ZVNO, GRU-FR-HKBD: a direct run on a stream worktree counts in that stream's queue", () => {
    const q = makeQueue([
      makeRun("s1", "queued"),
      direct("r1", "queued", { streamId: "s-1", streamName: "editor-work" }),
      makeRun("s2", "queued"),
    ]);
    expect(runQueueKey(q.runs[1])).toBe("s-1");
    expect(aheadOf(q, "r1")).toBe(1);
    expect(aheadOf(q, "s2")).toBe(2);
  });

  it("GRD-FR-ZVNO: two worktrees keep two queues", () => {
    const other = direct("r2", "queued", {
      directTarget: {
        worktreePath: "/Users/demo/dev/acme-spike",
        worktreeName: "acme-spike",
        branch: "spike",
      },
    });
    const q = makeQueue([direct("r1", "queued"), other]);
    expect(runQueueKey(q.runs[0])).not.toBe(runQueueKey(other));
    expect(aheadOf(q, "r2")).toBe(0);
  });

  it("GRU-FR-LORX: the announcements name the worktree for a direct run", () => {
    const run = direct("r1", "queued");
    expect(reorderAnnouncement(run, 1, 0)).toContain("acme-main");
    expect(pauseAnnouncement(run, false)).toContain("acme-main");
  });
});

describe("a direct run held for its branch", () => {
  const held = () =>
    direct("r1", "queued", {
      targetHold: {
        code: "target_branch_changed",
        expectedBranch: "feature/new-window",
        actualBranch: "release/2.1",
      },
    });

  it("GRU-FR-PFBY: it names both branches and says when it starts", async () => {
    queue = makeQueue([held()]);
    await open();
    const hold = await screen.findByTestId("graduation-target-hold");
    expect(hold).toHaveTextContent("feature/new-window");
    expect(hold).toHaveTextContent("release/2.1");
    expect(hold).toHaveTextContent(/starts when/);
  });

  it("GRU-FR-PFBY: a detached worktree says it is on no branch", () => {
    const run = held();
    run.targetHold = { ...run.targetHold!, actualBranch: null };
    expect(targetHoldStatement(run)).toContain("not on a branch");
  });

  it("GRU-FR-PFBY: a run with no hold, and a hold that has not queued, say nothing", () => {
    expect(targetHoldStatement(direct("r1", "queued"))).toBeNull();
    expect(targetHoldStatement({ ...held(), state: "working" })).toBeNull();
  });

  it("GRU-FR-PFBY, GRD-FR-XRDY: it offers Continue, which invokes continue_graduation_run", async () => {
    queue = makeQueue([held()]);
    await open();
    await screen.findByTestId("graduation-target-hold");
    // The rail's control and the region's: both are the same act.
    const buttons = await screen.findAllByRole("button", { name: /continue/i });
    expect(buttons.length).toBeGreaterThan(0);
    await userEvent.click(buttons[buttons.length - 1]);
    await waitFor(() => expect(commands()).toContain("continue_graduation_run"));
    expect(invoked.mock.calls.find(([c]) => c === "continue_graduation_run")?.[1]).toEqual({
      runId: "r1",
    });
  });

  it("GRU-FR-PFBY: a queued run without a hold offers no Continue", () => {
    expect(canContinue(direct("r1", "queued"))).toBe(false);
    expect(canContinue(held())).toBe(true);
  });
});

describe("restarting a discarded direct run", () => {
  function draw(run: GraduationRun) {
    const onRestarted = vi.fn();
    render(<RestartRegionControl run={run} busy={false} onRestarted={onRestarted} />);
    return { onRestarted };
  }

  it("GRT-FR-NPDC: the confirmation names the worktree and branch and asks no stream", async () => {
    const run = direct("r1", "discarded");
    draw(run);
    await userEvent.click(screen.getByTestId("graduation-restart"));
    const confirm = await screen.findByTestId("graduation-restart-confirm");
    expect(within(confirm).getByTestId("graduation-restart-direct")).toHaveTextContent(
      "acme-main",
    );
    expect(within(confirm).getByTestId("graduation-restart-direct")).toHaveTextContent(
      "feature/new-window",
    );
    expect(within(confirm).queryByRole("combobox", { name: "Work stream" })).toBeNull();
    expect(screen.queryByTestId("standing-work-choice")).toBeNull();
    // The streams are not even read: nothing here is chosen from them.
    expect(commands()).not.toContain("list_work_streams");
    expect(restartDirectStatement(run)).toContain("acme-main");
  });

  it("GRT-FR-NPDC: confirming invokes the restart with resting values and no stream choice", async () => {
    const run = direct("r1", "discarded");
    invoked.mockImplementation(async (command: string) =>
      command === "restart_graduation_run" ? direct("r9", "queued") : undefined,
    );
    const { onRestarted } = draw(run);
    await userEvent.click(screen.getByTestId("graduation-restart"));
    await userEvent.click(await screen.findByRole("button", { name: "Restart run" }));
    await waitFor(() => expect(onRestarted).toHaveBeenCalled());
    expect(invoked.mock.calls.find(([c]) => c === "restart_graduation_run")?.[1]).toEqual({
      runId: "r1",
      streamId: "",
      standingWork: "commit",
      standingWorkMessage: null,
    });
  });

  it("GRT-FR-VWHM: a stream run's confirmation is unchanged and still asks a stream", async () => {
    invoked.mockImplementation(async (command: string) =>
      command === "list_work_streams" ? [] : undefined,
    );
    draw(makeRun("r1", "discarded"));
    await userEvent.click(screen.getByTestId("graduation-restart"));
    const confirm = await screen.findByTestId("graduation-restart-confirm");
    expect(within(confirm).getByRole("combobox", { name: "Work stream" })).toBeInTheDocument();
    expect(within(confirm).queryByTestId("graduation-restart-direct")).toBeNull();
  });
});
