import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { BottomPanel, updateLabel } from "./BottomPanel";
import type { WorkStreamUpdateProgress } from "../types";
import { makeMergeRun, makeQueue, makeRun } from "../test/graduationFixtures";

// The Runs panel reads through `invoke` and subscribes to backend events on
// mount. Rendering it unmocked leaves those promises to reject after the test
// has finished, which Vitest flags as a false-positive risk.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

afterEach(cleanup);

describe("a merge run in the agent output section (RUN-FR-02, RUN-FR-LQDT, RUN-FR-BWNK)", () => {
  const queueOf = (runs: ReturnType<typeof makeRun>[]) =>
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "list_graduation_queue") return makeQueue(runs);
      if (cmd === "get_graduation_capacity")
        return { limit: "unlimited", inUse: 0, waitingForSlot: [] };
      if (cmd === "read_agent_activity")
        return { records: [], total: 0, dropped: 0 };
      return undefined;
    });

  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
    vi.mocked(listen).mockImplementation(async () => () => {});
  });

  const openOutput = async () => {
    render(
      <BottomPanel
        surface="runs"
        onHide={() => {}}
        activeEntity={null}
        onSwitchWorktree={async () => ({ ok: true })}
        canCheckOutBranches
      />,
    );
    await userEvent.click(screen.getByRole("tab", { name: /Agent output/ }));
  };

  const activityReads = () =>
    vi
      .mocked(invoke)
      .mock.calls.filter(([name]) => name === "read_agent_activity");

  it("RUN-FR-LQDT, RUN-FR-02: a working merge run is read by its own id and named by its title", async () => {
    queueOf([makeMergeRun("m-1", "working")]);
    await openOutput();

    // RUN-FR-LQDT: keyed by the run's id and read with the same operation.
    await waitFor(() =>
      expect(
        activityReads().some(([, args]) => (args as { runId?: string })?.runId === "m-1"),
      ).toBe(true),
    );
    expect(await screen.findByText("Merge editor-work")).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "" })).toHaveTextContent("running");
  });

  it("RUN-FR-LQDT, RUN-FR-02: a merge run's review turn is agent work, so the indicator is live", async () => {
    queueOf([makeMergeRun("m-1", "reviewing")]);
    await openOutput();

    await screen.findByText("Merge editor-work");
    expect(screen.getByRole("status", { name: "" })).toHaveTextContent("running");
  });

  it("RUN-FR-LQDT, RUN-FR-06: a merge run that has ended stays readable, no longer live", async () => {
    queueOf([makeMergeRun("m-1", "completed", { workTurns: 1, reviewTurns: 1 })]);
    await openOutput();

    // No run is working, so the most recently updated run that ran a turn is
    // what the section shows: the turn that has just ended is still readable.
    expect(await screen.findByText("Merge editor-work")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("finished")).toBeInTheDocument());
    expect(
      activityReads().some(([, args]) => (args as { runId?: string })?.runId === "m-1"),
    ).toBe(true);
  });

  it("RUN-FR-02, RUN-FR-LQDT: where a merge run and a draft run both work, the more recently updated is shown", async () => {
    queueOf([
      makeRun("g-1", "working", { updatedAt: "2026-09-06T09:00:00Z" }),
      makeMergeRun("m-1", "working", { updatedAt: "2026-09-06T10:00:00Z" }),
    ]);
    await openOutput();

    expect(await screen.findByText("Merge editor-work")).toBeInTheDocument();
    expect(screen.queryByText("Run g-1")).toBeNull();
  });

  it("RUN-FR-BWNK: a merge run is counted once in the section control and offers no section of its own", async () => {
    queueOf([makeMergeRun("m-1", "queued")]);
    render(
      <BottomPanel
        surface="runs"
        onHide={() => {}}
        activeEntity={null}
        onSwitchWorktree={async () => ({ ok: true })}
        canCheckOutBranches
      />,
    );

    await waitFor(() =>
      expect(screen.getByRole("tab", { name: /Graduation · 1/ })).toBeInTheDocument(),
    );
    // Two sections only: graduation and agent output.
    expect(screen.getAllByRole("tab")).toHaveLength(2);
  });

  it("RUN-FR-VKLS: no work stream merge progress event is listened for any more", async () => {
    queueOf([]);
    await openOutput();
    const names = vi.mocked(listen).mock.calls.map(([name]) => name);
    expect(names).not.toContain("work-stream-merge-progress");
    expect(names).toContain("work-stream-update-progress");
  });
});

describe("a work stream update in the agent output section (RUN-FR-02, RUN-FR-VKLS, RUN-FR-ZQWA)", () => {
  type Handler = Parameters<typeof listen>[1];
  let update: Handler | null = null;
  const deliver = (handler: Handler, event: string, payload: unknown) =>
    handler({ event, id: 1, payload });

  const updatePayload = (
    over: Partial<WorkStreamUpdateProgress> = {},
  ): WorkStreamUpdateProgress => ({
    projectKey: "p",
    streamId: "w1",
    attemptId: "u18d2c546ca6305d03a16",
    strategy: "rebase_source",
    turn: 2,
    turnsMax: 3,
    reconcilingPaths: ["specifications/ui/RUN-runs.md"],
    ...over,
  });

  beforeEach(() => {
    update = null;
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "list_graduation_queue") return { runs: [] };
      if (cmd === "get_work_stream") return { id: "w1", name: "test stream" };
      if (cmd === "read_agent_activity")
        return { records: [], total: 0, dropped: 0 };
      return undefined;
    });
    vi.mocked(listen).mockReset();
    vi.mocked(listen).mockImplementation(async (name, handler) => {
      if (name === "work-stream-update-progress") update = handler;
      return () => {};
    });
  });

  const openOutput = async () => {
    render(
      <BottomPanel
        surface="runs"
        onHide={() => {}}
        activeEntity={null}
        onSwitchWorktree={async () => ({ ok: true })}
        canCheckOutBranches
      />,
    );
    await userEvent.click(screen.getByRole("tab", { name: /Agent output/ }));
  };

  const activityReads = () =>
    vi
      .mocked(invoke)
      .mock.calls.filter(([name]) => name === "read_agent_activity");

  it("RUN-FR-VKLS: an update names the stream, the direction and the turn", () => {
    expect(updateLabel("test stream", updatePayload())).toBe(
      "Updating test stream from source by rebase · turn 2 of 3",
    );
    expect(
      updateLabel("test stream", updatePayload({ strategy: "merge_source" })),
    ).toBe("Updating test stream from source by merge · turn 2 of 3");
  });

  it("RUN-FR-VKLS: an update is never labelled as the opposite merge", () => {
    // The two operations run in opposite directions on one stream, so an
    // update that read as a merge would name the wrong work.
    expect(updateLabel("test stream", updatePayload())).not.toContain(
      "Merging",
    );
    expect(updateLabel("", updatePayload())).toBe(
      "Updating w1 from source by rebase · turn 2 of 3",
    );
  });

  it("RUN-FR-02, RUN-FR-ZQWA: the section follows the update attempt while it runs", async () => {
    await openOutput();
    expect(
      update,
      "the panel listens on work-stream-update-progress",
    ).toBeTruthy();

    await act(async () =>
      deliver(update!, "work-stream-update-progress", updatePayload()),
    );

    // RUN-FR-ZQWA: read by the attempt, an update belonging to a stream and to
    // no run of it.
    await waitFor(() =>
      expect(
        activityReads().some(
          ([, args]) =>
            (args as { runId?: string })?.runId === "u18d2c546ca6305d03a16",
        ),
      ).toBe(true),
    );
    expect(
      await screen.findByText(
        /Updating test stream from source by rebase · turn 2 of 3/,
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "" })).toHaveTextContent(
      "running",
    );
  });

  it("RUN-FR-02, RUN-FR-06: a settled update stays readable, no longer live", async () => {
    await openOutput();
    await act(async () =>
      deliver(update!, "work-stream-update-progress", updatePayload()),
    );
    await screen.findByText(/Updating test stream/);

    // WKS-FR-FQLS: turn zero says the update has stopped. Its records are the
    // whole account of why it ended, so they stay until newer work replaces
    // them.
    await act(async () =>
      deliver(
        update!,
        "work-stream-update-progress",
        updatePayload({ turn: 0 }),
      ),
    );
    await waitFor(() =>
      expect(screen.getByText("finished")).toBeInTheDocument(),
    );
    expect(screen.getByText(/Updating test stream/)).toBeInTheDocument();
  });

  it("RUN-FR-02: a working graduation run that updated later is shown ahead of a settled update", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "list_graduation_queue")
        return {
          runs: [
            {
              id: "g1",
              state: "working",
              updatedAt: new Date(Date.now() + 60_000).toISOString(),
              input: { draftName: "run logs" },
            },
          ],
        };
      if (cmd === "get_work_stream") return { id: "w1", name: "test stream" };
      if (cmd === "read_agent_activity")
        return { records: [], total: 0, dropped: 0 };
      return undefined;
    });
    await openOutput();
    await act(async () =>
      deliver(update!, "work-stream-update-progress", updatePayload()),
    );
    await act(async () =>
      deliver(
        update!,
        "work-stream-update-progress",
        updatePayload({ turn: 0 }),
      ),
    );

    await waitFor(() =>
      expect(screen.getByText("run logs")).toBeInTheDocument(),
    );
    expect(screen.queryByText(/Updating test stream/)).toBeNull();
  });

  it("RUN-FR-02: a working graduation run beats a settled update that reported later", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "list_graduation_queue")
        return {
          runs: [
            {
              id: "g1",
              state: "working",
              // The run last reported an hour ago and the update reports now,
              // so only the "running work first" rule can decide this. An
              // ordering that read the times alone would show the update.
              updatedAt: new Date(Date.now() - 3_600_000).toISOString(),
              input: { draftName: "run logs" },
            },
          ],
        };
      if (cmd === "get_work_stream") return { id: "w1", name: "test stream" };
      if (cmd === "read_agent_activity")
        return { records: [], total: 0, dropped: 0 };
      return undefined;
    });
    await openOutput();
    await act(async () =>
      deliver(update!, "work-stream-update-progress", updatePayload()),
    );
    await screen.findByText(/Updating test stream/);
    await act(async () =>
      deliver(
        update!,
        "work-stream-update-progress",
        updatePayload({ turn: 0 }),
      ),
    );

    await waitFor(() =>
      expect(screen.getByText("run logs")).toBeInTheDocument(),
    );
    expect(screen.queryByText(/Updating test stream/)).toBeNull();
  });

  it("RUN-FR-VKLS: the stream is named once however many turns the update reports", async () => {
    await openOutput();
    await act(async () =>
      deliver(
        update!,
        "work-stream-update-progress",
        updatePayload({ turn: 1 }),
      ),
    );
    await screen.findByText(/Updating test stream/);
    await act(async () =>
      deliver(
        update!,
        "work-stream-update-progress",
        updatePayload({ turn: 2 }),
      ),
    );
    await act(async () =>
      deliver(
        update!,
        "work-stream-update-progress",
        updatePayload({ turn: 3 }),
      ),
    );

    const named = vi
      .mocked(invoke)
      .mock.calls.filter(([name]) => name === "get_work_stream");
    expect(named).toHaveLength(1);
  });

  it("RUN-FR-VKLS: a stream the panel cannot name still renders the update", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "list_graduation_queue") return { runs: [] };
      if (cmd === "read_agent_activity")
        return { records: [], total: 0, dropped: 0 };
      if (cmd === "get_work_stream") throw new Error("unknown_stream");
      return undefined;
    });
    await openOutput();
    await act(async () =>
      deliver(update!, "work-stream-update-progress", updatePayload()),
    );

    expect(
      await screen.findByText(/Updating w1 from source by rebase · turn 2 of 3/),
    ).toBeInTheDocument();
  });
});
