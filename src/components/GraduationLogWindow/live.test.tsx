import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import {
  makeLogPage,
  makeLogs,
  makeObservability,
  makeRun,
  makeSourceEntry,
  pageForRead,
} from "../../test/graduationFixtures";
import type { ProgressStage } from "../../state/runProgress";
import type { GraduationLogPage, GraduationRun } from "../../types";
import { GraduationLogWindow } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);

const REVIEW: ProgressStage = { id: "review", label: "Review" };
const WORKING: ProgressStage = { id: "working", label: "Working" };

type Answer = (
  args: Record<string, unknown>,
) => GraduationLogPage | Promise<GraduationLogPage>;
let answer: Answer;
let appended: ((payload: unknown) => void) | null;

function reads(): Array<Record<string, unknown>> {
  return invoked.mock.calls
    .filter(([command]) => command === "read_graduation_logs")
    .map(([, args]) => args as Record<string, unknown>);
}

/** The pass number a read asked for, or null for the run-level scope. */
function passOf(args: Record<string, unknown>): number | null {
  const scope = args.pass as { kind: string; pass?: number };
  return scope.kind === "pass" ? (scope.pass ?? null) : null;
}

beforeEach(() => {
  invoked.mockReset();
  listened.mockReset();
  appended = null;
  listened.mockImplementation(async (name, handler) => {
    if (name === "graduation-log-records-appended") {
      appended = (payload: unknown) =>
        (handler as unknown as (event: { payload: unknown }) => void)({ payload });
    }
    return () => {};
  });
  answer = () => makeLogPage();
  invoked.mockImplementation(async (command: string, args) => {
    if (command !== "read_graduation_logs") return undefined;
    const request = (args ?? {}) as Record<string, unknown>;
    return pageForRead(request, await answer(request));
  });
});

afterEach(cleanup);

/**
 * A run that went work, review, back to work for a second pass, review again,
 * and back to work for a third. It stands in the working stage.
 */
function revisedRun(over: Parameters<typeof makeRun>[2] = {}): GraduationRun {
  return makeRun("r1", "working", {
    observability: makeObservability({
      currentStage: "working",
      stageHistory: [
        { from: "queued", to: "working", pass: 1, at: "1", reason: "work_started" },
        { from: "working", to: "review", pass: 1, at: "2", reason: "review_started" },
        { from: "review", to: "working", pass: 2, at: "3", reason: "review_revision" },
        { from: "working", to: "review", pass: 2, at: "4", reason: "review_started" },
        { from: "review", to: "working", pass: 3, at: "5", reason: "review_revision" },
      ],
    }),
    logs: makeLogs({
      source: [
        { phaseId: "working", pass: 1 },
        { phaseId: "review", pass: 1 },
        { phaseId: "working", pass: 2 },
        { phaseId: "working", pass: 3 },
      ],
    }),
    ...over,
  });
}

function draw(run: GraduationRun, stage: ProgressStage) {
  return render(
    <GraduationLogWindow run={run} stage={stage} onClose={() => {}} />,
  );
}

function passButtons(): string[] {
  return Array.from(
    screen.getByTestId("glw-pass-list").querySelectorAll("button"),
  ).map((button) => button.textContent ?? "");
}

describe("the graduation log window, read while a phase runs", () => {
  it("GLW-FR-RQLV: records already durable show at once, while the phase is still running", async () => {
    answer = () =>
      makeLogPage({
        status: "available",
        latestSequence: 2,
        entries: [makeSourceEntry(1, "first line"), makeSourceEntry(2, "second line")],
      });
    draw(
      makeRun("r1", "working", {
        observability: makeObservability({
          stageHistory: [
            { from: "queued", to: "working", pass: 1, at: "1", reason: "work_started" },
          ],
        }),
        logs: makeLogs({ source: [{ phaseId: "working", pass: 1 }] }),
      }),
      WORKING,
    );
    expect(await screen.findByText("second line")).toBeInTheDocument();
    expect(screen.getByText("first line")).toBeInTheDocument();
    // The first read is the newest page, with no cursor and no wait.
    expect(reads()).toHaveLength(1);
    expect(reads()[0].cursor ?? null).toBeNull();
  });

  it("GLW-FR-GZWN, GLW-FR-RQLV: an append updates the open view before the phase completes", async () => {
    answer = (args) =>
      args.cursor
        ? makeLogPage({ status: "available", entries: [makeSourceEntry(3, "streamed")] })
        : makeLogPage({ status: "available", entries: [makeSourceEntry(2, "held")] });
    draw(revisedRun(), WORKING);
    await screen.findByText("held");
    await act(async () => {
      appended?.({ runId: "r1", stream: "source", latestSequence: 3 });
    });
    expect(await screen.findByText("streamed")).toBeInTheDocument();
    expect(screen.getByText("held")).toBeInTheDocument();
    const after = reads().find((read) => read.cursor);
    expect(after?.cursor).toMatchObject({ direction: "after", sequence: 2 });
  });

  it("GLW-FR-GZWN: an append of another run is ignored", async () => {
    answer = () =>
      makeLogPage({ status: "available", entries: [makeSourceEntry(2, "held")] });
    draw(revisedRun(), WORKING);
    await screen.findByText("held");
    await act(async () => {
      appended?.({ runId: "r2", stream: "source", latestSequence: 9 });
    });
    expect(reads()).toHaveLength(1);
  });

  it("GLW-FR-GZWN: an append is read whether or not the opened phase is the current one", async () => {
    answer = (args) =>
      args.cursor
        ? makeLogPage({ status: "available", entries: [makeSourceEntry(6, "late review line")] })
        : makeLogPage({ status: "available", entries: [makeSourceEntry(5, "review line")] });
    // The run stands in the working stage and the window reads the review.
    draw(revisedRun(), REVIEW);
    await screen.findByText("review line");
    await act(async () => {
      appended?.({ runId: "r1", stream: "source", latestSequence: 6 });
    });
    expect(await screen.findByText("late review line")).toBeInTheDocument();
    expect(reads().every((read) => read.phaseId === "review")).toBe(true);
  });
});

describe("the review phase after the run returns to working", () => {
  it("GLW-FR-JMXD, GLW-FR-CQSE: the pass list is the passes that entered review in the stage history", async () => {
    draw(revisedRun(), REVIEW);
    await waitFor(() => expect(reads()).toHaveLength(1));
    // Pass 2 entered review and wrote nothing, so it is listed and not read
    // on opening. Pass 3 never entered review and is not listed.
    expect(passButtons()).toEqual(["Pass 1", "Pass 2"]);
    expect(reads()[0]).toMatchObject({
      phaseId: "review",
      pass: { kind: "pass", pass: 1 },
    });
  });

  it("GLW-FR-JMXD, GLW-FR-EUTW: each listed pass reads and shows only its own records", async () => {
    answer = (args) => {
      const pass = passOf(args);
      return makeLogPage({
        status: "available",
        entries: [makeSourceEntry(pass === 2 ? 20 : 10, `review of pass ${pass}`)],
      });
    };
    const run = revisedRun({
      logs: makeLogs({
        source: [
          { phaseId: "review", pass: 1 },
          { phaseId: "review", pass: 2 },
        ],
      }),
    });
    draw(run, REVIEW);
    expect(await screen.findByText("review of pass 2")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Pass 1" }));
    expect(await screen.findByText("review of pass 1")).toBeInTheDocument();
    expect(screen.queryByText("review of pass 2")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Pass 2" }));
    expect(await screen.findByText("review of pass 2")).toBeInTheDocument();
    expect(screen.queryByText("review of pass 1")).not.toBeInTheDocument();
    expect(reads().map((read) => [read.phaseId, passOf(read)])).toEqual([
      ["review", 2],
      ["review", 1],
      ["review", 2],
    ]);
  });

  it("GLW-FR-CQSE: working lists its own passes apart from review's", async () => {
    draw(revisedRun(), WORKING);
    await waitFor(() => expect(reads()).toHaveLength(1));
    expect(passButtons()).toEqual(["Pass 1", "Pass 2", "Pass 3"]);
  });

  it("GLW-FR-CQSE: a later pass is listed beside the earlier one when the run record changes", async () => {
    const earlier = revisedRun({
      observability: makeObservability({
        currentStage: "review",
        stageHistory: [
          { from: "queued", to: "working", pass: 1, at: "1", reason: "work_started" },
          { from: "working", to: "review", pass: 1, at: "2", reason: "review_started" },
        ],
      }),
    });
    const view = draw(earlier, REVIEW);
    await waitFor(() => expect(reads()).toHaveLength(1));
    expect(passButtons()).toEqual(["Pass 1"]);
    view.rerender(
      <GraduationLogWindow run={revisedRun()} stage={REVIEW} onClose={() => {}} />,
    );
    await waitFor(() => expect(passButtons()).toEqual(["Pass 1", "Pass 2"]));
  });
});

describe("what the window refuses to show", () => {
  const request = (over: Partial<GraduationLogPage>) =>
    makeLogPage({
      status: "available",
      entries: [makeSourceEntry(1, "foreign")],
      ...over,
    });

  /** A backend that ignores the read's scope and answers with this page. */
  function answerWith(page: GraduationLogPage) {
    invoked.mockImplementation(async (command: string) =>
      command === "read_graduation_logs" ? page : undefined,
    );
  }

  it.each([
    ["run", { runId: "r2" }],
    ["phase", { phaseId: "review" }],
    ["pass", { scope: { kind: "pass", pass: 1 } as const }],
    ["scope kind", { scope: { kind: "run_level" } as const }],
    ["stream", { stream: "structured" as const }],
  ])("GLW-FR-VRTC: a page for another %s is not shown", async (_name, over) => {
    answerWith(
      request({
        scope: { kind: "pass", pass: 3 },
        phaseId: "working",
        ...over,
      }),
    );
    draw(revisedRun(), WORKING);
    await waitFor(() => expect(reads()).toHaveLength(1));
    await screen.findByText(/another run, stage, or stream/);
    expect(screen.queryByText("foreign")).not.toBeInTheDocument();
  });

  it("GLW-FR-VRTC: a record of another run, phase, or pass in an accepted page is not shown", async () => {
    // The newest pass with records is pass 3 in the working stage.
    answer = () =>
      makeLogPage({
        status: "available",
        entries: [
          makeSourceEntry(1, "mine", { pass: 3 }),
          makeSourceEntry(2, "other run", { pass: 3, run_id: "r2" }),
          makeSourceEntry(3, "other phase", { pass: 3, phase_id: "review" }),
          makeSourceEntry(4, "other pass", { pass: 2 }),
          makeSourceEntry(5, "run level", { pass: null }),
        ],
      });
    invoked.mockImplementation(async (command: string, args) => {
      if (command !== "read_graduation_logs") return undefined;
      const page = await answer((args ?? {}) as Record<string, unknown>);
      // Keep the records exactly as the fixture wrote them.
      return {
        ...page,
        runId: "r1",
        phaseId: "working",
        stream: "source" as const,
        scope: { kind: "pass" as const, pass: 3 },
      };
    });
    draw(revisedRun(), WORKING);
    expect(await screen.findByText("mine")).toBeInTheDocument();
    expect(screen.queryByText("other run")).not.toBeInTheDocument();
    expect(screen.queryByText("other phase")).not.toBeInTheDocument();
    expect(screen.queryByText("other pass")).not.toBeInTheDocument();
    expect(screen.queryByText("run level")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("glw-row")).toHaveLength(1);
  });

  it("GLW-FR-VRTC, GLW-FR-RQEV: an append holds no record twice and a stale read does not replace the newest count", async () => {
    let appends = 0;
    answer = (args) => {
      if (!args.cursor) {
        return makeLogPage({
          status: "available",
          search: "matched",
          latestSequence: 2,
          matchedTotal: 2,
          entries: [makeSourceEntry(1, "one"), makeSourceEntry(2, "two")],
        });
      }
      appends += 1;
      return appends === 1
        ? makeLogPage({
            status: "available",
            search: "matched",
            latestSequence: 4,
            matchedTotal: 4,
            entries: [makeSourceEntry(2, "two"), makeSourceEntry(3, "three"), makeSourceEntry(4, "four")],
          })
        : makeLogPage({
            status: "available",
            search: "matched",
            latestSequence: 3,
            matchedTotal: 3,
            entries: [makeSourceEntry(2, "two"), makeSourceEntry(3, "three")],
          });
    };
    draw(revisedRun(), WORKING);
    await userEvent.type(screen.getByTestId("glw-search"), "o");
    await waitFor(() =>
      expect(screen.getByTestId("glw-match-count")).toHaveTextContent("2 matches"),
    );
    await act(async () => {
      appended?.({ runId: "r1", stream: "source", latestSequence: 4 });
    });
    await screen.findByText("four");
    expect(screen.getByTestId("glw-match-count")).toHaveTextContent("4 matches");
    await act(async () => {
      appended?.({ runId: "r1", stream: "source", latestSequence: 3 });
    });
    await waitFor(() => expect(appends).toBe(2));
    expect(screen.getByTestId("glw-match-count")).toHaveTextContent("4 matches");
    expect(screen.getAllByText("two")).toHaveLength(1);
    expect(screen.getAllByText("three")).toHaveLength(1);
    expect(screen.getAllByTestId("glw-row")).toHaveLength(4);
  });
});
