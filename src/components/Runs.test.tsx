import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/**
 * The event the panel follows, held so a test can deliver one the way the
 * backend does — the panel re-reads on it rather than rendering it, which is the
 * behaviour most of this file is about.
 */
let appended: ((state: unknown) => void) | null = null;
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: (e: { payload: unknown }) => void) => {
    if (name === "agent-activity-appended") {
      appended = (state) => handler({ payload: state });
    }
    return () => {
      appended = null;
    };
  }),
}));

import { Runs } from "./Runs";
import type { AgentActivityPage, AgentActivityRecord } from "../types";

/**
 * The agent-output section against a real activity stream
 * (`RUN-runs.md` RUN-FR-02, RUN-FR-03, RUN-FR-04, RUN-FR-08, RUN-FR-12,
 * RUN-FR-13, RUN-FR-14).
 *
 * What is pinned here is the part a canned stream could never have covered: the
 * panel reads its own records, follows one run rather than whatever arrives,
 * asks for a delta rather than the whole stream every time, and shows the
 * vendor's own bytes beside its reading of them.
 */

function record(over: Partial<AgentActivityRecord> & { seq: number }): AgentActivityRecord {
  return {
    at: "2026-08-16T10:00:00Z",
    channel: "stdout",
    kind: "message",
    summary: `line ${over.seq}`,
    payload: `{"seq":${over.seq}}`,
    payloadTruncated: false,
    ...over,
  };
}

function page(records: AgentActivityRecord[], over: Partial<AgentActivityPage> = {}): AgentActivityPage {
  return {
    runId: "run-1",
    records,
    total: records.length,
    dropped: 0,
    latestSeq: records.length === 0 ? 0 : records[records.length - 1].seq,
    ...over,
  };
}

/** Every call the panel made to `read_agent_activity`, in order. */
function reads(): Array<{ runId: string; after?: number }> {
  return invokeMock.mock.calls
    .filter(([name]) => name === "read_agent_activity")
    .map(([, args]) => args as { runId: string; after?: number });
}

function rows(): string[] {
  return Array.from(document.querySelectorAll(".runs-line__expand")).map(
    (el) => (el.textContent ?? "").trim(),
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  appended = null;
});
afterEach(cleanup);

describe("Runs agent output", () => {
  it("RUN-FR-09, RUN-FR-05, RUN-FR-10: sits at its empty state with no run, and asks the backend nothing", async () => {
    render(<Runs runId={null} live={false} />);

    expect(screen.getByText("No run")).toBeInTheDocument();
    expect(reads()).toHaveLength(0);
    // RUN-FR-09: nothing here starts anything.
    expect(screen.queryByRole("button", { name: /Re-run/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Start/ })).toBeNull();
  });

  it("RUN-FR-03: backfills the run it is given and renders each record in order", async () => {
    invokeMock.mockResolvedValue(page([record({ seq: 1 }), record({ seq: 2 })]));

    render(<Runs runId="run-1" live={true} />);

    await waitFor(() => expect(rows()).toHaveLength(2));
    expect(rows()).toEqual(["line 1", "line 2"]);
    // The first read carries no cursor: a panel opening on a run that has been
    // working for an hour takes the newest page rather than paging to it.
    expect(reads()[0]).toEqual({ runId: "run-1", after: undefined, limit: undefined });
    // RUN-FR-04: the indicator is the run's state, not this surface's.
    expect(screen.getByText("running")).toBeInTheDocument();
    expect(screen.queryByText("finished")).toBeNull();
  });

  it("RUN-FR-13: asks only for what followed the cursor when the run grows", async () => {
    invokeMock.mockResolvedValueOnce(page([record({ seq: 1 }), record({ seq: 2 })]));
    render(<Runs runId="run-1" live={true} />);
    await waitFor(() => expect(rows()).toHaveLength(2));

    invokeMock.mockResolvedValueOnce(
      page([record({ seq: 3, summary: "the third" })], { latestSeq: 3, total: 3 }),
    );
    appended?.({ runId: "run-1", total: 3, dropped: 0, latestSeq: 3 });

    await waitFor(() => expect(rows()).toHaveLength(3));
    expect(rows()[2]).toBe("the third");
    // The delta, not the whole stream again.
    expect(reads()[1]).toEqual({ runId: "run-1", after: 2, limit: undefined });
  });

  it("RUN-FR-02: ignores an event about a different run", async () => {
    invokeMock.mockResolvedValue(page([record({ seq: 1 })]));
    render(<Runs runId="run-1" live={true} />);
    await waitFor(() => expect(rows()).toHaveLength(1));

    appended?.({ runId: "run-2", total: 99, dropped: 0, latestSeq: 99 });

    // One read, from the mount. Another run's activity never reaches this one.
    await waitFor(() => expect(reads()).toHaveLength(1));
    expect(rows()).toEqual(["line 1"]);
  });

  it("reads on a duplicate event and shows nothing twice for it", async () => {
    invokeMock.mockResolvedValueOnce(page([record({ seq: 1 }), record({ seq: 2 })]));
    render(<Runs runId="run-1" live={true} />);
    await waitFor(() => expect(rows()).toHaveLength(2));

    // A duplicate or late event does cost one read — the panel cannot tell it
    // apart from the announcement it needs to act on, and a comparison that
    // could tell them apart is exactly what breaks when two threads announce out
    // of order. What it must never do is show anything twice.
    invokeMock.mockResolvedValueOnce(page([], { total: 2, latestSeq: 2 }));
    appended?.({ runId: "run-1", total: 2, dropped: 0, latestSeq: 2 });

    await waitFor(() => expect(reads()).toHaveLength(2));
    expect(reads()[1]).toEqual({ runId: "run-1", after: 2, limit: undefined });
    expect(rows()).toEqual(["line 1", "line 2"]);
  });

  it("RUN-FR-13: drops everything it holds when the run changes", async () => {
    invokeMock.mockResolvedValueOnce(page([record({ seq: 1, summary: "first run" })]));
    const view = render(<Runs runId="run-1" live={true} />);
    await waitFor(() => expect(rows()).toEqual(["first run"]));

    invokeMock.mockResolvedValueOnce(
      page([record({ seq: 1, summary: "second run" })], { runId: "run-2" }),
    );
    view.rerender(<Runs runId="run-2" live={true} />);

    await waitFor(() => expect(rows()).toEqual(["second run"]));
    // And the second run's read starts from no cursor, rather than from where
    // the first run had got to — sequences are per run.
    expect(reads()[1]).toEqual({ runId: "run-2", after: undefined, limit: undefined });
  });

  it("RUN-FR-14: shows the vendor's own event beneath a row on demand", async () => {
    invokeMock.mockResolvedValue(
      page([
        record({
          seq: 1,
          kind: "tool_call",
          summary: "Read(/workspace/spec.md)",
          payload: '{"type":"assistant","message":{"content":[{"type":"tool_use"}]}}',
        }),
      ]),
    );
    render(<Runs runId="run-1" live={true} />);
    await waitFor(() => expect(rows()).toHaveLength(1));

    expect(document.querySelector(".runs-payload")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Read(/workspace/spec.md)" }));

    const payload = document.querySelector(".runs-payload");
    expect(payload?.textContent).toContain('"type":"assistant"');
    // The kind is on the row, so a stream reads without opening anything.
    expect(screen.getByText("tool call")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Read(/workspace/spec.md)" }));
    expect(document.querySelector(".runs-payload")).toBeNull();
  });

  it("says so when an event was cut, rather than showing a prefix as the whole", async () => {
    invokeMock.mockResolvedValue(
      page([record({ seq: 1, payload: "the first half", payloadTruncated: true })]),
    );
    render(<Runs runId="run-1" live={true} />);
    await waitFor(() => expect(rows()).toHaveLength(1));

    await userEvent.click(screen.getByRole("button", { name: "line 1" }));
    expect(document.querySelector(".runs-payload")?.textContent).toContain(
      "event truncated",
    );
  });

  it("AGV-FR-05: says at its head when older records are no longer held", async () => {
    invokeMock.mockResolvedValue(
      page([record({ seq: 51 })], { dropped: 50, total: 51, latestSeq: 51 }),
    );

    render(<Runs runId="run-1" live={true} />);

    await waitFor(() =>
      expect(screen.getByRole("note").textContent).toContain("50 earlier events"),
    );
  });

  it("RUN-FR-08: clears what it is showing without touching the run's record", async () => {
    invokeMock.mockResolvedValue(page([record({ seq: 1 }), record({ seq: 2 })]));
    render(<Runs runId="run-1" live={false} />);
    await waitFor(() => expect(rows()).toHaveLength(2));

    await userEvent.click(screen.getByRole("button", { name: "Clear" }));

    expect(rows()).toHaveLength(0);
    // The run is still the run — clearing hid the output, it did not discard it,
    // and nothing here offers to start another.
    expect(screen.getByText("finished")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Re-run/ })).toBeNull();
    // No delete, no discard: this panel decides nothing about a run.
    expect(
      invokeMock.mock.calls.filter(([name]) => name !== "read_agent_activity"),
    ).toHaveLength(0);
  });

  it("waits visibly rather than looking finished while a live run is silent", async () => {
    invokeMock.mockResolvedValue(page([]));

    render(<Runs runId="run-1" live={true} />);

    await waitFor(() =>
      expect(screen.getByText(/Waiting for the agent's first line/)).toBeInTheDocument(),
    );
    // The distinction that matters at 03:00: a run that has said nothing yet is
    // not a run that said nothing.
    expect(screen.getByText("running")).toBeInTheDocument();
  });

  it("survives a backend that will not answer", async () => {
    invokeMock.mockRejectedValue(new Error("no such run"));

    render(<Runs runId="run-1" live={true} />);

    await waitFor(() => expect(reads()).toHaveLength(1));
    // Rendered, empty, and not thrown out of: a panel that crashes on a read is
    // worse than one that shows nothing.
    expect(screen.getByText("running")).toBeInTheDocument();
    expect(rows()).toHaveLength(0);
  });

  it("RUN-FR-12: releases the follow pin when the reader scrolls up", async () => {
    invokeMock.mockResolvedValue(page([record({ seq: 1 }), record({ seq: 2 })]));
    render(<Runs runId="run-1" live={true} />);
    await waitFor(() => expect(rows()).toHaveLength(2));

    const log = document.querySelector(".runs-log") as HTMLElement;
    expect(screen.queryByRole("button", { name: "Follow" })).toBeNull();

    // jsdom reports zero for every layout measurement, so the geometry a real
    // scroll would produce is set explicitly. What is asserted is the panel's
    // reading of it, which is the part that is this component's.
    Object.defineProperty(log, "scrollHeight", { value: 1000, configurable: true });
    Object.defineProperty(log, "clientHeight", { value: 200, configurable: true });
    log.scrollTop = 0;
    log.dispatchEvent(new Event("scroll"));

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Follow" })).toBeInTheDocument(),
    );

    // And resuming it is one control away.
    await userEvent.click(screen.getByRole("button", { name: "Follow" }));
    expect(screen.queryByRole("button", { name: "Follow" })).toBeNull();
  });

  it("names the run it is showing, so two runs are never confused", async () => {
    invokeMock.mockResolvedValue(page([record({ seq: 1 })]));

    render(<Runs runId="run-1" live={true} label="Propose prompt changes" />);

    await waitFor(() => expect(rows()).toHaveLength(1));
    const head = document.querySelector(".runs-head") as HTMLElement;
    expect(within(head).getByText("Propose prompt changes")).toBeInTheDocument();
  });

  it("RUN-FR-13: never merges the previous run's answer into the run now showing", async () => {
    // The read for run-a is left hanging until after the switch — which is the
    // ordinary case rather than a provoked race, because the graduation queue
    // moves on its own while a read is in flight.
    let answerA: (page: AgentActivityPage) => void = () => {};
    const inFlight = new Promise<AgentActivityPage>((resolve) => {
      answerA = resolve;
    });
    invokeMock.mockReturnValueOnce(inFlight);

    const view = render(<Runs runId="run-a" live={true} />);
    await waitFor(() => expect(reads()).toHaveLength(1));

    invokeMock.mockResolvedValue(
      page([record({ seq: 1, summary: "B's own line" })], { runId: "run-b" }),
    );
    view.rerender(<Runs runId="run-b" live={true} />);

    // Now the previous run answers, carrying records and a cursor far ahead of
    // anything run-b will ever have.
    answerA(
      page([record({ seq: 900, summary: "A's line" })], {
        runId: "run-a",
        latestSeq: 900,
      }),
    );

    await waitFor(() => expect(rows()).toEqual(["B's own line"]));
    expect(rows()).not.toContain("A's line");
    // And run-b was actually read, from its own beginning.
    expect(reads().some((r) => r.runId === "run-b" && r.after === undefined)).toBe(
      true,
    );

    // The cursor is run-b's, so run-b's own events still reach it. Had run-a's
    // cursor been adopted, every sequence run-b ever issues would read as older
    // than what the panel believes it holds, and the panel would go silent for
    // the rest of the run.
    invokeMock.mockResolvedValue(
      page([record({ seq: 2, summary: "B's next line" })], {
        runId: "run-b",
        latestSeq: 2,
      }),
    );
    appended?.({ runId: "run-b", total: 2, dropped: 0, latestSeq: 2 });
    await waitFor(() => expect(rows()).toContain("B's next line"));
  });

  it("collapses a burst into one read and one trailing read", async () => {
    let answer: (page: AgentActivityPage) => void = () => {};
    invokeMock.mockReturnValueOnce(
      new Promise<AgentActivityPage>((resolve) => {
        answer = resolve;
      }),
    );

    render(<Runs runId="run-1" live={true} />);
    await waitFor(() => expect(reads()).toHaveLength(1));

    // Three events while the first read is still in flight.
    invokeMock.mockResolvedValue(page([record({ seq: 2 })], { latestSeq: 2 }));
    appended?.({ runId: "run-1", total: 1, dropped: 0, latestSeq: 1 });
    appended?.({ runId: "run-1", total: 2, dropped: 0, latestSeq: 2 });
    appended?.({ runId: "run-1", total: 3, dropped: 0, latestSeq: 3 });
    expect(reads()).toHaveLength(1);

    answer(page([record({ seq: 1 })], { latestSeq: 1 }));

    // Exactly one trailing read, not three — and not none, which would leave the
    // last event of a burst as the one nobody asked about.
    await waitFor(() => expect(reads()).toHaveLength(2));
    expect(reads()[1]).toEqual({ runId: "run-1", after: 1, limit: undefined });
    await waitFor(() => expect(rows()).toEqual(["line 1", "line 2"]));
  });

  it("reads on any event for its run, whatever sequence the event carries", async () => {
    invokeMock.mockResolvedValueOnce(page([record({ seq: 5 })], { latestSeq: 5 }));
    render(<Runs runId="run-1" live={true} />);
    await waitFor(() => expect(rows()).toHaveLength(1));

    // Two threads record at once, so an event can announce a sequence below one
    // already announced. A panel that skipped on that comparison would skip the
    // read that was going to catch up, and would then never read again.
    invokeMock.mockResolvedValueOnce(
      page([record({ seq: 6, summary: "the one that follows" })], { latestSeq: 6 }),
    );
    appended?.({ runId: "run-1", total: 6, dropped: 0, latestSeq: 3 });

    await waitFor(() => expect(rows()).toContain("the one that follows"));
  });

  it("holds no record twice when a read overlaps what it already has", async () => {
    invokeMock.mockResolvedValueOnce(
      page([record({ seq: 1 }), record({ seq: 2 }), record({ seq: 3 })]),
    );
    render(<Runs runId="run-1" live={true} />);
    await waitFor(() => expect(rows()).toHaveLength(3));

    // A page that overlaps what is held — the normal outcome of a re-read
    // racing an append.
    invokeMock.mockResolvedValueOnce(
      page(
        [record({ seq: 2 }), record({ seq: 3 }), record({ seq: 4 }), record({ seq: 5 })],
        { latestSeq: 5 },
      ),
    );
    appended?.({ runId: "run-1", total: 5, dropped: 0, latestSeq: 5 });

    await waitFor(() => expect(rows()).toHaveLength(5));
    expect(rows()).toEqual(["line 1", "line 2", "line 3", "line 4", "line 5"]);
  });

  it("RUN-FR-08: does not claim a cleared run recorded nothing", async () => {
    invokeMock.mockResolvedValue(page([record({ seq: 1 }), record({ seq: 2 })]));
    render(<Runs runId="run-1" live={false} />);
    await waitFor(() => expect(rows()).toHaveLength(2));

    await userEvent.click(screen.getByRole("button", { name: "Clear" }));

    // The run recorded plenty; the reader dismissed it. Saying otherwise is the
    // panel reporting its own state as the run's.
    expect(screen.queryByText(/recorded no output/)).toBeNull();
    expect(screen.getByText(/Output cleared/)).toBeInTheDocument();
  });

  it("RUN-FR-11: renders each record's own time rather than the time it arrived", async () => {
    const at = "2026-08-16T09:41:07Z";
    invokeMock.mockResolvedValue(page([record({ seq: 1, at })]));

    render(<Runs runId="run-1" live={true} />);

    await waitFor(() => expect(rows()).toHaveLength(1));
    const stamp = document.querySelector(".runs-line__ts")?.textContent ?? "";
    // The local rendering of that instant, whatever zone the machine is in —
    // asserted against the same conversion rather than a fixed string, so the
    // test says "this instant" rather than "this time zone".
    const expected = new Date(at).toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
    expect(stamp).toBe(expected);
  });

  it("renders a record whose time the backend could not have written", async () => {
    invokeMock.mockResolvedValue(page([record({ seq: 1, at: "not a time" })]));

    render(<Runs runId="run-1" live={true} />);

    // The row still renders: an unreadable timestamp costs the stamp, not the
    // line the reader is here for.
    await waitFor(() => expect(rows()).toEqual(["line 1"]));
    expect(document.querySelector(".runs-line__ts")?.textContent).toBe("");
  });
});
