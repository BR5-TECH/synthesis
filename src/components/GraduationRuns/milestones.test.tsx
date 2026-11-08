/**
 * The milestone list of the selected run
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-WQHM, GRU-FR-NKDU,
 * GRU-FR-TBNE, GRU-FR-CYDK, GRU-FR-JHVX, GRU-FR-ZCTP, GRU-FR-ELDK, GRU-FR-LMTX).
 *
 * The state rules are covered in `../../state/graduation/milestones.test.ts`.
 * What is asserted here is what the list reads, when it reads again, and what it
 * renders.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { GRADUATION_LOG_RECORDS_APPENDED } from "../../events";
import {
  makeActivityEntry,
  makeLogPage,
  makeLogs,
  makeRun,
} from "../../test/graduationFixtures";
import type { GraduationLogPage } from "../../types";
import { GraduationRuns } from ".";
import { resetAppPreferencesCache } from "../../state/appPreferences";
import { forgetEveryDraft } from "../../state/escalationDrafts";
import { forgetEverything } from "../../state/graduationSelection";
import { makeObservability, makeQueue } from "../../test/graduationFixtures";
import { MilestoneList } from "./milestones";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);

const plan = (...titles: string[]) => JSON.stringify({ event: "plan", milestones: titles });
const start = (index: number) => JSON.stringify({ event: "start", index });
const done = (index: number) => JSON.stringify({ event: "done", index });

/** What the backend holds as `milestone` records, ascending, for the next read. */
let stored: string[];
/** Whether the next read fails. */
let failing: boolean;
/** The pages a read answers with when the test sets them, newest first. */
let pages: GraduationLogPage[] | null;

function pageOf(summaries: string[], first = 1): GraduationLogPage {
  return makeLogPage({
    status: summaries.length > 0 ? "available" : "empty",
    entries: summaries.map((summary, at) =>
      makeActivityEntry(first + at, summary, { kind: "milestone" }),
    ),
  });
}

beforeEach(() => {
  stored = [];
  failing = false;
  pages = null;
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    if (command !== "read_graduation_logs") return undefined;
    if (failing) throw new Error("read failed");
    if (pages) return pages.shift();
    return pageOf(stored);
  });
  listened.mockReset();
  listened.mockImplementation(async () => () => {});
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function reads(): Array<Record<string, unknown>> {
  return invoked.mock.calls
    .filter(([command]) => command === "read_graduation_logs")
    .map(([, args]) => args as Record<string, unknown>);
}

/** Tell the list that the activity stream of `runId` grew, as the backend does. */
async function append(runId: string, stream = "activity") {
  const handlers = listened.mock.calls
    .filter(([name]) => name === GRADUATION_LOG_RECORDS_APPENDED)
    .map(([, handler]) => handler as (event: unknown) => void);
  await act(async () => {
    for (const handler of handlers) {
      handler({ payload: { runId, stream, latestSequence: 9 } });
    }
  });
}

function states(): string[] {
  return screen.getAllByTestId("graduation-milestone").map((row) => row.dataset.state ?? "");
}

describe("the milestone list", () => {
  it("GRU-FR-WQHM, GRU-FR-JHVX, GRU-FR-CYDK: a working run shows the milestones of its turn, read by kind", async () => {
    stored = [plan("Read the spec", "Write the code", "Run checks"), start(1), done(1), start(2)];
    render(<MilestoneList run={makeRun("r1", "working", { checkpoint: { pass: 2 } as never })} />);
    const list = await screen.findByTestId("graduation-milestones");
    expect(list).toHaveAccessibleName("Milestones");
    expect(states()).toEqual(["done", "active", "pending"]);
    const rows = within(list).getAllByTestId("graduation-milestone");
    expect(rows[1]).toHaveTextContent("Active");
    expect(rows[1]).toHaveTextContent("Write the code");
    expect(reads()[0]).toMatchObject({
      runId: "r1",
      phaseId: "working",
      pass: { kind: "pass", pass: 2 },
      stream: "activity",
      kinds: ["milestone"],
    });
  });

  it("GRU-FR-NKDU: a turn with no valid plan renders no list", async () => {
    stored = [start(1)];
    render(<MilestoneList run={makeRun("r1", "working")} />);
    await waitFor(() => expect(reads()).toHaveLength(1));
    // The list is absent after a read that completed, not only after one began:
    // a plan that arrives next is what shows the read had been settled.
    stored = [plan("A"), start(1)];
    await append("r1");
    expect(await screen.findByTestId("graduation-milestones")).toBeInTheDocument();
    expect(states()).toEqual(["active"]);
  });

  it("GRU-FR-NKDU, GRU-FR-ELDK: a stream that answers unavailable leaves the list as it was", async () => {
    stored = [plan("A", "B")];
    render(<MilestoneList run={makeRun("r1", "working")} />);
    await screen.findByTestId("graduation-milestones");
    pages = [makeLogPage({ status: "unavailable" })];
    await append("r1");
    await waitFor(() => expect(pages).toHaveLength(0));
    await act(async () => {});
    expect(states()).toEqual(["pending", "pending"]);
    pages = null;
    stored = [plan("A", "B"), done(2)];
    await append("r1");
    await waitFor(() => expect(states()).toEqual(["pending", "done"]));
  });

  it("GRU-FR-ZCTP, GRU-FR-QKSY: an append of the activity stream of the run reads the list again", async () => {
    stored = [plan("A", "B")];
    render(<MilestoneList run={makeRun("r1", "working")} />);
    await screen.findByTestId("graduation-milestones");
    expect(states()).toEqual(["pending", "pending"]);

    stored = [plan("A", "B"), start(1)];
    await append("r1");
    await waitFor(() => expect(states()).toEqual(["active", "pending"]));

    const before = reads().length;
    await append("another-run");
    await append("r1", "structured");
    expect(reads()).toHaveLength(before);
  });

  it("GRU-FR-TBNE, GRU-FR-ZCTP: a new turn starts with an empty list and reads its own phase", async () => {
    stored = [plan("A")];
    const { rerender } = render(<MilestoneList run={makeRun("r1", "working")} />);
    await screen.findByTestId("graduation-milestones");

    stored = [];
    rerender(<MilestoneList run={makeRun("r1", "reviewing")} />);
    expect(screen.queryByTestId("graduation-milestones")).not.toBeInTheDocument();
    await waitFor(() => expect(reads()[reads().length - 1]).toMatchObject({ phaseId: "reviewing" }));
    expect(screen.queryByTestId("graduation-milestones")).not.toBeInTheDocument();

    stored = [plan("Judge the change")];
    await append("r1");
    expect(await screen.findByText("Judge the change")).toBeInTheDocument();
    expect(screen.queryByText("A")).not.toBeInTheDocument();
  });

  it("GRU-FR-TBNE, GRU-FR-FHLE: an ended run shows its last dispatched turn, and what is not done is unknown", async () => {
    stored = [plan("A", "B", "C"), start(1), done(1), start(2)];
    const run = makeRun("r1", "interrupted", {
      logs: makeLogs({
        activity: [
          { phaseId: "working", pass: 1 },
          { phaseId: "reviewing", pass: 1 },
        ],
      }),
    });
    render(<MilestoneList run={run} />);
    await screen.findByTestId("graduation-milestones");
    expect(states()).toEqual(["done", "unknown", "unknown"]);
    expect(reads()[0]).toMatchObject({ phaseId: "reviewing", pass: { kind: "pass", pass: 1 } });
  });

  it("GRU-FR-ZCTP: a terminal run is read once and listens to no append", async () => {
    stored = [plan("A")];
    const run = makeRun("r1", "completed", {
      logs: makeLogs({ activity: [{ phaseId: "reviewing", pass: 1 }] }),
    });
    render(<MilestoneList run={run} />);
    await screen.findByTestId("graduation-milestones");
    expect(
      listened.mock.calls.filter(([name]) => name === GRADUATION_LOG_RECORDS_APPENDED),
    ).toHaveLength(0);
    expect(reads()).toHaveLength(1);
  });

  it("GRU-FR-JHVX: it reads every page of the scope and keeps the order of the records", async () => {
    pages = [
      {
        ...pageOf([start(1), done(1)], 3),
        oldestReached: false,
        olderCursor: { runId: "r1", stream: "activity", direction: "before", sequence: 3 },
      },
      pageOf([plan("A", "B")], 1),
    ];
    render(<MilestoneList run={makeRun("r1", "working")} />);
    await screen.findByTestId("graduation-milestones");
    expect(states()).toEqual(["done", "pending"]);
    expect(reads()[1]).toMatchObject({
      cursor: { direction: "before", sequence: 3 },
      kinds: ["milestone"],
    });
  });

  it("GRU-FR-ELDK: a read that fails, and a record that is no marker, leave the list as it was", async () => {
    stored = [plan("A", "B"), "garbage"];
    render(<MilestoneList run={makeRun("r1", "working")} />);
    await screen.findByTestId("graduation-milestones");
    failing = true;
    const before = reads().length;
    await append("r1");
    await waitFor(() => expect(reads().length).toBe(before + 1));
    expect(states()).toEqual(["pending", "pending"]);
    // The failed read is settled: a later read that works is applied.
    failing = false;
    stored = [plan("A", "B"), "garbage", done(1)];
    await append("r1");
    await waitFor(() => expect(states()).toEqual(["done", "pending"]));
  });

  it("GRU-FR-LMTX: a title is plain text, never markup", async () => {
    stored = [plan("<b>bold</b> & <img src=x onerror=alert(1)>")];
    render(<MilestoneList run={makeRun("r1", "working")} />);
    const row = await screen.findByTestId("graduation-milestone");
    expect(row).toHaveTextContent("<b>bold</b> & <img src=x onerror=alert(1)>");
    expect(row.querySelector("b, img")).toBeNull();
  });
});

describe("the milestone list in the run region", () => {
  it("GRU-FR-WQHM: it stands under the stage row and above the change set and the pass history", async () => {
    stored = [plan("Read the spec", "Write the code"), start(1)];
    const queue = makeQueue([
      makeRun("r1", "working", {
        observability: makeObservability({
          stageHistory: [
            { from: "queued", to: "working", pass: 1, at: "1", reason: "work_started" },
          ],
        }),
        logs: makeLogs({ activity: [{ phaseId: "working", pass: 1 }] }),
      }),
    ]);
    invoked.mockImplementation(async (command: string) => {
      switch (command) {
        case "list_graduation_queue":
          return queue;
        case "load_app_preferences":
          return { graduationRailWidthFraction: 0.2 };
        case "list_work_streams":
          return [];
        case "read_graduation_logs":
          return pageOf(stored);
        default:
          return undefined;
      }
    });
    resetAppPreferencesCache();
    forgetEverything();
    forgetEveryDraft();
    render(<GraduationRuns />);
    const list = await screen.findByTestId("graduation-milestones");
    const stageRow = await screen.findByTestId("run-progress");
    const follows = (a: Node, b: Node) =>
      Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);
    expect(follows(stageRow, list)).toBe(true);
    const columns = document.querySelector(".graduation__columns");
    expect(columns).not.toBeNull();
    expect(follows(list, columns as Node)).toBe(true);
  });
});
