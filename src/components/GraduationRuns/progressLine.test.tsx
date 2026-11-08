/**
 * The progress line of the selected run
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-NPQY, GRU-FR-YMMF,
 * GRU-FR-SOIJ, GRU-FR-QKSY).
 *
 * What is asserted here is what the line reads, when it reads again, what it
 * keeps, and what it renders. The read rules it shares with the milestone list
 * (`useTurnRead`) are asserted here for the line, and in `milestones.test.tsx`
 * for the list.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Profiler } from "react";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { GRADUATION_LOG_RECORDS_APPENDED } from "../../events";
import {
  makeActivityEntry,
  makeLogPage,
  makeLogs,
  makeQueue,
  makeRun,
} from "../../test/graduationFixtures";
import type { GraduationLogPage, GraduationRun } from "../../types";
import { GraduationRuns } from ".";
import { resetAppPreferencesCache } from "../../state/appPreferences";
import { forgetEveryDraft } from "../../state/escalationDrafts";
import { forgetEverything } from "../../state/graduationSelection";
import { resetLogBufferForTest } from "../../logging";
import { ProgressLine } from "./progressLine";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);

/** The newest `message` summary of each run, for the next read. Absent: no record. */
let messages: Record<string, unknown>;
/** The `milestone` records of each run, for the region tests. */
let milestones: Record<string, string[]>;
/** How the next message read answers, where a test sets it. */
let next: (() => Promise<GraduationLogPage>) | null;
/** The queue the section lists. */
let queue = makeQueue([]);

function messagePage(runId: string): GraduationLogPage {
  if (!(runId in messages)) return makeLogPage({ status: "empty", entries: [] });
  const entry = makeActivityEntry(7, "", { kind: "message", run_id: runId });
  (entry.record as Record<string, unknown>).summary = messages[runId];
  return makeLogPage({ status: "available", entries: [entry] });
}

function milestonePage(runId: string): GraduationLogPage {
  const summaries = milestones[runId] ?? [];
  return makeLogPage({
    status: summaries.length > 0 ? "available" : "empty",
    entries: summaries.map((summary, at) =>
      makeActivityEntry(1 + at, summary, { kind: "milestone", run_id: runId }),
    ),
  });
}

beforeEach(() => {
  resetLogBufferForTest();
  resetAppPreferencesCache();
  forgetEverything();
  forgetEveryDraft();
  messages = {};
  milestones = {};
  next = null;
  queue = makeQueue([]);
  invoked.mockReset();
  invoked.mockImplementation((command: string, args?: unknown) => {
    const request = (args ?? {}) as { runId?: string; kinds?: string[] };
    switch (command) {
      case "append_log_records":
        return Promise.resolve(undefined);
      case "list_graduation_queue":
        return Promise.resolve(queue);
      case "load_app_preferences":
        return Promise.resolve({ graduationRailWidthFraction: 0.2 });
      case "list_work_streams":
        return Promise.resolve([]);
      case "read_graduation_logs": {
        const runId = String(request.runId);
        if (request.kinds?.[0] === "message") {
          if (next) {
            const answer = next;
            next = null;
            return answer();
          }
          return Promise.resolve(messagePage(runId));
        }
        if (request.kinds?.[0] === "milestone") return Promise.resolve(milestonePage(runId));
        return Promise.resolve(makeLogPage({ status: "empty", entries: [] }));
      }
      default:
        return Promise.reject(new Error(`unexpected command ${command}`));
    }
  });
  listened.mockReset();
  listened.mockImplementation(async () => () => {});
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function messageReads(): Array<Record<string, unknown>> {
  return invoked.mock.calls
    .filter(([command]) => command === "read_graduation_logs")
    .map(([, args]) => args as Record<string, unknown>)
    .filter((args) => (args.kinds as string[] | undefined)?.[0] === "message");
}

function lastMessageRead(): Record<string, unknown> | undefined {
  const all = messageReads();
  return all[all.length - 1];
}

function appendListens(): number {
  return listened.mock.calls.filter(([name]) => name === GRADUATION_LOG_RECORDS_APPENDED).length;
}

/** Tell the line that a stream of `runId` grew, as the backend does. */
async function append(runId: string, stream = "activity") {
  const handlers = listened.mock.calls
    .filter(([name]) => name === GRADUATION_LOG_RECORDS_APPENDED)
    .map(([, handler]) => handler as (event: unknown) => void);
  await act(async () => {
    for (const handler of handlers) handler({ payload: { runId, stream, latestSequence: 9 } });
  });
}

/** A read the test answers by hand. */
function held(): { promise: Promise<GraduationLogPage>; resolve: (page: GraduationLogPage) => void } {
  let resolve: (page: GraduationLogPage) => void = () => {};
  const promise = new Promise<GraduationLogPage>((r) => (resolve = r));
  return { promise, resolve };
}

const working = (pass = 1, id = "r1"): GraduationRun =>
  makeRun(id, "working", { checkpoint: { pass } as never });

describe("the progress line", () => {
  it("GRU-FR-NPQY, GRU-FR-YMMF: a working run shows the newest message summary, read by kind and limit", async () => {
    messages = { r1: "Reading the spec" };
    render(<ProgressLine run={working(2)} />);
    const line = await screen.findByTestId("graduation-progress");
    expect(line).toHaveAccessibleName("Agent progress");
    expect(line).toHaveTextContent("Reading the spec");
    expect(messageReads()[0]).toMatchObject({
      runId: "r1",
      phaseId: "working",
      pass: { kind: "pass", pass: 2 },
      stream: "activity",
      cursor: null,
      limit: 1,
      kinds: ["message"],
    });
  });

  it("GRU-FR-SOIJ: a muted Now label, the summary as plain text and as the tooltip, and no control", async () => {
    const markup = "<b>x</b> & <img src=x onerror=alert(1)>";
    messages = { r1: markup };
    render(<ProgressLine run={working()} />);
    const line = await screen.findByTestId("graduation-progress");
    expect(within(line).getByText("Now")).toHaveClass("graduation__progress-label");
    const text = within(line).getByText(markup);
    expect(text).toHaveClass("graduation__progress-text");
    expect(text).toHaveAttribute("title", markup);
    expect(line.querySelector("b, img")).toBeNull();
    expect(within(line).queryAllByRole("button")).toHaveLength(0);
    expect(within(line).queryAllByRole("link")).toHaveLength(0);
    expect(line.querySelector("[tabindex]")).toBeNull();
  });

  it.each([
    ["  Writing code \n", "Writing code"],
    ["   ", null],
    [42, null],
    [undefined, null],
  ])("GRU-FR-YMMF, GRU-FR-NPQY: the summary %j shows %j", async (summary, shown) => {
    messages = { r1: summary };
    render(<ProgressLine run={working()} />);
    await waitFor(() => expect(messageReads()).toHaveLength(1));
    if (shown === null) {
      await act(async () => {});
      expect(screen.queryByTestId("graduation-progress")).toBeNull();
    } else {
      const text = within(await screen.findByTestId("graduation-progress")).getByText(shown);
      expect(text).toHaveAttribute("title", shown);
    }
  });

  it("GRU-FR-NPQY, GRU-FR-YMMF: the line shows the newest record of the page", async () => {
    next = () =>
      Promise.resolve(
        makeLogPage({
          status: "available",
          entries: [
            makeActivityEntry(4, "Older step", { kind: "message" }),
            makeActivityEntry(5, "Newer step", { kind: "message" }),
          ],
        }),
      );
    render(<ProgressLine run={working()} />);
    expect(await screen.findByTestId("graduation-progress")).toHaveTextContent("Newer step");
    expect(screen.queryByText("Older step")).toBeNull();
  });

  it("GRU-FR-NPQY: a turn with no message record shows no line until one comes", async () => {
    render(<ProgressLine run={working()} />);
    await waitFor(() => expect(messageReads()).toHaveLength(1));
    await act(async () => {});
    expect(screen.queryByTestId("graduation-progress")).toBeNull();
    messages = { r1: "First step" };
    await append("r1");
    expect(await screen.findByTestId("graduation-progress")).toHaveTextContent("First step");
  });

  it.each(["awaiting_author", "blocked", "interrupted", "completed"] as const)(
    "GRU-FR-NPQY, GRU-FR-FHLE: a run in %s has ended its turn, shows no line, and reads nothing",
    async (state) => {
      messages = { r1: "Old step" };
      render(
        <ProgressLine
          run={makeRun("r1", state, {
            logs: makeLogs({ activity: [{ phaseId: "working", pass: 1 }] }),
          })}
        />,
      );
      await act(async () => {});
      expect(screen.queryByTestId("graduation-progress")).toBeNull();
      expect(messageReads()).toHaveLength(0);
      expect(appendListens()).toBe(0);
    },
  );

  it("GRU-FR-NPQY: a working run with no dispatched turn shows no line and reads nothing", async () => {
    messages = { r1: "Old step" };
    render(<ProgressLine run={working(0)} />);
    await act(async () => {});
    expect(screen.queryByTestId("graduation-progress")).toBeNull();
    expect(messageReads()).toHaveLength(0);
  });

  it("GRU-FR-QKSY, GRU-FR-YMMF: an append of the run's activity stream reads again, other appends do not", async () => {
    messages = { r1: "Step A" };
    render(<ProgressLine run={working()} />);
    await screen.findByText("Step A");
    messages = { r1: "Step B" };
    await append("r1");
    await screen.findByText("Step B");
    const count = messageReads().length;
    await append("other-run");
    await append("r1", "structured");
    expect(messageReads()).toHaveLength(count);
  });

  it("GRU-FR-YMMF: a read that returns the summary the line holds changes nothing on screen", async () => {
    messages = { r1: "Same step" };
    let commits = 0;
    render(
      <Profiler id="progress" onRender={() => (commits += 1)}>
        <ProgressLine run={working()} />
      </Profiler>,
    );
    await screen.findByTestId("graduation-progress");
    const before = commits;
    for (let at = 0; at < 20; at += 1) await append("r1");
    await act(async () => {});
    expect(messageReads().length).toBeGreaterThanOrEqual(2);
    // The Profiler count is the check: equal text keeps the DOM node anyway.
    // React can commit once on an equal state before it bails out; twenty
    // appends must not make twenty commits.
    expect(commits - before).toBeLessThanOrEqual(1);
    messages = { r1: "New step" };
    await append("r1");
    await screen.findByText("New step");
    expect(commits).toBeGreaterThan(before);
  });

  it.each([
    ["a rejected read", () => Promise.reject(new Error("read failed"))],
    ["an unavailable stream", () => Promise.resolve(makeLogPage({ status: "unavailable", entries: [] }))],
    [
      "a persistence failure",
      () => Promise.resolve(makeLogPage({ status: "persistence_failed", entries: [] })),
    ],
  ])("GRU-FR-YMMF: %s leaves the line as it was", async (_name, failure) => {
    messages = { r1: "Step A" };
    render(<ProgressLine run={working()} />);
    await screen.findByText("Step A");
    const count = messageReads().length;
    next = failure as () => Promise<GraduationLogPage>;
    await append("r1");
    await waitFor(() => expect(messageReads()).toHaveLength(count + 1));
    await act(async () => {});
    expect(screen.getByTestId("graduation-progress")).toHaveTextContent("Step A");
    messages = { r1: "Step B" };
    await append("r1");
    await screen.findByText("Step B");
  });

  it("GRU-FR-YMMF, GRU-FR-TBNE: a new turn starts with no line and reads its own phase", async () => {
    messages = { r1: "Working step" };
    const { rerender } = render(<ProgressLine run={working()} />);
    await screen.findByText("Working step");
    messages = {};
    rerender(
      <ProgressLine run={makeRun("r1", "reviewing", { checkpoint: { pass: 1 } as never })} />,
    );
    expect(screen.queryByTestId("graduation-progress")).toBeNull();
    await waitFor(() =>
      expect(lastMessageRead()).toMatchObject({ phaseId: "reviewing", pass: { kind: "pass", pass: 1 } }),
    );
  });

  it("GRU-FR-YMMF, GRU-FR-TBNE: a read of an earlier turn that settles late never shows", async () => {
    const late = held();
    next = () => late.promise;
    const { rerender } = render(<ProgressLine run={working(1)} />);
    await waitFor(() => expect(messageReads()).toHaveLength(1));
    rerender(<ProgressLine run={working(2)} />);
    await waitFor(() => expect(messageReads()).toHaveLength(2));
    await act(async () => {
      late.resolve(
        makeLogPage({
          status: "available",
          entries: [makeActivityEntry(3, "Stale", { kind: "message" })],
        }),
      );
    });
    expect(screen.queryByText("Stale")).toBeNull();
  });

  it("GRU-FR-YMMF, GRU-FR-TBNE: a new pass with the same summary still shows its line", async () => {
    messages = { r1: "Same step" };
    const { rerender } = render(<ProgressLine run={working(1)} />);
    await screen.findByText("Same step");
    rerender(<ProgressLine run={working(2)} />);
    await waitFor(() => expect(lastMessageRead()).toMatchObject({ pass: { kind: "pass", pass: 2 } }));
    expect(await screen.findByTestId("graduation-progress")).toHaveTextContent("Same step");
  });

  it("GRU-FR-YMMF: one read runs at a time, and appends during it ask for one more", async () => {
    const first = held();
    next = () => first.promise;
    render(<ProgressLine run={working()} />);
    await waitFor(() => expect(messageReads()).toHaveLength(1));
    messages = { r1: "Second answer" };
    await append("r1");
    await append("r1");
    await append("r1");
    expect(messageReads()).toHaveLength(1);
    await act(async () => {
      first.resolve(
        makeLogPage({
          status: "available",
          entries: [makeActivityEntry(1, "First answer", { kind: "message" })],
        }),
      );
    });
    await screen.findByText("Second answer");
    expect(messageReads()).toHaveLength(2);
  });

  it("GRU-FR-NPQY, GRU-FR-FHLE: the line goes away when the turn ends, and reads no more", async () => {
    messages = { r1: "Last step" };
    const run = working();
    const { rerender } = render(<ProgressLine run={run} />);
    await screen.findByText("Last step");
    rerender(<ProgressLine run={{ ...run, state: "awaiting_author" }} />);
    expect(screen.queryByTestId("graduation-progress")).toBeNull();
    const count = messageReads().length;
    await append("r1");
    expect(messageReads()).toHaveLength(count);
  });

  it("GRU-FR-YMMF: the line stops listening when it unmounts, also before the listener is ready", async () => {
    const off = vi.fn();
    listened.mockImplementation(async () => off);
    messages = { r1: "Step" };
    const first = render(<ProgressLine run={working()} />);
    await screen.findByText("Step");
    first.unmount();
    expect(off).toHaveBeenCalled();

    const lateOff = vi.fn();
    let ready: (value: () => void) => void = () => {};
    listened.mockImplementation(() => new Promise((r) => (ready = r)));
    const second = render(<ProgressLine run={working()} />);
    second.unmount();
    await act(async () => ready(lateOff));
    expect(lateOff).toHaveBeenCalled();
  });
});

describe("the progress line in the run region", () => {
  const plan = JSON.stringify({ event: "plan", milestones: ["Read the instructions"] });
  const turn = { logs: makeLogs({ activity: [{ phaseId: "working", pass: 1 }] }), checkpoint: { pass: 1 } as never };

  function mount(runs: GraduationRun[]) {
    queue = makeQueue(runs);
    render(<GraduationRuns />);
  }

  const row = (id: string) =>
    screen
      .getAllByTestId("graduation-row")
      .find((r) => r.getAttribute("aria-label")?.endsWith(id))!;

  it("GRU-FR-NPQY, GRU-FR-SOIJ, GRU-FR-AFSW: one line at most, under the list, of the selected run alone", async () => {
    messages = { r1: "r1 now", r2: "r2 now" };
    milestones = { r1: [plan] };
    mount([makeRun("r1", "working", turn), makeRun("r2", "working", turn), makeRun("r3", "queued")]);
    const region = await screen.findByRole("region", { name: "The selected run" });
    await waitFor(() => expect(region.dataset.run).toBe("r1"));
    const list = await screen.findByTestId("graduation-milestones");
    const line = await screen.findByText("r1 now");
    expect(list.compareDocumentPosition(line) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    // GRU-FR-QKSY: an append refreshes the line and reloads no listing.
    const listings = () =>
      invoked.mock.calls.filter(([command]) => command === "list_graduation_queue").length;
    const listed = listings();
    await append("r1");
    await append("r1");
    expect(listings()).toBe(listed);

    for (const id of ["r2", "r1", "r3", "r2"]) {
      await userEvent.click(row(id));
      await waitFor(() => expect(region.dataset.run).toBe(id));
      await append("r1");
      await append("r2");
      if (id === "r3") {
        await act(async () => {});
        expect(screen.queryAllByTestId("graduation-progress")).toHaveLength(0);
        expect(screen.queryByText("r1 now")).toBeNull();
        expect(screen.queryByText("r2 now")).toBeNull();
        continue;
      }
      await screen.findByText(`${id} now`);
      expect(screen.getAllByTestId("graduation-progress")).toHaveLength(1);
      if (id === "r2") {
        // GRU-FR-NPQY: the line shows also where no plan stands.
        expect(screen.queryByTestId("graduation-milestones")).toBeNull();
        expect(screen.queryByText("r1 now")).toBeNull();
      }
    }
  });
});
