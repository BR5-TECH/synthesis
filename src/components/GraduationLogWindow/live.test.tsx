import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { makeActivityEntry, makeLogPage, makeLogs, makeObservability, makeRun } from "../../test/graduationFixtures";
import {
  REVIEW,
  WORKING,
  draw,
  entryNames,
  installLogBackend,
  passOf,
  revisedRun,
  workingRun,
  type LogBackend,
} from "../../test/graduationLogWindowHarness";
import type { GraduationLogPage } from "../../types";
import { GraduationLogWindow } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

let backend: LogBackend;

beforeEach(() => {
  backend = installLogBackend();
});

afterEach(cleanup);

/** A run in its first work turn, which has made one pass durable. */
function firstTurn() {
  return makeRun("r1", "working", {
    observability: makeObservability({
      stageHistory: [
        { from: "queued", to: "working", pass: 1, at: "1", reason: "work_started" },
      ],
    }),
    logs: makeLogs({ activity: [{ phaseId: "working", pass: 1 }] }),
  });
}

describe("the graduation log window, read while a phase runs", () => {
  it("GLW-FR-RQLV, GRS-FR-WRAS: rows already durable show at once, while the phase is still running", async () => {
    backend.answer = () =>
      makeLogPage({
        status: "available",
        latestSequence: 2,
        entries: [makeActivityEntry(1, "first row"), makeActivityEntry(2, "second row")],
      });
    draw({ run: firstTurn() });
    expect(await screen.findByText("second row")).toBeInTheDocument();
    expect(screen.getByText("first row")).toBeInTheDocument();
    // The first read is the newest page, with no cursor and no wait.
    expect(backend.reads()).toHaveLength(1);
    expect(backend.reads()[0].cursor ?? null).toBeNull();
  });

  it("GLW-FR-RQLV, GLW-FR-GZWN: an append shows new rows while the run state and the phase stay as they were", async () => {
    backend.answer = (args) =>
      args.cursor
        ? makeLogPage({ status: "available", entries: [makeActivityEntry(3, "streamed row")] })
        : makeLogPage({ status: "available", entries: [makeActivityEntry(2, "held row")] });
    // The run record is handed over once and never changes: no phase change,
    // no new turn, and no change of run state reaches the window.
    const target = workingRun();
    const { rerender } = draw({ run: target });
    await screen.findByText("held row");
    await backend.appended({ runId: "r1", stream: "activity", latestSequence: 3 });
    expect(await screen.findByText("streamed row")).toBeInTheDocument();
    expect(screen.getByText("held row")).toBeInTheDocument();
    expect(target.state).toBe("working");
    rerender(<GraduationLogWindow run={target} stage={WORKING} onClose={() => {}} />);
    expect(screen.getByText("streamed row")).toBeInTheDocument();
    const after = backend.reads().find((read) => read.cursor);
    expect(after).toMatchObject({
      runId: "r1",
      phaseId: "working",
      stream: "activity",
      pass: { kind: "pass", pass: 2 },
      cursor: { direction: "after", sequence: 2, stream: "activity" },
    });
  });

  it("GLW-FR-GZWN, GLW-FR-HCOI: an append of another run or of the structured stream is ignored", async () => {
    backend.answer = () =>
      makeLogPage({ status: "available", entries: [makeActivityEntry(2, "held row")] });
    draw();
    await screen.findByText("held row");
    await backend.appended({ runId: "r2", stream: "activity", latestSequence: 9 });
    await backend.appended({ runId: "r1", stream: "structured", latestSequence: 9 });
    expect(backend.reads()).toHaveLength(1);
  });

  it("GLW-FR-GZWN: an append is read whether or not the opened phase is the run's current one", async () => {
    backend.answer = (args) =>
      args.cursor
        ? makeLogPage({ status: "available", entries: [makeActivityEntry(6, "late review row")] })
        : makeLogPage({ status: "available", entries: [makeActivityEntry(5, "review row")] });
    // The run stands in the working phase and the window reads the review.
    draw({ run: revisedRun(), stage: REVIEW });
    await screen.findByText("review row");
    await backend.appended({ runId: "r1", stream: "activity", latestSequence: 6 });
    expect(await screen.findByText("late review row")).toBeInTheDocument();
    expect(backend.reads().every((read) => read.phaseId === "review")).toBe(true);
  });

  it("GLW-FR-GZWN: an append that arrives while the first page is in flight is read after that page lands", async () => {
    let release: (page: GraduationLogPage) => void = () => {};
    backend.answer = (args) =>
      args.cursor
        ? makeLogPage({ status: "available", entries: [makeActivityEntry(11, "arrived row")] })
        : new Promise<GraduationLogPage>((resolve) => {
            release = resolve;
          });
    draw();
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    await backend.appended({ runId: "r1", stream: "activity", latestSequence: 11 });
    expect(backend.reads()).toHaveLength(1);
    await act(async () => {
      release(makeLogPage({ status: "available", entries: [makeActivityEntry(10, "held row")] }));
    });
    expect(await screen.findByText("arrived row")).toBeInTheDocument();
    expect(screen.getByText("held row")).toBeInTheDocument();
    expect(backend.reads()[1].cursor).toMatchObject({ direction: "after", sequence: 10 });
  });

  it("GLW-FR-GZWN: closing the window stops it listening", async () => {
    backend.answer = () =>
      makeLogPage({ status: "available", entries: [makeActivityEntry(2, "held row")] });
    const view = draw();
    await screen.findByText("held row");
    await waitFor(() => expect(backend.listeners()).toBe(1));
    view.unmount();
    await waitFor(() => expect(backend.listeners()).toBe(0));
    await backend.appended({ runId: "r1", stream: "activity", latestSequence: 3 });
    expect(backend.reads()).toHaveLength(1);
  });
});

describe("the graduation log window, read after a relaunch", () => {
  it("GLW-FR-THAX, GLW-FR-RQLV: a window opened from fresh mocks renders what the read returns, with no event and no earlier session", async () => {
    const persisted = [
      makeActivityEntry(1, "persisted before the relaunch", { kind: "message" }),
      makeActivityEntry(2, "also persisted", { kind: "finished" }),
    ];
    backend.answer = () =>
      makeLogPage({ status: "available", latestSequence: 2, entries: persisted });
    draw();
    await screen.findByText("also persisted");
    cleanup();

    // A new session: every mock, store, and handler is new.
    backend = installLogBackend();
    backend.answer = () =>
      makeLogPage({ status: "available", latestSequence: 2, entries: persisted });
    draw();
    expect(await screen.findByText("persisted before the relaunch")).toBeInTheDocument();
    expect(screen.getByText("also persisted")).toBeInTheDocument();
    expect(backend.reads()).toHaveLength(1);
    expect(backend.reads()[0].cursor ?? null).toBeNull();
    expect(backend.commands().filter((c) => c !== "append_log_records")).toEqual([
      "read_graduation_logs",
    ]);
  });
});

describe("the review phase after the run returns to working", () => {
  it("GLW-FR-JMXD, GLW-FR-CQSE: the pass list is the passes that entered review in the stage history", async () => {
    draw({ run: revisedRun(), stage: REVIEW });
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    // Pass 2 entered review and wrote nothing, so it is listed and not read on
    // opening. Pass 3 never entered review and is not listed.
    expect(entryNames(screen.getByTestId("glw-pass-list"))).toEqual(["Pass 1", "Pass 2"]);
    expect(backend.reads()[0]).toMatchObject({
      phaseId: "review",
      stream: "activity",
      pass: { kind: "pass", pass: 1 },
    });
  });

  it("GLW-FR-JMXD, GLW-FR-EUTW: each listed pass reads and shows only its own rows", async () => {
    backend.answer = (args) => {
      const pass = passOf(args);
      return makeLogPage({
        status: "available",
        entries: [makeActivityEntry(pass === 2 ? 20 : 10, `review of pass ${pass}`)],
      });
    };
    const target = revisedRun({
      logs: makeLogs({
        activity: [
          { phaseId: "review", pass: 1 },
          { phaseId: "review", pass: 2 },
        ],
      }),
    });
    draw({ run: target, stage: REVIEW });
    expect(await screen.findByText("review of pass 2")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Pass 1" }));
    expect(await screen.findByText("review of pass 1")).toBeInTheDocument();
    expect(screen.queryByText("review of pass 2")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Pass 2" }));
    expect(await screen.findByText("review of pass 2")).toBeInTheDocument();
    expect(screen.queryByText("review of pass 1")).not.toBeInTheDocument();
    expect(backend.reads().map((read) => [read.phaseId, passOf(read)])).toEqual([
      ["review", 2],
      ["review", 1],
      ["review", 2],
    ]);
  });

  it("GLW-FR-CQSE: working lists its own passes apart from review's", async () => {
    draw({ run: revisedRun(), stage: WORKING });
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    expect(entryNames(screen.getByTestId("glw-pass-list"))).toEqual([
      "Pass 1",
      "Pass 2",
      "Pass 3",
    ]);
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
    const view = draw({ run: earlier, stage: REVIEW });
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    expect(entryNames(screen.getByTestId("glw-pass-list"))).toEqual(["Pass 1"]);
    view.rerender(<GraduationLogWindow run={revisedRun()} stage={REVIEW} onClose={() => {}} />);
    await waitFor(() =>
      expect(entryNames(screen.getByTestId("glw-pass-list"))).toEqual(["Pass 1", "Pass 2"]),
    );
    // The opening selection settles once and later growth moves no selection.
    expect(backend.reads()).toHaveLength(1);
  });
});

describe("what the window refuses to show", () => {
  /** A backend that ignores the read's scope and answers with this page. */
  function answerWith(page: GraduationLogPage) {
    backend.raw = true;
    backend.answer = () => page;
  }

  it.each([
    ["run", { runId: "r2" }],
    ["phase", { phaseId: "review" }],
    ["pass", { scope: { kind: "pass", pass: 1 } as const }],
    ["scope kind", { scope: { kind: "run_level" } as const }],
    ["stream", { stream: "structured" as const }],
  ])("GLW-FR-VRTC, GLW-FR-HCOI: a page for another %s is not shown", async (_name, over) => {
    answerWith(
      makeLogPage({
        status: "available",
        entries: [makeActivityEntry(1, "foreign row")],
        scope: { kind: "pass", pass: 3 },
        phaseId: "working",
        ...over,
      }),
    );
    draw({ run: revisedRun(), stage: WORKING });
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    await screen.findByText(/another run, phase, or scope/);
    expect(screen.queryByText("foreign row")).not.toBeInTheDocument();
  });

  it("GLW-FR-VRTC, GLW-FR-NFLN: a page that names the activity stream, run, phase, and pass is shown", async () => {
    answerWith(
      makeLogPage({
        status: "available",
        entries: [makeActivityEntry(1, "own row", { pass: 3 })],
        scope: { kind: "pass", pass: 3 },
        phaseId: "working",
      }),
    );
    draw({ run: revisedRun(), stage: WORKING });
    expect(await screen.findByText("own row")).toBeInTheDocument();
  });

  it("GLW-FR-VRTC, GLW-FR-FCVA: a record of another run, phase, or pass in an accepted page is not shown", async () => {
    answerWith(
      makeLogPage({
        status: "available",
        scope: { kind: "pass", pass: 3 },
        phaseId: "working",
        entries: [
          makeActivityEntry(1, "mine", { pass: 3 }),
          makeActivityEntry(2, "other run", { pass: 3, run_id: "r2" }),
          makeActivityEntry(3, "other phase", { pass: 3, phase_id: "review" }),
          makeActivityEntry(4, "other pass", { pass: 2 }),
          makeActivityEntry(5, "run level", { pass: null }),
        ],
      }),
    );
    draw({ run: revisedRun(), stage: WORKING });
    expect(await screen.findByText("mine")).toBeInTheDocument();
    for (const other of ["other run", "other phase", "other pass", "run level"]) {
      expect(screen.queryByText(other)).not.toBeInTheDocument();
    }
    expect(screen.getAllByTestId("glw-row")).toHaveLength(1);
  });

  it("GLW-FR-VRTC, GLW-FR-CQXJ: the run-level entry shows only run-level records of its phase", async () => {
    backend.raw = true;
    backend.answer = () =>
      makeLogPage({
        status: "available",
        scope: { kind: "run_level" },
        phaseId: "working",
        entries: [
          makeActivityEntry(1, "run-level mine", { pass: null }),
          makeActivityEntry(2, "a pass row", { pass: 2 }),
          makeActivityEntry(3, "run-level of another phase", { pass: null, phase_id: "queued" }),
        ],
      });
    draw({
      run: workingRun({
        logs: makeLogs({ activity: [{ phaseId: "working", pass: null }] }),
      }),
    });
    expect(await screen.findByText("run-level mine")).toBeInTheDocument();
    expect(screen.queryByText("a pass row")).not.toBeInTheDocument();
    expect(screen.queryByText("run-level of another phase")).not.toBeInTheDocument();
  });

  it("GLW-FR-VRTC, GLW-FR-RQEV: an append holds no row twice and a stale read does not replace the newest count", async () => {
    let appends = 0;
    backend.answer = (args) => {
      if (!args.cursor) {
        return makeLogPage({
          status: "available",
          search: "matched",
          latestSequence: 2,
          matchedTotal: 2,
          entries: [makeActivityEntry(1, "one"), makeActivityEntry(2, "two")],
        });
      }
      appends += 1;
      return appends === 1
        ? makeLogPage({
            status: "available",
            search: "matched",
            latestSequence: 4,
            matchedTotal: 4,
            entries: [
              makeActivityEntry(2, "two"),
              makeActivityEntry(3, "three"),
              makeActivityEntry(4, "four"),
            ],
          })
        : makeLogPage({
            status: "available",
            search: "matched",
            latestSequence: 3,
            matchedTotal: 3,
            entries: [makeActivityEntry(2, "two"), makeActivityEntry(3, "three")],
          });
    };
    draw({ run: revisedRun(), stage: WORKING });
    await userEvent.type(screen.getByTestId("glw-search"), "o");
    await waitFor(() =>
      expect(screen.getByTestId("glw-match-count")).toHaveTextContent("2 matches"),
    );
    await backend.appended({ runId: "r1", stream: "activity", latestSequence: 4 });
    await screen.findByText("four");
    await waitFor(() =>
      expect(screen.getByTestId("glw-match-count")).toHaveTextContent("4 matches"),
    );
    await backend.appended({ runId: "r1", stream: "activity", latestSequence: 3 });
    await waitFor(() => expect(appends).toBe(2));
    expect(screen.getByTestId("glw-match-count")).toHaveTextContent("4 matches");
    expect(screen.getAllByText("two")).toHaveLength(1);
    expect(screen.getAllByText("three")).toHaveLength(1);
    expect(screen.getAllByTestId("glw-row")).toHaveLength(4);
  });

  it("GLW-FR-RQEV: rows that arrive out of order are held in ascending sequence order", async () => {
    backend.answer = (args) =>
      args.cursor
        ? makeLogPage({
            status: "available",
            entries: [makeActivityEntry(12, "twelve"), makeActivityEntry(11, "eleven")],
          })
        : makeLogPage({
            status: "available",
            entries: [makeActivityEntry(10, "ten")],
          });
    draw();
    await screen.findByText("ten");
    await backend.appended({ runId: "r1", stream: "activity", latestSequence: 12 });
    await screen.findByText("twelve");
    const order = screen
      .getAllByTestId("glw-row")
      .map((row) => row.querySelector(".runs-line__msg")?.textContent);
    expect(order).toEqual(["ten", "eleven", "twelve"]);
  });

  it("GLW-FR-FCVA, GLW-FR-NFLN: two windows of two runs hold no row of each other", async () => {
    backend.answer = (args) =>
      makeLogPage({
        status: "available",
        entries: [
          makeActivityEntry(1, `row of ${String(args.runId)}`, { run_id: String(args.runId) }),
        ],
      });
    const view = render(
      <GraduationLogWindow
        run={makeRun("r1", "working", {
          observability: workingRun().observability,
          logs: workingRun().logs,
        })}
        stage={WORKING}
        onClose={() => {}}
      />,
    );
    await screen.findByText("row of r1");
    view.unmount();
    render(
      <GraduationLogWindow
        run={makeRun("r2", "working", {
          observability: workingRun().observability,
          logs: workingRun().logs,
        })}
        stage={WORKING}
        onClose={() => {}}
      />,
    );
    expect(await screen.findByText("row of r2")).toBeInTheDocument();
    expect(screen.queryByText("row of r1")).not.toBeInTheDocument();
  });
});
