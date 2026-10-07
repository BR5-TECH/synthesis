import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  makeActivityEntry,
  makeLogPage,
  makeLogs,
  makeObservability,
} from "../../test/graduationFixtures";
import {
  WORKING,
  draw,
  entryNames,
  installLogBackend,
  workingRun,
  type LogBackend,
} from "../../test/graduationLogWindowHarness";
import type { GraduationLogPage } from "../../types";
import type { GraduationObservability } from "../../types/graduationObservability";
import { SEARCH_DEBOUNCE_MS } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const RUN_LEVEL = "Run-level agent activity of this phase";

let backend: LogBackend;

beforeEach(() => {
  backend = installLogBackend();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("the graduation log window: identity and scope", () => {
  it("GLW-FR-OMZA, GLW-FR-BLWH: it names its run and its phase, and holds no run selector", async () => {
    draw();
    expect(await screen.findByTestId("glw-title")).toHaveTextContent("Run r1 · Working");
    expect(
      screen.getByRole("dialog", {
        name: /Agent activity of Run r1, phase Working/,
      }),
    ).toBeInTheDocument();
    const overlay = screen.getByTestId("graduation-log-window");
    expect(within(overlay).getAllByRole("button").map((b) => b.textContent)).toEqual([
      "",
      "Pass 1",
      "Pass 2",
      "Run-level",
      "Following",
    ]);
    expect(within(overlay).queryAllByRole("combobox")).toHaveLength(0);
    expect(within(overlay).queryAllByRole("listbox")).toHaveLength(0);
    expect(overlay.textContent).not.toMatch(/Run r2/);
  });

  it("GLW-FR-MZUP: the phase is the descriptor the row passed, whatever its id", async () => {
    draw({
      stage: { id: "zeta", label: "Zeta" },
      run: workingRun({
        observability: makeObservability({
          stageHistory: [
            {
              from: "eta",
              to: "zeta",
              pass: 1,
              at: "1",
              reason: "work_started",
            } as unknown as GraduationObservability["stageHistory"][number],
          ],
        }),
        logs: makeLogs({ activity: [{ phaseId: "zeta", pass: 1 }] }),
      }),
    });
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    expect(backend.reads()[0].phaseId).toBe("zeta");
    expect(screen.getByTestId("glw-title")).toHaveTextContent("Zeta");
  });

  it("GLW-FR-FPUX: it carries no stream toggle and reads no structured stream", async () => {
    draw();
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    expect(screen.queryByRole("button", { name: /^(Source|Structured)$/ })).toBeNull();
    expect(screen.queryByTestId("glw-stream-toggle")).toBeNull();
    expect(screen.queryByRole("group", { name: /stream/i })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Pass 1" }));
    await userEvent.click(screen.getByRole("button", { name: RUN_LEVEL }));
    await waitFor(() => expect(backend.reads()).toHaveLength(3));
    for (const read of backend.reads()) expect(read.stream).toBe("activity");
  });

  it("GLW-FR-HCOI, GLW-FR-FCVA, GLW-FR-DKWB: every read and every search names the run, the phase, the scope, and the activity stream", async () => {
    draw();
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    await userEvent.click(screen.getByRole("button", { name: "Pass 1" }));
    await userEvent.type(screen.getByTestId("glw-search"), "gate");
    await waitFor(() => expect(backend.reads().some((read) => read.query)).toBe(true));
    expect(backend.reads().length).toBeGreaterThanOrEqual(3);
    for (const read of backend.reads()) {
      expect(read.runId).toBe("r1");
      expect(read.phaseId).toBe("working");
      expect(read.stream).toBe("activity");
      expect(read.pass).toBeTruthy();
    }
  });

  it("GLW-FR-ELJO: it opens on the newest pass whose own records the activity index holds", async () => {
    draw();
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    expect(backend.reads()[0]).toMatchObject({
      runId: "r1",
      phaseId: "working",
      stream: "activity",
      pass: { kind: "pass", pass: 2 },
    });
  });

  it("GLW-FR-ELJO: the structured index counts for nothing in the opening choice", async () => {
    draw({
      run: workingRun({
        logs: makeLogs({
          activity: [{ phaseId: "working", pass: 1 }],
          structured: [{ phaseId: "working", pass: 2 }],
        }),
      }),
    });
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    expect(backend.reads()[0].pass).toEqual({ kind: "pass", pass: 1 });
  });

  it("GLW-FR-CKLZ, GLW-FR-DDXJ, GLW-FR-EUTW: one entry per pass, and a run-level entry labelled in words", async () => {
    draw();
    const list = await screen.findByTestId("glw-pass-list");
    const names = entryNames(list);
    expect(names).toEqual(["Pass 1", "Pass 2", RUN_LEVEL]);
    expect(new Set(names).size).toBe(names.length);
    const labels = within(list).getAllByRole("button").map((b) => b.textContent);
    expect(labels).toEqual(["Pass 1", "Pass 2", "Run-level"]);
  });

  it("GLW-FR-RVKT: a run-level segment of the structured index alone adds no run-level entry", async () => {
    draw({
      run: workingRun({
        logs: makeLogs({
          activity: [
            { phaseId: "working", pass: 1 },
            { phaseId: "working", pass: 2 },
          ],
          structured: [{ phaseId: "working", pass: null }],
        }),
      }),
    });
    const list = await screen.findByTestId("glw-pass-list");
    expect(entryNames(list)).toEqual(["Pass 1", "Pass 2"]);
  });

  it("GLW-FR-WBTE: a pass that wrote nothing is still listed and still selectable", async () => {
    draw({
      run: workingRun({ logs: makeLogs({ activity: [{ phaseId: "working", pass: 1 }] }) }),
    });
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    expect(entryNames(screen.getByTestId("glw-pass-list"))).toEqual(["Pass 1", "Pass 2"]);
    await userEvent.click(screen.getByRole("button", { name: "Pass 2" }));
    await waitFor(() => expect(backend.reads()).toHaveLength(2));
    expect(backend.reads()[1].pass).toEqual({ kind: "pass", pass: 2 });
  });

  it("GLW-FR-YVKD, GLW-FR-ELJO: a phase holding run-level records alone opens on the run-level entry", async () => {
    draw({
      stage: { id: "done", label: "Done" },
      run: workingRun({
        observability: makeObservability({
          stageHistory: [{ from: "review", to: "done", pass: 2, at: "1", reason: "finished" }],
        }),
        logs: makeLogs({ activity: [{ phaseId: "done", pass: null }] }),
      }),
    });
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    expect(backend.reads()[0].pass).toEqual({ kind: "run_level" });
  });

  it("GLW-FR-PDGA, GLW-FR-CQXJ: selecting the run-level entry reads the run-level scope, never the phase scope", async () => {
    draw();
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    await userEvent.click(screen.getByRole("button", { name: RUN_LEVEL }));
    await waitFor(() => expect(backend.reads()).toHaveLength(2));
    expect(backend.reads()[1].pass).toEqual({ kind: "run_level" });
    for (const read of backend.reads()) {
      expect((read.pass as { kind: string }).kind).not.toBe("phase");
    }
  });

  it("GLW-FR-DDXJ, GLW-FR-OMZA: the viewport names the phase and the selected entry in words", async () => {
    draw();
    const viewport = await screen.findByTestId("glw-viewport");
    expect(viewport).toHaveAccessibleName("Working agent activity, Pass 2");
    await userEvent.click(screen.getByRole("button", { name: RUN_LEVEL }));
    await waitFor(() =>
      expect(screen.getByTestId("glw-viewport")).toHaveAccessibleName(
        "Working agent activity, Run-level",
      ),
    );
  });

  it("GLW-FR-CJBE, GLW-FR-THAX: opening one phase rewrites nothing, and every other phase still opens", async () => {
    const target = workingRun({
      observability: makeObservability({
        stageHistory: [
          { from: "queued", to: "queued", pass: 1, at: "0", reason: "enqueued" },
          { from: "queued", to: "working", pass: 1, at: "1", reason: "work_started" },
        ],
      }),
      logs: makeLogs({
        activity: [
          { phaseId: "queued", pass: null },
          { phaseId: "working", pass: 1 },
        ],
      }),
    });
    const before = JSON.stringify(target);
    draw({ run: target, stage: WORKING });
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    expect(JSON.stringify(target)).toBe(before);
    cleanup();
    draw({ run: target, stage: { id: "queued", label: "Queued" } });
    await waitFor(() => expect(backend.reads()).toHaveLength(2));
    expect(backend.reads()[1].phaseId).toBe("queued");
    expect(backend.reads()[1].pass).toEqual({ kind: "run_level" });
  });
});

describe("the graduation log window: search", () => {
  it("GLW-FR-INLR: the query is sent as typed, and the window offers no regular-expression mode", async () => {
    draw();
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    await userEvent.type(screen.getByTestId("glw-search"), "GaTe");
    await waitFor(() => expect(backend.reads().some((read) => read.query === "GaTe")).toBe(true));
    expect(screen.queryByRole("button", { name: /regular expression|regex/i })).toBeNull();
    expect(screen.queryByRole("checkbox")).toBeNull();
  });

  it("GLW-FR-IMKM: a typed query is sent once after the debounce and not before it", async () => {
    vi.useFakeTimers();
    draw();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(backend.reads()).toHaveLength(1);
    const search = screen.getByTestId("glw-search");
    fireEvent.change(search, { target: { value: "a" } });
    fireEvent.change(search, { target: { value: "ab" } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS - 1);
    });
    expect(backend.reads()).toHaveLength(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    const searched = backend.reads().filter((read) => read.query);
    expect(searched).toHaveLength(1);
    expect(searched[0].query).toBe("ab");
  });

  it("GLW-FR-IMKM, GLW-FR-KBZE: a search names the scope and states the whole scope's count", async () => {
    backend.answer = (args) =>
      makeLogPage({
        status: "available",
        entries: args.query ? [makeActivityEntry(1, "escalate_to_user")] : [],
        matchedTotal: args.query ? 4 : 0,
        search: args.query ? "matched" : "not_requested",
      });
    draw();
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    await userEvent.type(screen.getByTestId("glw-search"), "escalate");
    await waitFor(() =>
      expect(screen.getByTestId("glw-match-count")).toHaveTextContent("4 matches"),
    );
    const last = backend.reads()[backend.reads().length - 1];
    expect(last).toMatchObject({
      query: "escalate",
      stream: "activity",
      pass: { kind: "pass", pass: 2 },
    });
    // The count is the whole scope's, though the page holds one row.
    expect(screen.getAllByTestId("glw-row")).toHaveLength(1);
  });

  it("GLW-FR-KBZE: a single match reads as one match", async () => {
    backend.answer = (args) =>
      makeLogPage({
        status: "available",
        entries: args.query ? [makeActivityEntry(1, "only one")] : [],
        matchedTotal: args.query ? 1 : 0,
        search: args.query ? "matched" : "not_requested",
      });
    draw();
    await userEvent.type(screen.getByTestId("glw-search"), "only");
    await waitFor(() =>
      expect(screen.getByTestId("glw-match-count")).toHaveTextContent("1 match"),
    );
    expect(screen.getByTestId("glw-match-count").textContent).not.toMatch(/matches/);
  });

  it("GLW-FR-KEXN: clearing the search returns the viewport to the unfiltered rows", async () => {
    backend.answer = (args) =>
      makeLogPage({
        status: "available",
        entries: args.query
          ? [makeActivityEntry(2, "matched row")]
          : [makeActivityEntry(1, "every row")],
        matchedTotal: args.query ? 1 : 0,
        search: args.query ? "matched" : "not_requested",
      });
    draw();
    await screen.findByText("every row");
    await userEvent.type(screen.getByTestId("glw-search"), "matched");
    await screen.findByText("matched row");
    await userEvent.clear(screen.getByTestId("glw-search"));
    await waitFor(() => expect(screen.getByText("every row")).toBeInTheDocument());
    expect(screen.queryByTestId("glw-match-count")).not.toBeInTheDocument();
  });

  it("GLW-FR-IMKM, GLW-FR-HGXL: search is for the selected scope, and the window matches nothing itself", async () => {
    // A backend that answers with a row whose summary lacks the query. The
    // window shows what the read returned and filters nothing on its own.
    backend.answer = () =>
      makeLogPage({
        status: "available",
        search: "matched",
        matchedTotal: 1,
        entries: [makeActivityEntry(1, "row from the backend", { origin: "needle" })],
      });
    draw();
    await userEvent.type(screen.getByTestId("glw-search"), "needle");
    await waitFor(() =>
      expect(screen.getByTestId("glw-match-count")).toHaveTextContent("1 match"),
    );
    expect(screen.getByText("row from the backend")).toBeInTheDocument();
    // The query is sent for the one scope the window has selected.
    const last = backend.reads()[backend.reads().length - 1];
    expect(last.pass).toEqual({ kind: "pass", pass: 2 });
    expect(last.phaseId).toBe("working");
  });
});

describe("the graduation log window: the five states", () => {
  it("GLW-FR-NMOD: loading shows while a read is in flight and nothing is held", async () => {
    let release: (page: GraduationLogPage) => void = () => {};
    backend.answer = () =>
      new Promise<GraduationLogPage>((resolve) => {
        release = resolve;
      });
    draw();
    expect(await screen.findByTestId("glw-loading")).toBeInTheDocument();
    expect(screen.queryByTestId("glw-empty")).not.toBeInTheDocument();
    await act(async () => {
      release(makeLogPage({ status: "available", entries: [makeActivityEntry(1, "arrived")] }));
    });
    expect(await screen.findByText("arrived")).toBeInTheDocument();
    expect(screen.queryByTestId("glw-loading")).not.toBeInTheDocument();
  });

  it("GLW-FR-NMOD, GLW-FR-NYQF: a search with no match is not the empty state", async () => {
    backend.answer = (args) =>
      makeLogPage({
        status: "available",
        entries: [],
        matchedTotal: 0,
        search: args.query ? "search_no_match" : "not_requested",
      });
    draw();
    await userEvent.type(screen.getByTestId("glw-search"), "nothing");
    expect(await screen.findByTestId("glw-no-match")).toBeInTheDocument();
    expect(screen.queryByTestId("glw-empty")).not.toBeInTheDocument();
  });

  it("GLW-FR-NYQF, GLW-FR-NMOD: the empty state names the entry, mentions no other stream, and never says the run produced no output", async () => {
    backend.answer = () => makeLogPage({ status: "empty", entries: [] });
    draw();
    const empty = await screen.findByTestId("glw-empty");
    expect(empty).toHaveTextContent("Pass 2 holds no agent activity yet.");
    expect(empty.textContent).not.toMatch(/run produced no output|no output/i);
    expect(empty.textContent).not.toMatch(/stream|structured|source|toggle/i);
    expect(screen.queryByTestId("glw-no-match")).not.toBeInTheDocument();
  });

  it("GLW-FR-NYQF, GLW-FR-DDXJ: an empty run-level entry says so in words", async () => {
    backend.answer = () => makeLogPage({ status: "empty", entries: [] });
    draw();
    await userEvent.click(await screen.findByRole("button", { name: RUN_LEVEL }));
    await waitFor(() =>
      expect(screen.getByTestId("glw-empty")).toHaveTextContent(
        "The run-level entry holds no agent activity yet.",
      ),
    );
  });

  it("GLW-FR-VJNK, GLW-FR-NMOD: an unreadable stream states the typed read failure and how far it got", async () => {
    backend.answer = () =>
      makeLogPage({
        status: "unavailable",
        failure: {
          kind: "read",
          code: "log_stream_corrupt",
          stream: "activity",
          message: "damaged",
          at: "2026-09-06T09:00:00Z",
          stoppedSequence: 12,
          byteOffset: 400,
          pendingRecordIds: [],
        },
      });
    draw();
    const state = await screen.findByTestId("glw-unavailable");
    expect(state).toHaveTextContent("log_stream_corrupt");
    expect(state).toHaveTextContent("activity");
    expect(state).toHaveTextContent("sequence 12");
    expect(state).toHaveTextContent("run itself is unaffected");
    expect(screen.queryByTestId("glw-persistence-failed")).not.toBeInTheDocument();
    expect(screen.queryByTestId("glw-empty")).not.toBeInTheDocument();
  });

  it("GLW-FR-OATU, GLW-FR-VJNK: a persistence failure says the run stopped and offers no control on it", async () => {
    backend.answer = () =>
      makeLogPage({
        status: "persistence_failed",
        failure: {
          kind: "write",
          code: "log_append_failed",
          stream: "activity",
          message: "the stream could not be appended to",
          at: "2026-09-06T09:00:00Z",
          pendingRecordIds: ["rec-1"],
        },
      });
    draw();
    const state = await screen.findByTestId("glw-persistence-failed");
    expect(state).toHaveTextContent("log_append_failed · activity");
    expect(state).toHaveTextContent("The log could not be written and the run stopped.");
    expect(state).toHaveTextContent("The run is interrupted. Continue retries the writes.");
    expect(within(state).queryByRole("button")).toBeNull();
    expect(screen.queryByTestId("glw-unavailable")).not.toBeInTheDocument();
    expect(screen.queryByTestId("glw-empty")).not.toBeInTheDocument();
  });

  it("GLW-FR-OJXH, GLW-FR-NMOD: a read that fails leaves the window rendered and says why", async () => {
    backend.answer = () => {
      throw new Error("no_project_open");
    };
    draw();
    expect(await screen.findByTestId("glw-unavailable")).toHaveTextContent("no_project_open");
    expect(screen.getByTestId("graduation-log-window")).toBeInTheDocument();
    expect(screen.queryByTestId("glw-empty")).not.toBeInTheDocument();
  });

  it("GLW-FR-OJXH: a failed read can be asked for again", async () => {
    let fail = true;
    backend.answer = () => {
      if (fail) throw new Error("no_project_open");
      return makeLogPage({ status: "available", entries: [makeActivityEntry(1, "recovered")] });
    };
    draw();
    await screen.findByTestId("glw-unavailable");
    fail = false;
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("recovered")).toBeInTheDocument();
  });

  it("GLW-FR-THAX, GLW-FR-OJXH: a backend that answers no read leaves a window that says the stream could not be read", async () => {
    backend.answer = () => Promise.reject(new Error("backend is gone"));
    draw();
    const state = await screen.findByTestId("glw-unavailable");
    expect(state).toHaveTextContent("could not be read");
  });
});
