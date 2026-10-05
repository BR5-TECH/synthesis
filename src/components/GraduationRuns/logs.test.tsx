/**
 * The stage row's route into the run log window
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-ZBMU,
 * GRU-FR-TQJW, GRU-FR-VKPD, GRU-FR-HXNC).
 *
 * What the window itself does is covered in
 * `../GraduationLogWindow/index.test.tsx`; what is asserted here is which stage
 * may be opened, what a stage that holds nothing says, and that opening one
 * run's stage shares nothing with another's.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { resetAppPreferencesCache } from "../../state/appPreferences";
import { forgetEveryDraft } from "../../state/escalationDrafts";
import { forgetEverything } from "../../state/graduationSelection";
import {
  makeLogPage,
  makeLogs,
  makeObservability,
  makeQueue,
  makeRun,
  makeSourceEntry,
  pageForRead,
} from "../../test/graduationFixtures";
import type { GraduationQueue } from "../../types";
import { GRADUATION_STAGES } from "../../state/graduation/stages";
import { GRADUATION_LOG_RECORDS_APPENDED } from "../../events";
import { GraduationRuns } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);

let queue: GraduationQueue;

/** A run whose `working` stage holds a log and whose `review` stage holds none. */
function run(id: string, over: Parameters<typeof makeRun>[2] = {}) {
  return makeRun(id, "working", {
    observability: makeObservability({
      stageHistory: [
        { from: "queued", to: "working", pass: 1, at: "1", reason: "work_started" },
      ],
    }),
    logs: makeLogs({
      source: [
        { phaseId: "queued", pass: null },
        { phaseId: "working", pass: 1 },
      ],
    }),
    ...over,
  });
}

beforeEach(() => {
  queue = makeQueue([run("r1")]);
  invoked.mockReset();
  invoked.mockImplementation(async (command: string, args) => {
    switch (command) {
      case "list_graduation_queue":
        return queue;
      case "load_app_preferences":
        return { graduationRailWidthFraction: 0.2 };
      case "list_work_streams":
        return [];
      case "read_graduation_logs": {
        const request = (args ?? {}) as Record<string, unknown>;
        return pageForRead(
          request,
          makeLogPage({
            status: "available",
            entries: [
              makeSourceEntry(1, `output of ${String(request.runId)}`, {
                run_id: String(request.runId),
                phase_id: String(request.phaseId),
              }),
            ],
          }),
        );
      }
      default:
        return undefined;
    }
  });
  listened.mockReset();
  listened.mockImplementation(async () => () => {});
  resetAppPreferencesCache();
  forgetEverything();
  forgetEveryDraft();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

async function open() {
  render(<GraduationRuns />);
  await screen.findByTestId("graduation-section");
  await waitFor(() =>
    expect(invoked.mock.calls.map(([c]) => c)).toContain("list_graduation_queue"),
  );
}

/** Every `read_graduation_logs` this test provoked. */
function reads(): Array<Record<string, unknown>> {
  return invoked.mock.calls
    .filter(([command]) => command === "read_graduation_logs")
    .map(([, args]) => args as Record<string, unknown>);
}

describe("the stage row's route into the log window", () => {
  it("GRU-FR-TQJW: a stage a segment names is activatable, whether the segment is a pass or run-level", async () => {
    await open();
    const working = await screen.findByTestId("run-progress-stage-working");
    const queued = screen.getByTestId("run-progress-stage-queued");
    expect(working).not.toHaveAttribute("aria-disabled");
    // The queued stage's only segment names `pass = null`, and that is enough.
    expect(queued).not.toHaveAttribute("aria-disabled");
  });

  it("GRU-FR-VKPD: a stage no segment names is disabled, says why, and reads no log", async () => {
    await open();
    const review = await screen.findByTestId("run-progress-stage-review");
    expect(review).toHaveAttribute("aria-disabled", "true");
    expect(review).toHaveAccessibleName(/no log yet/);
    await userEvent.click(review);
    expect(screen.queryByTestId("graduation-log-window")).not.toBeInTheDocument();
    expect(reads()).toHaveLength(0);
  });

  it("GRU-FR-HXNC: activating an eligible stage opens the window bound to that run and stage", async () => {
    await open();
    await userEvent.click(await screen.findByTestId("run-progress-stage-working"));
    const window = await screen.findByTestId("graduation-log-window");
    expect(window).toHaveAttribute("data-run", "r1");
    expect(window).toHaveAttribute("data-phase", "working");
    await waitFor(() => expect(reads()).toHaveLength(1));
    expect(reads()[0]).toMatchObject({ runId: "r1", phaseId: "working" });
  });

  it("GLW-FR-ALZI: the log window is one of the window's floating overlays", async () => {
    const onOverlayOpening = vi.fn();
    render(<GraduationRuns onOverlayOpening={onOverlayOpening} />);
    await screen.findByTestId("graduation-section");
    await waitFor(() =>
      expect(invoked.mock.calls.map(([c]) => c)).toContain("list_graduation_queue"),
    );
    const review = await screen.findByTestId("run-progress-stage-review");
    await userEvent.click(review);
    // A stage that opens nothing takes nothing else down.
    expect(onOverlayOpening).not.toHaveBeenCalled();

    await userEvent.click(screen.getByTestId("run-progress-stage-working"));
    await screen.findByTestId("graduation-log-window");
    // SNV-FR-56: opening it closes whatever else the window had open.
    expect(onOverlayOpening).toHaveBeenCalledTimes(1);
  });

  it("GLW-FR-JMXD, GLW-FR-CQSE: review stays openable after the run returns to working, with one entry per pass that entered it", async () => {
    queue = makeQueue([
      run("r1", {
        observability: makeObservability({
          currentStage: "working",
          stageHistory: [
            { from: "queued", to: "working", pass: 1, at: "1", reason: "work_started" },
            { from: "working", to: "review", pass: 1, at: "2", reason: "review_started" },
            { from: "review", to: "working", pass: 2, at: "3", reason: "review_revision" },
          ],
        }),
        logs: makeLogs({
          source: [
            { phaseId: "working", pass: 1 },
            { phaseId: "review", pass: 1 },
            { phaseId: "working", pass: 2 },
          ],
        }),
      }),
    ]);
    await open();
    const review = await screen.findByTestId("run-progress-stage-review");
    expect(review).not.toHaveAttribute("aria-disabled");
    await userEvent.click(review);
    const opened = await screen.findByTestId("graduation-log-window");
    expect(opened).toHaveAttribute("data-phase", "review");
    await waitFor(() => expect(reads()).toHaveLength(1));
    expect(reads()[0]).toMatchObject({
      phaseId: "review",
      pass: { kind: "pass", pass: 1 },
    });
    const list = within(opened).getByTestId("glw-pass-list");
    expect(within(list).getAllByRole("button").map((b) => b.textContent)).toEqual([
      "Pass 1",
    ]);
  });

  it("GRU-FR-HXNC, GLW-FR-OOYK: closing returns focus to the stage that opened it", async () => {
    await open();
    const working = await screen.findByTestId("run-progress-stage-working");
    await userEvent.click(working);
    await screen.findByTestId("graduation-log-window");
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() =>
      expect(screen.queryByTestId("graduation-log-window")).not.toBeInTheDocument(),
    );
    expect(document.activeElement).toBe(working);
  });

  it("GLW-FR-BVYN, GLW-FR-BTKB: another run's window reuses nothing of the previous one", async () => {
    queue = makeQueue([
      run("r1"),
      run("r2", {
        logs: makeLogs({ source: [{ phaseId: "working", pass: 1 }] }),
      }),
    ]);
    await open();

    await userEvent.click(await screen.findByTestId("run-progress-stage-working"));
    await screen.findByTestId("graduation-log-window");
    // Read the Structured stream and search inside the first run's window.
    await userEvent.click(screen.getByRole("button", { name: "Structured" }));
    await userEvent.type(screen.getByTestId("glw-search"), "escalate");
    await waitFor(() => expect(reads().some((read) => read.query)).toBe(true));
    await userEvent.click(screen.getByRole("button", { name: "Close" }));

    // The second run's own stage row opens a window bound to that run.
    const rows = screen.getAllByTestId("graduation-row");
    const second = rows.find((row) => row.textContent?.includes("Run r2"));
    await userEvent.click(within(second!).getByText("Run r2"));
    await userEvent.click(await screen.findByTestId("run-progress-stage-working"));
    const opened = await screen.findByTestId("graduation-log-window");
    expect(opened).toHaveAttribute("data-run", "r2");
    // GLW-FR-BVYN: not its stream, not its search text, not a single record.
    expect(screen.getByRole("button", { name: "Source" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByTestId("glw-search")).toHaveValue("");
    expect(screen.queryByTestId("glw-match-count")).not.toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByText("output of r2")).toBeInTheDocument(),
    );
    expect(screen.queryByText("output of r1")).not.toBeInTheDocument();
  });

  it("GRU-FR-ZVTC, GLW-FR-NFLN: a reload of the listing does not re-read the open window's page", async () => {
    await open();
    await userEvent.click(await screen.findByTestId("run-progress-stage-working"));
    await screen.findByTestId("graduation-log-window");
    await waitFor(() => expect(reads()).toHaveLength(1));

    // The section reloads on every run-changed event. The window is reading the
    // same run, the same stage, the same scope and the same stream, so nothing
    // about its scope changed and its page must survive the reload.
    const handlers = listened.mock.calls
      .filter(([name]) => name === "graduation-run-changed")
      .map(([, handler]) => handler as (event: { payload: unknown }) => void);
    expect(handlers.length).toBeGreaterThan(0);
    await act(async () => {
      for (const handler of handlers) handler({ payload: { runId: "r1" } });
    });
    await waitFor(() =>
      expect(
        invoked.mock.calls.filter(([c]) => c === "list_graduation_queue"),
      ).toHaveLength(2),
    );
    expect(reads()).toHaveLength(1);
    expect(screen.getByText("output of r1")).toBeInTheDocument();
  });

  it("GRU-FR-ZBMU: the row renders exactly the configured descriptors, and eligibility follows them", async () => {
    await open();
    await screen.findByTestId("run-progress");
    const entries = [
      ...screen.getByTestId("run-progress").querySelectorAll("[data-stage]"),
    ].map((entry) => entry.getAttribute("data-stage"));
    // The section adds and drops no stage: the row is exactly the descriptor
    // list, in its own order.
    expect(entries).toEqual(GRADUATION_STAGES.map((stage) => stage.id));
    // And every rule about a stage reads that list rather than a stage id: the
    // two the index names are openable and the two it does not are not, whether
    // the segment named a pass or `pass = null`.
    for (const stage of GRADUATION_STAGES) {
      const eligible = stage.id === "queued" || stage.id === "working";
      const entry = screen.queryByTestId(`run-progress-stage-${stage.id}`);
      expect(entry).not.toBeNull();
      expect(entry?.hasAttribute("aria-disabled")).toBe(!eligible);
    }
  });

  it("GRS-FR-MBED: a storage version this build does not know makes no stage openable", async () => {
    queue = makeQueue([
      run("r1", {
        logs: makeLogs({
          logStorageVersion: 2,
          source: [{ phaseId: "working", pass: 1 }],
        }),
      }),
    ]);
    await open();
    await screen.findByTestId("run-progress");
    // RPV-FR-JSQW: with no stage activatable the row adds no control at all,
    // so there is nothing to open and no focus stop that promises one.
    expect(
      within(screen.getByTestId("run-progress")).queryAllByRole("button"),
    ).toHaveLength(0);
    expect(reads()).toHaveLength(0);
  });
});

describe("a log append that can open a stage", () => {
  const APPENDED = GRADUATION_LOG_RECORDS_APPENDED;

  /** A run at its first work turn, whose indexes name nothing of `working` yet. */
  const firstTurn = (id: string, over: Parameters<typeof makeRun>[2] = {}) =>
    run(id, { logs: makeLogs({ source: [{ phaseId: "queued", pass: null }] }), ...over });

  /** Tell the section that a stream of `runId` grew, as the backend does. */
  async function append(runId: string, times = 1) {
    const handlers = listened.mock.calls
      .filter(([name]) => name === APPENDED)
      .map(([, handler]) => handler as (event: unknown) => void);
    expect(handlers.length).toBeGreaterThan(0);
    await act(async () => {
      for (let sequence = 1; sequence <= times; sequence += 1) {
        for (const handler of handlers) {
          handler({ payload: { runId, stream: "source", latestSequence: sequence } });
        }
      }
    });
  }

  const listings = () =>
    invoked.mock.calls.filter(([command]) => command === "list_graduation_queue").length;

  /** Render under fake timers, so a test can step past the coalescing window. */
  async function openWithFakeTimers() {
    vi.useFakeTimers();
    render(<GraduationRuns />);
    await vi.waitFor(() => expect(listings()).toBeGreaterThan(0));
    await vi.waitFor(() =>
      expect(listened.mock.calls.map(([name]) => name)).toContain(APPENDED),
    );
    // The listing is committed once the section stops saying it holds no run;
    // an append before that would find no listed run and prove nothing.
    await vi.waitFor(() => {
      expect(screen.getByTestId("graduation-section")).toBeInTheDocument();
      expect(screen.queryByTestId("graduation-empty")).not.toBeInTheDocument();
    });
  }

  afterEach(() => {
    vi.useRealTimers();
  });

  it("GRU-FR-QKSY, GRU-FR-TQJW: the first append of a stage reloads the listing and the stage becomes activatable", async () => {
    queue = makeQueue([firstTurn("r1")]);
    await open();
    const working = await screen.findByTestId("run-progress-stage-working");
    expect(working).toHaveAttribute("aria-disabled", "true");
    const before = listings();

    // The backend now reports the live index, which names the working stage.
    queue = makeQueue([run("r1")]);
    await append("r1");

    await waitFor(() =>
      expect(screen.getByTestId("run-progress-stage-working")).not.toHaveAttribute(
        "aria-disabled",
      ),
    );
    expect(listings()).toBe(before + 1);
  });

  it("GRU-FR-QKSY, GRD-FR-EFAU: a burst of appends before the stage opens reads the listing once", async () => {
    queue = makeQueue([firstTurn("r1")]);
    await openWithFakeTimers();
    const before = listings();
    await append("r1", 5);
    await vi.advanceTimersByTimeAsync(500);
    expect(listings()).toBe(before + 1);
  });

  it("GRU-FR-QKSY: once the listing names the current stage and pass, appends reload nothing", async () => {
    queue = makeQueue([firstTurn("r1")]);
    await openWithFakeTimers();
    queue = makeQueue([run("r1")]);
    await append("r1");
    await vi.advanceTimersByTimeAsync(500);
    await vi.waitFor(() =>
      expect(screen.getByTestId("run-progress-stage-working")).not.toHaveAttribute(
        "aria-disabled",
      ),
    );
    const settled = listings();

    await append("r1", 3);
    await vi.advanceTimersByTimeAsync(500);
    expect(listings()).toBe(settled);
  });

  it.each([
    ["the run is not in the listing", () => makeQueue([firstTurn("r1")]), "r9"],
    ["the run is terminal", () => makeQueue([firstTurn("r1", { state: "completed" })]), "r1"],
    [
      "the run's progress is at a version this build cannot read",
      () =>
        makeQueue([
          firstTurn("r1", {
            observability: makeObservability({ observabilityVersion: 2 as 1 }),
          }),
        ]),
      "r1",
    ],
    [
      "the run's log storage is at a version this build cannot read",
      () => makeQueue([firstTurn("r1", { logs: makeLogs({ logStorageVersion: 2 }) })]),
      "r1",
    ],
  ])("GRU-FR-QKSY: an append reloads nothing where %s", async (_label, listing, runId) => {
    queue = listing();
    await openWithFakeTimers();
    const before = listings();
    await append(runId, 3);
    await vi.advanceTimersByTimeAsync(500);
    expect(listings()).toBe(before);
  });

  it("GRU-FR-QKSY: unmounting stops listening, and an append that still arrives reads nothing", async () => {
    const offs = new Map<string, ReturnType<typeof vi.fn>>();
    listened.mockImplementation(async (name: string) => {
      const off = vi.fn();
      offs.set(name, off);
      return off;
    });
    queue = makeQueue([firstTurn("r1")]);
    await open();
    await waitFor(() => expect(offs.has(APPENDED)).toBe(true));
    const before = listings();
    cleanup();
    await waitFor(() => expect(offs.get(APPENDED)).toHaveBeenCalled());

    await append("r1");
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(listings()).toBe(before);
  });

  it("GRU-FR-QKSY: a run the section read once to select it is checked against that listing", async () => {
    const without = makeQueue([run("r1")]);
    const withIt = makeQueue([run("r1"), firstTurn("r2")]);
    let reads = 0;
    const base = invoked.getMockImplementation();
    invoked.mockImplementation(async (command: string, args) => {
      if (command === "list_graduation_queue") {
        reads += 1;
        return reads === 1 ? without : withIt;
      }
      return base?.(command, args);
    });
    render(<GraduationRuns selectRun={{ runId: "r2", nonce: 1 }} />);
    await screen.findByTestId("graduation-section");
    // The first listing does not hold r2, so the section reads it once more.
    await waitFor(() => expect(reads).toBe(2));
    await waitFor(() =>
      expect(screen.getAllByTestId("graduation-row").length).toBe(2),
    );

    await append("r2");
    await waitFor(() => expect(reads).toBe(3));
  });

  it("GRU-FR-QKSY: an append heard while a read is in flight is checked against what that read brings", async () => {
    // r1 stands in its working stage, which its indexes already name.
    queue = makeQueue([run("r1")]);
    await open();
    await screen.findByTestId("run-progress-stage-working");

    let release: (value: GraduationQueue) => void = () => {};
    let held = false;
    const base = invoked.getMockImplementation();
    invoked.mockImplementation(async (command: string, args) => {
      if (command === "list_graduation_queue" && !held) {
        held = true;
        return new Promise<GraduationQueue>((resolve) => {
          release = resolve;
        });
      }
      return base?.(command, args);
    });
    const before = listings();

    // The run moves to review. The read this starts answers from before the
    // review turn's first record, and that record is appended while it is out.
    const runChanged = listened.mock.calls
      .filter(([name]) => name === "graduation-run-changed")
      .map(([, handler]) => handler as (event: unknown) => void);
    await act(async () => {
      for (const handler of runChanged) handler({ payload: { runId: "r1", state: "reviewing" } });
    });
    await waitFor(() => expect(listings()).toBe(before + 1));
    await append("r1");

    const atReview = run("r1", {
      state: "reviewing",
      observability: makeObservability({ currentStage: "review" }),
    });
    // The next read reports the live indexes, which name the review stage.
    queue = makeQueue([
      {
        ...atReview,
        logs: makeLogs({
          source: [
            { phaseId: "working", pass: 1 },
            { phaseId: "review", pass: 1 },
          ],
        }),
      },
    ]);
    await act(async () => {
      release(makeQueue([atReview]));
    });

    // The stale answer holds no review segment, so the append is checked again
    // and the listing is read once more.
    await waitFor(() => expect(listings()).toBe(before + 2));
    await waitFor(() =>
      expect(screen.getByTestId("run-progress-stage-review")).not.toHaveAttribute(
        "aria-disabled",
      ),
    );
  });
});
